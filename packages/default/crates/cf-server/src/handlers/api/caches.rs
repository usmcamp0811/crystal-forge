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
    CacheDestination, CreateCacheDestination, LEGACY_QUERY_CREDENTIALS_TEST_ERROR,
    UpdateCacheDestination, cache_url_has_query_credentials,
    cache_url_query_parameter_is_sensitive, effective_cache_url, sanitize_cache_url_credentials,
};
use crate::queries::{cache_destinations, cache_push};

mod attic_probe;
mod s3_probe;

#[cfg(test)]
mod retained_probe_tests;

fn probe_error(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (
        status,
        Json(ApiError {
            error: code.into(),
            message: message.into(),
            details: None,
        }),
    )
        .into_response()
}

fn optional_probe_csrf(headers: &HeaderMap) -> Result<(), axum::response::Response> {
    use crate::auth::session::{CSRF_COOKIE_NAME, CSRF_HEADER_NAME, extract_cookie};
    // Read-only probes follow cache API session authorization. If the caller
    // supplies double-submit CSRF state, require the existing matching pattern.
    if headers.contains_key(&CSRF_HEADER_NAME)
        || extract_cookie(headers, CSRF_COOKIE_NAME).is_some()
    {
        crate::handlers::api::auth_session::require_csrf(headers)?;
    }
    Ok(())
}

async fn probe_json<T: serde::de::DeserializeOwned>(
    request: axum::extract::Request,
) -> Result<T, axum::response::Response> {
    let invalid = || {
        probe_error(
            StatusCode::BAD_REQUEST,
            "invalid_cache_test_config",
            "Invalid cache test JSON",
        )
    };
    if !request
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|value| value.trim() == "application/json")
        })
    {
        return Err(invalid());
    }
    let body = axum::body::to_bytes(request.into_body(), 1024 * 1024)
        .await
        .map_err(|_| invalid())?;
    serde_json::from_slice(&body).map_err(|_| invalid())
}

/// Tests a stored destination with the same unwrapped update JSON used by Save.
///
/// `POST /api/v1/caches/:id/test-credentials` returns `403` before lookup or JSON
/// parsing for non-admin callers, `404` for an absent ID, and `400` for invalid
/// JSON, effective settings, or targets. Provided CSRF state must match. A `200`
/// result describes only observed read access, never upload authorization.
/// Stored secrets are decrypted and merged in memory. No destination, assignment,
/// timestamp, usage, encryption, or job write occurs. The unlocked snapshot may
/// become stale during the probe; the result is not a guarantee about later Save.
/// Legacy Http/Nix URL Basic auth stays server-only and is sent through Reqwest's
/// sensitive Authorization header. Same-type sanitized URL round trips retain
/// that identity; different URLs cannot borrow it. Recognized credential queries
/// return `400 legacy_query_credentials_unsupported` before DNS or network work.
///
/// # Examples
/// ```text
/// POST /api/v1/caches/42/test-credentials
/// Content-Type: application/json
///
/// {}
/// ```
pub async fn test_stored_cache_destination_credentials(
    State(pool): State<PgPool>,
    State(server_config): State<ServerConfig>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    request: axum::extract::Request,
) -> axum::response::Response {
    test_stored_with_probe(&pool, &headers, id, request, |effective| async move {
        run_cache_destination_test(&effective, server_config.allow_private_cache_test_targets).await
    })
    .await
}

// Test injection is per-call and private, not global or part of server state.
// Production always supplies the pinned HTTPS probe above.
async fn test_stored_with_probe<F, Fut>(
    pool: &PgPool,
    headers: &HeaderMap,
    id: i32,
    request: axum::extract::Request,
    probe: F,
) -> axum::response::Response
where
    F: FnOnce(CreateCacheDestination) -> Fut,
    Fut: std::future::Future<Output = Result<CacheCredentialTestResult, String>>,
{
    if require_admin_user(pool, headers).await.is_none() {
        return probe_error(StatusCode::FORBIDDEN, "forbidden", "Admin role required");
    }
    if let Err(response) = optional_probe_csrf(headers) {
        return response;
    }
    // CONCURRENCY: This single SELECT releases its connection before any network
    // work. Do not reuse the publication snapshot helper, which retains locks.
    let current = match cache_destinations::get_cache_destination(pool, id).await {
        Ok(Some(current)) => current,
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
                "Failed to load cache destination",
            );
        }
    };
    let update: UpdateCacheDestination = match probe_json(request).await {
        Ok(update) => update,
        Err(response) => return response,
    };
    let same_type = update
        .cache_type
        .as_deref()
        .is_none_or(|ty| ty == current.cache_type);
    let push_to = effective_cache_url(
        current.push_to.as_deref(),
        update.push_to.as_deref(),
        same_type,
    );
    let s3_endpoint = effective_cache_url(
        current.s3_endpoint_url.as_deref(),
        update.s3_endpoint_url.as_deref(),
        same_type,
    );
    let niks3_server = update
        .niks3_server_url
        .as_deref()
        .or(current.niks3_server_url.as_deref());
    if let Err(message) =
        reject_probe_query_credentials([push_to.as_deref(), s3_endpoint.as_deref(), niks3_server])
    {
        if update.cache_type.as_deref().unwrap_or(&current.cache_type) == "Attic" {
            return attic_probe::policy_error_response(message);
        }
        return probe_error(
            StatusCode::BAD_REQUEST,
            "legacy_query_credentials_unsupported",
            message,
        );
    }
    let effective = match cache_destinations::effective_update(&current, &update) {
        Ok(effective) => effective,
        Err(_) => {
            return probe_error(
                StatusCode::BAD_REQUEST,
                "invalid_cache_test_config",
                "Invalid effective cache configuration",
            );
        }
    };
    let is_attic = effective.cache_type == "Attic";
    match probe(effective).await {
        Ok(mut result) => {
            // Stored-ID results describe stages, not the private snapshot.
            result.tested_url = None;
            (StatusCode::OK, Json(result)).into_response()
        }
        Err(message) if is_attic => attic_probe::policy_error_response(&message),
        Err(message) => probe_error(
            StatusCode::BAD_REQUEST,
            "invalid_cache_test_config",
            &message,
        ),
    }
}

fn normalize_test_url(
    cache_type: &str,
    push_to: Option<&str>,
    s3_endpoint_url: Option<&str>,
) -> Option<String> {
    let cache_type = cache_type.to_lowercase();

    match cache_type.as_str() {
        "s3" => s3_endpoint_url
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        // Attic must use its named-cache API, never a generic root GET.
        "attic" => None,
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
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    niks3: Option<CacheProbeDetails>,
}

// Untagged flattening preserves the existing S3 and Niks3 wire shapes. Only
// Attic adds the cache-access evidence; inactive types cannot report it.
#[derive(Debug, serde::Serialize)]
#[serde(untagged)]
enum CacheProbeDetails {
    Niks3(Niks3ConnectionTestResult),
    Attic(attic_probe::AtticTestResult),
}

// Each stage describes only observed read-only behavior. In particular, the
// public cache-config endpoint cannot establish write authorization.
#[derive(Debug, Default, serde::Serialize)]
struct Niks3ConnectionTestResult {
    server_reachable: bool,
    discovery_valid: bool,
    write_auth_valid: Option<bool>,
    read_endpoint_reachable: bool,
    signing_keys_found: bool,
}

/// Specifies the Niks3 write server to discover without sending credentials.
///
/// # Examples
/// ```
/// use crystal_forge::handlers::api::caches::Niks3DiscoverRequest;
/// let request: Niks3DiscoverRequest = serde_json::from_str(
///     r#"{"server_url":"https://cache.example.com"}"#,
/// )?;
/// assert_eq!(request.server_url, "https://cache.example.com");
/// # Ok::<(), serde_json::Error>(())
/// ```
#[derive(Deserialize)]
pub struct Niks3DiscoverRequest {
    /// HTTPS server base URL; userinfo, queries, and fragments are rejected.
    pub server_url: String,
}

// COMPATIBILITY: Niks3 v1.6.0 api/types.go defines this exact wire shape for
// GET /api/cache-config. oidc_audience is omitted without an issuer query.
#[derive(Debug, Deserialize, serde::Serialize)]
struct Niks3Discovery {
    server_url: String,
    substituter_url: String,
    public_keys: Vec<String>,
    oidc_audience: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Niks3CacheConfig {
    substituter_url: String,
    public_keys: Vec<String>,
    #[serde(default)]
    oidc_audience: Option<String>,
}

fn niks3_base_url(raw: &str, allow_private_targets: bool) -> Result<Url, String> {
    let mut url = Url::parse(raw.trim()).map_err(|_| "Invalid Niks3 HTTPS URL".to_string())?;
    validate_cache_test_url(&url, allow_private_targets)?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Niks3 URLs must not contain userinfo, queries, or fragments".into());
    }
    // Preserve reverse-proxy path prefixes when appending API/object paths.
    let path = format!("{}/", url.path().trim_end_matches('/'));
    url.set_path(&path);
    Ok(url)
}

async fn cache_test_addresses(
    url: &Url,
    allow_private_targets: bool,
) -> Result<Vec<SocketAddr>, String> {
    validate_cache_test_url(url, allow_private_targets)?;
    let host = url.host_str().ok_or("Cache test URL must include a host")?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let port = url.port_or_known_default().unwrap_or(443);
    let addrs = if let Ok(ip) = host.parse::<IpAddr>() {
        vec![SocketAddr::new(ip, port)]
    } else {
        tokio::time::timeout(
            Duration::from_secs(8),
            tokio::net::lookup_host((host, port)),
        )
        .await
        .map_err(|_| "Cache test DNS lookup timed out")?
        .map_err(|_| "Failed to resolve cache test host")?
        .collect::<Vec<_>>()
    };
    if addrs.is_empty() {
        return Err("Cache test host did not resolve to any addresses".into());
    }
    if !allow_private_targets {
        validate_resolved_addrs_public(&addrs)?;
    }
    Ok(addrs)
}

async fn cache_test_client(
    url: &Url,
    allow_private_targets: bool,
    cert: Option<&str>,
    key: Option<&str>,
    ca: Option<&str>,
) -> Result<reqwest::Client, String> {
    let addrs = cache_test_addresses(url, allow_private_targets).await?;
    cache_test_client_pinned(url, &addrs, cert, key, ca).await
}

async fn cache_test_client_pinned(
    url: &Url,
    addrs: &[SocketAddr],
    cert: Option<&str>,
    key: Option<&str>,
    ca: Option<&str>,
) -> Result<reqwest::Client, String> {
    // SECURITY: Connect only to the addresses checked above, retaining the URL
    // host for TLS verification. Proxies and redirects would bypass this pin.
    let mut builder = reqwest::Client::builder()
        .use_rustls_tls()
        .no_proxy()
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(url.host_str().ok_or("Missing cache host")?, addrs);
    // Rustls's bundled public roots do not include an operator's system CA.
    // Honor standard process CA configuration without a test-only TLS bypass.
    // VM fixtures install their CA before starting the server process.
    let configured_roots =
        std::env::var_os("SSL_CERT_FILE").or_else(|| std::env::var_os("NIX_SSL_CERT_FILE"));
    let root_path = configured_roots
        .clone()
        .unwrap_or_else(|| "/etc/ssl/certs/ca-certificates.crt".into());
    match tokio::fs::read(root_path).await {
        Ok(pem) => {
            let certificates = reqwest::Certificate::from_pem_bundle(&pem)
                .map_err(|_| "Invalid system CA certificate bundle")?;
            for certificate in certificates {
                builder = builder.add_root_certificate(certificate);
            }
        }
        Err(_) if configured_roots.is_some() => {
            return Err("Failed to read configured system CA bundle".into());
        }
        Err(_) => {}
    }
    match (cert, key) {
        (Some(cert), Some(key)) => {
            let pem = format!("{cert}\n{key}");
            let identity = reqwest::Identity::from_pem(pem.as_bytes())
                .map_err(|_| "Invalid cache mTLS certificate or private key")?;
            builder = builder.identity(identity);
        }
        (None, None) => {}
        _ => return Err("Cache mTLS requires both certificate and private key".into()),
    }
    if let Some(ca) = ca {
        let certificates = reqwest::Certificate::from_pem_bundle(ca.as_bytes())
            .map_err(|_| "Invalid cache CA certificate")?;
        if certificates.is_empty() {
            return Err("Cache CA bundle contains no certificates".into());
        }
        for certificate in certificates {
            builder = builder.add_root_certificate(certificate);
        }
    }
    builder
        .build()
        .map_err(|_| "Failed to initialize cache test HTTP client".into())
}

fn require_probe_success(status: reqwest::StatusCode) -> Result<(), String> {
    if status.is_redirection() {
        Err("Cache endpoint redirects are not allowed".into())
    } else if !status.is_success() {
        Err(format!(
            "Cache endpoint responded with status {}",
            status.as_u16()
        ))
    } else {
        Ok(())
    }
}

async fn probe_body(mut response: reqwest::Response) -> Result<Vec<u8>, String> {
    require_probe_success(response.status())?;
    // Discovery and nix-cache-info are small metadata. Bound untrusted bodies
    // even when Content-Length is absent or inaccurate.
    const MAX_METADATA_BYTES: usize = 64 * 1024;
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Failed to read cache metadata")?
    {
        if body.len() + chunk.len() > MAX_METADATA_BYTES {
            return Err("Cache metadata exceeds 64 KiB".into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn parse_niks3_config(body: &[u8]) -> Result<Niks3CacheConfig, String> {
    serde_json::from_slice(body).map_err(|_| "Invalid Niks3 cache-config JSON".into())
}

fn validate_niks3_keys(config: &Niks3CacheConfig) -> Result<(), String> {
    if config.public_keys.is_empty() {
        return Err("Niks3 discovery returned no signing keys".into());
    }
    for key in &config.public_keys {
        cf_protocol::cache::validate_nix_public_key(key).map_err(str::to_string)?;
    }
    Ok(())
}

async fn validate_niks3_discovery(
    body: &[u8],
    allow_private_targets: bool,
    stages: &mut Niks3ConnectionTestResult,
) -> Result<(Niks3CacheConfig, Url), String> {
    let config = parse_niks3_config(body)?;
    validate_niks3_keys(&config)?;
    stages.signing_keys_found = true;
    let read = niks3_base_url(&config.substituter_url, allow_private_targets)?;
    cache_test_addresses(&read, allow_private_targets).await?;
    Ok((config, read))
}

async fn discover_niks3(
    server_url: &str,
    allow_private_targets: bool,
) -> Result<Niks3Discovery, String> {
    let server = niks3_base_url(server_url, allow_private_targets)?;
    let client = cache_test_client(&server, allow_private_targets, None, None, None).await?;
    let endpoint = server
        .join("api/cache-config")
        .map_err(|_| "Invalid Niks3 API URL")?;
    let response = client
        .get(endpoint)
        .send()
        .await
        .map_err(|_| "Niks3 server connection failed")?;
    let (config, read) = validate_niks3_discovery(
        &probe_body(response).await?,
        allow_private_targets,
        &mut Niks3ConnectionTestResult::default(),
    )
    .await?;
    Ok(Niks3Discovery {
        server_url: server.to_string(),
        substituter_url: read.to_string(),
        public_keys: config.public_keys,
        oidc_audience: config.oidc_audience.filter(|value| !value.is_empty()),
    })
}

async fn run_niks3_test(
    create: &CreateCacheDestination,
    allow_private_targets: bool,
) -> Result<CacheCredentialTestResult, String> {
    let server = niks3_base_url(
        create
            .niks3_server_url
            .as_deref()
            .ok_or("Missing Niks3 server URL")?,
        allow_private_targets,
    )?;
    let read = niks3_base_url(
        create.push_to.as_deref().ok_or("Missing Niks3 read URL")?,
        allow_private_targets,
    )?;
    let (write_cert, write_key, write_ca) = match create.niks3_write_auth_mode.as_deref() {
        Some("token") => (None, None, None),
        Some("mtls") => (
            create.niks3_write_client_cert.as_deref(),
            create.niks3_write_client_key.as_deref(),
            create.niks3_write_ca_cert.as_deref(),
        ),
        _ => return Err("Niks3 write authentication must be token or mtls".into()),
    };
    let (read_cert, read_key, read_ca) = match create.niks3_read_auth_mode.as_deref() {
        Some("none") => (None, None, None),
        Some("mtls") => (
            create.niks3_read_client_cert.as_deref(),
            create.niks3_read_client_key.as_deref(),
            create.niks3_read_ca_cert.as_deref(),
        ),
        _ => return Err("Niks3 read authentication must be none or mtls".into()),
    };
    if create.niks3_write_auth_mode.as_deref() == Some("mtls")
        && (write_cert.is_none() || write_key.is_none())
        || create.niks3_read_auth_mode.as_deref() == Some("mtls")
            && (read_cert.is_none() || read_key.is_none())
    {
        return Err("Niks3 mTLS requires certificate and private key".into());
    }
    let client = cache_test_client(
        &server,
        allow_private_targets,
        write_cert,
        write_key,
        write_ca,
    )
    .await?;
    let read_client =
        cache_test_client(&read, allow_private_targets, read_cert, read_key, read_ca).await?;
    let mut stages = Niks3ConnectionTestResult::default();
    let mut status_code = None;
    let result: Result<(), String> = async {
        let endpoint = server
            .join("api/cache-config")
            .map_err(|_| "Invalid Niks3 API URL")?;
        // SECURITY: Do not send the write token to this public endpoint. Never
        // infer write authorization from discovery or TLS handshake success.
        let response = client
            .get(endpoint)
            .send()
            .await
            .map_err(|_| "Niks3 server connection failed")?;
        stages.server_reachable = true;
        status_code = Some(response.status().as_u16());
        let (_, discovered) = validate_niks3_discovery(
            &probe_body(response).await?,
            allow_private_targets,
            &mut stages,
        )
        .await?;
        // SECURITY: Discovery cannot redirect the configured read identity to
        // another origin/path and receive its client certificate. Probe only
        // the configured URL with a separately pinned read client.
        if discovered != read {
            return Err("Discovered substituter URL does not match the configured read URL".into());
        }
        stages.discovery_valid = true;
        let endpoint = read
            .join("nix-cache-info")
            .map_err(|_| "Invalid Niks3 read URL")?;
        let response = read_client
            .get(endpoint)
            .send()
            .await
            .map_err(|_| "Niks3 read endpoint connection failed")?;
        status_code = Some(response.status().as_u16());
        let body = probe_body(response).await?;
        let text = std::str::from_utf8(&body).map_err(|_| "Invalid nix-cache-info response")?;
        if !text
            .lines()
            .any(|line| line.trim() == "StoreDir: /nix/store")
        {
            return Err("Invalid nix-cache-info response: expected StoreDir: /nix/store".into());
        }
        stages.read_endpoint_reachable = true;
        Ok(())
    }
    .await;
    Ok(CacheCredentialTestResult {
        ok: result.is_ok(),
        status_code,
        message: result.err().unwrap_or_else(|| {
            "Discovery and read endpoint successful; write authorization untested".into()
        }),
        tested_url: Some(read.to_string()),
        niks3: Some(CacheProbeDetails::Niks3(stages)),
    })
}

fn validate_cache_test_url(url: &Url, allow_private_targets: bool) -> Result<(), String> {
    if url.fragment().is_some()
        || url
            .query_pairs()
            .any(|(name, _)| cache_url_query_parameter_is_sensitive(&name))
    {
        return Err("Cache test URLs must not contain credentials or fragments".into());
    }
    match url.scheme() {
        "https" => {}
        _ => {
            return Err("Unsupported cache test URL scheme. Only https is allowed".into());
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

#[cfg(test)]
async fn validate_cache_test_url_resolves_publicly(
    url: &Url,
    allow_private_targets: bool,
) -> Result<(), String> {
    cache_test_addresses(url, allow_private_targets)
        .await
        .map(|_| ())
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
            // IANA special-use ranges also include 0/8, reserved 240/4, and
            // shared-address space 100.64/10; none is a public cache target.
            if v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
                || v4.octets()[0] == 0
                || v4.octets()[0] >= 240
                || (v4.octets()[0] == 100 && (64..=127).contains(&v4.octets()[1]))
            {
                return Err("Refusing to test private, loopback, or non-routable IP".to_string());
            }
        }
        IpAddr::V6(v6) => {
            // SECURITY: IPv4-mapped addresses must obey the IPv4 policy too.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return reject_non_public_ip(IpAddr::V4(v4));
            }
            if v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
                || v6.is_multicast()
                // RFC 3849 reserves 2001:db8::/32 for documentation.
                || (v6.segments()[0] == 0x2001 && v6.segments()[1] == 0x0db8)
            {
                return Err("Refusing to test private, loopback, or non-routable IP".to_string());
            }
        }
    }

    Ok(())
}

#[cfg(test)]
fn sanitize_test_url_for_response(url: &Url) -> String {
    sanitize_cache_url_credentials(url.as_str())
}

// SECURITY: Refuse recognized credential queries before DNS, client creation,
// callbacks, or network work, even when the URL belongs to an inactive field.
fn reject_probe_query_credentials<'a>(
    urls: impl IntoIterator<Item = Option<&'a str>>,
) -> Result<(), &'static str> {
    if urls
        .into_iter()
        .flatten()
        .any(cache_url_has_query_credentials)
    {
        return Err(LEGACY_QUERY_CREDENTIALS_TEST_ERROR);
    }
    Ok(())
}

async fn run_cache_destination_test(
    create: &CreateCacheDestination,
    allow_private_targets: bool,
) -> Result<CacheCredentialTestResult, String> {
    reject_probe_query_credentials([
        create.push_to.as_deref(),
        create.s3_endpoint_url.as_deref(),
        create.niks3_server_url.as_deref(),
    ])?;
    create.validate()?;
    let cache_type = create.cache_type.trim();
    if cache_type == "Attic" {
        return attic_probe::probe(create, allow_private_targets).await;
    }
    if cache_type == "S3" {
        return s3_probe::probe(create, allow_private_targets).await;
    }
    if cache_type == "Niks3" {
        return run_niks3_test(create, allow_private_targets).await;
    }
    if !matches!(
        cache_type,
        "S3" | "Attic" | "Http" | "Nix" | "s3" | "attic" | "http" | "nix"
    ) {
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

    let parsed_url = Url::parse(&test_url).map_err(|_| "Invalid cache test URL")?;
    if !matches!(cache_type, "Http" | "Nix")
        && (!parsed_url.username().is_empty() || parsed_url.password().is_some())
    {
        return Err(
            "URL Basic authentication is supported only for Http and Nix cache tests".into(),
        );
    }

    let client = cache_test_client(&parsed_url, allow_private_targets, None, None, None).await?;

    let request = legacy_probe_request(&client, parsed_url.clone(), create);

    let response = request
        .send()
        .await
        .map_err(|_| "Cache connection failed")?;

    if response.status().is_success() {
        Ok(CacheCredentialTestResult {
            ok: true,
            status_code: Some(response.status().as_u16()),
            message: "Connection successful".to_string(),
            tested_url: None,
            niks3: None,
        })
    } else {
        Ok(CacheCredentialTestResult {
            ok: false,
            status_code: Some(response.status().as_u16()),
            message: format!("Endpoint responded with status {}", response.status()),
            tested_url: None,
            niks3: None,
        })
    }
}

// SECURITY: Reqwest extracts Http/Nix URL userinfo into a sensitive Basic
// Authorization header and removes it from the sent URL. Inactive Attic fields
// cannot authenticate any other cache type. All target checks precede send.
fn legacy_probe_request(
    client: &reqwest::Client,
    url: Url,
    create: &CreateCacheDestination,
) -> reqwest::RequestBuilder {
    let mut request = client.get(url);
    if create.cache_type == "Attic" {
        if let Some(token) = create
            .attic_token
            .as_deref()
            .filter(|token| !token.trim().is_empty())
        {
            request = request.bearer_auth(token.trim());
        }
    }
    request
}

fn redact_cache_secrets(mut destination: CacheDestination) -> CacheDestination {
    destination.refresh_niks3_configured();
    destination.niks3_server_url = destination
        .niks3_server_url
        .as_deref()
        .map(sanitize_niks3_url_credentials);
    destination.niks3_auth_token = None;
    destination.niks3_write_client_key = None;
    destination.niks3_read_client_key = None;
    destination.push_to = destination
        .push_to
        .as_deref()
        .map(sanitize_cache_url_credentials);
    destination.attic_token = None;
    destination.s3_access_key_id = None;
    destination.s3_secret_access_key = None;
    destination.s3_session_token = None;
    destination.s3_endpoint_url = destination
        .s3_endpoint_url
        .as_deref()
        .map(sanitize_niks3_url_credentials);
    destination
}

// Keep the existing internal name for discovery/redaction tests. Every URL field
// uses the same expanded policy, including generic and S3 legacy queries.
fn sanitize_niks3_url_credentials(value: &str) -> String {
    sanitize_cache_url_credentials(value)
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

/// Tests cache connectivity without changing the destination or remote cache.
///
/// `POST /api/v1/caches/test-credentials` requires cache-administration permission.
/// Niks3 adds top-level `server_reachable`, `discovery_valid`, `write_auth_valid`,
/// `read_endpoint_reachable`, and `signing_keys_found` to the existing result.
/// `server_reachable` means an HTTP response was received, including an error.
/// `signing_keys_found` means discovery supplied valid Nix Ed25519 public keys.
/// `discovery_valid` means the advertised read URL passed target validation and
/// matches the configured read URL. `read_endpoint_reachable` means an
/// authenticated GET returned successful `nix-cache-info` for `/nix/store`.
/// `write_auth_valid` is always null: public discovery does not verify write
/// authorization. `ok` reports discovery/read success only, not write readiness.
///
/// Returns `403` without admin permission, `400` for invalid local settings or
/// rejected configured targets, and `200` with stage results for probe failures.
/// Redirects are errors. Read mTLS credentials go only to the configured read
/// endpoint; the write token is never sent. An untested stage is false or null.
/// JSON parsing follows admin authorization and rejects invalid bodies without
/// echoing values. Validates the same create configuration as persistence.
/// Supplied double-submit CSRF state must match. S3 checks path-style bucket
/// ListObjectsV2 with explicit keys; success does not prove write permission.
/// Attic probes the canonical named-cache config API, not the server root.
/// Its flat `probe_kind`, `stage`, `cache_access_valid`, `token_auth_valid`, and
/// `write_auth_valid` fields expose only observed access. Private cache success
/// verifies the supplied token for that read; public success leaves token validity
/// null. Write authorization is always null and remains Untested. No upstream
/// URLs or response-body values are returned or followed.
pub async fn test_cache_destination_credentials(
    State(pool): State<PgPool>,
    State(server_config): State<ServerConfig>,
    headers: HeaderMap,
    request: axum::extract::Request,
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

    if let Err(response) = optional_probe_csrf(&headers) {
        return response;
    }
    let create: CreateCacheDestination = match probe_json(request).await {
        Ok(create) => create,
        Err(response) => return response,
    };
    if let Err(message) = reject_probe_query_credentials([
        create.push_to.as_deref(),
        create.s3_endpoint_url.as_deref(),
        create.niks3_server_url.as_deref(),
    ]) {
        if create.cache_type == "Attic" {
            return attic_probe::policy_error_response(message);
        }
        return probe_error(
            StatusCode::BAD_REQUEST,
            "legacy_query_credentials_unsupported",
            message,
        );
    }
    if create.validate().is_err() {
        return probe_error(
            StatusCode::BAD_REQUEST,
            "invalid_cache_test_config",
            "Invalid cache configuration",
        );
    }
    match run_cache_destination_test(&create, server_config.allow_private_cache_test_targets).await
    {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(message) if create.cache_type == "Attic" => {
            attic_probe::policy_error_response(&message)
        }
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

/// Discovers a Niks3 server's public read URL and signing keys for an admin.
///
/// `POST /api/caches/niks3/discover` accepts a [`Niks3DiscoverRequest`]. Returns
/// `403` without cache-administration permission and `400` for rejected targets,
/// redirects, connection errors, invalid JSON, or missing/invalid signing keys.
/// Uses Niks3 v1.6.0 `GET /api/cache-config` without an issuer query or credentials.
/// The read URL is validated but not fetched. No destination is persisted.
/// `oidc_audience` is returned as null when the upstream omits that field.
pub async fn discover_niks3_cache(
    State(pool): State<PgPool>,
    State(server_config): State<ServerConfig>,
    headers: HeaderMap,
    Json(request): Json<Niks3DiscoverRequest>,
) -> impl IntoResponse {
    if require_admin_user(&pool, &headers).await.is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "forbidden".into(),
                message: "Admin role required".into(),
                details: None,
            }),
        )
            .into_response();
    }
    match discover_niks3(
        &request.server_url,
        server_config.allow_private_cache_test_targets,
    )
    .await
    {
        Ok(discovery) => (StatusCode::OK, Json(discovery)).into_response(),
        Err(message) => (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "invalid_niks3_discovery".into(),
                message,
                details: None,
            }),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::{
        normalize_test_url, sanitize_test_url_for_response, validate_cache_test_url,
        validate_cache_test_url_resolves_publicly, validate_resolved_addrs_public,
    };
    use crate::models::cache_destination::nix_public_key_fixture;
    use std::net::{Ipv4Addr, SocketAddr};
    use url::Url;

    pub(super) async fn admin_headers(pool: &PgPool) -> HeaderMap {
        use crate::auth::session::{SESSION_COOKIE_NAME, hash_token};
        use crate::models::auth_identity::AuthRole;
        use crate::queries::auth_identity::{create_user_session, sync_user_role};
        let user = uuid::Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, username, first_name, last_name, email, user_type) VALUES ($1, 'cache-key-admin', 'Cache', 'Admin', 'cache-key-admin@example.invalid', 'human')")
            .bind(user).execute(pool).await.unwrap();
        sync_user_role(pool, user, AuthRole::Admin).await.unwrap();
        let token = "cache-key-admin-session";
        create_user_session(
            pool,
            user,
            hash_token(token),
            chrono::Utc::now() + chrono::Duration::hours(1),
            None,
            None,
            "local".into(),
        )
        .await
        .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            format!("{SESSION_COOKIE_NAME}={token}").parse().unwrap(),
        );
        headers
    }

    fn key_validation_create(write: &str, read: &str) -> CreateCacheDestination {
        let mut create = CreateCacheDestination {
            name: format!("key-validation-{write}-{read}"),
            cache_type: "Niks3".into(),
            push_to: Some("https://read.example/cache".into()),
            niks3_server_url: Some("https://write.example/api".into()),
            niks3_public_keys: vec![nix_public_key_fixture("cache-1")],
            niks3_write_auth_mode: Some(write.into()),
            niks3_read_auth_mode: Some(read.into()),
            ..Default::default()
        };
        if write == "token" {
            create.niks3_auth_token = Some("test-write-token".into());
        } else {
            create.niks3_write_client_cert =
                Some(crate::security::cache_secrets::TEST_CERTIFICATE.into());
            create.niks3_write_client_key = Some("test-write-key".into());
        }
        if read == "mtls" {
            create.niks3_read_client_cert =
                Some(crate::security::cache_secrets::TEST_CERTIFICATE.into());
            create.niks3_read_client_key = Some("test-read-key".into());
        }
        create
    }

    fn malformed_keys() -> Vec<String> {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        vec![
            String::new(),
            "cache:!not-base64!".into(),
            format!("cache:{}", STANDARD.encode([0; 31])),
            format!("cache:{}", STANDARD.encode([0; 33])),
            nix_public_key_fixture("white space"),
            nix_public_key_fixture(""),
            format!("{}\n", nix_public_key_fixture("cache")),
        ]
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified task PostgreSQL and ephemeral database creation"]
    async fn niks3_api_create_rejects_malformed_keys_in_all_auth_modes(pool: PgPool) {
        let headers = admin_headers(&pool).await;
        for write in ["token", "mtls"] {
            for read in ["none", "mtls"] {
                let valid = key_validation_create(write, read);
                valid.validate().unwrap();
                for key in malformed_keys() {
                    let mut create = valid.clone();
                    create.niks3_public_keys = vec![nix_public_key_fixture("valid"), key.clone()];
                    let discovery = Niks3CacheConfig {
                        substituter_url: create.push_to.clone().unwrap(),
                        public_keys: create.niks3_public_keys.clone(),
                        oidc_audience: None,
                    };
                    assert!(validate_niks3_keys(&discovery).is_err());
                    let response = create_cache_destination(
                        State(pool.clone()),
                        headers.clone(),
                        Json(create),
                    )
                    .await
                    .into_response();
                    assert_eq!(
                        response.status(),
                        StatusCode::BAD_REQUEST,
                        "{write}/{read}: {key:?}"
                    );
                }
            }
        }
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM cache_destinations WHERE name LIKE 'key-validation-%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified task PostgreSQL and ephemeral database creation"]
    async fn niks3_api_update_rejects_malformed_keys_in_all_auth_modes(pool: PgPool) {
        let headers = admin_headers(&pool).await;
        for write in ["token", "mtls"] {
            for read in ["none", "mtls"] {
                let create = key_validation_create(write, read);
                let destination = cache_destinations::create_cache_destination(&pool, &create)
                    .await
                    .unwrap();
                for key in malformed_keys() {
                    let update = UpdateCacheDestination {
                        niks3_public_keys: vec![nix_public_key_fixture("valid"), key.clone()],
                        ..Default::default()
                    };
                    let response = update_cache_destination(
                        State(pool.clone()),
                        headers.clone(),
                        Path(destination.id),
                        Json(update),
                    )
                    .await
                    .into_response();
                    assert_eq!(
                        response.status(),
                        StatusCode::BAD_REQUEST,
                        "{write}/{read}: {key:?}"
                    );
                    let restored = cache_destinations::get_cache_destination(&pool, destination.id)
                        .await
                        .unwrap()
                        .unwrap();
                    assert_eq!(restored.niks3_public_keys, create.niks3_public_keys);
                }
            }
        }
    }

    #[test]
    fn niks3_discovery_matches_v160_wire_shape() {
        let key = nix_public_key_fixture("cache-1");
        let config = parse_niks3_config(
            format!(r#"{{"substituter_url":"https://cache.example.com","public_keys":["{key}"]}}"#)
                .as_bytes(),
        )
        .unwrap();
        validate_niks3_keys(&config).unwrap();
        assert!(config.oidc_audience.is_none());
        let with_audience = parse_niks3_config(format!(r#"{{"substituter_url":"https://cache.example.com","public_keys":["{key}"],"oidc_audience":"ci"}}"#).as_bytes()).unwrap();
        assert_eq!(with_audience.oidc_audience.as_deref(), Some("ci"));
    }

    #[test]
    fn niks3_discovery_rejects_invalid_json_and_signing_keys() {
        for body in [
            "not json",
            "{}",
            r#"{"substituter_url":null,"public_keys":[]}"#,
            r#"{"substituter_url":"https://example.com","public_keys":"key"}"#,
        ] {
            assert!(parse_niks3_config(body.as_bytes()).is_err());
        }
        for keys in [
            vec![],
            vec![""],
            vec!["name:invalid"],
            vec!["name:YWJj"],
            vec![" :AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="],
        ] {
            let config = Niks3CacheConfig {
                substituter_url: "https://example.com".into(),
                public_keys: keys.into_iter().map(str::to_string).collect(),
                oidc_audience: None,
            };
            assert!(validate_niks3_keys(&config).is_err());
        }
    }

    #[tokio::test]
    async fn niks3_discovery_failure_states_do_not_claim_later_stages() {
        let key = nix_public_key_fixture("cache-1");
        for (body, keys_found, error) in [
            ("invalid".to_string(), false, "JSON"),
            (
                r#"{"substituter_url":"https://127.0.0.1","public_keys":[]}"#.into(),
                false,
                "no signing keys",
            ),
            (
                format!(r#"{{"substituter_url":"invalid","public_keys":["{key}"]}}"#),
                true,
                "URL",
            ),
            (
                format!(r#"{{"substituter_url":"https://127.0.0.1","public_keys":["{key}"]}}"#),
                true,
                "private",
            ),
        ] {
            let mut stages = Niks3ConnectionTestResult {
                server_reachable: true,
                ..Default::default()
            };
            assert!(
                validate_niks3_discovery(body.as_bytes(), false, &mut stages)
                    .await
                    .unwrap_err()
                    .contains(error)
            );
            assert!(stages.server_reachable);
            assert_eq!(stages.signing_keys_found, keys_found);
            assert!(!stages.discovery_valid);
            assert!(!stages.read_endpoint_reachable);
            assert!(stages.write_auth_valid.is_none());
        }
        let body = format!(r#"{{"substituter_url":"https://127.0.0.1","public_keys":["{key}"]}}"#);
        let mut stages = Niks3ConnectionTestResult::default();
        let (_, read) = validate_niks3_discovery(body.as_bytes(), true, &mut stages)
            .await
            .unwrap();
        assert_eq!(read.as_str(), "https://127.0.0.1/");
        assert!(stages.signing_keys_found);
        assert!(!stages.read_endpoint_reachable);
    }

    #[tokio::test]
    async fn niks3_tls_input_errors_do_not_echo_private_material() {
        let url = niks3_base_url("https://127.0.0.1", true).unwrap();
        for (cert, key, ca) in [
            (Some("certificate"), None, None),
            (None, Some("private-secret"), None),
            (Some("certificate"), Some("private-secret"), None),
            (None, None, Some("invalid-ca")),
        ] {
            let err = cache_test_client(&url, true, cert, key, ca)
                .await
                .unwrap_err();
            assert!(!err.contains("private-secret"));
            assert!(!err.contains("invalid-ca"));
        }
    }

    #[test]
    fn niks3_urls_preserve_prefixes_and_reject_unsafe_forms() {
        let base = niks3_base_url("https://example.com/niks3", false).unwrap();
        assert_eq!(
            base.join("api/cache-config").unwrap().as_str(),
            "https://example.com/niks3/api/cache-config"
        );
        assert_eq!(
            base.join("nix-cache-info").unwrap().as_str(),
            "https://example.com/niks3/nix-cache-info"
        );
        for raw in [
            "invalid",
            "http://example.com",
            "file:///cache",
            "https://user:secret@example.com",
            "https://example.com/?token=secret",
            "https://example.com/#fragment",
            "https://127.0.0.1",
            "https://[::ffff:127.0.0.1]",
        ] {
            assert!(niks3_base_url(raw, false).is_err(), "{raw}");
        }
        assert!(niks3_base_url("https://127.0.0.1", true).is_ok());
    }

    #[test]
    fn niks3_redirects_are_errors_and_write_auth_is_untested() {
        for code in [301, 302, 303, 307, 308] {
            assert!(
                require_probe_success(reqwest::StatusCode::from_u16(code).unwrap())
                    .unwrap_err()
                    .contains("redirects")
            );
        }
        assert!(require_probe_success(reqwest::StatusCode::UNAUTHORIZED).is_err());
        assert!(require_probe_success(reqwest::StatusCode::OK).is_ok());
        let result = CacheCredentialTestResult {
            ok: false,
            status_code: Some(302),
            message: "Cache endpoint redirects are not allowed".into(),
            tested_url: None,
            niks3: Some(CacheProbeDetails::Niks3(Niks3ConnectionTestResult {
                server_reachable: true,
                ..Default::default()
            })),
        };
        let json = serde_json::to_value(result).unwrap();
        assert_eq!(json["server_reachable"], true);
        assert_eq!(json["discovery_valid"], false);
        assert_eq!(json["read_endpoint_reachable"], false);
        assert_eq!(json["signing_keys_found"], false);
        assert!(json["write_auth_valid"].is_null());
        let legacy = serde_json::to_value(CacheCredentialTestResult {
            ok: true,
            status_code: Some(200),
            message: "Connection successful".into(),
            tested_url: Some("https://cache.example.com".into()),
            niks3: None,
        })
        .unwrap();
        assert!(legacy.get("server_reachable").is_none());
        assert!(legacy.get("write_auth_valid").is_none());
    }

    #[tokio::test]
    async fn niks3_private_test_targets_fail_before_connecting() {
        let create = CreateCacheDestination {
            name: "private-target-test".into(),
            cache_type: "Niks3".into(),
            niks3_server_url: Some("https://127.0.0.1".into()),
            push_to: Some("https://127.0.0.1".into()),
            niks3_write_auth_mode: Some("token".into()),
            niks3_auth_token: Some("fixture-token-marker".into()),
            niks3_public_keys: vec![nix_public_key_fixture("fixture")],
            niks3_read_auth_mode: Some("none".into()),
            ..Default::default()
        };
        assert!(
            run_cache_destination_test(&create, false)
                .await
                .unwrap_err()
                .contains("private")
        );
        let url = niks3_base_url("https://[::1]", true).unwrap();
        assert_eq!(
            cache_test_addresses(&url, true).await.unwrap(),
            vec!["[::1]:443".parse::<SocketAddr>().unwrap()]
        );
    }

    #[tokio::test]
    async fn niks3_discovery_requires_cache_administration_before_probing() {
        // No cookie means RBAC returns before querying this unused lazy pool.
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgresql://localhost/unused_cache_api_test")
            .unwrap();
        let response = discover_niks3_cache(
            State(pool),
            State(ServerConfig::default()),
            HeaderMap::new(),
            Json(Niks3DiscoverRequest {
                server_url: "invalid".into(),
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn niks3_response_redaction_preserves_configured_flags() {
        let destination = CacheDestination {
            cache_type: "Niks3".into(),
            niks3_server_url: Some("https://user:secret@example.com".into()),
            niks3_auth_token: Some("token-secret".into()),
            niks3_write_client_cert: Some("write-cert".into()),
            niks3_write_client_key: Some("write-key-secret".into()),
            niks3_read_client_cert: Some("read-cert".into()),
            niks3_read_client_key: Some("read-key-secret".into()),
            ..Default::default()
        };
        let redacted = redact_cache_secrets(destination);
        assert!(redacted.niks3_auth_token.is_none());
        assert!(redacted.niks3_write_client_key.is_none());
        assert!(redacted.niks3_read_client_key.is_none());
        assert!(redacted.niks3_write_token_configured);
        assert!(redacted.niks3_write_mtls_configured);
        assert!(redacted.niks3_read_mtls_configured);
        let json = serde_json::to_string(&redacted).unwrap();
        for secret in [
            "token-secret",
            "write-key-secret",
            "read-key-secret",
            "user:secret",
        ] {
            assert!(!json.contains(secret));
        }
        let redacted = redact_cache_secrets(CacheDestination {
            niks3_auth_token: Some(" ".into()),
            niks3_write_client_cert: Some("cert".into()),
            niks3_read_client_key: Some("key".into()),
            ..Default::default()
        });
        assert!(!redacted.niks3_write_token_configured);
        assert!(!redacted.niks3_write_mtls_configured);
        assert!(!redacted.niks3_read_mtls_configured);
    }

    #[test]
    fn niks3_response_urls_redact_decoded_sensitive_query_names_on_both_planes() {
        for name in [
            "token",
            "TOKEN",
            "%74oken",
            "auth-token",
            "AuTh-ToKeN",
            "%61uth%2dtoken",
            "access_token",
            "ACCESS%5fTOKEN",
            "password",
            "PASSWORD",
            "ssl-verify",
            "SSL-CERT-FILE",
            "ca-certificate",
            "CA%2DCERTIFICATE",
            "tls-private-key",
            "TLS-CERTIFICATE",
            "%74ls%2Danything",
        ] {
            let raw = format!(
                "https://user:unique-userinfo-secret@cache.example.com/cache?priority=30&{name}=unique-query-secret&priority=40#unique-fragment-secret"
            );
            let redacted = redact_cache_secrets(CacheDestination {
                cache_type: "Niks3".into(),
                niks3_server_url: Some(raw.clone()),
                push_to: Some(raw),
                ..Default::default()
            });
            let json = serde_json::to_string(&redacted).unwrap();
            for secret in [
                "unique-userinfo-secret",
                "unique-query-secret",
                "unique-fragment-secret",
            ] {
                assert!(!json.contains(secret));
            }
            for raw in [
                redacted.niks3_server_url.unwrap(),
                redacted.push_to.unwrap(),
            ] {
                let url = Url::parse(&raw).unwrap();
                assert!(url.username().is_empty());
                assert!(url.password().is_none());
                assert!(url.fragment().is_none());
                assert_eq!(
                    url.query_pairs().into_owned().collect::<Vec<_>>(),
                    vec![
                        ("priority".into(), "30".into()),
                        ("priority".into(), "40".into())
                    ]
                );
                cf_config::cache_credentials::PreparedCacheRead::new(
                    &raw,
                    &[],
                    &cf_protocol::cache::CacheReadAuth::None,
                )
                .unwrap();
            }
        }
        assert_eq!(
            sanitize_niks3_url_credentials(
                "https://cache.example.com/?token=unique-query-secret&TOKEN=another-secret"
            ),
            "https://cache.example.com/"
        );
        assert_eq!(
            sanitize_niks3_url_credentials("invalid?token=unique-query-secret"),
            "[REDACTED]"
        );
    }

    #[test]
    fn safe_queries_and_generic_cache_response_semantics_are_preserved() {
        let raw = "https://cache.example.com/cache?priority=30&region=us%2Deast%2D1";
        assert_eq!(sanitize_niks3_url_credentials(raw), raw);
        let generic = redact_cache_secrets(CacheDestination {
            cache_type: "Nix".into(),
            push_to: Some("https://user:secret@cache.example.com/?token=generic-token".into()),
            ..Default::default()
        });
        assert_eq!(
            generic.push_to.as_deref(),
            Some("https://cache.example.com/")
        );
    }

    #[test]
    fn niks3_probe_url_errors_never_echo_query_credentials() {
        for name in [
            "token",
            "auth-token",
            "%61uth%2dtoken",
            "TOKEN",
            "tls-private-key",
        ] {
            let raw = format!("https://cache.example.com/?{name}=unique-query-secret");
            for allow_private in [true, false] {
                let error = niks3_base_url(&raw, allow_private).unwrap_err();
                assert!(!error.contains("unique-query-secret"));
                assert!(!error.contains(&raw));
            }
        }
    }

    #[test]
    fn normalize_test_url_handles_attic_scheme() {
        let normalized = normalize_test_url("Attic", Some("attic://cache.example.com/team"), None);
        assert!(normalized.is_none());
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
        assert!(
            validate_cache_test_url_resolves_publicly(&url, false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn validate_cache_test_url_dns_allows_localhost_when_enabled() {
        let url = Url::parse("https://localhost").unwrap();
        assert!(
            validate_cache_test_url_resolves_publicly(&url, true)
                .await
                .is_ok()
        );
    }

    #[test]
    fn validate_resolved_addrs_public_rejects_private_resolution() {
        let addrs = vec![SocketAddr::from((Ipv4Addr::new(10, 0, 0, 8), 443))];
        assert!(validate_resolved_addrs_public(&addrs).is_err());
        let mixed = vec![
            "8.8.8.8:443".parse().unwrap(),
            "127.0.0.1:443".parse().unwrap(),
        ];
        assert!(validate_resolved_addrs_public(&mixed).is_err());
        assert!(validate_resolved_addrs_public(&[]).is_err());
        for ip in [
            "100.64.0.1",
            "0.0.0.2",
            "224.0.0.1",
            "240.0.0.1",
            "::ffff:127.0.0.1",
            "2001:db8::1",
            "ff02::1",
        ] {
            assert!(reject_non_public_ip(ip.parse().unwrap()).is_err(), "{ip}");
        }
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
            tracing::error!("Failed to update cache destination {}", id);
            let message = e.to_string();
            let status = if message == "Invalid effective cache configuration"
                || message.contains("required for")
                || message.starts_with("Invalid Nix public signing key:")
                || message == "niks3_public_keys requires nonempty signing keys"
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
                    message: if status == StatusCode::BAD_REQUEST {
                        "Invalid effective cache configuration".into()
                    } else {
                        "Failed to update cache destination".into()
                    },
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
