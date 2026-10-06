//! Probes named Attic cache access without testing uploads or echoing metadata.

use super::*;
use cf_config::attic_urls::resolve_attic_urls;

// Cache configuration is small. Bound untrusted endpoint metadata to 64 KiB,
// matching the existing discovery probe limit rather than reading arbitrary
// error pages or object data into memory.
const MAX_BODY_BYTES: usize = 64 * 1024;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum Stage {
    TargetPolicy,
    Dns,
    Transport,
    Authentication,
    CacheNotFound,
    Response,
    Complete,
}

/// Contains credential-free evidence from one named-cache read operation.
#[derive(Debug, serde::Serialize)]
pub(super) struct AtticTestResult {
    probe_kind: &'static str,
    stage: Stage,
    cache_access_valid: Option<bool>,
    token_auth_valid: Option<bool>,
    // Read access never establishes push permission, including private caches.
    write_auth_valid: Option<bool>,
}

fn outcome(stage: Stage, status: Option<u16>, message: &'static str) -> CacheCredentialTestResult {
    let cache_access_valid = match stage {
        Stage::TargetPolicy | Stage::Dns | Stage::Transport => None,
        _ => Some(false),
    };
    CacheCredentialTestResult {
        ok: false,
        status_code: status,
        message: message.into(),
        tested_url: None,
        niks3: Some(CacheProbeDetails::Attic(AtticTestResult {
            probe_kind: "attic_cache_config",
            stage,
            cache_access_valid,
            token_auth_valid: None,
            write_auth_valid: None,
        })),
    }
}

/// Returns an HTTP 400 policy result with static actionable text and no URL.
pub(super) fn policy_error_response(message: &str) -> axum::response::Response {
    // Only static validation strings reach this function. Upstream response
    // bodies and Reqwest errors never become client-facing messages.
    let mut result = outcome(
        Stage::TargetPolicy,
        None,
        "Target blocked. Check the HTTPS server URL and cache name; ask an administrator to review target policy. Write authorization: Untested.",
    );
    let mut error = "invalid_cache_test_config";
    // Preserve the static legacy-query refusal reason without echoing the URL.
    if message == LEGACY_QUERY_CREDENTIALS_TEST_ERROR {
        result.message = LEGACY_QUERY_CREDENTIALS_TEST_ERROR.into();
        error = "legacy_query_credentials_unsupported";
    }
    (
        StatusCode::BAD_REQUEST,
        Json(PolicyError {
            error,
            details: None,
            result,
        }),
    )
        .into_response()
}

#[derive(serde::Serialize)]
struct PolicyError {
    error: &'static str,
    details: Option<()>,
    #[serde(flatten)]
    result: CacheCredentialTestResult,
}

#[derive(Deserialize)]
struct CacheConfig {
    public_key: String,
    is_public: bool,
    store_dir: String,
    priority: i32,
}

#[derive(Deserialize)]
enum ErrorName {
    NoSuchCache,
}

#[derive(Deserialize)]
struct CacheError {
    code: u16,
    error: ErrorName,
}

/// Interprets bounded metadata without exposing or following upstream values.
pub(super) fn classify_response(status: u16, body: &[u8]) -> CacheCredentialTestResult {
    if status == 401 || status == 403 {
        // Attic hides cache existence from callers without discovery access.
        return outcome(
            Stage::Authentication,
            Some(status),
            "Cache access denied. Check the token and its cache access permissions; cache existence is unresolved. Write authorization: Untested.",
        );
    }
    if status == 404 {
        if serde_json::from_slice::<CacheError>(body)
            .is_ok_and(|error| error.code == 404 && matches!(error.error, ErrorName::NoSuchCache))
        {
            return outcome(
                Stage::CacheNotFound,
                Some(status),
                "Cache not found. Check the configured cache name. Write authorization: Untested.",
            );
        }
        return outcome(
            Stage::Response,
            Some(status),
            "Attic endpoint unavailable (endpoint_unavailable). Check the server URL and proxy path; cache existence is unresolved. Write authorization: Untested.",
        );
    }
    if status != 200 {
        return outcome(
            Stage::Response,
            Some(status),
            "Attic endpoint returned an unexpected response. Check the server URL and proxy configuration. Write authorization: Untested.",
        );
    }
    let config = serde_json::from_slice::<CacheConfig>(body)
        .ok()
        .filter(|config| {
            config.store_dir == "/nix/store"
                && cf_protocol::cache::validate_nix_public_key(&config.public_key).is_ok()
        });
    let Some(config) = config else {
        return outcome(
            Stage::Response,
            Some(status),
            "Invalid Attic cache-config response. Check the server URL and proxy configuration. Write authorization: Untested.",
        );
    };
    // Required priority is parsed as i32 even though the probe does not persist
    // or advertise upstream values. No advertised URL is followed.
    let _ = config.priority;
    let mut result = outcome(
        Stage::Complete,
        Some(status),
        "Cache access verified. Write authorization: Untested.",
    );
    result.ok = true;
    if let Some(CacheProbeDetails::Attic(evidence)) = &mut result.niks3 {
        evidence.cache_access_valid = Some(true);
        evidence.token_auth_valid = (!config.is_public).then_some(true);
    }
    result
}

async fn bounded_body(mut response: reqwest::Response) -> Result<Vec<u8>, ()> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| ())? {
        if body.len() + chunk.len() > MAX_BODY_BYTES {
            return Err(());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Checks the configured named-cache API through the existing pinned transport.
///
/// # Errors
/// Returns static errors for invalid configured URLs/names or blocked targets.
/// DNS, TLS, HTTP, and metadata failures return credential-free stage results.
pub(super) async fn probe(
    create: &CreateCacheDestination,
    allow_private_targets: bool,
) -> Result<CacheCredentialTestResult, String> {
    let urls = resolve_attic_urls(
        create
            .push_to
            .as_deref()
            .ok_or("Missing Attic server URL")?,
        create
            .attic_cache_name
            .as_deref()
            .ok_or("Missing Attic cache name")?,
    )
    .map_err(|_| "Invalid Attic server URL or cache name")?;
    let endpoint = urls.cache_config_url;
    validate_cache_test_url(&endpoint, allow_private_targets)?;
    let addrs = match cache_test_addresses(&endpoint, allow_private_targets).await {
        Ok(addrs) => addrs,
        Err(error) if error.starts_with("Refusing to test") => return Err(error),
        Err(_) => {
            return Ok(outcome(
                Stage::Dns,
                None,
                "Cannot resolve the Attic server. Check DNS and the server URL. Write authorization: Untested.",
            ));
        }
    };
    let client = match cache_test_client_pinned(&endpoint, &addrs, None, None, None).await {
        Ok(client) => client,
        Err(_) => {
            return Ok(outcome(
                Stage::Transport,
                None,
                "Cannot initialize verified TLS transport. Check the configured CA trust. Write authorization: Untested.",
            ));
        }
    };
    // SECURITY: Only the canonical configured API gets the effective Attic
    // token. DNS is pinned, proxies/redirects are disabled, and TLS verifies
    // the URL host. Neither response URLs nor inactive credentials are used.
    let response = match legacy_probe_request(&client, endpoint, create).send().await {
        Ok(response) => response,
        Err(_) => {
            return Ok(outcome(
                Stage::Transport,
                None,
                "Cannot connect to the Attic server with verified TLS. Check connectivity, certificate trust, and the server URL. Write authorization: Untested.",
            ));
        }
    };
    let status = response.status().as_u16();
    if status != 200 && status != 404 {
        // Auth/access denial is established by status, not an upstream body.
        // In particular, a large or stalled 401 page must not obscure denial.
        return Ok(classify_response(status, &[]));
    }
    let body = match bounded_body(response).await {
        Ok(body) => body,
        Err(_) => {
            return Ok(outcome(
                Stage::Response,
                Some(status),
                "Cannot read bounded Attic metadata (64 KiB limit). Check the endpoint response. Write authorization: Untested.",
            ));
        }
    };
    Ok(classify_response(status, &body))
}
