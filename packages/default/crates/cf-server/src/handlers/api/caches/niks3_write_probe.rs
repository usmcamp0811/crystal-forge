//! Verifies protected Niks3 API access without publishing or starting GC.
//!
//! The compatibility gate recognizes the protected GC snapshot contract from
//! [v1.6.0](https://github.com/Mic92/niks3/blob/c29f3641de064545d75f00318cd45bea2b4ea1d0/server/closure.go)
//! and [v1.8.0](https://github.com/Mic92/niks3/blob/61501c4223ec3ad804c20625128cab1155166041/server/closure.go).
//! Accepted GET evidence plus a credential-omission control permits one invalid
//! `POST /api/pending_closures` with exactly `{}`. Both versions authenticate
//! before decoding and return `missing closure key\n` before DB/S3 operations:
//! [v1.6 validation order](https://github.com/Mic92/niks3/blob/c29f3641de064545d75f00318cd45bea2b4ea1d0/server/uploads.go#L100-L150)
//! and [v1.8 validation order](https://github.com/Mic92/niks3/blob/61501c4223ec3ad804c20625128cab1155166041/server/uploads.go#L100-L150).
//!
//! This gate identifies a supported handler contract, not a server version.
//! Public metadata, generic HTTP errors and TLS success cannot open the gate.
//! Unknown or contradictory contracts leave write authorization unproven.
//! mTLS omission can match nginx's complete certificate-required HTTP 400 page.
//! That ingress gate does not prove native Go subject enforcement. Token mode,
//! generic HTML errors and incomplete pages cannot use that exception.
//! A matching POST proves protected-handler access only. It does not prove S3
//! permissions, signing, uploads or independently verified publication.
//! No valid closure, retry, cleanup, process, persistence or GC-start call exists
//! in this module. v1.8 HTTP instrumentation can change in-memory metrics.

use cf_protocol::cache::Niks3WriteAuth;
use chrono::{DateTime, FixedOffset};
use reqwest::header::CONTENT_TYPE;
use serde::Deserialize;
use tokio::time::Instant;
use url::Url;

use super::{CreateCacheDestination, Niks3ConnectionTestResult, RootPolicy};

// Upstream responses are small snapshots/errors. Bound both successful and
// unsuccessful bodies, including chunked responses without Content-Length.
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const UNAUTHORIZED: &[u8] = b"Unauthorized\n";
const NO_GC: &[u8] = b"no garbage collection has run yet\n";
const MISSING_CLOSURE: &[u8] = b"missing closure key\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fingerprint {
    GcSnapshot,
    Unauthorized,
    MissingClosure,
    CertificateRequired,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Observation {
    Http {
        status: u16,
        fingerprint: Fingerprint,
    },
    Transport(&'static str),
    ResponseFailure {
        status: u16,
        code: &'static str,
    },
}

#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum GcState {
    Running,
    Succeeded,
    Failed,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum GcPhase {
    CleanupPendingUploads,
    CleanupOldClosures,
    CleanupOrphanObjects,
    VacuumTables,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GcParams {
    older_than: String,
    failed_uploads_older_than: String,
    force: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GcStats {
    failed_uploads_deleted: i64,
    old_closures_deleted: i64,
    objects_marked_for_deletion: i64,
    objects_deleted_after_grace_period: i64,
    objects_failed_to_delete: i64,
}

// No Debug/Serialize implementation: the upstream error may contain arbitrary
// provider prose. Decode its string type, discard it and never copy it to output.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GcSnapshot {
    state: GcState,
    phase: Option<GcPhase>,
    params: GcParams,
    stats: GcStats,
    #[serde(default, deserialize_with = "discard_error")]
    error: Option<()>,
    started_at: DateTime<FixedOffset>,
    updated_at: DateTime<FixedOffset>,
    finished_at: Option<DateTime<FixedOffset>>,
}

fn discard_error<'de, D: serde::Deserializer<'de>>(decoder: D) -> Result<Option<()>, D::Error> {
    // Borrow directly from the bounded JSON buffer where possible. Escaped JSON
    // strings can allocate temporarily, but no error contents survive decoding.
    use serde::de::{Error, Visitor};
    struct Discard;
    impl<'de> Visitor<'de> for Discard {
        type Value = Option<()>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a GC error string")
        }
        fn visit_str<E: Error>(self, _: &str) -> Result<Self::Value, E> {
            Ok(Some(()))
        }
    }
    decoder.deserialize_str(Discard)
}

fn valid_gc_snapshot(body: &[u8]) -> bool {
    let Ok(snapshot) = serde_json::from_slice::<GcSnapshot>(body) else {
        return false;
    };
    let counts = snapshot.stats;
    let nonnegative = [
        counts.failed_uploads_deleted,
        counts.old_closures_deleted,
        counts.objects_marked_for_deletion,
        counts.objects_deleted_after_grace_period,
        counts.objects_failed_to_delete,
    ]
    .into_iter()
    .all(|count| count >= 0);
    let chronology = snapshot.updated_at >= snapshot.started_at
        && snapshot.finished_at.as_ref().is_none_or(|finished| {
            *finished >= snapshot.started_at && *finished <= snapshot.updated_at
        });
    let lifecycle = match snapshot.state {
        GcState::Running => snapshot.finished_at.is_none() && snapshot.error.is_none(),
        GcState::Succeeded => {
            snapshot.finished_at.is_some() && snapshot.phase.is_none() && snapshot.error.is_none()
        }
        GcState::Failed => snapshot.finished_at.is_some(),
    };
    // The native parameters preserve the original Go duration strings. Do not
    // interpret them as Rust durations or execute the requested GC parameters.
    let _force = snapshot.params.force;
    nonnegative
        && chronology
        && lifecycle
        && !snapshot.params.older_than.is_empty()
        && !snapshot.params.failed_uploads_older_than.is_empty()
}

fn fingerprint(status: u16, content_type: Option<&str>, body: &[u8]) -> Fingerprint {
    // Require native media types and exact UTF-8 bytes for errors. Substring
    // matching, HTML, generic 400s and JSON error envelopes prove nothing.
    match (status, content_type) {
        (401, Some("text/plain; charset=utf-8")) if body == UNAUTHORIZED => {
            Fingerprint::Unauthorized
        }
        (404, Some("text/plain; charset=utf-8")) if body == NO_GC => Fingerprint::GcSnapshot,
        (400, Some("text/plain; charset=utf-8")) if body == MISSING_CLOSURE => {
            Fingerprint::MissingClosure
        }
        (400, Some("text/html")) if nginx_certificate_required(body) => {
            Fingerprint::CertificateRequired
        }
        (200, Some("application/json")) if valid_gc_snapshot(body) => Fingerprint::GcSnapshot,
        _ => Fingerprint::Unknown,
    }
}

// COMPATIBILITY: nginx's ngx_http_error_496_page and standard tail produce
// this complete HTTP 400 response when ssl_verify_client on sees no identity.
// Match every byte, permitting only the stock hidden or numeric-version footer.
// Custom pages, padding, truncation and extra provider content remain unknown.
fn nginx_certificate_required(body: &[u8]) -> bool {
    const HEAD: &[u8] = concat!(
        "<html>\r\n",
        "<head><title>400 No required SSL certificate was sent</title></head>\r\n",
        "<body>\r\n",
        "<center><h1>400 Bad Request</h1></center>\r\n",
        "<center>No required SSL certificate was sent</center>\r\n",
        "<hr><center>nginx"
    )
    .as_bytes();
    const TAIL: &[u8] = b"</center>\r\n</body>\r\n</html>\r\n";
    let Some(footer) = body
        .strip_prefix(HEAD)
        .and_then(|rest| rest.strip_suffix(TAIL))
    else {
        return false;
    };
    if footer.is_empty() {
        return true;
    }
    let Some(version) = footer.strip_prefix(b"/") else {
        return false;
    };
    // A stock nginx release has three numeric components. Bound each component
    // to three digits; build names and arbitrary footer values are unsupported.
    let mut components = version.split(|byte| *byte == b'.');
    for _ in 0..3 {
        if !components.next().is_some_and(|part| {
            (1..=3).contains(&part.len()) && part.iter().all(u8::is_ascii_digit)
        }) {
            return false;
        }
    }
    components.next().is_none()
}

fn selected_auth(create: &CreateCacheDestination) -> Result<Niks3WriteAuth, String> {
    match create.niks3_write_auth_mode.as_deref() {
        // Scoped Write validation retains its public-metadata-era token default.
        // Selecting that mode does not supply credentials or borrow read auth.
        None | Some("token") => {
            let token = create
                .niks3_auth_token
                .clone()
                .filter(|token| !token.trim().is_empty())
                .ok_or("write_credentials_missing")?;
            reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| "write_credentials_invalid")?;
            Ok(Niks3WriteAuth::Token { token })
        }
        Some("mtls") => Ok(Niks3WriteAuth::Mtls {
            client_certificate: create
                .niks3_write_client_cert
                .clone()
                .filter(|cert| !cert.trim().is_empty())
                .ok_or("write_credentials_missing")?,
            client_private_key: create
                .niks3_write_client_key
                .clone()
                .filter(|key| !key.trim().is_empty())
                .ok_or("write_credentials_missing")?,
            ca_certificate: create.niks3_write_ca_cert.clone(),
        }),
        _ => Err("write_credentials_invalid".into()),
    }
}

/// Validates local write credentials before metadata, DNS or protected requests.
///
/// The caller first applies the selected scope's input validation. This extra
/// gate checks a token's HTTP header representation and selects only write
/// credentials. Missing or invalid candidates set local failure evidence;
/// reachability stays unattempted. Public Discover does not use this gate.
///
/// # Errors
/// Returns an allowlisted local credential diagnostic without provider claims,
/// submitted material or network activity.
///
/// # Examples
/// ```text
/// All: complete Save validation -> local preflight -> public metadata -> auth
/// Write: selected input validation -> local preflight -> public metadata -> auth
/// ```
pub(super) fn preflight(
    create: &CreateCacheDestination,
    stages: &mut Niks3ConnectionTestResult,
) -> Result<Niks3WriteAuth, String> {
    selected_auth(create).map_err(|code| {
        local_credential_failure(stages);
        code
    })
}

/// Verifies the protected GET and invalid-POST contracts with omission controls.
///
/// Uses write credentials selected by [`preflight`]. Both clients share one
/// policy-checked DNS pin and server-trust pool. Token mode has no TLS identity;
/// mTLS mode sends no Bearer. The control omits both token and identity, preserving
/// custom roots. Credentials never enter metadata/read requests or diagnostics.
/// The caller's deadline includes DNS, client preparation and bounded bodies.
/// Cancellation drops request/response futures; there are no child processes.
///
/// Updates only in-memory evidence. An accepted native GET and rejected control
/// permit exactly one `{}` POST plus its control. Unknown GET contracts prohibit
/// POST. A 200/201 POST response is inconclusive and is never retried.
///
/// # Errors
/// Returns only allowlisted diagnostic codes for target/trust/identity failures,
/// rejection, timeout, oversized bodies or unsupported/contradictory contracts.
/// Definite denial sets false; inconclusive/unattempted authorization stays null.
/// Missing or invalid local write credentials also set authentication and handler
/// access false before DNS. These failures do not claim upstream rejection, and
/// protected API reachability remains unattempted. Public discovery is separate.
/// A certificate-requiring ingress can reject the control before HTTP: matching
/// protected positive evidence then proves session acceptance, not native subject
/// enforcement. This distinction is returned as a static success diagnostic.
///
/// # Examples
/// ```text
/// GET /api/gc/status          -> native snapshot or exact no-GC 404
/// GET /api/gc/status          -> omission: native 401 or identity TLS alert
/// POST /api/pending_closures  -> {}: exact missing-closure 400
/// POST /api/pending_closures  -> omission: native 401 or identity TLS alert
/// ```
pub(super) async fn run(
    auth: &Niks3WriteAuth,
    server: &Url,
    allow_private_targets: bool,
    deadline: Instant,
    stages: &mut Niks3ConnectionTestResult,
    status_code: &mut Option<u16>,
) -> Result<&'static str, String> {
    let transport = match auth {
        Niks3WriteAuth::Token { .. } => super::niks3_write_transport(None, None, None, None),
        Niks3WriteAuth::Mtls {
            client_certificate,
            client_private_key,
            ca_certificate,
        } => super::niks3_write_transport(
            Some("mtls"),
            Some(client_certificate),
            Some(client_private_key),
            ca_certificate.as_deref(),
        ),
    }
    .map_err(|error| {
        let code = super::static_probe_diagnostic(&error).code;
        if matches!(
            code,
            "client_certificate_invalid"
                | "client_private_key_invalid"
                | "client_identity_invalid"
                | "client_identity_mismatch"
        ) {
            local_credential_failure(stages);
        }
        code.to_owned()
    })?;
    let addresses = super::cache_test_addresses(server, allow_private_targets)
        .await
        .map_err(|error| super::static_target_failure(&error))?;
    let policy = if transport.ca.is_some() {
        RootPolicy::ReplaceCustom
    } else {
        RootPolicy::ExtendSystem
    };
    let selected = super::cache_test_client_pinned_policy(
        server,
        &addresses,
        transport.cert,
        transport.key,
        transport.ca,
        policy,
    )
    .await
    .map_err(|error| super::static_probe_diagnostic(&error).code.to_owned())?;
    let control = super::cache_test_client_pinned_policy(
        server,
        &addresses,
        None,
        None,
        transport.ca,
        policy,
    )
    .await
    .map_err(|error| super::static_probe_diagnostic(&error).code.to_owned())?;
    probe_with_clients(
        &selected,
        &control,
        auth,
        server,
        deadline,
        stages,
        status_code,
    )
    .await
}

fn local_credential_failure(stages: &mut Niks3ConnectionTestResult) {
    // False describes an unusable local credential candidate. No provider auth
    // response exists; do not mark an unattempted protected API as reachable.
    stages.write_authn_valid = Some(false);
    stages.write_authorization_valid = Some(false);
    stages.write_auth_valid = Some(false);
}

async fn observe(request: reqwest::RequestBuilder, deadline: Instant) -> Observation {
    let operation = async {
        let mut response = match request.send().await {
            Ok(response) => response,
            Err(error) => {
                return Observation::Transport(
                    super::static_probe_diagnostic(&super::request_failure(&error)).code,
                );
            }
        };
        let status = response.status().as_u16();
        let mut media = response.headers().get_all(CONTENT_TYPE).iter();
        let content_type = media
            .next()
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let content_type = if media.next().is_none() {
            content_type
        } else {
            None
        };
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Observation::ResponseFailure {
                status,
                code: "probe_response_too_large",
            };
        }
        let body = async {
            let mut body = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|error| super::request_failure(&error))?
            {
                if chunk.len() > MAX_RESPONSE_BYTES - body.len() {
                    return Err("probe_response_too_large".into());
                }
                body.extend_from_slice(&chunk);
            }
            Ok::<_, String>(body)
        };
        // A body-stream failure after HTTP headers is not evidence that an
        // identity-free TLS handshake was rejected. Preserve that distinction.
        match tokio::time::timeout_at(deadline, body).await {
            Ok(Ok(body)) => Observation::Http {
                status,
                fingerprint: fingerprint(status, content_type.as_deref(), &body),
            },
            Ok(Err(code)) => Observation::ResponseFailure {
                status,
                code: super::static_probe_diagnostic(&code).code,
            },
            Err(_) => Observation::ResponseFailure {
                status,
                code: "probe_timeout",
            },
        }
    };
    match tokio::time::timeout_at(deadline, operation).await {
        Ok(observation) => observation,
        Err(_) => Observation::Transport("probe_timeout"),
    }
}

fn protected_request(
    client: &reqwest::Client,
    endpoint: Url,
    post: bool,
    auth: Option<&Niks3WriteAuth>,
) -> reqwest::RequestBuilder {
    let request = if post {
        client
            .post(endpoint)
            .header(CONTENT_TYPE, "application/json")
            .body("{}")
    } else {
        client.get(endpoint)
    };
    match auth {
        Some(Niks3WriteAuth::Token { token }) => request.bearer_auth(token),
        _ => request,
    }
}

fn rejected_control(control: Observation, mtls: bool) -> bool {
    matches!(
        control,
        Observation::Http {
            status: 401,
            fingerprint: Fingerprint::Unauthorized
        }
    ) || (mtls && certificate_control(control))
}

// Accepted certificate-omission controls prove an ingress/session gate only.
// Use the same predicate for acceptance and diagnostic selection so HTTP and
// TLS ingress gates cannot accidentally claim native subject enforcement.
fn certificate_control(control: Observation) -> bool {
    matches!(
        control,
        Observation::Transport("client_identity_invalid")
            | Observation::Http {
                status: 400,
                fingerprint: Fingerprint::CertificateRequired
            }
    )
}

fn rejection(positive: Observation, mtls: bool, post: bool) -> &'static str {
    match positive {
        Observation::Http { status: 403, .. }
        | Observation::ResponseFailure { status: 403, .. } => "proxy_access_denied",
        Observation::Http { status: 405, .. }
        | Observation::ResponseFailure { status: 405, .. } => "method_rejected",
        Observation::Transport(code) => code,
        Observation::ResponseFailure { code, .. } => code,
        Observation::Http {
            status: 401,
            fingerprint: Fingerprint::Unauthorized,
        } => {
            if mtls {
                "mtls_subject_rejected"
            } else {
                "token_rejected"
            }
        }
        Observation::Http {
            status: 200..=299, ..
        } if post => "unexpected_write_response",
        _ => "route_unsupported",
    }
}

fn record_positive(
    positive: Observation,
    stages: &mut Niks3ConnectionTestResult,
    status_code: &mut Option<u16>,
) {
    if let Observation::ResponseFailure { status, .. } = positive {
        stages.server_reachable = true;
        *status_code = Some(status);
    }
    if let Observation::Http { status, .. } = positive {
        stages.server_reachable = true;
        *status_code = Some(status);
        // Public cache-config metadata owns API reachability. Even a native
        // auth error or accepted protected response cannot establish that field.
    }
}

async fn probe_with_clients(
    selected: &reqwest::Client,
    control: &reqwest::Client,
    auth: &Niks3WriteAuth,
    server: &Url,
    deadline: Instant,
    stages: &mut Niks3ConnectionTestResult,
    status_code: &mut Option<u16>,
) -> Result<&'static str, String> {
    let mtls = matches!(auth, Niks3WriteAuth::Mtls { .. });
    let gc = server
        .join("api/gc/status")
        .map_err(|_| "route_unsupported")?;
    let positive = observe(
        protected_request(selected, gc.clone(), false, Some(auth)),
        deadline,
    )
    .await;
    record_positive(positive, stages, status_code);
    let negative = observe(protected_request(control, gc, false, None), deadline).await;
    if !matches!(
        positive,
        Observation::Http {
            fingerprint: Fingerprint::GcSnapshot,
            ..
        }
    ) {
        let code = rejection(positive, mtls, false);
        if matches!(code, "token_rejected" | "mtls_subject_rejected")
            || (mtls && positive == Observation::Transport("client_identity_invalid"))
        {
            stages.write_authn_valid = Some(false);
            stages.write_authorization_valid = Some(false);
            stages.write_auth_valid = Some(false);
        } else if code == "proxy_access_denied" {
            stages.write_authorization_valid = Some(false);
            stages.write_auth_valid = Some(false);
        }
        return Err(code.into());
    }
    if !rejected_control(negative, mtls) {
        return Err(control_failure(negative).into());
    }
    stages.write_authn_valid = Some(true);
    let mut certificate_gate = certificate_control(negative);

    // INVARIANT: The invalid POST is unreachable until both protected GET
    // evidence and the omission gate match. Never use a valid closure or retry.
    let pending = server
        .join("api/pending_closures")
        .map_err(|_| "route_unsupported")?;
    let positive = observe(
        protected_request(selected, pending.clone(), true, Some(auth)),
        deadline,
    )
    .await;
    record_positive(positive, stages, status_code);
    // An unexpected successful invalid POST could indicate a changed contract.
    // Stop immediately; even a second unauthenticated invalid POST is unnecessary.
    if matches!(
        positive,
        Observation::Http {
            status: 200..=299,
            ..
        } | Observation::ResponseFailure {
            status: 200..=299,
            ..
        }
    ) {
        return Err("unexpected_write_response".into());
    }
    let negative = observe(protected_request(control, pending, true, None), deadline).await;
    if !matches!(
        positive,
        Observation::Http {
            status: 400,
            fingerprint: Fingerprint::MissingClosure
        }
    ) {
        let code = rejection(positive, mtls, true);
        if matches!(
            code,
            "token_rejected" | "mtls_subject_rejected" | "proxy_access_denied"
        ) {
            stages.write_authorization_valid = Some(false);
            stages.write_auth_valid = Some(false);
        }
        return Err(code.into());
    }
    if !rejected_control(negative, mtls) {
        return Err(control_failure(negative).into());
    }
    certificate_gate |= certificate_control(negative);
    stages.write_authorization_valid = Some(true);
    stages.write_auth_valid = Some(true);
    Ok(if certificate_gate {
        "write_route_verified_certificate_gate"
    } else {
        "write_route_verified"
    })
}

fn control_failure(control: Observation) -> &'static str {
    match control {
        Observation::Transport(code) => code,
        Observation::ResponseFailure { code, .. } => code,
        _ => "auth_control_inconclusive",
    }
}

#[cfg(test)]
#[path = "niks3_write_probe_tests.rs"]
mod tests;
