//! Observes provider inventory without persistence or inventory enumeration.
//!
//! Niks3 1.8+ exposes public `GET /api/cache-stats`. The packaged 1.6 CLI
//! remains unchanged. A missing remote endpoint is unavailable, not evidence
//! of an empty cache or a known server version. Attic requires an explicit
//! destination-ID-bound inventory adapter with a separate runtime credential.
//! Other providers and unconfigured Attic cause no network work.
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
    metadata_outcome_with_attic(cache_type, enabled, admin, false)
}

fn metadata_outcome_with_attic(
    cache_type: &str,
    enabled: bool,
    admin: bool,
    attic_configured: bool,
) -> Option<Metrics> {
    if !enabled {
        return Some(Metrics::absent(Status::Unavailable, "disabled"));
    }
    match cache_type {
        "Attic" if attic_configured && !admin => {
            Some(Metrics::absent(Status::Unavailable, "requires_admin"))
        }
        "Attic" if attic_configured => None,
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
/// other readers receive `unavailable` with `requires_admin` for Niks3 and
/// explicitly mapped Attic. Attic snapshots contain only active type, enabled
/// state and the configured native name; no cache credential is decrypted.
///
/// Attic adapters require an explicit destination-ID mapping and HTTPS base.
/// The request appends `v1/caches/<encoded-native-name>/inventory` beneath the
/// configured prefix and sends only the separate protected-file Bearer token.
/// Schema 1 requires exact cache identity and unsigned 64-bit path/byte totals.
/// Unknown fields are ignored; missing, null, negative or overflowing required
/// fields are errors. Invalid mappings or token files yield the static reason
/// `invalid_configuration`. Unmapped Attic remains unsupported, never zero.
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
/// are null, never zero. Attic reports uncompressed NAR bytes and cached Nix
/// paths, with `object_count` equal to `path_count` for display compatibility.
/// Niks3 `path_count` remains null. `measured_at` is the local
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
    let attic_configured = config
        .attic_metrics_endpoints
        .iter()
        .any(|entry| entry.destination_id == id);
    if let Some(outcome) =
        metadata_outcome_with_attic(&cache_type, enabled, admin, attic_configured)
    {
        return metrics_response(outcome);
    }
    if cache_type == "Attic" {
        // SECURITY: Adapter observation needs only the canonical cache name,
        // never Attic upload credentials. Re-read active metadata after auth.
        let snapshot = sqlx::query_as::<_, (String, bool, Option<String>)>(
            "SELECT cache_type, enabled, attic_cache_name FROM cache_destinations WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&pool)
        .await;
        let (current_type, current_enabled, name) = match snapshot {
            Ok(Some(snapshot)) => snapshot,
            Ok(None) => {
                return metrics_response(Metrics::absent(
                    Status::Unavailable,
                    "destination_unavailable",
                ));
            }
            Err(_) => {
                return metrics_response(Metrics::absent(Status::Error, "invalid_configuration"));
            }
        };
        if let Some(outcome) =
            metadata_outcome_with_attic(&current_type, current_enabled, admin, attic_configured)
        {
            return metrics_response(outcome);
        }
        if current_type != "Attic" {
            return metrics_response(Metrics::absent(
                Status::Unavailable,
                "destination_unavailable",
            ));
        }
        return metrics_response(
            with_deadline(
                probe_attic(&config, id, name.as_deref()),
                OBSERVATION_TIMEOUT,
            )
            .await,
        );
    }
    // SECURITY: The admin decision above precedes all decryption and network
    // work. Recheck type/enabled from the later snapshot to handle intervening
    // edits without probing a disabled or converted destination.
    let mut snapshot = match sqlx::query_as::<_, CacheDestination>(
        "SELECT * FROM cache_destinations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&pool)
    .await
    {
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
    // SECURITY: Recheck the raw snapshot before loading the write TLS key.
    // Only active mTLS mode uses that key; tokens and read keys are not needed.
    if snapshot.normalize_ca_fields().is_err() || snapshot.validate_niks3_certificates().is_err() {
        return metrics_response(Metrics::absent(Status::Error, "invalid_configuration"));
    }
    snapshot.niks3_write_client_key = match crate::security::cache_secrets::decrypt_optional(
        snapshot.niks3_write_client_key.as_deref(),
    ) {
        Ok(key) => key,
        Err(_) => return metrics_response(Metrics::absent(Status::Error, "invalid_configuration")),
    };
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

// COMPATIBILITY: Schema 1 requires every field. Unknown fields are ignored for
// additive evolution; all known fields retain exact types and identity.
#[derive(Deserialize)]
struct AtticInventory {
    schema_version: u64,
    cache: String,
    cached_path_count: u64,
    logical_nar_bytes: u64,
}

fn parse_attic_inventory(body: &[u8], name: &str) -> Metrics {
    let Ok(inventory) = serde_json::from_slice::<AtticInventory>(body) else {
        return Metrics::absent(Status::Error, "invalid_stats");
    };
    if inventory.schema_version != 1 || inventory.cache != name {
        return Metrics::absent(Status::Error, "invalid_stats");
    }
    Metrics {
        status: Status::Available,
        reason_code: "attic_inventory",
        storage_bytes: Some(inventory.logical_nar_bytes),
        storage_bytes_basis: Some("uncompressed_nar_bytes"),
        object_count: Some(inventory.cached_path_count),
        object_count_basis: Some("cached_nix_paths"),
        path_count: Some(inventory.cached_path_count),
        measured_at: Some(Utc::now()),
    }
}

fn token_path_allowed(path: &std::path::Path) -> bool {
    path.is_absolute()
        && !path.starts_with("/nix/store")
        && path
            .components()
            .all(|part| !matches!(part, std::path::Component::ParentDir))
}

fn attic_adapter_base(raw: &str, allow_private: bool) -> Result<Url, ()> {
    // Url removes syntactically empty userinfo. Reject it before parsing too.
    let authority = raw
        .split_once("://")
        .ok_or(())?
        .1
        .split(['/', '?', '#'])
        .next()
        .ok_or(())?;
    if authority.contains('@') {
        return Err(());
    }
    let mut url = Url::parse(raw).map_err(|_| ())?;
    validate_cache_test_url(&url, allow_private).map_err(|_| ())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(());
    }
    // Extend segments rather than resolving a relative URL against a file base.
    url.path_segments_mut()
        .map_err(|_| ())?
        .pop_if_empty()
        .push("");
    Ok(url)
}

fn attic_mapping(
    config: &ServerConfig,
    id: i32,
) -> Result<(&cf_config::config::AtticMetricsEndpoint, Url), ()> {
    let mut seen = std::collections::HashSet::new();
    let mut selected = None;
    for entry in &config.attic_metrics_endpoints {
        if entry.destination_id <= 0
            || !seen.insert(entry.destination_id)
            || !token_path_allowed(&entry.token_file)
        {
            return Err(());
        }
        let base = attic_adapter_base(&entry.url, config.allow_private_cache_test_targets)?;
        if entry.destination_id == id {
            selected = Some((entry, base));
        }
    }
    selected.ok_or(())
}

fn attic_inventory_request(
    client: &reqwest::Client,
    base: &Url,
    name: &str,
    token: reqwest::header::HeaderValue,
) -> Result<reqwest::RequestBuilder, ()> {
    let mut endpoint = base.clone();
    endpoint
        .path_segments_mut()
        .map_err(|_| ())?
        .pop_if_empty()
        .extend(["v1", "caches", name, "inventory"]);
    // SECURITY: Only the dedicated adapter token is attached. There is no
    // destination token, Basic auth or TLS identity on this request builder.
    Ok(client
        .get(endpoint)
        .header(reqwest::header::AUTHORIZATION, token))
}

fn metrics_bearer(bytes: &[u8]) -> Result<reqwest::header::HeaderValue, ()> {
    // SECURITY: Match the adapter's exact ASCII token contract, without trimming
    // or accepting padding. The Bearer header is at most 1031 bytes.
    if !(32..=1024).contains(&bytes.len())
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(byte))
    {
        return Err(());
    }
    let mut value = b"Bearer ".to_vec();
    value.extend_from_slice(bytes);
    let mut header = reqwest::header::HeaderValue::from_bytes(&value).map_err(|_| ())?;
    header.set_sensitive(true);
    Ok(header)
}

async fn load_metrics_token(path: &std::path::Path) -> Result<reqwest::header::HeaderValue, ()> {
    use std::os::unix::fs::MetadataExt;
    use tokio::io::AsyncReadExt;
    if !token_path_allowed(path) {
        return Err(());
    }
    // SECURITY: O_NOFOLLOW rejects a final symlink, O_NONBLOCK prevents a FIFO
    // open from occupying a blocking worker. Validate the opened descriptor,
    // not pre-open metadata; replacement cannot bypass file/owner/mode checks.
    let file = tokio::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .await
        .map_err(|_| ())?;
    let metadata = file.metadata().await.map_err(|_| ())?;
    // SAFETY: geteuid has no preconditions and does not access Rust memory.
    let owner = unsafe { libc::geteuid() };
    if !metadata.is_file()
        || metadata.uid() == 0
        || metadata.uid() != owner
        || metadata.mode() & 0o077 != 0
        || metadata.len() > 4096
    {
        return Err(());
    }
    // The extra byte detects growth after metadata validation without an
    // unbounded allocation or read. This IO shares the observation deadline.
    let mut bytes = Vec::new();
    file.take(4097)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| ())?;
    if bytes.len() > 4096 {
        return Err(());
    }
    metrics_bearer(&bytes)
}

async fn probe_attic(config: &ServerConfig, id: i32, configured_name: Option<&str>) -> Metrics {
    let invalid = || Metrics::absent(Status::Error, "invalid_configuration");
    // Re-resolve by database ID immediately before file/network work. Never
    // infer a destination from display name, URL, order or other metadata.
    let Ok((entry, base)) = attic_mapping(config, id) else {
        return invalid();
    };
    let Some(name) = configured_name else {
        return invalid();
    };
    // Only name validation is needed. Use a fixed nonsecret resolver base so
    // destination URLs and credentials cannot influence adapter authority.
    let Ok(urls) = cf_config::attic_urls::resolve_attic_urls("https://attic.invalid/", name.trim())
    else {
        return invalid();
    };
    let token = match load_metrics_token(&entry.token_file).await {
        Ok(token) => token,
        Err(_) => return invalid(),
    };
    let client = match cache_test_client(
        &base,
        config.allow_private_cache_test_targets,
        None,
        None,
        None,
    )
    .await
    {
        Ok(client) => client,
        Err(_) => return Metrics::absent(Status::Unavailable, "transport_unavailable"),
    };
    let Ok(request) = attic_inventory_request(&client, &base, &urls.cache_name, token) else {
        return invalid();
    };
    let response = match request.send().await {
        Ok(response) => response,
        Err(_) => return Metrics::absent(Status::Unavailable, "transport_unavailable"),
    };
    consume_attic_response(response, &urls.cache_name).await
}

async fn consume_attic_response(mut response: reqwest::Response, name: &str) -> Metrics {
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
            Ok(None) => return parse_attic_inventory(&body, name),
            Err(_) => return Metrics::absent(Status::Unavailable, "response_unavailable"),
        }
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
    let client = match niks3_write_test_client(&server, allow_private_targets, &transport).await {
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

    fn adapter_config() -> ServerConfig {
        ServerConfig {
            attic_metrics_endpoints: vec![cf_config::config::AtticMetricsEndpoint {
                destination_id: 42,
                url: "https://metrics.example/prefix".into(),
                token_file: "/run/credentials/crystal-forge/metrics".into(),
            }],
            ..ServerConfig::default()
        }
    }

    #[test]
    fn attic_inventory_preserves_exact_identity_u64_and_path_basis() {
        for count in [0, 7, u64::MAX] {
            let body = format!(
                r#"{{"schema_version":1,"cache":"Team+1","cached_path_count":{count},"logical_nar_bytes":{count},"future_safe_field":true}}"#
            );
            let metrics = parse_attic_inventory(body.as_bytes(), "Team+1");
            assert_eq!(metrics.status, Status::Available);
            assert_eq!(metrics.reason_code, "attic_inventory");
            assert_eq!(metrics.storage_bytes, Some(count));
            assert_eq!(metrics.path_count, Some(count));
            assert_eq!(metrics.object_count, Some(count));
            assert_eq!(metrics.storage_bytes_basis, Some("uncompressed_nar_bytes"));
            assert_eq!(metrics.object_count_basis, Some("cached_nix_paths"));
            assert!(metrics.measured_at.is_some());
        }
    }

    #[test]
    fn attic_inventory_rejects_missing_null_negative_overflow_and_identity_mismatch() {
        let valid = serde_json::json!({"schema_version": 1, "cache": "Team+1", "cached_path_count": 0, "logical_nar_bytes": 0});
        for field in [
            "schema_version",
            "cache",
            "cached_path_count",
            "logical_nar_bytes",
        ] {
            let mut missing = valid.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert_eq!(
                parse_attic_inventory(&serde_json::to_vec(&missing).unwrap(), "Team+1").status,
                Status::Error
            );
            for invalid in [
                serde_json::Value::Null,
                serde_json::json!(-1),
                serde_json::json!(1.5),
                serde_json::json!("0"),
            ] {
                let mut body = valid.clone();
                body[field] = invalid;
                assert_eq!(
                    parse_attic_inventory(&serde_json::to_vec(&body).unwrap(), "Team+1").status,
                    Status::Error
                );
            }
        }
        for body in [
            r#"{"schema_version":2,"cache":"Team+1","cached_path_count":0,"logical_nar_bytes":0}"#,
            r#"{"schema_version":1,"cache":"team+1","cached_path_count":0,"logical_nar_bytes":0}"#,
            r#"{"schema_version":1,"cache":"Team+1","cached_path_count":18446744073709551616,"logical_nar_bytes":0}"#,
            r#"{"schema_version":1,"cache":"Team+1","cached_path_count":0,"logical_nar_bytes":18446744073709551616}"#,
            "private-upstream-marker",
        ] {
            let metrics = parse_attic_inventory(body.as_bytes(), "Team+1");
            assert_eq!(metrics.status, Status::Error);
            assert!(metrics.path_count.is_none());
            assert!(metrics.measured_at.is_none());
            assert!(
                !serde_json::to_string(&metrics)
                    .unwrap()
                    .contains("private-upstream-marker")
            );
        }
    }

    #[test]
    fn attic_optional_metadata_gate_requires_current_enabled_type_and_admin() {
        for configured in [false, true] {
            for admin in [false, true] {
                assert_eq!(
                    metadata_outcome_with_attic("Attic", false, admin, configured)
                        .unwrap()
                        .reason_code,
                    "disabled"
                );
                assert_eq!(
                    metadata_outcome_with_attic("S3", true, admin, configured)
                        .unwrap()
                        .reason_code,
                    "provider_totals_unsupported"
                );
            }
        }
        assert_eq!(
            metadata_outcome_with_attic("Attic", true, true, false)
                .unwrap()
                .status,
            Status::Unsupported
        );
        assert_eq!(
            metadata_outcome_with_attic("Attic", true, false, true)
                .unwrap()
                .reason_code,
            "requires_admin"
        );
        assert!(metadata_outcome_with_attic("Attic", true, true, true).is_none());
        assert!(attic_mapping(&adapter_config(), 43).is_err());
    }

    #[test]
    fn attic_mapping_is_explicit_https_and_fails_closed_for_every_invalid_entry() {
        let config = adapter_config();
        assert_eq!(
            attic_mapping(&config, 42).unwrap().1.as_str(),
            "https://metrics.example/prefix/"
        );
        let mut duplicate = config.clone();
        duplicate
            .attic_metrics_endpoints
            .push(duplicate.attic_metrics_endpoints[0].clone());
        assert!(attic_mapping(&duplicate, 42).is_err());
        for url in [
            "http://metrics.example",
            "https://@metrics.example",
            "https://user:marker@metrics.example",
            "https://metrics.example?",
            "https://metrics.example#",
            "https://127.0.0.1",
        ] {
            let mut invalid = config.clone();
            invalid.attic_metrics_endpoints[0].url = url.into();
            assert!(attic_mapping(&invalid, 42).is_err());
        }
        for path in [
            "relative",
            "/nix/store/credential",
            "/run/../nix/store/credential",
        ] {
            let mut invalid = config.clone();
            invalid.attic_metrics_endpoints[0].token_file = path.into();
            assert!(attic_mapping(&invalid, 42).is_err());
        }
        let mut invalid_other = config.clone();
        let mut entry = config.attic_metrics_endpoints[0].clone();
        entry.destination_id = 99;
        entry.url = "http://metrics.example".into();
        invalid_other.attic_metrics_endpoints.push(entry);
        assert!(attic_mapping(&invalid_other, 42).is_err());
        let mut private = config;
        private.allow_private_cache_test_targets = true;
        private.attic_metrics_endpoints[0].url = "https://127.0.0.1/prefix".into();
        assert!(attic_mapping(&private, 42).is_ok());
    }

    #[test]
    fn attic_request_uses_only_adapter_bearer_and_exact_canonical_name() {
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let token = metrics_bearer(&vec![b'a'; 32]).unwrap();
        for raw in [
            "https://metrics.example/p%2Frefix",
            "https://metrics.example/p%2Frefix/",
        ] {
            let base = attic_adapter_base(raw, false).unwrap();
            let name = cf_config::attic_urls::resolve_attic_urls(
                "https://attic.invalid/",
                "remote:Team+1",
            )
            .unwrap()
            .cache_name;
            let request = attic_inventory_request(&client, &base, &name, token.clone())
                .unwrap()
                .build()
                .unwrap();
            assert_eq!(
                request.url().as_str(),
                "https://metrics.example/p%2Frefix/v1/caches/Team+1/inventory"
            );
            assert_eq!(request.method(), reqwest::Method::GET);
            assert_eq!(
                request.headers()[reqwest::header::AUTHORIZATION],
                format!("Bearer {}", "a".repeat(32))
            );
            assert!(request.headers()[reqwest::header::AUTHORIZATION].is_sensitive());
        }
        // Encoding is segment-safe even before the separate native-name gate.
        let base = attic_adapter_base("https://metrics.example/", false).unwrap();
        let request = attic_inventory_request(&client, &base, "a/b", token)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request.url().path(), "/v1/caches/a%2Fb/inventory");
    }

    #[tokio::test]
    async fn attic_token_loader_rejects_symlinks_public_files_and_bad_tokens() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("token");
        tokio::fs::write(&path, vec![b'a'; 32]).await.unwrap();
        tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .await
            .unwrap();
        assert!(load_metrics_token(&path).await.is_ok());
        let link = directory.path().join("link");
        symlink(&path, &link).unwrap();
        assert!(load_metrics_token(&link).await.is_err());
        assert!(load_metrics_token(directory.path()).await.is_err());
        for mode in [0o640, 0o604, 0o666] {
            tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
                .await
                .unwrap();
            assert!(load_metrics_token(&path).await.is_err());
        }
        tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .await
            .unwrap();
        tokio::fs::write(&path, vec![b'a'; 4097]).await.unwrap();
        assert!(load_metrics_token(&path).await.is_err());
        for bytes in [
            vec![b'a'; 31],
            vec![b'a'; 1025],
            vec![b'\r'; 32],
            vec![0xff; 32],
            vec![b' '; 32],
        ] {
            assert!(metrics_bearer(&bytes).is_err());
        }
        assert!(metrics_bearer(&vec![b'a'; 1024]).is_ok());
        assert!(metrics_bearer(format!("{}\n", "a".repeat(32)).as_bytes()).is_err());
    }

    #[tokio::test]
    async fn attic_protected_token_file_enforces_exact_adapter_alphabet_and_boundaries() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("token");
        tokio::fs::write(&path, []).await.unwrap();
        tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .await
            .unwrap();
        for length in [32, 1024] {
            let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
            let token: Vec<u8> = alphabet.iter().copied().cycle().take(length).collect();
            tokio::fs::write(&path, &token).await.unwrap();
            let header = load_metrics_token(&path).await.unwrap();
            assert_eq!(header.as_bytes().len(), length + 7);
            assert_eq!(&header.as_bytes()[..7], b"Bearer ");
            assert_eq!(&header.as_bytes()[7..], token);
            assert!(header.is_sensitive());
        }
        for length in [31, 1025] {
            tokio::fs::write(&path, vec![b'a'; length]).await.unwrap();
            assert!(load_metrics_token(&path).await.is_err());
        }
        for forbidden in b".~+/=\n\r " {
            let mut token = vec![b'a'; 32];
            token[31] = *forbidden;
            tokio::fs::write(&path, token).await.unwrap();
            assert!(load_metrics_token(&path).await.is_err());
        }
        // A newline appended to an otherwise valid token must not be stripped.
        let mut token = vec![b'a'; 32];
        token.push(b'\n');
        tokio::fs::write(&path, token).await.unwrap();
        assert!(load_metrics_token(&path).await.is_err());
    }

    #[tokio::test]
    async fn attic_response_is_bounded_and_status_errors_are_static() {
        fn response(status: u16, mime: &str, body: Vec<u8>) -> reqwest::Response {
            openidconnect::http::Response::builder()
                .status(status)
                .header("content-type", mime)
                .body(body)
                .unwrap()
                .into()
        }
        let valid =
            br#"{"schema_version":1,"cache":"team","cached_path_count":3,"logical_nar_bytes":9}"#
                .to_vec();
        assert_eq!(
            consume_attic_response(
                response(200, "application/json; charset=utf-8", valid),
                "team"
            )
            .await
            .path_count,
            Some(3)
        );
        for (status, reason) in [
            (401, "access_denied"),
            (403, "access_denied"),
            (404, "endpoint_unavailable"),
            (302, "unexpected_status"),
            (500, "unexpected_status"),
        ] {
            let metrics = consume_attic_response(
                response(
                    status,
                    "application/json",
                    b"private-upstream-marker".to_vec(),
                ),
                "team",
            )
            .await;
            assert_eq!(metrics.reason_code, reason);
            assert!(
                !serde_json::to_string(&metrics)
                    .unwrap()
                    .contains("private-upstream-marker")
            );
        }
        assert_eq!(
            consume_attic_response(response(200, "text/html", vec![]), "team")
                .await
                .reason_code,
            "invalid_stats"
        );
        assert_eq!(
            consume_attic_response(
                response(200, "application/json", vec![b' '; MAX_STATS_BYTES + 1]),
                "team"
            )
            .await
            .reason_code,
            "stats_too_large"
        );
    }

    #[tokio::test]
    async fn attic_invalid_mapping_or_name_stops_before_file_or_network() {
        let config = adapter_config();
        for (id, name) in [(43, Some("team")), (42, None), (42, Some("bad/name"))] {
            assert_eq!(
                probe_attic(&config, id, name).await.reason_code,
                "invalid_configuration"
            );
        }
    }

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
