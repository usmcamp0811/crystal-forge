//! Observes provider inventory without persistence or inventory enumeration.
//!
//! Niks3 1.8+ exposes public `GET /api/cache-stats`. The packaged 1.6 CLI
//! remains unchanged. A missing remote endpoint is unavailable, not evidence
//! of an empty cache or a known server version. Other configured provider
//! types have no supported cheap totals contract and cause no network work.
//!
//! Observations are request-local. Callers must limit concurrent requests and
//! must not interpret tracked objects as Nix paths or logical bytes as disk
//! usage. No observation updates destination credentials, scope, usage or jobs.

use super::*;
use crate::handlers::api::rbac::{has_admin_role, has_viewer_or_above_role};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::future::Future;

// The overall budget includes DNS, CA loading, TLS and streamed response
// consumption. Reqwest's existing eight-second timeout alone excludes DNS.
const OBSERVATION_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_STATS_BYTES: usize = 64 * 1024;

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Available,
    Unsupported,
    Unavailable,
    Error,
}

// Response-only values. No URL, provider body, identity or credential is kept.
#[derive(Debug, Serialize)]
struct Metrics {
    status: Status,
    reason_code: &'static str,
    storage_bytes: Option<u64>,
    storage_bytes_basis: Option<&'static str>,
    object_count: Option<u64>,
    object_count_basis: Option<&'static str>,
    path_count: Option<u64>,
    measured_at: Option<DateTime<Utc>>,
}

impl Metrics {
    fn absent(status: Status, reason_code: &'static str) -> Self {
        Self {
            status,
            reason_code,
            storage_bytes: None,
            storage_bytes_basis: None,
            object_count: None,
            object_count_basis: None,
            path_count: None,
            measured_at: None,
        }
    }
}

// COMPATIBILITY: Niks3 v1.8.0 api/types.go:68-75 uses signed 64-bit fields.
// Required fields, range checks and non-negative validation prevent absent,
// malformed or overflowing provider values from becoming fabricated zeros.
#[derive(Deserialize)]
struct NativeStats {
    objects: i64,
    logical_bytes: i64,
}

fn parse_stats(body: &[u8]) -> Metrics {
    let parsed = serde_json::from_slice::<NativeStats>(body)
        .ok()
        .and_then(|stats| {
            Some((
                u64::try_from(stats.objects).ok()?,
                u64::try_from(stats.logical_bytes).ok()?,
            ))
        });
    let Some((objects, bytes)) = parsed else {
        return Metrics::absent(Status::Error, "invalid_stats");
    };
    // Niks3 sums known client-reported uncompressed sizes of live tracked
    // objects. Legacy/unknown sizes are excluded. The count includes metadata
    // objects, not only NARs, and excludes tombstones and untracked S3 objects.
    // This observation time is local: upstream provides no measured timestamp.
    Metrics {
        status: Status::Available,
        reason_code: "native_stats",
        storage_bytes: Some(bytes),
        storage_bytes_basis: Some("reported_logical"),
        object_count: Some(objects),
        object_count_basis: Some("live_tracked_objects"),
        path_count: None,
        measured_at: Some(Utc::now()),
    }
}

fn metadata_outcome(cache_type: &str, enabled: bool, admin: bool) -> Option<Metrics> {
    if !enabled {
        return Some(Metrics::absent(Status::Unavailable, "disabled"));
    }
    match cache_type {
        "Attic" | "S3" | "Http" | "Nix" => Some(Metrics::absent(
            Status::Unsupported,
            "provider_totals_unsupported",
        )),
        "Niks3" if !admin => Some(Metrics::absent(Status::Unavailable, "requires_admin")),
        "Niks3" => None,
        _ => Some(Metrics::absent(Status::Error, "unsupported_cache_type")),
    }
}

fn metrics_response(metrics: Metrics) -> axum::response::Response {
    // In particular, shared/browser HTTP caches must not retain an admin's
    // observation after logout or serve it to another role.
    (
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(metrics),
    )
        .into_response()
}

/// Returns a request-local, credential-free cache inventory observation.
///
/// `GET /api/v1/caches/:id/metrics` authenticates before lookup. Readers get
/// unsupported/disabled states from a secret-free type/enabled projection.
/// Only Admin can load/decrypt a Niks3 snapshot or initiate a provider request;
/// other readers receive `unavailable` with `requires_admin` for Niks3.
///
/// The Niks3 request uses only the selected write TLS identity. It sends no
/// Bearer, Basic or read-plane credential to the public stats endpoint. DNS,
/// TLS and body reading share an eight-second deadline; redirects and proxies
/// are disabled, addresses are pinned and metadata is limited to 64 KiB.
/// No result is persisted or cached. Disabled destinations never cause a probe.
/// Concurrent edits can make the unlocked snapshot stale before observation.
///
/// Returns `401` before lookup for unauthenticated callers, `403` for callers
/// below Viewer, `404` for an absent ID and `500` for a metadata database
/// failure. Otherwise `200`
/// carries `available`, `unsupported`, `unavailable` or `error`. Missing values
/// are null, never zero. `path_count` is always null. `measured_at` is the local
/// successful observation time, not an upstream measurement timestamp.
///
/// # Examples
/// ```text
/// GET /api/v1/caches/42/metrics
/// {"status":"unsupported","reason_code":"provider_totals_unsupported",
///  "storage_bytes":null,"storage_bytes_basis":null,"object_count":null,
///  "object_count_basis":null,"path_count":null,"measured_at":null}
/// ```
pub async fn get_cache_metrics(
    State(pool): State<PgPool>,
    State(config): State<ServerConfig>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> axum::response::Response {
    let Some((_, roles)) = authenticated_user_roles(&pool, &headers).await else {
        return probe_error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "Authentication required",
        );
    };
    if !has_viewer_or_above_role(&roles) {
        return probe_error(StatusCode::FORBIDDEN, "forbidden", "Viewer role required");
    }
    // SECURITY: This projection contains no credentials. Do not call the
    // existing whole-row decrypting query on a viewer's metrics request.
    let metadata = sqlx::query_as::<_, (String, bool)>(
        "SELECT cache_type, enabled FROM cache_destinations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&pool)
    .await;
    let (cache_type, enabled) = match metadata {
        Ok(Some(metadata)) => metadata,
        Ok(None) => {
            return probe_error(
                StatusCode::NOT_FOUND,
                "not_found",
                "Cache destination not found",
            );
        }
        Err(_) => {
            return probe_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Failed to load cache metadata",
            );
        }
    };
    let admin = has_admin_role(&roles);
    if let Some(outcome) = metadata_outcome(&cache_type, enabled, admin) {
        return metrics_response(outcome);
    }
    // SECURITY: The admin decision above precedes all decryption and network
    // work. Recheck type/enabled from the later snapshot to handle intervening
    // edits without probing a disabled or converted destination.
    let snapshot = match cache_destinations::get_cache_destination(&pool, id).await {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => {
            return metrics_response(Metrics::absent(
                Status::Unavailable,
                "destination_unavailable",
            ));
        }
        Err(_) => return metrics_response(Metrics::absent(Status::Error, "invalid_configuration")),
    };
    if let Some(outcome) = metadata_outcome(&snapshot.cache_type, snapshot.enabled, admin) {
        return metrics_response(outcome);
    }
    metrics_response(
        with_deadline(
            probe(&snapshot, config.allow_private_cache_test_targets),
            OBSERVATION_TIMEOUT,
        )
        .await,
    )
}

async fn with_deadline(future: impl Future<Output = Metrics>, budget: Duration) -> Metrics {
    match tokio::time::timeout(budget, future).await {
        Ok(metrics) => metrics,
        Err(_) => Metrics::absent(Status::Unavailable, "deadline_exceeded"),
    }
}

async fn probe(snapshot: &CacheDestination, allow_private_targets: bool) -> Metrics {
    let Some(raw_url) = snapshot.niks3_server_url.as_deref() else {
        return Metrics::absent(Status::Error, "invalid_configuration");
    };
    let server = match niks3_base_url(raw_url, allow_private_targets) {
        Ok(server) => server,
        Err(_) => return Metrics::absent(Status::Error, "target_policy"),
    };
    let transport = match snapshot.niks3_write_auth_mode.as_deref() {
        Some("token") => niks3_write_transport(Some("token"), None, None, None),
        Some("mtls") => niks3_write_transport(
            Some("mtls"),
            snapshot.niks3_write_client_cert.as_deref(),
            snapshot.niks3_write_client_key.as_deref(),
            snapshot.niks3_write_ca_cert.as_deref(),
        ),
        _ => return Metrics::absent(Status::Error, "invalid_configuration"),
    };
    let Ok(transport) = transport else {
        return Metrics::absent(Status::Error, "invalid_configuration");
    };
    let client = match cache_test_client(
        &server,
        allow_private_targets,
        transport.cert,
        transport.key,
        transport.ca,
    )
    .await
    {
        Ok(client) => client,
        Err(_) => return Metrics::absent(Status::Unavailable, "transport_unavailable"),
    };
    let request = match stats_request(&client, &server) {
        Ok(request) => request,
        Err(_) => return Metrics::absent(Status::Error, "target_policy"),
    };
    let response = match request.send().await {
        Ok(response) => response,
        Err(_) => return Metrics::absent(Status::Unavailable, "transport_unavailable"),
    };
    consume_response(response).await
}

fn stats_request(client: &reqwest::Client, server: &Url) -> Result<reqwest::RequestBuilder, ()> {
    let endpoint = server.join("api/cache-stats").map_err(|_| ())?;
    // Public native metadata never uses token, Basic or read credentials.
    Ok(client.get(endpoint))
}

async fn consume_response(mut response: reqwest::Response) -> Metrics {
    if let Some(outcome) = status_outcome(response.status().as_u16()) {
        return outcome;
    }
    if !response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/json"))
        })
    {
        return Metrics::absent(Status::Error, "invalid_stats");
    }
    let mut body = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if !append_stats_chunk(&mut body, &chunk) {
                    return Metrics::absent(Status::Error, "stats_too_large");
                }
            }
            Ok(None) => return parse_stats(&body),
            Err(_) => return Metrics::absent(Status::Unavailable, "response_unavailable"),
        }
    }
}

fn status_outcome(status: u16) -> Option<Metrics> {
    match status {
        200 => None,
        404 => Some(Metrics::absent(Status::Unavailable, "endpoint_unavailable")),
        401 | 403 => Some(Metrics::absent(Status::Unavailable, "access_denied")),
        _ => Some(Metrics::absent(Status::Error, "unexpected_status")),
    }
}

fn append_stats_chunk(body: &mut Vec<u8>, chunk: &[u8]) -> bool {
    if body.len().saturating_add(chunk.len()) > MAX_STATS_BYTES {
        return false;
    }
    body.extend_from_slice(chunk);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_values_preserve_basis_without_inventing_paths() {
        for (objects, bytes) in [(0, 0), (7, 2048), (i64::MAX, i64::MAX)] {
            let metrics = parse_stats(
                format!(r#"{{"objects":{objects},"logical_bytes":{bytes}}}"#).as_bytes(),
            );
            assert_eq!(metrics.status, Status::Available);
            assert_eq!(metrics.storage_bytes, Some(bytes as u64));
            assert_eq!(metrics.object_count, Some(objects as u64));
            assert_eq!(metrics.storage_bytes_basis, Some("reported_logical"));
            assert_eq!(metrics.object_count_basis, Some("live_tracked_objects"));
            assert!(metrics.path_count.is_none());
            assert!(metrics.measured_at.is_some());
        }
    }

    #[test]
    fn malformed_negative_missing_and_overflow_stats_never_become_zero() {
        for body in [
            "{}",
            r#"{"objects":1}"#,
            r#"{"logical_bytes":1}"#,
            r#"{"objects":-1,"logical_bytes":0}"#,
            r#"{"objects":0,"logical_bytes":-1}"#,
            r#"{"objects":9223372036854775808,"logical_bytes":0}"#,
            r#"{"objects":0,"logical_bytes":18446744073709551615}"#,
            r#"{"objects":1.5,"logical_bytes":0}"#,
            r#"{"objects":"1","logical_bytes":0}"#,
            r#"{"objects":null,"logical_bytes":0}"#,
            "<html>private-upstream-marker</html>",
        ] {
            let metrics = parse_stats(body.as_bytes());
            assert_eq!(metrics.status, Status::Error);
            assert!(metrics.storage_bytes.is_none());
            assert!(metrics.object_count.is_none());
            assert!(metrics.measured_at.is_none());
            assert!(
                !serde_json::to_string(&metrics)
                    .unwrap()
                    .contains("private-upstream-marker")
            );
        }
    }

    #[test]
    fn metadata_gates_skip_all_unsupported_disabled_and_non_admin_probes() {
        for cache_type in ["Niks3", "Attic", "S3", "Http", "Nix"] {
            for admin in [false, true] {
                let disabled = metadata_outcome(cache_type, false, admin).unwrap();
                assert_eq!(disabled.reason_code, "disabled");
                assert!(disabled.storage_bytes.is_none());
                assert!(disabled.object_count.is_none());
                if cache_type != "Niks3" {
                    assert_eq!(
                        metadata_outcome(cache_type, true, admin).unwrap().status,
                        Status::Unsupported
                    );
                }
            }
        }
        assert_eq!(
            metadata_outcome("Niks3", true, false).unwrap().reason_code,
            "requires_admin"
        );
        assert!(metadata_outcome("Niks3", true, true).is_none());
        assert_eq!(
            metadata_outcome("Unknown", true, true).unwrap().status,
            Status::Error
        );
    }

    #[test]
    fn missing_endpoint_denial_redirects_and_non_native_success_are_not_empty_stats() {
        for (status, expected, reason) in [
            (404, Status::Unavailable, "endpoint_unavailable"),
            (401, Status::Unavailable, "access_denied"),
            (403, Status::Unavailable, "access_denied"),
            (201, Status::Error, "unexpected_status"),
            (204, Status::Error, "unexpected_status"),
            (302, Status::Error, "unexpected_status"),
            (500, Status::Error, "unexpected_status"),
        ] {
            let metrics = status_outcome(status).unwrap();
            assert_eq!(metrics.status, expected);
            assert_eq!(metrics.reason_code, reason);
            assert!(metrics.storage_bytes.is_none());
            assert!(metrics.object_count.is_none());
            assert!(metrics.path_count.is_none());
            assert!(metrics.measured_at.is_none());
        }
        assert!(status_outcome(200).is_none());
    }

    #[test]
    fn streamed_body_limit_rejects_overflow_without_retaining_rejected_chunk() {
        let mut body = Vec::new();
        assert!(append_stats_chunk(
            &mut body,
            &vec![b' '; MAX_STATS_BYTES - 1]
        ));
        assert!(append_stats_chunk(&mut body, b" "));
        assert_eq!(body.len(), MAX_STATS_BYTES);
        assert!(!append_stats_chunk(&mut body, b"private-upstream-marker"));
        assert_eq!(body.len(), MAX_STATS_BYTES);
        assert!(body.iter().all(|byte| *byte == b' '));
    }

    #[tokio::test]
    async fn absent_metrics_wire_shape_is_explicit_and_not_http_cached() {
        let response = metrics_response(Metrics::absent(Status::Unavailable, "requires_admin"));
        assert_eq!(
            response.headers()[axum::http::header::CACHE_CONTROL],
            "no-store"
        );
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "unavailable");
        assert_eq!(json["reason_code"], "requires_admin");
        for field in [
            "storage_bytes",
            "storage_bytes_basis",
            "object_count",
            "object_count_basis",
            "path_count",
            "measured_at",
        ] {
            assert!(json.get(field).unwrap().is_null());
        }
    }

    #[test]
    fn stats_request_keeps_configured_authority_prefix_and_no_authorization() {
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let server = niks3_base_url("https://cache.example/prefix/", false).unwrap();
        let request = stats_request(&client, &server).unwrap().build().unwrap();
        assert_eq!(request.method(), reqwest::Method::GET);
        assert_eq!(
            request.url().as_str(),
            "https://cache.example/prefix/api/cache-stats"
        );
        assert!(
            !request
                .headers()
                .contains_key(reqwest::header::AUTHORIZATION)
        );
        for url in [
            "http://cache.example",
            "https://user:password@cache.example",
            "https://cache.example?token=private-marker",
            "https://cache.example#fragment",
        ] {
            assert!(niks3_base_url(url, false).is_err());
        }
    }

    #[tokio::test]
    async fn overall_deadline_cancels_pending_work_and_returns_nulls() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        struct Guard(Arc<AtomicBool>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let dropped = Arc::new(AtomicBool::new(false));
        let guard = Guard(dropped.clone());
        let metrics = with_deadline(
            async move {
                let _guard = guard;
                std::future::pending::<Metrics>().await
            },
            Duration::from_millis(1),
        )
        .await;
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(metrics.reason_code, "deadline_exceeded");
        assert!(metrics.storage_bytes.is_none());
        assert!(metrics.object_count.is_none());
        assert!(metrics.measured_at.is_none());
    }

    #[tokio::test]
    async fn unauthenticated_request_stops_before_database_lookup() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgresql://unused:unused@127.0.0.1:1/unused")
            .unwrap();
        let response = get_cache_metrics(
            State(pool),
            State(ServerConfig::default()),
            HeaderMap::new(),
            Path(42),
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
