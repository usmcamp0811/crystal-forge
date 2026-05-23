use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::Deserialize;
use sqlx::PgPool;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::time::Duration;
use url::Url;

use crate::api::models::ApiError;
use crate::config::ServerConfig;
use crate::handlers::api::rbac::{authenticated_user_roles, require_admin as require_admin_user};
use crate::models::cache_destination::{
    CacheDestination, CreateCacheDestination, UpdateCacheDestination,
};
use crate::queries::{cache_destinations, cache_push};

fn normalize_test_url(cache_type: &str, push_to: Option<&str>, s3_endpoint_url: Option<&str>) -> Option<String> {
    let cache_type = cache_type.to_lowercase();

    match cache_type.as_str() {
        "s3" => s3_endpoint_url
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        "attic" => push_to.and_then(|raw| {
            let raw = raw.trim();
            if raw.is_empty() {
                return None;
            }
            if raw.starts_with("http://") || raw.starts_with("https://") {
                return Some(raw.to_string());
            }
            if let Some(rest) = raw.strip_prefix("attic://") {
                let host = rest.split('/').next().unwrap_or_default().trim();
                if host.is_empty() {
                    return None;
                }
                return Some(format!("https://{host}"));
            }
            None
        }),
        _ => push_to
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
    }
}

#[derive(Debug, serde::Serialize)]
struct CacheCredentialTestResult {
    ok: bool,
    status_code: Option<u16>,
    message: String,
    tested_url: Option<String>,
}

fn validate_cache_test_url(url: &Url, allow_private_targets: bool) -> Result<(), String> {
    match url.scheme() {
        "https" => {}
        other => {
            return Err(format!(
                "Unsupported cache test URL scheme: {other}. Only https is allowed"
            ));
        }
    }

    let host = url
        .host_str()
        .ok_or_else(|| "Cache test URL must include a host".to_string())?;

    let blocked_host = !allow_private_targets
        && (host.eq_ignore_ascii_case("localhost")
            || host.ends_with(".localhost")
            || host.ends_with(".local")
            || host.ends_with(".internal"));

    if blocked_host {
        return Err("Refusing to test localhost or internal cache endpoint".to_string());
    }

    let host_for_ip_parse = host.trim_start_matches('[').trim_end_matches(']');
    if !allow_private_targets {
        if let Ok(ip) = host_for_ip_parse.parse::<IpAddr>() {
            reject_non_public_ip(ip)?;
        }
    }

    Ok(())
}

async fn validate_cache_test_url_resolves_publicly(
    url: &Url,
    allow_private_targets: bool,
) -> Result<(), String> {
    validate_cache_test_url(url, allow_private_targets)?;

    let host = url
        .host_str()
        .ok_or_else(|| "Cache test URL must include a host".to_string())?;

    if host.parse::<IpAddr>().is_ok() {
        return Ok(());
    }

    let port = url.port_or_known_default().unwrap_or(443);
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| format!("Failed to resolve cache test host: {e}"))?
        .collect();

    if allow_private_targets {
        if addrs.is_empty() {
            return Err("Cache test host did not resolve to any addresses".to_string());
        }
        Ok(())
    } else {
        validate_resolved_addrs_public(&addrs)
    }
}

fn validate_resolved_addrs_public(addrs: &[SocketAddr]) -> Result<(), String> {
    if addrs.is_empty() {
        return Err("Cache test host did not resolve to any addresses".to_string());
    }

    for addr in addrs {
        reject_non_public_ip(addr.ip())?;
    }

    Ok(())
}

fn reject_non_public_ip(ip: IpAddr) -> Result<(), String> {
    match ip {
        IpAddr::V4(v4) => {
            if v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
            {
                return Err("Refusing to test private, loopback, or non-routable IP".to_string());
            }
        }
        IpAddr::V6(v6) => {
            if v6.is_loopback() || v6.is_unspecified() || v6.is_unique_local() || v6.is_unicast_link_local() {
                return Err("Refusing to test private, loopback, or non-routable IP".to_string());
            }
        }
    }

    Ok(())
}

fn sanitize_test_url_for_response(url: &Url) -> String {
    let mut sanitized = url.clone();
    let _ = sanitized.set_username("");
    let _ = sanitized.set_password(None);
    sanitized.to_string()
}

async fn run_cache_destination_test(
    create: &CreateCacheDestination,
    allow_private_targets: bool,
) -> Result<CacheCredentialTestResult, String> {
    let cache_type = create.cache_type.trim();
    if !matches!(cache_type, "S3" | "Attic" | "Http" | "Nix" | "s3" | "attic" | "http" | "nix") {
        return Err(format!(
            "Validation failed: Invalid cache_type: {cache_type}. Must be one of: S3, Attic, Http, Nix"
        ));
    }

    let Some(test_url) = normalize_test_url(
        &create.cache_type,
        create.push_to.as_deref(),
        create.s3_endpoint_url.as_deref(),
    ) else {
        return Err("No testable endpoint URL derived from cache configuration".to_string());
    };

    let parsed_url = Url::parse(&test_url).map_err(|e| format!("Invalid cache test URL: {e}"))?;
    validate_cache_test_url_resolves_publicly(&parsed_url, allow_private_targets).await?;
    let tested_url = sanitize_test_url_for_response(&parsed_url);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("Failed to initialize HTTP client: {e}"))?;

    let mut request = client.get(parsed_url.clone());
    if let Some(token) = create.attic_token.as_ref().filter(|t| !t.trim().is_empty()) {
        request = request.bearer_auth(token.trim());
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Connectivity test failed: {e}"))?;

    if response.status().is_success() {
        Ok(CacheCredentialTestResult {
            ok: true,
            status_code: Some(response.status().as_u16()),
            message: "Connection successful".to_string(),
            tested_url: Some(tested_url),
        })
    } else {
        Ok(CacheCredentialTestResult {
            ok: false,
            status_code: Some(response.status().as_u16()),
            message: format!("Endpoint responded with status {}", response.status()),
            tested_url: Some(tested_url),
        })
    }
}

fn redact_cache_secrets(mut destination: CacheDestination) -> CacheDestination {
    destination.push_to = destination
        .push_to
        .as_deref()
        .map(sanitize_push_to_url_credentials);
    destination.attic_token = None;
    destination.s3_access_key_id = None;
    destination.s3_secret_access_key = None;
    destination.s3_session_token = None;
    destination
}

fn sanitize_push_to_url_credentials(push_to: &str) -> String {
    let Ok(mut parsed) = Url::parse(push_to) else {
        return push_to.to_string();
    };

    if parsed.password().is_none() && parsed.username().is_empty() {
        return push_to.to_string();
    }

    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);
    parsed.to_string()
}

// ============================================================================
// Cache Destinations API
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ListCacheDestinationsQuery {
    #[serde(default)]
    pub enabled_only: bool,
}

/// GET /api/caches - List all cache destinations
pub async fn list_cache_destinations(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(query): Query<ListCacheDestinationsQuery>,
) -> impl IntoResponse {
    let Some((_user_id, _roles)) = authenticated_user_roles(&pool, &headers).await else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiError {
                error: "unauthorized".to_string(),
                message: "Authentication required".to_string(),
                details: None,
            }),
        )
            .into_response();
    };

    match cache_destinations::list_cache_destinations(&pool, query.enabled_only).await {
        Ok(destinations) => {
            let redacted: Vec<CacheDestination> =
                destinations.into_iter().map(redact_cache_secrets).collect();
            (StatusCode::OK, Json(redacted)).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to list cache destinations: {:#}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to list cache destinations".to_string(),
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

/// GET /api/caches/:id - Get a single cache destination
pub async fn get_cache_destination(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    let Some((_user_id, _roles)) = authenticated_user_roles(&pool, &headers).await else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiError {
                error: "unauthorized".to_string(),
                message: "Authentication required".to_string(),
                details: None,
            }),
        )
            .into_response();
    };

    match cache_destinations::get_cache_destination(&pool, id).await {
        Ok(Some(destination)) => {
            (StatusCode::OK, Json(redact_cache_secrets(destination))).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "not_found".to_string(),
                message: format!("Cache destination with id {} not found", id),
                details: None,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to get cache destination {}: {:#}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to get cache destination".to_string(),
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

/// POST /api/caches - Create a new cache destination (admin only)
pub async fn create_cache_destination(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(create): Json<CreateCacheDestination>,
) -> impl IntoResponse {
    // Require admin role
    if require_admin_user(&pool, &headers).await.is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Admin role required".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    // Validate the request
    if let Err(e) = create.validate() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "error".to_string(),
                message: e,
                details: None,
            }),
        )
            .into_response();
    }

    match cache_destinations::create_cache_destination(&pool, &create).await {
        Ok(destination) => {
            (StatusCode::CREATED, Json(redact_cache_secrets(destination))).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to create cache destination: {:#}", e);
            let error_msg = if e.to_string().contains("duplicate key")
                || e.to_string().contains("unique constraint")
            {
                format!(
                    "Cache destination with name '{}' already exists",
                    create.name
                )
            } else {
                "Failed to create cache destination".to_string()
            };
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "error".to_string(),
                    message: error_msg,
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

/// POST /api/caches/test-credentials - Test cache destination configuration (admin only)
pub async fn test_cache_destination_credentials(
    State(pool): State<PgPool>,
    State(server_config): State<ServerConfig>,
    headers: HeaderMap,
    Json(create): Json<CreateCacheDestination>,
) -> impl IntoResponse {
    if require_admin_user(&pool, &headers).await.is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Admin role required".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    match run_cache_destination_test(&create, server_config.allow_private_cache_test_targets).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(message) => (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "invalid_cache_test_config".to_string(),
                message,
                details: None,
            }),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_test_url, sanitize_test_url_for_response, validate_cache_test_url,
        validate_cache_test_url_resolves_publicly, validate_resolved_addrs_public,
    };
    use std::net::{Ipv4Addr, SocketAddr};
    use url::Url;

    #[test]
    fn normalize_test_url_handles_attic_scheme() {
        let normalized = normalize_test_url("Attic", Some("attic://cache.example.com/team"), None);
        assert_eq!(normalized.as_deref(), Some("https://cache.example.com"));
    }

    #[test]
    fn validate_cache_test_url_rejects_http() {
        let url = Url::parse("http://cache.example.com").unwrap();
        let err = validate_cache_test_url(&url, false).unwrap_err();
        assert!(err.contains("Only https is allowed"));
    }

    #[test]
    fn validate_cache_test_url_rejects_localhost() {
        let url = Url::parse("https://localhost/cache").unwrap();
        let err = validate_cache_test_url(&url, false).unwrap_err();
        assert!(err.contains("localhost or internal"));
    }

    #[test]
    fn validate_cache_test_url_rejects_private_ip() {
        let url = Url::parse("https://10.0.0.8/cache").unwrap();
        let err = validate_cache_test_url(&url, false).unwrap_err();
        assert!(err.contains("private, loopback, or non-routable IP"));
    }

    #[test]
    fn validate_cache_test_url_rejects_local_suffixes() {
        let internal = Url::parse("https://cache.internal").unwrap();
        let local = Url::parse("https://cache.local").unwrap();
        assert!(validate_cache_test_url(&internal, false).is_err());
        assert!(validate_cache_test_url(&local, false).is_err());
    }

    #[test]
    fn validate_cache_test_url_rejects_loopback_and_link_local_ips() {
        let ipv4_loopback = Url::parse("https://127.0.0.1/cache").unwrap();
        let link_local = Url::parse("https://169.254.169.254/latest").unwrap();
        let ipv6_loopback = Url::parse("https://[::1]/cache").unwrap();

        assert!(validate_cache_test_url(&ipv4_loopback, false).is_err());
        assert!(validate_cache_test_url(&link_local, false).is_err());
        assert!(validate_cache_test_url(&ipv6_loopback, false).is_err());
    }

    #[test]
    fn validate_cache_test_url_allows_public_https_host() {
        let url = Url::parse("https://cache.nixos.org").unwrap();
        assert!(validate_cache_test_url(&url, false).is_ok());
    }

    #[test]
    fn validate_cache_test_url_allows_private_when_enabled() {
        let loopback = Url::parse("https://127.0.0.1/cache").unwrap();
        assert!(validate_cache_test_url(&loopback, true).is_ok());
    }

    #[tokio::test]
    async fn validate_cache_test_url_dns_rejects_localhost_resolution() {
        let url = Url::parse("https://localhost").unwrap();
        assert!(validate_cache_test_url_resolves_publicly(&url, false)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn validate_cache_test_url_dns_allows_localhost_when_enabled() {
        let url = Url::parse("https://localhost").unwrap();
        assert!(validate_cache_test_url_resolves_publicly(&url, true)
            .await
            .is_ok());
    }

    #[test]
    fn validate_resolved_addrs_public_rejects_private_resolution() {
        let addrs = vec![SocketAddr::from((Ipv4Addr::new(10, 0, 0, 8), 443))];
        assert!(validate_resolved_addrs_public(&addrs).is_err());
    }

    #[test]
    fn sanitize_test_url_strips_embedded_credentials() {
        let url = Url::parse("https://user:secret@example.com/cache").unwrap();
        let sanitized = sanitize_test_url_for_response(&url);
        assert_eq!(sanitized, "https://example.com/cache");
    }
}

/// PUT /api/caches/:id - Update a cache destination (admin only)
pub async fn update_cache_destination(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(update): Json<UpdateCacheDestination>,
) -> impl IntoResponse {
    // Require admin role
    if require_admin_user(&pool, &headers).await.is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Admin role required".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    match cache_destinations::update_cache_destination(&pool, id, &update).await {
        Ok(Some(destination)) => {
            (StatusCode::OK, Json(redact_cache_secrets(destination))).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "not_found".to_string(),
                message: format!("Cache destination with id {} not found", id),
                details: None,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to update cache destination {}: {:#}", id, e);
            let message = e.to_string();
            let status = if message.contains("required for")
                || message.contains("Invalid cache_type")
                || message.contains("cannot be empty")
            {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(ApiError {
                    error: if status == StatusCode::BAD_REQUEST {
                        "validation_error".to_string()
                    } else {
                        "internal_error".to_string()
                    },
                    message,
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

/// DELETE /api/caches/:id - Delete a cache destination (admin only)
pub async fn delete_cache_destination(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    // Require admin role
    if require_admin_user(&pool, &headers).await.is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Admin role required".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    match cache_destinations::delete_cache_destination(&pool, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "not_found".to_string(),
                message: format!("Cache destination with id {} not found", id),
                details: None,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to delete cache destination {}: {:#}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to delete cache destination".to_string(),
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

// ============================================================================
// Cache Push Jobs API
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ListCachePushJobsQuery {
    pub status: Option<String>,
    pub cache_destination: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i32,
    #[serde(default)]
    pub offset: i32,
}

fn default_limit() -> i32 {
    50
}

/// GET /api/cache-push-jobs - List cache push jobs with filtering
pub async fn list_cache_push_jobs(
    State(pool): State<PgPool>,
    Query(query): Query<ListCachePushJobsQuery>,
) -> impl IntoResponse {
    match cache_push::list_cache_push_jobs(
        &pool,
        query.status.as_deref(),
        query.cache_destination.as_deref(),
        Some(query.limit),
        Some(query.offset),
    )
    .await
    {
        Ok(jobs) => (StatusCode::OK, Json(jobs)).into_response(),
        Err(e) => {
            tracing::error!("Failed to list cache push jobs: {:#}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to list cache push jobs".to_string(),
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

/// GET /api/cache-push-jobs/:id - Get cache push job details
pub async fn get_cache_push_job(
    State(pool): State<PgPool>,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    match cache_push::get_cache_push_job_detail(&pool, id).await {
        Ok(Some(job)) => (StatusCode::OK, Json(job)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "not_found".to_string(),
                message: format!("Cache push job with id {} not found", id),
                details: None,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to get cache push job {}: {:#}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to get cache push job".to_string(),
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

/// POST /api/cache-push-jobs/:id/retry - Retry a failed cache push job (admin only)
pub async fn retry_cache_push_job(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    // Require admin role
    if require_admin_user(&pool, &headers).await.is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Admin role required".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    match cache_push::retry_cache_push_job(&pool, id).await {
        Ok(true) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "message": "Cache push job queued for retry"
            })),
        )
            .into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "error".to_string(),
                message: format!(
                    "Cache push job {} not found or not in a retryable state",
                    id
                ),
                details: None,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to retry cache push job {}: {:#}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to retry cache push job".to_string(),
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

/// POST /api/cache-push-jobs/:id/cancel - Cancel a pending or failed cache push job (admin only)
pub async fn cancel_cache_push_job(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> impl IntoResponse {
    // Require admin role
    if require_admin_user(&pool, &headers).await.is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Admin role required".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    match cache_push::cancel_cache_push_job(&pool, id).await {
        Ok(true) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "message": "Cache push job cancelled"
            })),
        )
            .into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "error".to_string(),
                message: format!(
                    "Cache push job {} not found or not in a cancellable state",
                    id
                ),
                details: None,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to cancel cache push job {}: {:#}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to cancel cache push job".to_string(),
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct BulkJobAction {
    pub job_ids: Vec<i32>,
}

/// POST /api/cache-push-jobs/bulk-retry - Bulk retry cache push jobs (admin only)
pub async fn bulk_retry_cache_push_jobs(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(action): Json<BulkJobAction>,
) -> impl IntoResponse {
    // Require admin role
    if require_admin_user(&pool, &headers).await.is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Admin role required".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    if action.job_ids.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "validation_error".to_string(),
                message: "No job IDs provided".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    match cache_push::bulk_retry_cache_push_jobs(&pool, &action.job_ids).await {
        Ok(count) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "message": format!("Queued {} jobs for retry", count),
                "count": count
            })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to bulk retry cache push jobs: {:#}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to bulk retry cache push jobs".to_string(),
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

/// POST /api/cache-push-jobs/bulk-cancel - Bulk cancel cache push jobs (admin only)
pub async fn bulk_cancel_cache_push_jobs(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(action): Json<BulkJobAction>,
) -> impl IntoResponse {
    // Require admin role
    if require_admin_user(&pool, &headers).await.is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Admin role required".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    if action.job_ids.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "validation_error".to_string(),
                message: "No job IDs provided".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    match cache_push::bulk_cancel_cache_push_jobs(&pool, &action.job_ids).await {
        Ok(count) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "message": format!("Cancelled {} jobs", count),
                "count": count
            })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to bulk cancel cache push jobs: {:#}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_error".to_string(),
                    message: "Failed to bulk cancel cache push jobs".to_string(),
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Environment Assignment Handlers
// ─────────────────────────────────────────────────────────────────────────────

/// Request to assign environments to a cache destination
#[derive(Debug, Deserialize)]
pub struct AssignEnvironmentsRequest {
    pub environment_ids: Vec<uuid::Uuid>,
}

/// GET /api/caches/:id/environments - Get environments assigned to a cache
pub async fn get_cache_environments_handler(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(cache_id): Path<i32>,
) -> impl IntoResponse {
    // Require authentication
    let Some((_user_id, _roles)) = authenticated_user_roles(&pool, &headers).await else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiError {
                error: "unauthorized".to_string(),
                message: "Authentication required".to_string(),
                details: None,
            }),
        )
            .into_response();
    };

    match crate::queries::cache_destinations::get_cache_environments(&pool, cache_id).await {
        Ok(environment_ids) => (StatusCode::OK, Json(environment_ids)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "internal_server_error".to_string(),
                message: format!("Failed to get cache environments: {e}"),
                details: None,
            }),
        )
            .into_response(),
    }
}

/// PUT /api/caches/:id/environments - Assign environments to a cache destination
pub async fn assign_cache_environments_handler(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(cache_id): Path<i32>,
    Json(req): Json<AssignEnvironmentsRequest>,
) -> impl IntoResponse {
    // Require admin role
    if require_admin_user(&pool, &headers).await.is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".to_string(),
                message: "Admin role required".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    match crate::queries::cache_destinations::cache_destination_exists(&pool, cache_id).await {
        Ok(false) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: "not_found".to_string(),
                    message: format!("Cache destination with id {} not found", cache_id),
                    details: None,
                }),
            )
                .into_response();
        }
        Ok(true) => {}
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "internal_server_error".to_string(),
                    message: format!("Failed to validate cache destination: {e}"),
                    details: None,
                }),
            )
                .into_response();
        }
    }

    match crate::queries::cache_destinations::assign_environments_to_cache(
        &pool,
        cache_id,
        &req.environment_ids,
    )
    .await
    {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "message": "Environments assigned successfully",
                "cache_id": cache_id,
                "environment_count": req.environment_ids.len()
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "internal_server_error".to_string(),
                message: format!("Failed to assign environments: {e}"),
                details: None,
            }),
        )
            .into_response(),
    }
}

/// GET /api/environments/:id/caches - Get caches assigned to an environment
pub async fn get_environment_caches_handler(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(environment_id): Path<uuid::Uuid>,
) -> impl IntoResponse {
    // Require authentication
    let Some((_user_id, _roles)) = authenticated_user_roles(&pool, &headers).await else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiError {
                error: "unauthorized".to_string(),
                message: "Authentication required".to_string(),
                details: None,
            }),
        )
            .into_response();
    };

    match crate::queries::cache_destinations::get_caches_for_environment(&pool, environment_id)
        .await
    {
        Ok(caches) => {
            let redacted: Vec<CacheDestination> =
                caches.into_iter().map(redact_cache_secrets).collect();
            (StatusCode::OK, Json(redacted)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "internal_server_error".to_string(),
                message: format!("Failed to get environment caches: {e}"),
                details: None,
            }),
        )
            .into_response(),
    }
}
