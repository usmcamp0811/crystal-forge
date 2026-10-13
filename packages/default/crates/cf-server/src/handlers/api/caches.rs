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
use crate::models::cache_destination::Niks3ProbeScope;
use crate::models::cache_destination::{
    CacheDestination, CreateCacheDestination, LEGACY_QUERY_CREDENTIALS_TEST_ERROR,
    UpdateCacheDestination, cache_url_has_query_credentials,
    cache_url_query_parameter_is_sensitive, effective_cache_url, sanitize_cache_url_credentials,
};
use crate::queries::{cache_destinations, cache_push};

mod attic_probe;
mod niks3_write_probe;
mod s3_probe;
mod storage_metrics;

pub use storage_metrics::get_cache_metrics;

#[cfg(test)]
mod basic_read_tests;
#[cfg(test)]
mod discovery_tests;
#[cfg(test)]
mod niks3_ca_tests;
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

// SECURITY: Only this allowlist produces diagnostic prose. Neither submitted
// values nor provider/transport error formatting enters the response envelope.
#[derive(Clone, Copy)]
struct ProbeDiagnostic {
    code: &'static str,
    message: &'static str,
    field: Option<&'static str>,
}

fn diagnostic(code: &str) -> ProbeDiagnostic {
    let (code, message) = match code {
        "ca_bundle_framing_invalid" => (
            "ca_bundle_framing_invalid",
            "Check CA PEM block framing and encoding.",
        ),
        "ca_certificate_invalid" => (
            "ca_certificate_invalid",
            "The CA bundle contains an invalid X.509 certificate.",
        ),
        "ca_bundle_content_unsupported" => (
            "ca_bundle_content_unsupported",
            "The CA bundle contains unsupported content. Private keys are prohibited.",
        ),
        "ca_bundle_trust_ambiguous" => (
            "ca_bundle_trust_ambiguous",
            "The CA bundle contains unsupported or ambiguous trust annotations.",
        ),
        "ca_bundle_empty" => (
            "ca_bundle_empty",
            "The CA bundle must contain ordinary CERTIFICATE roots.",
        ),
        "ca_bundle_too_large" => (
            "ca_bundle_too_large",
            "The CA bundle exceeds the supported size limit.",
        ),
        "client_identity_invalid" => (
            "client_identity_invalid",
            "Check the certificate-only client chain and matching supported private key.",
        ),
        "client_certificate_invalid" => (
            "client_certificate_invalid",
            "Use a valid certificate-only client chain with the leaf certificate first.",
        ),
        "client_private_key_invalid" => (
            "client_private_key_invalid",
            "Use exactly one supported unencrypted PEM private key.",
        ),
        "client_identity_mismatch" => (
            "client_identity_mismatch",
            "The private key must match the first certificate in the client chain.",
        ),
        "tls_verification_failed" => (
            "tls_verification_failed",
            "Check server certificate trust, validity and hostname coverage.",
        ),
        "dns_or_connection_failed" => (
            "dns_or_connection_failed",
            "Check DNS, routing and endpoint availability.",
        ),
        "http_response_failed" => (
            "http_response_failed",
            "The endpoint returned an unsuccessful HTTP response or redirect.",
        ),
        "invalid_niks3_metadata" => (
            "invalid_niks3_metadata",
            "The endpoint returned invalid or inconsistent Niks3 metadata.",
        ),
        "target_policy_rejected" => (
            "target_policy_rejected",
            "The target is not permitted by the cache probe policy.",
        ),
        "probe_timeout" => ("probe_timeout", "The cache probe exceeded its deadline."),
        "token_rejected" => (
            "token_rejected",
            "The protected API rejected the selected token.",
        ),
        "write_credentials_missing" => (
            "write_credentials_missing",
            "Selected write credentials are missing. No protected API request was sent.",
        ),
        "write_credentials_invalid" => (
            "write_credentials_invalid",
            "Selected write credentials are invalid. No protected API request was sent.",
        ),
        "mtls_subject_rejected" => (
            "mtls_subject_rejected",
            "The protected API rejected the selected client certificate identity or subject.",
        ),
        "proxy_access_denied" => (
            "proxy_access_denied",
            "The endpoint denied access. This is not the native Niks3 authentication fingerprint.",
        ),
        "method_rejected" => ("method_rejected", "The endpoint rejected the probe method."),
        "route_unsupported" => (
            "route_unsupported",
            "The protected route did not match the supported Niks3 contract. Write authorization: Untested.",
        ),
        "auth_control_inconclusive" => (
            "auth_control_inconclusive",
            "Credential omission did not establish the expected authentication gate. Write authorization: Untested.",
        ),
        "unexpected_write_response" => (
            "unexpected_write_response",
            "The invalid write request received an unexpected response. Write authorization: Untested; no retry was sent.",
        ),
        "probe_response_too_large" => (
            "probe_response_too_large",
            "The protected API response exceeded 64 KiB.",
        ),
        "write_route_verified" => (
            "write_route_verified",
            "API authentication and protected write-handler access verified. Upload, storage permissions and publication: Untested.",
        ),
        "write_route_verified_certificate_gate" => (
            "write_route_verified_certificate_gate",
            "Protected API and write-handler access accepted the client identity; omission was rejected at TLS. Native subject enforcement, upload and publication: Untested.",
        ),
        "read_access_verified" => (
            "read_access_verified",
            "Read access and /nix/store metadata verified. Artifact signatures and write authorization: Untested.",
        ),
        _ => (
            "probe_failed",
            "The cache probe failed. Check configuration and endpoint availability.",
        ),
    };
    ProbeDiagnostic {
        code,
        message,
        field: None,
    }
}

fn ca_diagnostic(error: &anyhow::Error) -> ProbeDiagnostic {
    let mut result = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<cf_config::ca_bundle::CaBundleError>())
        .map(|error| diagnostic(error.code()))
        .unwrap_or_else(|| diagnostic("probe_failed"));
    result.field = error
        .chain()
        .find_map(|cause| match cause.to_string().as_str() {
            "niks3_write_ca_cert" => Some("niks3_write_ca_cert"),
            "niks3_read_ca_cert" => Some("niks3_read_ca_cert"),
            _ => None,
        });
    result
}

fn client_identity_diagnostic(error: &anyhow::Error) -> ProbeDiagnostic {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<cf_config::ca_bundle::ClientIdentityError>())
        .map(|error| diagnostic(error.code()))
        .unwrap_or_else(|| diagnostic("client_identity_invalid"))
}

fn diagnostic_error(
    status: StatusCode,
    base_code: &str,
    hint: ProbeDiagnostic,
) -> axum::response::Response {
    (
        status,
        Json(serde_json::json!({
            "error": base_code, "message": hint.message, "details": null,
            "diagnostic_code": hint.code, "field": hint.field,
        })),
    )
        .into_response()
}

// Compatibility adapters accept only exact static strings emitted by our own
// validators. They never classify arbitrary upstream bodies or URL-bearing text.
fn static_probe_diagnostic(message: &str) -> ProbeDiagnostic {
    let code = match message {
        "Cache test DNS lookup timed out" | "Cache endpoint request timed out" => "probe_timeout",
        "Failed to resolve cache test host"
        | "Cache test host did not resolve to any addresses" => "dns_or_connection_failed",
        "Cache endpoint redirects are not allowed" => "http_response_failed",
        "Invalid cache mTLS certificate or private key"
        | "Cache mTLS requires both certificate and private key" => "client_identity_invalid",
        "Invalid Niks3 cache-config JSON"
        | "Niks3 discovery returned no signing keys"
        | "Cache metadata exceeds 64 KiB"
        | "Invalid nix-cache-info response"
        | "Invalid nix-cache-info response: expected StoreDir: /nix/store"
        | "Advertised signing keys do not match configured trust"
        | "Discovered substituter URL does not match the configured read URL" => {
            "invalid_niks3_metadata"
        }
        other => other,
    };
    diagnostic(code)
}

fn client_chain_diagnostic(write: Option<&str>, read: Option<&str>) -> Result<(), ProbeDiagnostic> {
    for (field, cert) in [
        ("niks3_write_client_cert", write),
        ("niks3_read_client_cert", read),
    ] {
        if cert.is_some_and(|cert| {
            crate::security::cache_secrets::validate_certificate_bundle(cert).is_err()
        }) {
            let mut hint = diagnostic("client_certificate_invalid");
            hint.field = Some(field);
            return Err(hint);
        }
    }
    Ok(())
}

fn request_failure(error: &reqwest::Error) -> String {
    use std::error::Error;
    if error.is_timeout() {
        return "probe_timeout".into();
    }
    if let Some(code) = tls_verification_cause(error.source()) {
        return code.into();
    }
    if error.is_connect() {
        "dns_or_connection_failed".into()
    } else {
        "probe_failed".into()
    }
}

fn tls_verification_cause(
    mut source: Option<&(dyn std::error::Error + 'static)>,
) -> Option<&'static str> {
    // Reqwest, Hyper and TLS adapters compose several local wrappers. Sixteen
    // nodes cover that chain while bounding cycles or unusual error sources.
    const MAX_CAUSE_DEPTH: usize = 16;
    let mut local_io_cause = false;
    for _ in 0..MAX_CAUSE_DEPTH {
        let cause = source?;
        // SECURITY: Inspect only descendants of an io::Error's stored cause.
        // Never format the URL-bearing reqwest/Hyper outer error or HTTP bodies.
        // Rustls 0.21 verification failures have these fixed display prefixes;
        // the adapter may use either Other or InvalidData for the I/O wrapper.
        if local_io_cause {
            let message = cause.to_string();
            if message.starts_with("invalid peer certificate:")
                || message.starts_with("invalid certificate:")
            {
                return Some("tls_verification_failed");
            }
            // A peer rejecting our identity is distinct from our server-trust
            // verifier rejecting the peer. Other fatal alerts prove neither.
            if message.starts_with("received fatal alert:") {
                return Some(match message.as_str() {
                    "received fatal alert: BadCertificate"
                    | "received fatal alert: CertificateRequired"
                    | "received fatal alert: CertificateUnknown"
                    | "received fatal alert: UnknownCA" => "client_identity_invalid",
                    _ => "probe_failed",
                });
            }
        }
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            if let Some(inner) = io.get_ref() {
                // io::Error::source can skip the stored error itself. Visit
                // get_ref first so the actual TLS cause is not lost.
                source = Some(inner);
                local_io_cause = true;
                continue;
            }
        }
        source = cause.source();
    }
    None
}

fn static_target_failure(message: &str) -> String {
    match message {
        "Refusing to test localhost or internal cache endpoint"
        | "Refusing to test private, loopback, or non-routable IP"
        | "Cache test URLs must not contain credentials or fragments"
        | "Unsupported cache test URL scheme. Only https is allowed" => {
            "target_policy_rejected".into()
        }
        _ => static_probe_diagnostic(message).code.into(),
    }
}

fn discovery_diagnostic(message: &str) -> ProbeDiagnostic {
    let mut hint = static_probe_diagnostic(message);
    if matches!(
        hint.code,
        "ca_bundle_framing_invalid"
            | "ca_certificate_invalid"
            | "ca_bundle_content_unsupported"
            | "ca_bundle_trust_ambiguous"
            | "ca_bundle_empty"
            | "ca_bundle_too_large"
    ) {
        hint.field = Some("niks3_write_ca_cert");
    }
    if hint.code == "client_certificate_invalid" {
        hint.field = Some("niks3_write_client_cert");
    } else if hint.code == "client_private_key_invalid" {
        hint.field = Some("niks3_write_client_key");
    }
    hint
}

fn niks3_config_diagnostic(
    create: &CreateCacheDestination,
    scope: Niks3ProbeScope,
) -> ProbeDiagnostic {
    // This is an input-only policy check. No DNS or requests occur here. Private
    // address policy remains in the runtime check with the configured opt-in.
    if scope != Niks3ProbeScope::Read {
        if create
            .niks3_server_url
            .as_deref()
            .is_some_and(|url| niks3_base_url(url, true).is_err())
        {
            return diagnostic("target_policy_rejected");
        }
        if create.niks3_write_auth_mode.as_deref() == Some("mtls")
            && (create
                .niks3_write_client_cert
                .as_deref()
                .is_none_or(|s| s.trim().is_empty())
                || create
                    .niks3_write_client_key
                    .as_deref()
                    .is_none_or(|s| s.trim().is_empty()))
        {
            return diagnostic("client_identity_invalid");
        }
    }
    if scope != Niks3ProbeScope::Write {
        if create
            .push_to
            .as_deref()
            .is_some_and(|url| niks3_read_base_url(url, true).is_err())
        {
            return diagnostic("target_policy_rejected");
        }
        if create.niks3_read_auth_mode.as_deref() == Some("mtls")
            && (create
                .niks3_read_client_cert
                .as_deref()
                .is_none_or(|s| s.trim().is_empty())
                || create
                    .niks3_read_client_key
                    .as_deref()
                    .is_none_or(|s| s.trim().is_empty()))
        {
            return diagnostic("client_identity_invalid");
        }
    }
    diagnostic("probe_failed")
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
/// Niks3 accepts `probe_scope: "write" | "read" | "all"` (default `all`). Scoped
/// probes share Save's authority/credential merge but validate only the selected
/// plane. They cannot persist an incomplete configuration. Write metadata never
/// sends a token or read credential; read probes use only configured read auth.
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
    test_stored_with_scoped_probe(
        &pool,
        &headers,
        id,
        request,
        |effective, scope| async move {
            run_cache_destination_test_scope(
                &effective,
                server_config.allow_private_cache_test_targets,
                scope,
            )
            .await
        },
    )
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
    test_stored_with_scoped_probe(pool, headers, id, request, |effective, _| probe(effective)).await
}

#[derive(Deserialize)]
struct ScopedProbe<T> {
    #[serde(flatten)]
    settings: T,
    #[serde(default)]
    probe_scope: Niks3ProbeScope,
}

// Save and all-plane Test keep full validation. Scoped tests share the merge
// invariants but validate only the plane that will receive a network request.
async fn test_stored_with_scoped_probe<F, Fut>(
    pool: &PgPool,
    headers: &HeaderMap,
    id: i32,
    request: axum::extract::Request,
    probe: F,
) -> axum::response::Response
where
    F: FnOnce(CreateCacheDestination, Niks3ProbeScope) -> Fut,
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
        Err(error)
            if error
                .downcast_ref::<cf_config::ca_bundle::CaBundleError>()
                .is_some() =>
        {
            return diagnostic_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                ca_diagnostic(&error),
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
    let request: ScopedProbe<UpdateCacheDestination> = match probe_json(request).await {
        Ok(request) => request,
        Err(response) => return response,
    };
    let scope = request.probe_scope;
    let mut update = request.settings;
    if let Err(error) = update.normalize_ca_fields() {
        return diagnostic_error(
            StatusCode::BAD_REQUEST,
            "invalid_cache_test_config",
            ca_diagnostic(&error),
        );
    }
    if let Err(hint) = client_chain_diagnostic(
        update.niks3_write_client_cert.as_deref(),
        update.niks3_read_client_cert.as_deref(),
    ) {
        return diagnostic_error(StatusCode::BAD_REQUEST, "invalid_cache_test_config", hint);
    }
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
    let urls = if current.cache_type == "Niks3" && scope != Niks3ProbeScope::All {
        if scope == Niks3ProbeScope::Write {
            [niks3_server, None, None]
        } else {
            [push_to.as_deref(), None, None]
        }
    } else {
        [push_to.as_deref(), s3_endpoint.as_deref(), niks3_server]
    };
    if let Err(message) = reject_probe_query_credentials(urls) {
        if update.cache_type.as_deref().unwrap_or(&current.cache_type) == "Attic" {
            return attic_probe::policy_error_response(message);
        }
        return probe_error(
            StatusCode::BAD_REQUEST,
            "legacy_query_credentials_unsupported",
            message,
        );
    }
    let effective = match cache_destinations::effective_update_unvalidated(&current, &update) {
        Ok(effective) => effective,
        Err(_) => {
            return probe_error(
                StatusCode::BAD_REQUEST,
                "invalid_cache_test_config",
                "Invalid effective cache configuration",
            );
        }
    };
    if effective.validate_probe(scope).is_err() {
        if effective.cache_type == "Niks3" {
            return diagnostic_error(
                StatusCode::BAD_REQUEST,
                "invalid_cache_test_config",
                niks3_config_diagnostic(&effective, scope),
            );
        }
        return probe_error(
            StatusCode::BAD_REQUEST,
            "invalid_cache_test_config",
            "Invalid effective cache configuration",
        );
    }
    let is_attic = effective.cache_type == "Attic";
    let is_niks3 = effective.cache_type == "Niks3";
    match probe(effective, scope).await {
        Ok(mut result) => {
            // Stored-ID results describe stages, not the private snapshot.
            result.tested_url = None;
            (StatusCode::OK, Json(result)).into_response()
        }
        Err(message) if is_attic => attic_probe::policy_error_response(&message),
        Err(message) if is_niks3 => diagnostic_error(
            StatusCode::BAD_REQUEST,
            "invalid_cache_test_config",
            static_probe_diagnostic(&message),
        ),
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

// Each stage describes observed access, never publication. Write authorization
// means an authenticated invalid POST reached the supported validation gate.
#[derive(Debug, Default, serde::Serialize)]
struct Niks3ConnectionTestResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    diagnostic_code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    write_probe_diagnostic_code: Option<&'static str>,
    server_reachable: bool,
    discovery_valid: bool,
    write_auth_valid: Option<bool>,
    read_endpoint_reachable: bool,
    signing_keys_found: bool,
    probe_scope: Option<Niks3ProbeScope>,
    // True means verified write TLS returned expected public cache-config
    // metadata. Protected-route auth responses cannot establish this field.
    write_api_reachable: Option<bool>,
    // Records accepted GET authentication plus its omission control.
    write_authn_valid: Option<bool>,
    // Records invalid-POST handler access plus its omission control. Publication
    // is a separate operation; the legacy write_auth_valid aliases this field.
    write_authorization_valid: Option<bool>,
    read_access_valid: Option<bool>,
    signing_keys_valid: Option<bool>,
}

/// Specifies a Niks3 write server and optional mTLS discovery transport.
///
/// An omitted mode selects the existing public, token-mode metadata request.
/// No Bearer token is selected or sent. mTLS requires a certificate/key pair;
/// its optional CA bundle replaces built-in and ambient roots, matching Niks3.
/// The response contains only public metadata, never supplied TLS material.
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
    /// Optional `token` or `mtls` mode; omission preserves public discovery.
    pub niks3_write_auth_mode: Option<String>,
    /// Certificate-only PEM client chain, required for `mtls`.
    pub niks3_write_client_cert: Option<String>,
    /// PEM private key for the client chain, required for `mtls`.
    pub niks3_write_client_key: Option<String>,
    /// Optional supported CA bundle, normalized to ordinary server-trust roots.
    /// Trusted-only records are validated but never promoted into roots.
    pub niks3_write_ca_cert: Option<String>,
}

// SECURITY: This projection has no token/read-credential fields and no Debug
// implementation. Discovery and Test select the same write TLS identity.
struct Niks3WriteTransport<'a> {
    cert: Option<&'a str>,
    key: Option<&'a str>,
    ca: Option<&'a str>,
}

fn niks3_write_transport<'a>(
    mode: Option<&str>,
    cert: Option<&'a str>,
    key: Option<&'a str>,
    ca: Option<&'a str>,
) -> Result<Niks3WriteTransport<'a>, String> {
    // SECURITY: Client chains stay certificate-only. CA annotations use the
    // separate shared grammar and cannot silently hide keys or widen root trust.
    // Validate both inputs and the matching identity before any DNS work.
    if let Some(cert) = cert {
        crate::security::cache_secrets::validate_certificate_bundle(cert)
            .map_err(|_| "client_certificate_invalid")?;
    }
    if let Some(ca) = ca {
        crate::security::cache_secrets::normalize_ca_bundle(ca)
            .map_err(|error| ca_diagnostic(&error).code.to_owned())?;
    }
    match mode.unwrap_or("token") {
        "token" if cert.is_none() && key.is_none() && ca.is_none() => {
            Ok(Niks3WriteTransport { cert: None, key: None, ca: None })
        }
        "mtls" if cert.is_some() && key.is_some_and(|value| !value.trim().is_empty()) => {
            crate::security::cache_secrets::validate_client_identity(
                cert.unwrap_or_default(), key.unwrap_or_default(),
            ).map_err(|error| client_identity_diagnostic(&error).code.to_owned())?;
            let pem = format!("{}\n{}", cert.unwrap_or_default(), key.unwrap_or_default());
            let identity = reqwest::Identity::from_pem(pem.as_bytes())
                .map_err(|_| "client_private_key_invalid")?;
            // The shared validator proves pairing. The actual probe TLS backend
            // also needs to support the key algorithm before any DNS work.
            reqwest::Client::builder().use_rustls_tls().no_proxy().identity(identity)
                .build().map_err(|_| "client_private_key_invalid")?;
            Ok(Niks3WriteTransport { cert, key, ca })
        }
        "mtls" => Err("client_identity_invalid".into()),
        _ => Err("Niks3 discovery requires token mode without TLS fields, or mtls with certificate and private key".into()),
    }
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
    cache_test_client_pinned_policy(url, addrs, cert, key, ca, RootPolicy::ExtendSystem).await
}

#[derive(Clone, Copy, PartialEq)]
enum RootPolicy {
    ExtendSystem,
    ReplaceCustom,
}

fn system_ca_source(policy: RootPolicy) -> Option<(std::path::PathBuf, bool)> {
    if policy == RootPolicy::ReplaceCustom {
        return None;
    }
    let configured =
        std::env::var_os("SSL_CERT_FILE").or_else(|| std::env::var_os("NIX_SSL_CERT_FILE"));
    let explicit = configured.is_some();
    Some((
        configured
            .unwrap_or_else(|| "/etc/ssl/certs/ca-certificates.crt".into())
            .into(),
        explicit,
    ))
}

// SECURITY: Niks3's custom write roots replace the entire trust pool, including
// presigned S3 trust in the consuming CLI. A probe must not succeed by borrowing
// public or process roots that the corresponding upload cannot use.
async fn niks3_write_test_client(
    url: &Url,
    allow_private_targets: bool,
    transport: &Niks3WriteTransport<'_>,
) -> Result<reqwest::Client, String> {
    let addrs = cache_test_addresses(url, allow_private_targets)
        .await
        .map_err(|error| static_target_failure(&error))?;
    let policy = if transport.ca.is_some() {
        RootPolicy::ReplaceCustom
    } else {
        RootPolicy::ExtendSystem
    };
    cache_test_client_pinned_policy(
        url,
        &addrs,
        transport.cert,
        transport.key,
        transport.ca,
        policy,
    )
    .await
}

async fn cache_test_client_pinned_policy(
    url: &Url,
    addrs: &[SocketAddr],
    cert: Option<&str>,
    key: Option<&str>,
    ca: Option<&str>,
    policy: RootPolicy,
) -> Result<reqwest::Client, String> {
    // SECURITY: Connect only to the addresses checked above, retaining the URL
    // host for TLS verification. Proxies and redirects would bypass this pin.
    let mut builder = reqwest::Client::builder()
        .use_rustls_tls()
        .no_proxy()
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(url.host_str().ok_or("Missing cache host")?, addrs);
    if policy == RootPolicy::ReplaceCustom {
        if ca.is_none() {
            return Err("ca_bundle_empty".into());
        }
        builder = builder.tls_built_in_root_certs(false);
    }
    // Rustls's bundled public roots do not include an operator's system CA.
    // Honor standard process CA configuration without a test-only TLS bypass.
    // VM fixtures install their CA before starting the server process.
    if let Some((root_path, explicitly_configured)) = system_ca_source(policy) {
        match tokio::fs::read(root_path).await {
            Ok(pem) => {
                let certificates = reqwest::Certificate::from_pem_bundle(&pem)
                    .map_err(|_| "Invalid system CA certificate bundle")?;
                for certificate in certificates {
                    builder = builder.add_root_certificate(certificate);
                }
            }
            Err(_) if explicitly_configured => {
                return Err("Failed to read configured system CA bundle".into());
            }
            Err(_) => {}
        }
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
        let canonical = crate::security::cache_secrets::normalize_ca_bundle(ca)
            .map_err(|error| ca_diagnostic(&error).code.to_owned())?;
        crate::security::cache_secrets::validate_certificate_bundle(&canonical)
            .map_err(|_| "ca_certificate_invalid")?;
        let certificates = reqwest::Certificate::from_pem_bundle(canonical.as_bytes())
            .map_err(|_| "ca_certificate_invalid")?;
        if certificates.is_empty() {
            return Err("ca_bundle_empty".into());
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
        Err("http_response_failed".into())
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
        .map_err(|error| request_failure(&error))?
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
    let (config, read) = validate_niks3_metadata(body, allow_private_targets)?;
    stages.signing_keys_found = true;
    cache_test_addresses(&read, allow_private_targets)
        .await
        .map_err(|error| static_target_failure(&error))?;
    Ok((config, read))
}

fn validate_niks3_metadata(
    body: &[u8],
    allow_private_targets: bool,
) -> Result<(Niks3CacheConfig, Url), String> {
    let config = parse_niks3_config(body)?;
    validate_niks3_keys(&config).map_err(|_| "invalid_niks3_metadata")?;
    let read = niks3_base_url(&config.substituter_url, allow_private_targets)
        .map_err(|_| "target_policy_rejected")?;
    Ok((config, read))
}

async fn discover_niks3(
    request: &Niks3DiscoverRequest,
    allow_private_targets: bool,
) -> Result<Niks3Discovery, String> {
    reject_probe_query_credentials([Some(request.server_url.as_str())])
        .map_err(|_| "target_policy_rejected")?;
    let server = niks3_base_url(&request.server_url, allow_private_targets)
        .map_err(|_| "target_policy_rejected")?;
    let transport = niks3_write_transport(
        request.niks3_write_auth_mode.as_deref(),
        request.niks3_write_client_cert.as_deref(),
        request.niks3_write_client_key.as_deref(),
        request.niks3_write_ca_cert.as_deref(),
    )?;
    let client = niks3_write_test_client(&server, allow_private_targets, &transport).await?;
    let response = niks3_metadata_request(&client, &server)?
        .send()
        .await
        .map_err(|error| request_failure(&error))?;
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

// SECURITY: Public metadata has no Bearer authorization, including when the
// saved write mode is token. mTLS is supplied by the write client, not headers.
fn niks3_metadata_request(
    client: &reqwest::Client,
    server: &Url,
) -> Result<reqwest::RequestBuilder, String> {
    let endpoint = server
        .join("api/cache-config")
        .map_err(|_| "Invalid Niks3 API URL")?;
    Ok(client.get(endpoint))
}

async fn run_niks3_test(
    create: &CreateCacheDestination,
    allow_private_targets: bool,
) -> Result<CacheCredentialTestResult, String> {
    run_niks3_test_scope(create, allow_private_targets, Niks3ProbeScope::All).await
}

async fn run_niks3_test_scope(
    create: &CreateCacheDestination,
    allow_private_targets: bool,
    scope: Niks3ProbeScope,
) -> Result<CacheCredentialTestResult, String> {
    // A single budget covers DNS, trust/identity preparation, discovery, every
    // protected request/control and the separately configured read plane.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    let mut stages = Niks3ConnectionTestResult {
        probe_scope: Some(scope),
        ..Default::default()
    };
    let mut status_code = None;
    let operation = run_niks3_test_scope_inner(
        create,
        allow_private_targets,
        scope,
        deadline,
        &mut stages,
        &mut status_code,
    );
    let (ok, code) = match tokio::time::timeout_at(deadline, operation).await {
        Ok(result) => result?,
        Err(_) => (false, "probe_timeout"),
    };
    stages.diagnostic_code = Some(code);
    Ok(CacheCredentialTestResult {
        ok,
        status_code,
        message: diagnostic(code).message.into(),
        tested_url: None,
        niks3: Some(CacheProbeDetails::Niks3(stages)),
    })
}

async fn run_niks3_test_scope_inner(
    create: &CreateCacheDestination,
    allow_private_targets: bool,
    scope: Niks3ProbeScope,
    deadline: tokio::time::Instant,
    stages: &mut Niks3ConnectionTestResult,
    status_code: &mut Option<u16>,
) -> Result<(bool, &'static str), String> {
    if scope == Niks3ProbeScope::Read {
        let result = run_niks3_read_test(
            create,
            allow_private_targets,
            scope,
            Niks3ConnectionTestResult::default(),
        )
        .await?;
        *status_code = result.status_code;
        if let Some(CacheProbeDetails::Niks3(read)) = result.niks3 {
            *stages = read;
        }
        return Ok((
            result.ok,
            stages.diagnostic_code.unwrap_or("read_access_verified"),
        ));
    }
    // All retains complete Save validation, including missing-token rejection.
    // Header-validity preflight then rejects unusable write credentials before
    // any metadata request, DNS, read request or provider authentication claim.
    if scope == Niks3ProbeScope::All {
        create.validate_probe(scope)?;
    }
    let auth = match niks3_write_probe::preflight(create, stages) {
        Ok(auth) => auth,
        Err(error) => {
            let code = static_probe_diagnostic(&error).code;
            stages.write_probe_diagnostic_code = Some(code);
            return Ok((false, code));
        }
    };
    let server = niks3_base_url(
        create
            .niks3_server_url
            .as_deref()
            .ok_or("Missing Niks3 server URL")?,
        allow_private_targets,
    )
    .map_err(|_| "target_policy_rejected")?;
    let write = niks3_write_transport(
        create.niks3_write_auth_mode.as_deref(),
        create.niks3_write_client_cert.as_deref(),
        create.niks3_write_client_key.as_deref(),
        create.niks3_write_ca_cert.as_deref(),
    )?;
    // Both write scopes observe public metadata once, without a Bearer. Write
    // needs no configured read URL/keys; All separately checks their consistency.
    stages.write_api_reachable = Some(false);
    let discovery: Result<(), String> = async {
        let client = niks3_write_test_client(&server, allow_private_targets, &write).await?;
        probe_niks3_metadata(
            create,
            scope,
            &client,
            &server,
            allow_private_targets,
            stages,
            status_code,
        )
        .await
    }
    .await;
    // A metadata mismatch cannot skip write authentication. Each requested plane
    // contributes independent evidence; the overall result requires all of them.
    let write_result = niks3_write_probe::run(
        &auth,
        &server,
        allow_private_targets,
        deadline,
        stages,
        status_code,
    )
    .await;
    let write_ok = write_result.is_ok();
    let write_code = match write_result {
        Ok(code) => code,
        Err(code) => static_probe_diagnostic(&code).code,
    };
    stages.write_probe_diagnostic_code = Some(write_code);
    let mut read_ok = true;
    let mut read_code = None;
    if scope == Niks3ProbeScope::All {
        let read = run_niks3_read_test(
            create,
            allow_private_targets,
            scope,
            Niks3ConnectionTestResult::default(),
        )
        .await;
        match read {
            Ok(result) => {
                read_ok = result.ok;
                if let Some(CacheProbeDetails::Niks3(read)) = result.niks3 {
                    stages.read_endpoint_reachable = read.read_endpoint_reachable;
                    stages.read_access_valid = read.read_access_valid;
                    read_code = read.diagnostic_code;
                }
            }
            Err(error) => {
                read_ok = false;
                read_code = Some(static_probe_diagnostic(&error).code);
            }
        }
    }
    let code = if !write_ok {
        write_code
    } else if let Err(error) = &discovery {
        static_probe_diagnostic(error).code
    } else if !read_ok {
        read_code.unwrap_or("probe_failed")
    } else {
        write_code
    };
    Ok((write_ok && discovery.is_ok() && read_ok, code))
}

async fn probe_niks3_metadata(
    create: &CreateCacheDestination,
    scope: Niks3ProbeScope,
    client: &reqwest::Client,
    server: &Url,
    allow_private_targets: bool,
    stages: &mut Niks3ConnectionTestResult,
    status_code: &mut Option<u16>,
) -> Result<(), String> {
    stages.write_api_reachable = Some(false);
    // SECURITY: Public metadata never receives a write Bearer. A successful
    // metadata observation establishes reachability, not authentication/version.
    let response = niks3_metadata_request(client, server)?
        .send()
        .await
        .map_err(|error| request_failure(&error))?;
    stages.server_reachable = true;
    *status_code = Some(response.status().as_u16());
    // Scope results require the native metadata status/media type, not an
    // arbitrary successful proxy response. Public Discover keeps its own policy.
    if response.status().as_u16() != 200
        || response
            .headers()
            .get_all(reqwest::header::CONTENT_TYPE)
            .iter()
            .count()
            != 1
        || response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            != Some("application/json")
    {
        return Err("invalid_niks3_metadata".into());
    }
    let (config, discovered) =
        validate_niks3_metadata(&probe_body(response).await?, allow_private_targets)?;
    stages.write_api_reachable = Some(true);
    stages.signing_keys_found = true;
    if scope == Niks3ProbeScope::All {
        if !config
            .public_keys
            .iter()
            .any(|key| create.niks3_public_keys.contains(key))
        {
            stages.signing_keys_valid = Some(false);
            return Err("Advertised signing keys do not match configured trust".into());
        }
        stages.signing_keys_valid = Some(true);
        // Discovery cannot retarget the configured read identity. All additionally
        // validates advertised read DNS and requires the configured URL to match.
        if discovered
            != niks3_base_url(
                create.push_to.as_deref().ok_or("Missing Niks3 read URL")?,
                allow_private_targets,
            )?
        {
            return Err("Discovered substituter URL does not match the configured read URL".into());
        }
        cache_test_addresses(&discovered, allow_private_targets)
            .await
            .map_err(|error| static_target_failure(&error))?;
    }
    stages.discovery_valid = true;
    Ok(())
}

// SECURITY: Basic is sent only to the configured HTTPS authority. The shared
// effective-update merge prevents borrowing a retained pair for another origin.
// Redirects/proxies are disabled by the pinned client; write auth never enters
// this request, and discovered read URLs are never used as its destination.
fn niks3_read_request(
    client: &reqwest::Client,
    endpoint: Url,
    create: &CreateCacheDestination,
) -> Result<reqwest::RequestBuilder, String> {
    let request = client.get(endpoint);
    if create.niks3_read_auth_mode.as_deref() == Some("basic") {
        Ok(request.basic_auth(
            create
                .niks3_read_basic_username
                .as_deref()
                .ok_or("Missing Basic username")?,
            Some(
                create
                    .niks3_read_basic_password
                    .as_deref()
                    .ok_or("Missing Basic password")?,
            ),
        ))
    } else {
        Ok(request)
    }
}

fn niks3_read_base_url(raw: &str, allow_private_targets: bool) -> Result<Url, String> {
    let mut url = Url::parse(raw).map_err(|_| "Invalid Niks3 read URL")?;
    validate_cache_test_url(&url, allow_private_targets)?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Niks3 read URL cannot contain userinfo".into());
    }
    url.set_path(&format!("{}/", url.path().trim_end_matches('/')));
    Ok(url)
}

fn niks3_read_root_policy(ca: Option<&str>) -> RootPolicy {
    if ca.is_some_and(|ca| !ca.trim().is_empty()) {
        RootPolicy::ReplaceCustom
    } else {
        RootPolicy::ExtendSystem
    }
}

async fn niks3_read_test_client(
    url: &Url,
    allow_private_targets: bool,
    cert: Option<&str>,
    key: Option<&str>,
    ca: Option<&str>,
) -> Result<reqwest::Client, String> {
    let addresses = cache_test_addresses(url, allow_private_targets).await?;
    // SECURITY: Explicit Niks3 read roots replace ambient and built-in trust,
    // matching child-local NIX_SSL_CERT_FILE. Generic provider policy is separate.
    cache_test_client_pinned_policy(url, &addresses, cert, key, ca, niks3_read_root_policy(ca))
        .await
}

async fn run_niks3_read_test(
    create: &CreateCacheDestination,
    allow_private_targets: bool,
    scope: Niks3ProbeScope,
    mut stages: Niks3ConnectionTestResult,
) -> Result<CacheCredentialTestResult, String> {
    let read = niks3_read_base_url(
        create.push_to.as_deref().ok_or("Missing Niks3 read URL")?,
        allow_private_targets,
    )
    .map_err(|_| "target_policy_rejected")?;
    let (cert, key, ca) = if create.niks3_read_auth_mode.as_deref() == Some("mtls") {
        (
            create.niks3_read_client_cert.as_deref(),
            create.niks3_read_client_key.as_deref(),
            create.niks3_read_ca_cert.as_deref(),
        )
    } else {
        (None, None, None)
    };
    let client = niks3_read_test_client(&read, allow_private_targets, cert, key, ca)
        .await
        .map_err(|error| static_target_failure(&error))?;
    stages.probe_scope = Some(scope);
    stages.read_access_valid = Some(false);
    let mut status_code = None;
    let result: Result<(), String> = async {
        let mut endpoint = read.clone();
        endpoint
            .path_segments_mut()
            .map_err(|_| "Invalid Niks3 read URL")?
            .pop_if_empty()
            .push("nix-cache-info");
        let response = niks3_read_request(&client, endpoint, create)?
            .send()
            .await
            .map_err(|error| request_failure(&error))?;
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
        stages.read_access_valid = Some(true);
        stages.signing_keys_valid = (!create.niks3_public_keys.is_empty()).then_some(true);
        Ok(())
    }
    .await;
    let hint = result
        .as_ref()
        .err()
        .map(|error| static_probe_diagnostic(error));
    stages.diagnostic_code = hint.map(|hint| hint.code);
    Ok(CacheCredentialTestResult { ok: result.is_ok(), status_code,
        message: hint.map(|hint| hint.message.into()).unwrap_or_else(|| if create.niks3_public_keys.is_empty() {
            "Read access and /nix/store metadata verified. Signing keys: Unattempted. Artifact signatures and write authorization: Untested.".into()
        } else {
            "Read access and /nix/store metadata verified. Configured signing key format checked; artifact signatures and write authorization: Untested.".into()
        }), tested_url: None, niks3: Some(CacheProbeDetails::Niks3(stages)) })
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
    run_cache_destination_test_scope(create, allow_private_targets, Niks3ProbeScope::All).await
}

async fn run_cache_destination_test_scope(
    create: &CreateCacheDestination,
    allow_private_targets: bool,
    scope: Niks3ProbeScope,
) -> Result<CacheCredentialTestResult, String> {
    let urls = if create.cache_type == "Niks3" && scope != Niks3ProbeScope::All {
        if scope == Niks3ProbeScope::Write {
            [create.niks3_server_url.as_deref(), None, None]
        } else {
            [create.push_to.as_deref(), None, None]
        }
    } else {
        [
            create.push_to.as_deref(),
            create.s3_endpoint_url.as_deref(),
            create.niks3_server_url.as_deref(),
        ]
    };
    reject_probe_query_credentials(urls)?;
    create.validate_probe(scope)?;
    let cache_type = create.cache_type.trim();
    if cache_type == "Attic" {
        return attic_probe::probe(create, allow_private_targets).await;
    }
    if cache_type == "S3" {
        return s3_probe::probe(create, allow_private_targets).await;
    }
    if cache_type == "Niks3" {
        return run_niks3_test_scope(create, allow_private_targets, scope).await;
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
    destination.niks3_read_basic_username = None;
    destination.niks3_read_basic_password = None;
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

/// Creates a cache destination and assignments atomically for an administrator.
///
/// `POST /api/caches` normalizes supplied CA fields before persistence and
/// returns public canonical certificate roots with private credentials redacted.
/// Client chains remain certificate-only. Save does not check client-key pairing
/// or remote authorization; write Discovery/Test performs local pairing checks.
///
/// # Errors
/// Returns HTTP 403 without admin access, 400 for invalid input, or 500 for
/// persistence/encryption failure. CA errors add an allowlisted diagnostic code
/// and static field name; error responses never contain the supplied bundle.
///
/// # Examples
/// ```text
/// POST /api/caches
/// Content-Type: application/json
///
/// {"name":"public-read","cache_type":"Http","push_to":"https://cache.example/"}
/// ```
pub async fn create_cache_destination(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(mut create): Json<CreateCacheDestination>,
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

    if let Err(error) = create.normalize_ca_fields() {
        return diagnostic_error(
            StatusCode::BAD_REQUEST,
            "validation_error",
            ca_diagnostic(&error),
        );
    }
    if let Err(hint) = client_chain_diagnostic(
        create.niks3_write_client_cert.as_deref(),
        create.niks3_read_client_cert.as_deref(),
    ) {
        return diagnostic_error(StatusCode::BAD_REQUEST, "validation_error", hint);
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
/// `discovery_valid` means expected public metadata and advertised URL validation
/// succeeded. `All` additionally requires configured read URL/key consistency.
/// Write needs no configured read URL or keys. `read_endpoint_reachable` means an
/// authenticated GET returned successful `nix-cache-info` for `/nix/store`.
/// `write_auth_valid` aliases protected write-handler access evidence, not upload
/// or publication readiness. `ok` requires complete evidence for every requested
/// plane. Inconclusive write evidence leaves `ok` false and authorization null.
///
/// # Errors
/// Returns `403` without admin permission, `400` for invalid local settings or
/// rejected configured targets, and `200` with stage results for probe failures.
/// Redirects are errors. Read mTLS credentials go only to the configured read
/// endpoint; the write token goes only to protected API routes. Unproven nullable
/// stages stay null. No provider error prose or credential material is returned.
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
/// Niks3 accepts optional `probe_scope` (`write`, `read`, or default `all`). The
/// result adds nullable `write_api_reachable`, `write_authn_valid`,
/// `write_authorization_valid`, `read_access_valid`, and `signing_keys_valid`.
/// Both write scopes fetch token-free cache-config once with selected write TLS.
/// `write_api_reachable` means expected public metadata was received over verified
/// TLS; protected auth responses alone cannot establish API reachability. Public
/// metadata failure does not skip the independent protected-route probes.
/// Null means unattempted or unproven. Public API metadata leaves write auth
/// unproven. Read signing-key evidence checks configured format (and metadata
/// overlap for `all`), not an artifact signature. Basic auth uses system CA
/// trust and a sensitive header pinned to the configured HTTPS authority.
/// Niks3 failures add an allowlisted `diagnostic_code`. Local certificate errors
/// can add a static `field`; no submitted material or upstream prose is returned.
/// Write probes pair protected `GET /api/gc/status` with credential omission.
/// Only its strict native JSON or exact no-GC 404 fingerprint plus rejection of
/// the control permits `POST /api/pending_closures` with `{}` and its control.
/// Exact native validation 400 proves handler access without a valid upload.
/// `write_probe_diagnostic_code` retains the per-write outcome when another plane
/// fails. Certificate-required TLS controls establish accepted protected sessions
/// but do not establish native subject enforcement. One eight-second deadline
/// includes DNS, client preparation, requests and bounded response bodies.
/// Local Bearer header validation precedes every metadata/DNS/read operation.
/// Explicit Niks3 read CA roots replace ambient and built-in roots; omission uses
/// normal system trust. Generic provider trust policies are independent.
/// Scoped Write permits a missing token through historical input validation, but
/// reports a local failed credential candidate with false write-authentication
/// and handler-access flags. Protected API reachability stays null without an
/// attempt. `write_credentials_missing` and `write_credentials_invalid` do not
/// claim native rejection. `All` keeps its complete-validation HTTP 400 behavior.
///
/// # Examples
/// ```text
/// POST /api/v1/caches/test-credentials
/// Content-Type: application/json
///
/// {
///   "cache_type": "Niks3", "probe_scope": "write",
///   "niks3_server_url": "https://cache.example",
///   "niks3_write_auth_mode": "token", "niks3_auth_token": "<write-token>"
/// }
/// ```
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
    let request: ScopedProbe<CreateCacheDestination> = match probe_json(request).await {
        Ok(request) => request,
        Err(response) => return response,
    };
    let scope = request.probe_scope;
    let mut create = request.settings;
    if let Err(error) = create.normalize_ca_fields() {
        return diagnostic_error(
            StatusCode::BAD_REQUEST,
            "invalid_cache_test_config",
            ca_diagnostic(&error),
        );
    }
    if let Err(hint) = client_chain_diagnostic(
        create.niks3_write_client_cert.as_deref(),
        create.niks3_read_client_cert.as_deref(),
    ) {
        return diagnostic_error(StatusCode::BAD_REQUEST, "invalid_cache_test_config", hint);
    }
    let urls = if create.cache_type == "Niks3" && scope != Niks3ProbeScope::All {
        if scope == Niks3ProbeScope::Write {
            [create.niks3_server_url.as_deref(), None, None]
        } else {
            [create.push_to.as_deref(), None, None]
        }
    } else {
        [
            create.push_to.as_deref(),
            create.s3_endpoint_url.as_deref(),
            create.niks3_server_url.as_deref(),
        ]
    };
    if let Err(message) = reject_probe_query_credentials(urls) {
        if create.cache_type == "Attic" {
            return attic_probe::policy_error_response(message);
        }
        return probe_error(
            StatusCode::BAD_REQUEST,
            "legacy_query_credentials_unsupported",
            message,
        );
    }
    if create.validate_probe(scope).is_err() {
        if create.cache_type == "Niks3" {
            return diagnostic_error(
                StatusCode::BAD_REQUEST,
                "invalid_cache_test_config",
                niks3_config_diagnostic(&create, scope),
            );
        }
        return probe_error(
            StatusCode::BAD_REQUEST,
            "invalid_cache_test_config",
            "Invalid cache configuration",
        );
    }
    match run_cache_destination_test_scope(
        &create,
        server_config.allow_private_cache_test_targets,
        scope,
    )
    .await
    {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(message) if create.cache_type == "Attic" => {
            attic_probe::policy_error_response(&message)
        }
        Err(message) if create.cache_type == "Niks3" => diagnostic_error(
            StatusCode::BAD_REQUEST,
            "invalid_cache_test_config",
            static_probe_diagnostic(&message),
        ),
        Err(message) => probe_error(
            StatusCode::BAD_REQUEST,
            "invalid_cache_test_config",
            &message,
        ),
    }
}

/// Discovers a Niks3 server's public read URL and signing keys for an admin.
///
/// `POST /api/caches/niks3/discover` accepts a [`Niks3DiscoverRequest`]. Returns
/// `403` without cache-administration permission and `400` for rejected targets,
/// redirects, connection errors, invalid JSON, or missing/invalid signing keys.
/// Uses Niks3 v1.6.0 `GET /api/cache-config` without an issuer query or Bearer
/// token. Optional write mTLS protects discovery transport, not upload authority.
/// The read URL is validated but not fetched. No destination is persisted.
/// `oidc_audience` is returned as null when the upstream omits that field.
pub async fn discover_niks3_cache(
    State(pool): State<PgPool>,
    State(server_config): State<ServerConfig>,
    headers: HeaderMap,
    request: axum::extract::Request,
) -> axum::response::Response {
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
    if let Err(response) = optional_probe_csrf(&headers) {
        return response;
    }
    let request: Niks3DiscoverRequest = match probe_json(request).await {
        Ok(request) => request,
        Err(response) => return response,
    };
    match discover_niks3(&request, server_config.allow_private_cache_test_targets).await {
        Ok(discovery) => (StatusCode::OK, Json(discovery)).into_response(),
        Err(message) => diagnostic_error(
            StatusCode::BAD_REQUEST,
            "invalid_niks3_discovery",
            discovery_diagnostic(&message),
        ),
    }
}

/// Discovers public Niks3 metadata using a stored destination's effective update.
///
/// `POST /api/v1/caches/:id/niks3/discover` accepts the same unwrapped
/// [`UpdateCacheDestination`] JSON as Save. Authorization precedes parsing and
/// lookup; supplied CSRF state must match. Returns `403` for denied callers,
/// `404` for absent IDs, `400` for non-Niks3 or invalid effective settings and
/// discovery failures, and `200` with public discovery metadata on success.
/// Stored credentials are decrypted and merged through the full shared Save
/// validator. A single unlocked SELECT releases its connection before network
/// work. No ciphertext, assignment, usage, timestamp, or job write occurs.
/// Write mTLS alone is projected to the metadata request; no token or read
/// credentials are sent. Metadata and mTLS success do not prove write access.
///
/// # Examples
/// ```text
/// POST /api/v1/caches/42/niks3/discover
/// Content-Type: application/json
///
/// {}
/// ```
pub async fn discover_stored_niks3_cache(
    State(pool): State<PgPool>,
    State(server_config): State<ServerConfig>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    request: axum::extract::Request,
) -> axum::response::Response {
    stored_niks3_discovery_with(&pool, &headers, id, request, |request| async move {
        discover_niks3(&request, server_config.allow_private_cache_test_targets).await
    })
    .await
}

// Per-call injection is private to this module; production always uses the
// pinned, bounded HTTPS implementation. No publication lock spans discovery.
async fn stored_niks3_discovery_with<F, Fut>(
    pool: &PgPool,
    headers: &HeaderMap,
    id: i32,
    request: axum::extract::Request,
    discover: F,
) -> axum::response::Response
where
    F: FnOnce(Niks3DiscoverRequest) -> Fut,
    Fut: std::future::Future<Output = Result<Niks3Discovery, String>>,
{
    if require_admin_user(pool, headers).await.is_none() {
        return probe_error(StatusCode::FORBIDDEN, "forbidden", "Admin role required");
    }
    if let Err(response) = optional_probe_csrf(headers) {
        return response;
    }
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
    let mut update: UpdateCacheDestination = match probe_json(request).await {
        Ok(update) => update,
        Err(response) => return response,
    };
    if let Err(error) = update.normalize_ca_fields() {
        return diagnostic_error(
            StatusCode::BAD_REQUEST,
            "invalid_niks3_discovery",
            ca_diagnostic(&error),
        );
    }
    if current.cache_type != "Niks3" || update.cache_type.as_deref().is_some_and(|ty| ty != "Niks3")
    {
        return probe_error(
            StatusCode::BAD_REQUEST,
            "invalid_niks3_discovery",
            "Stored discovery requires a Niks3 destination",
        );
    }
    let effective = match cache_destinations::effective_update(&current, &update) {
        Ok(effective) => effective,
        Err(_) => {
            return probe_error(
                StatusCode::BAD_REQUEST,
                "invalid_niks3_discovery",
                "Invalid effective Niks3 configuration",
            );
        }
    };
    if reject_probe_query_credentials([
        effective.push_to.as_deref(),
        effective.s3_endpoint_url.as_deref(),
        effective.niks3_server_url.as_deref(),
    ])
    .is_err()
    {
        return probe_error(
            StatusCode::BAD_REQUEST,
            "invalid_niks3_discovery",
            LEGACY_QUERY_CREDENTIALS_TEST_ERROR,
        );
    }
    let request = Niks3DiscoverRequest {
        server_url: effective.niks3_server_url.unwrap_or_default(),
        niks3_write_auth_mode: effective.niks3_write_auth_mode,
        niks3_write_client_cert: effective.niks3_write_client_cert,
        niks3_write_client_key: effective.niks3_write_client_key,
        niks3_write_ca_cert: effective.niks3_write_ca_cert,
    };
    if let Err(message) = niks3_write_transport(
        request.niks3_write_auth_mode.as_deref(),
        request.niks3_write_client_cert.as_deref(),
        request.niks3_write_client_key.as_deref(),
        request.niks3_write_ca_cert.as_deref(),
    ) {
        return diagnostic_error(
            StatusCode::BAD_REQUEST,
            "invalid_niks3_discovery",
            discovery_diagnostic(&message),
        );
    }
    match discover(request).await {
        Ok(discovery) => (StatusCode::OK, Json(discovery)).into_response(),
        Err(message) => diagnostic_error(
            StatusCode::BAD_REQUEST,
            "invalid_niks3_discovery",
            discovery_diagnostic(&message),
        ),
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
                "invalid_niks3_metadata",
            ),
            (
                format!(r#"{{"substituter_url":"invalid","public_keys":["{key}"]}}"#),
                false,
                "target_policy_rejected",
            ),
            (
                format!(r#"{{"substituter_url":"https://127.0.0.1","public_keys":["{key}"]}}"#),
                false,
                "target_policy_rejected",
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
            // Invalid advertised URLs reject the complete metadata observation;
            // no accepted key or protected-route evidence can be reported.
            assert_eq!(stages.signing_keys_found, keys_found);
            assert!(!stages.discovery_valid);
            assert!(!stages.read_endpoint_reachable);
            assert!(stages.write_auth_valid.is_none());
            assert!(stages.write_authn_valid.is_none());
            assert!(stages.write_authorization_valid.is_none());
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
        assert_eq!(
            run_cache_destination_test(&create, false)
                .await
                .unwrap_err(),
            "target_policy_rejected"
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
            axum::http::Request::builder()
                .body(axum::body::Body::from("invalid-json"))
                .unwrap(),
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

/// Updates a destination and optional assignments atomically for an administrator.
///
/// `PUT /api/caches/:id` normalizes explicit CA replacements and validates the
/// merged settings under the destination row lock. Omitted same-mode credentials
/// retain their stored bytes. Mode/type changes and explicit clears keep the
/// existing credential-clearing rules. Reads and probes never persist normalized
/// CA projections; this Save operation can persist canonical CA fields.
///
/// # Errors
/// Returns HTTP 403 without admin access, 404 for an absent destination, 400 for
/// invalid effective input, or 500 for storage/encryption failure. CA errors use
/// static diagnostic codes and field names without submitted certificate text.
///
/// # Examples
/// ```text
/// PUT /api/caches/42
/// Content-Type: application/json
///
/// {"clear_niks3_write_ca_cert":true}
/// ```
pub async fn update_cache_destination(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(mut update): Json<UpdateCacheDestination>,
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

    if let Err(error) = update.normalize_ca_fields() {
        return diagnostic_error(
            StatusCode::BAD_REQUEST,
            "validation_error",
            ca_diagnostic(&error),
        );
    }
    if let Err(hint) = client_chain_diagnostic(
        update.niks3_write_client_cert.as_deref(),
        update.niks3_read_client_cert.as_deref(),
    ) {
        return diagnostic_error(StatusCode::BAD_REQUEST, "validation_error", hint);
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
