use super::*;
use axum::{Router, body::Body, extract::State, response::Response};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

fn snapshot() -> serde_json::Value {
    serde_json::json!({
        "state": "running", "phase": "cleanup_pending_uploads",
        "params": {"older_than": "24h", "failed_uploads_older_than": "6h", "force": false},
        "stats": {"failed_uploads_deleted": 0, "old_closures_deleted": 0,
            "objects_marked_for_deletion": 0, "objects_deleted_after_grace_period": 0,
            "objects_failed_to_delete": 0},
        "started_at": "2026-10-12T10:00:00.123456789Z",
        "updated_at": "2026-10-12T10:00:01Z"
    })
}

#[test]
fn native_gc_schema_rejects_generic_json_and_invalid_typed_fields() {
    let good = snapshot();
    assert!(valid_gc_snapshot(&serde_json::to_vec(&good).unwrap()));
    for (pointer, value) in [
        ("/state", serde_json::json!("idle")),
        ("/phase", serde_json::json!("delete_everything")),
        ("/stats/old_closures_deleted", serde_json::json!(-1)),
        ("/stats/old_closures_deleted", serde_json::json!(0.5)),
        ("/stats/old_closures_deleted", serde_json::json!("0")),
        ("/params/force", serde_json::json!("false")),
        ("/params/older_than", serde_json::json!(null)),
        ("/started_at", serde_json::json!("not a timestamp")),
        ("/updated_at", serde_json::json!("2026-10-11T10:00:00Z")),
    ] {
        let mut candidate = good.clone();
        *candidate.pointer_mut(pointer).unwrap() = value;
        assert!(
            !valid_gc_snapshot(&serde_json::to_vec(&candidate).unwrap()),
            "{pointer}"
        );
    }
    for field in ["params", "stats", "started_at", "updated_at", "state"] {
        let mut candidate = good.clone();
        candidate.as_object_mut().unwrap().remove(field);
        assert!(!valid_gc_snapshot(&serde_json::to_vec(&candidate).unwrap()));
    }
    let mut future = good.clone();
    future["unknown_capability"] = serde_json::json!(true);
    assert!(!valid_gc_snapshot(&serde_json::to_vec(&future).unwrap()));
    assert!(!valid_gc_snapshot(br#"{"ok":true,"state":"running"}"#));
    for state in ["succeeded", "failed"] {
        let mut finished = good.clone();
        finished["state"] = serde_json::json!(state);
        finished["finished_at"] = finished["updated_at"].clone();
        if state == "succeeded" {
            finished.as_object_mut().unwrap().remove("phase");
        } else {
            finished["error"] = serde_json::json!("PRIVATE_PROVIDER_ERROR_SENTINEL");
        }
        assert!(valid_gc_snapshot(&serde_json::to_vec(&finished).unwrap()));
        finished["finished_at"] = serde_json::json!("2026-10-13T10:00:00Z");
        assert!(!valid_gc_snapshot(&serde_json::to_vec(&finished).unwrap()));
    }
}

#[test]
fn exact_status_media_type_and_body_are_required() {
    assert_eq!(
        fingerprint(404, Some("text/plain; charset=utf-8"), NO_GC),
        Fingerprint::GcSnapshot
    );
    assert_eq!(
        fingerprint(400, Some("text/plain; charset=utf-8"), MISSING_CLOSURE),
        Fingerprint::MissingClosure
    );
    assert_eq!(
        fingerprint(401, Some("text/plain; charset=utf-8"), UNAUTHORIZED),
        Fingerprint::Unauthorized
    );
    for (status, media, body) in [
        (
            400,
            Some("text/plain; charset=utf-8"),
            b"Bad Request\n".as_slice(),
        ),
        (403, Some("text/plain; charset=utf-8"), UNAUTHORIZED),
        (
            401,
            Some("application/json"),
            br#"{"error":"Unauthorized"}"#.as_slice(),
        ),
        (404, Some("text/html"), NO_GC),
        (404, None, NO_GC),
        (400, Some("application/json"), MISSING_CLOSURE),
        (
            404,
            Some("text/plain; charset=utf-8"),
            b"no garbage collection has run yet".as_slice(),
        ),
        (200, Some("text/plain; charset=utf-8"), MISSING_CLOSURE),
    ] {
        assert_eq!(fingerprint(status, media, body), Fingerprint::Unknown);
    }
}

#[test]
fn mtls_control_requires_a_specific_identity_alert_not_any_transport_failure() {
    for code in [
        "probe_timeout",
        "dns_or_connection_failed",
        "tls_verification_failed",
        "probe_failed",
    ] {
        assert!(!rejected_control(Observation::Transport(code), true));
    }
    assert!(rejected_control(
        Observation::Transport("client_identity_invalid"),
        true
    ));
    assert!(!rejected_control(
        Observation::Transport("client_identity_invalid"),
        false
    ));
    assert!(!rejected_control(
        Observation::ResponseFailure {
            status: 200,
            code: "client_identity_invalid"
        },
        true
    ));
    assert!(!rejected_control(
        Observation::Http {
            status: 403,
            fingerprint: Fingerprint::Unknown
        },
        true
    ));
}

fn nginx_required(footer: &str) -> Vec<u8> {
    format!(
        "<html>\r\n<head><title>400 No required SSL certificate was sent</title></head>\r\n\
         <body>\r\n<center><h1>400 Bad Request</h1></center>\r\n\
         <center>No required SSL certificate was sent</center>\r\n\
         <hr><center>{footer}</center>\r\n</body>\r\n</html>\r\n"
    )
    .into_bytes()
}

#[test]
fn nginx_omission_requires_complete_page_and_only_opens_mtls_control() {
    for footer in ["nginx", "nginx/1.30.0"] {
        let body = nginx_required(footer);
        let observed = fingerprint(400, Some("text/html"), &body);
        assert_eq!(observed, Fingerprint::CertificateRequired);
        let control = Observation::Http {
            status: 400,
            fingerprint: observed,
        };
        assert!(rejected_control(control, true));
        assert!(!rejected_control(control, false));
        for (status, media) in [
            (401, Some("text/html")),
            (403, Some("text/html")),
            (400, Some("text/plain; charset=utf-8")),
            (400, Some("text/html; charset=utf-8")),
            (400, None),
        ] {
            assert_eq!(fingerprint(status, media, &body), Fingerprint::Unknown);
        }
        for candidate in [
            body[..body.len() - 1].to_vec(),
            [body.as_slice(), b"extra"].concat(),
            [b"prefix", body.as_slice()].concat(),
            String::from_utf8(body.clone())
                .unwrap()
                .replace(
                    "No required SSL certificate was sent",
                    "The SSL certificate error",
                )
                .into_bytes(),
            String::from_utf8(body)
                .unwrap()
                .replace("\r\n", "\n")
                .into_bytes(),
        ] {
            assert_eq!(
                fingerprint(400, Some("text/html"), &candidate),
                Fingerprint::Unknown
            );
        }
    }
    for footer in [
        "other",
        "nginx/1.30",
        "nginx/1.30.0.extra",
        "nginx/1234.1.1",
        "nginx/1.x.1",
        "nginx/1..1",
    ] {
        assert!(!nginx_certificate_required(&nginx_required(footer)));
    }
    assert_eq!(
        fingerprint(400, Some("text/html"), b"<html>Bad Request</html>"),
        Fingerprint::Unknown
    );
}

#[tokio::test]
async fn nginx_ingress_control_still_requires_both_native_handler_fingerprints() {
    let denial = Reply {
        status: 400,
        media: "text/html",
        body: nginx_required("nginx"),
    };
    let auth = Niks3WriteAuth::Mtls {
        client_certificate: "unused fake transport certificate".into(),
        client_private_key: "unused fake transport key".into(),
        ca_certificate: None,
    };
    for (get, post, expected, count) in [
        (
            native_get(),
            native_post(),
            Ok("write_route_verified_certificate_gate"),
            4,
        ),
        (
            reply(404, b"Not Found\n"),
            native_post(),
            Err("route_unsupported"),
            2,
        ),
        (
            native_get(),
            reply(400, b"Bad Request\n"),
            Err("route_unsupported"),
            4,
        ),
        (native_get(), denial.clone(), Err("route_unsupported"), 4),
    ] {
        let fake = fake(vec![get, denial.clone(), post, denial.clone()]).await;
        let mut stages = Niks3ConnectionTestResult::default();
        let mut status = None;
        let result = probe_with_clients(
            &fake.client,
            &fake.client,
            &auth,
            &fake.url,
            Instant::now() + Duration::from_secs(2),
            &mut stages,
            &mut status,
        )
        .await;
        assert_eq!(
            result
                .as_ref()
                .map(|value| *value)
                .map_err(|error| error.as_str()),
            expected
        );
        assert_eq!(
            stages.write_authorization_valid,
            expected.is_ok().then_some(true)
        );
        assert_eq!(stages.write_auth_valid, expected.is_ok().then_some(true));
        let fixture = fake.state.lock().unwrap();
        assert_eq!(fixture.requests.len(), count);
        assert!(
            fixture
                .requests
                .iter()
                .all(|request| !request.authorization_present)
        );
        assert!(
            fixture
                .requests
                .iter()
                .filter(|request| request.method == "POST")
                .all(|request| request.body == b"{}")
        );
    }
    let fake = fake(vec![native_get(), denial]).await;
    let (result, stages, _) = probe(&fake).await;
    assert_eq!(result.unwrap_err(), "auth_control_inconclusive");
    assert_eq!(stages.write_authn_valid, None);
    assert_eq!(fake.state.lock().unwrap().requests.len(), 2);
}

#[derive(Clone)]
struct Reply {
    status: u16,
    media: &'static str,
    body: Vec<u8>,
}
fn reply(status: u16, body: &[u8]) -> Reply {
    Reply {
        status,
        media: "text/plain; charset=utf-8",
        body: body.to_vec(),
    }
}
fn native_get() -> Reply {
    reply(404, NO_GC)
}
fn unauthorized() -> Reply {
    reply(401, UNAUTHORIZED)
}
fn native_post() -> Reply {
    reply(400, MISSING_CLOSURE)
}

struct Recorded {
    path: String,
    method: String,
    bearer: bool,
    authorization_present: bool,
    body: Vec<u8>,
}
struct Fixture {
    replies: VecDeque<Reply>,
    requests: Vec<Recorded>,
}
struct Fake {
    url: Url,
    client: reqwest::Client,
    state: Arc<Mutex<Fixture>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Fake {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve(
    State(state): State<Arc<Mutex<Fixture>>>,
    request: axum::extract::Request,
) -> Response {
    let path = request.uri().path().to_owned();
    let method = request.method().to_string();
    let bearer = request
        .headers()
        .get("authorization")
        .is_some_and(|value| value == "Bearer synthetic-probe-token");
    let authorization_present = request.headers().contains_key("authorization");
    let body = axum::body::to_bytes(request.into_body(), 128)
        .await
        .unwrap()
        .to_vec();
    let mut fixture = state.lock().unwrap();
    fixture.requests.push(Recorded {
        path,
        method,
        bearer,
        authorization_present,
        body,
    });
    let reply = fixture
        .replies
        .pop_front()
        .expect("Unexpected additional probe request");
    Response::builder()
        .status(reply.status)
        .header("content-type", reply.media)
        .body(Body::from(reply.body))
        .unwrap()
}

async fn fake(replies: Vec<Reply>) -> Fake {
    let state = Arc::new(Mutex::new(Fixture {
        replies: replies.into(),
        requests: Vec::new(),
    }));
    let app = Router::new().fallback(serve).with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = Url::parse(&format!(
        "http://{}/prefix/",
        listener.local_addr().unwrap()
    ))
    .unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Fake {
        url,
        client: reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap(),
        state,
        task,
    }
}

async fn probe(
    fake: &Fake,
) -> (
    Result<&'static str, String>,
    Niks3ConnectionTestResult,
    Option<u16>,
) {
    let mut stages = Niks3ConnectionTestResult::default();
    let mut status = None;
    let auth = Niks3WriteAuth::Token {
        token: "synthetic-probe-token".into(),
    };
    let result = probe_with_clients(
        &fake.client,
        &fake.client,
        &auth,
        &fake.url,
        Instant::now() + Duration::from_secs(2),
        &mut stages,
        &mut status,
    )
    .await;
    (result, stages, status)
}

#[tokio::test]
async fn native_token_route_pair_proves_access_and_sends_only_invalid_bodies() {
    let fake = fake(vec![
        native_get(),
        unauthorized(),
        native_post(),
        unauthorized(),
    ])
    .await;
    let (result, stages, status) = probe(&fake).await;
    assert_eq!(result.unwrap(), "write_route_verified");
    assert_eq!(status, Some(400));
    assert!(stages.server_reachable);
    // These four protected requests contain no public metadata observation.
    // Even accepted native auth must not claim the separate API-reachability UI.
    assert_eq!(stages.write_api_reachable, None);
    assert_eq!(stages.write_authn_valid, Some(true));
    assert_eq!(stages.write_authorization_valid, Some(true));
    assert_eq!(stages.write_auth_valid, Some(true));
    assert!(!stages.discovery_valid && !stages.read_endpoint_reachable);
    let fixture = fake.state.lock().unwrap();
    assert_eq!(fixture.requests.len(), 4);
    for (index, request) in fixture.requests.iter().enumerate() {
        assert_eq!(request.bearer, index % 2 == 0);
        assert_eq!(request.authorization_present, index % 2 == 0);
        assert_eq!(request.method, if index < 2 { "GET" } else { "POST" });
        assert_eq!(
            request.path,
            if index < 2 {
                "/prefix/api/gc/status"
            } else {
                "/prefix/api/pending_closures"
            }
        );
        assert_eq!(
            request.body,
            if index < 2 {
                b"".as_slice()
            } else {
                b"{}".as_slice()
            }
        );
    }
    let serialized = serde_json::to_string(&stages).unwrap();
    assert!(!serialized.contains("synthetic-probe-token"));
}

#[tokio::test]
async fn no_post_follows_unknown_get_or_non_discriminating_authentication_control() {
    for replies in [
        vec![reply(400, b"Bad Request\n"), unauthorized()],
        vec![reply(403, b"Forbidden\n"), unauthorized()],
        vec![reply(405, b"Method Not Allowed\n"), unauthorized()],
        vec![reply(404, b"Not Found\n"), unauthorized()],
        vec![
            Reply {
                status: 200,
                media: "application/json",
                body: br#"{"ok":true}"#.to_vec(),
            },
            unauthorized(),
        ],
        vec![native_get(), native_get()],
        vec![native_get(), reply(400, b"Bad Request\n")],
        vec![
            native_get(),
            Reply {
                status: 401,
                media: "application/json",
                body: br#"{"error":"Unauthorized"}"#.to_vec(),
            },
        ],
    ] {
        let fake = fake(replies).await;
        let (result, stages, _) = probe(&fake).await;
        assert!(result.is_err());
        assert_ne!(stages.write_authn_valid, Some(true));
        assert_ne!(stages.write_authorization_valid, Some(true));
        assert_eq!(fake.state.lock().unwrap().requests.len(), 2);
    }
}

#[tokio::test]
async fn definite_native_token_denial_is_false_not_untested() {
    let fake = fake(vec![unauthorized(), unauthorized()]).await;
    let (result, stages, _) = probe(&fake).await;
    assert_eq!(result.unwrap_err(), "token_rejected");
    assert_eq!(stages.write_authn_valid, Some(false));
    assert_eq!(stages.write_authorization_valid, Some(false));
    assert_eq!(stages.write_auth_valid, Some(false));
}

#[tokio::test]
async fn strict_gc_json_accepts_finished_failure_without_exposing_provider_error() {
    let mut snapshot = snapshot();
    snapshot["state"] = serde_json::json!("failed");
    snapshot["finished_at"] = snapshot["updated_at"].clone();
    snapshot["error"] = serde_json::json!("PRIVATE_PROVIDER_ERROR_SENTINEL");
    let fake = fake(vec![
        Reply {
            status: 200,
            media: "application/json",
            body: serde_json::to_vec(&snapshot).unwrap(),
        },
        unauthorized(),
        native_post(),
        unauthorized(),
    ])
    .await;
    let (result, stages, _) = probe(&fake).await;
    assert_eq!(result.unwrap(), "write_route_verified");
    assert_eq!(stages.write_authn_valid, Some(true));
    assert!(
        !serde_json::to_string(&stages)
            .unwrap()
            .contains("PRIVATE_PROVIDER_ERROR_SENTINEL")
    );
}

#[tokio::test]
async fn mtls_route_requests_send_no_bearer_and_native_subject_denial_is_false() {
    // This fake HTTP test covers mode selection and route/control choreography.
    // Real certificate verification and native subject matching belong to the
    // owner's native fixture, not to this scheduled-response server.
    let auth = Niks3WriteAuth::Mtls {
        client_certificate: "unused fake transport certificate".into(),
        client_private_key: "unused fake transport key".into(),
        ca_certificate: None,
    };
    for (replies, expected) in [
        (
            vec![native_get(), unauthorized(), native_post(), unauthorized()],
            Some(true),
        ),
        (vec![unauthorized(), unauthorized()], Some(false)),
    ] {
        let fake = fake(replies).await;
        let mut stages = Niks3ConnectionTestResult::default();
        let mut status = None;
        let result = probe_with_clients(
            &fake.client,
            &fake.client,
            &auth,
            &fake.url,
            Instant::now() + Duration::from_secs(2),
            &mut stages,
            &mut status,
        )
        .await;
        assert_eq!(stages.write_authn_valid, expected);
        assert_eq!(stages.write_authorization_valid, expected);
        if expected == Some(false) {
            assert_eq!(result.unwrap_err(), "mtls_subject_rejected");
        } else {
            assert_eq!(result.unwrap(), "write_route_verified");
        }
        assert!(
            fake.state
                .lock()
                .unwrap()
                .requests
                .iter()
                .all(|request| !request.authorization_present)
        );
    }
}

#[test]
fn retained_and_replacement_effective_snapshots_select_only_active_write_credentials() {
    use crate::models::cache_destination::{CacheDestination, UpdateCacheDestination};
    use crate::queries::cache_destinations::effective_update_unvalidated;
    let current = CacheDestination {
        cache_type: "Niks3".into(),
        name: "probe fixture".into(),
        niks3_server_url: Some("https://write.example/".into()),
        niks3_write_auth_mode: Some("token".into()),
        niks3_auth_token: Some("retained synthetic token".into()),
        ..Default::default()
    };
    let effective =
        effective_update_unvalidated(&current, &UpdateCacheDestination::default()).unwrap();
    assert_eq!(
        selected_auth(&effective).unwrap(),
        Niks3WriteAuth::Token {
            token: "retained synthetic token".into()
        }
    );
    let replacement = effective_update_unvalidated(
        &current,
        &UpdateCacheDestination {
            niks3_auth_token: Some("replacement synthetic token".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        selected_auth(&replacement).unwrap(),
        Niks3WriteAuth::Token {
            token: "replacement synthetic token".into()
        }
    );
    let replacement = effective_update_unvalidated(
        &current,
        &UpdateCacheDestination {
            niks3_write_auth_mode: Some("mtls".into()),
            niks3_write_client_cert: Some("replacement cert".into()),
            niks3_write_client_key: Some("replacement key".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        selected_auth(&replacement).unwrap(),
        Niks3WriteAuth::Mtls { .. }
    ));
    assert!(replacement.niks3_auth_token.is_none());
    assert_eq!(
        current.niks3_auth_token.as_deref(),
        Some("retained synthetic token")
    );
}

#[tokio::test]
async fn missing_or_invalid_write_token_reports_local_failure_without_dns_or_read_borrowing() {
    use super::super::{CacheProbeDetails, Niks3ProbeScope, run_niks3_test_scope};
    for (token, expected) in [
        (None, "write_credentials_missing"),
        (Some(""), "write_credentials_missing"),
        (Some(" \t"), "write_credentials_missing"),
        (Some("invalid\r\nheader"), "write_credentials_invalid"),
    ] {
        let create = CreateCacheDestination {
            cache_type: "Niks3".into(),
            niks3_server_url: Some("https://must-not-resolve.invalid/".into()),
            niks3_write_auth_mode: Some("token".into()),
            niks3_auth_token: token.map(str::to_owned),
            niks3_read_auth_mode: Some("basic".into()),
            niks3_read_basic_username: Some("unused read identity".into()),
            niks3_read_basic_password: Some("unused read credential".into()),
            ..Default::default()
        };
        // Write-only validation permits absent tokens for historical metadata
        // compatibility. The actual protected probe must reject locally instead.
        create.validate_probe(Niks3ProbeScope::Write).unwrap();
        let result = run_niks3_test_scope(&create, false, Niks3ProbeScope::Write)
            .await
            .unwrap();
        assert!(!result.ok);
        assert_eq!(result.status_code, None);
        assert_eq!(result.tested_url, None);
        assert!(!result.message.contains("API rejected"));
        let wire = serde_json::to_string(&result).unwrap();
        assert!(!wire.contains("unused read credential"));
        assert!(!wire.contains("must-not-resolve.invalid"));
        let Some(CacheProbeDetails::Niks3(stages)) = result.niks3 else {
            panic!("Missing Niks3 evidence")
        };
        assert_eq!(stages.diagnostic_code, Some(expected));
        assert_eq!(stages.write_probe_diagnostic_code, Some(expected));
        assert!(!stages.server_reachable && !stages.discovery_valid);
        assert_eq!(stages.write_api_reachable, None);
        assert_eq!(stages.write_authn_valid, Some(false));
        assert_eq!(stages.write_authorization_valid, Some(false));
        assert_eq!(stages.write_auth_valid, Some(false));
        assert_eq!(stages.read_access_valid, None);
    }
}

#[tokio::test]
async fn all_scope_invalid_bearer_preflight_precedes_metadata_dns_and_read_requests() {
    use super::super::{CacheProbeDetails, Niks3ProbeScope, run_niks3_test_scope};
    use futures::FutureExt;
    let create = CreateCacheDestination {
        name: "invalid header fixture".into(),
        cache_type: "Niks3".into(),
        niks3_server_url: Some("https://must-not-resolve.invalid/".into()),
        push_to: Some("https://also-must-not-resolve.invalid/".into()),
        niks3_public_keys: vec![crate::models::cache_destination::nix_public_key_fixture(
            "fixture",
        )],
        niks3_write_auth_mode: Some("token".into()),
        niks3_auth_token: Some("nonempty\r\ninvalid-header".into()),
        niks3_read_auth_mode: Some("none".into()),
        ..Default::default()
    };
    // Full Save validation currently accepts this nonempty token. Runtime
    // preflight must still reject its HTTP representation before any DNS work.
    create.validate_probe(Niks3ProbeScope::All).unwrap();
    let result = run_niks3_test_scope(&create, false, Niks3ProbeScope::All)
        .now_or_never()
        .expect("Local header rejection must complete before any asynchronous lookup or request")
        .unwrap();
    assert!(!result.ok);
    assert_eq!(result.status_code, None);
    assert_eq!(result.tested_url, None);
    let Some(CacheProbeDetails::Niks3(stages)) = result.niks3 else {
        panic!("Missing Niks3 evidence")
    };
    assert_eq!(stages.diagnostic_code, Some("write_credentials_invalid"));
    assert_eq!(
        stages.write_probe_diagnostic_code,
        Some("write_credentials_invalid")
    );
    assert_eq!(stages.write_api_reachable, None);
    assert_eq!(stages.write_authn_valid, Some(false));
    assert_eq!(stages.write_authorization_valid, Some(false));
    assert_eq!(stages.write_auth_valid, Some(false));
    assert!(!stages.server_reachable && !stages.discovery_valid && !stages.read_endpoint_reachable);
    assert_eq!(stages.read_access_valid, None);
}

#[tokio::test]
async fn niks3_read_custom_ca_replaces_ambient_roots_and_invalid_bundles_do_not_fallback() {
    use super::super::{
        RootPolicy, cache_test_client_pinned_policy, niks3_read_root_policy, system_ca_source,
    };
    use crate::security::cache_secrets::TEST_CERTIFICATE;
    let explicit = niks3_read_root_policy(Some(TEST_CERTIFICATE));
    assert!(explicit == RootPolicy::ReplaceCustom);
    assert!(system_ca_source(explicit).is_none());
    assert!(niks3_read_root_policy(None) == RootPolicy::ExtendSystem);
    assert!(system_ca_source(niks3_read_root_policy(None)).is_some());
    let url = Url::parse("https://read.example/").unwrap();
    let addresses = ["93.184.216.34:443".parse().unwrap()];
    // Builder compatibility only: constructing these clients opens no socket.
    // The native owner supplies the A-only-versus-ambient-B handshake proof.
    let client = cache_test_client_pinned_policy(
        &url,
        &addresses,
        None,
        None,
        Some(TEST_CERTIFICATE),
        explicit,
    )
    .await
    .unwrap();
    let request = client.get(url.clone()).build().unwrap();
    assert!(!request.headers().contains_key("authorization"));
    assert!(
        cache_test_client_pinned_policy(
            &url,
            &addresses,
            None,
            None,
            Some("invalid CA"),
            niks3_read_root_policy(Some("invalid CA"))
        )
        .await
        .is_err()
    );
    assert!(
        cache_test_client_pinned_policy(
            &url,
            &addresses,
            None,
            None,
            Some(""),
            niks3_read_root_policy(Some(""))
        )
        .await
        .is_err()
    );
}

fn native_metadata() -> Reply {
    Reply {
        status: 200,
        media: "application/json",
        body: serde_json::to_vec(&serde_json::json!({
            "substituter_url": "https://93.184.216.34/",
            "public_keys": [crate::models::cache_destination::nix_public_key_fixture("native")]
        }))
        .unwrap(),
    }
}

#[tokio::test]
async fn write_metadata_reachability_is_independent_of_protected_authentication() {
    use super::super::{Niks3ProbeScope, probe_niks3_metadata};
    // Fake HTTP tests assert evidence/order only. Production selects verified
    // write TLS; this fixture does not establish a native TLS runtime result.
    for (metadata, protected, api_expected, auth_expected) in [
        (
            native_metadata(),
            vec![unauthorized(), unauthorized()],
            Some(true),
            Some(false),
        ),
        (
            native_metadata(),
            vec![native_get(), unauthorized(), native_post(), unauthorized()],
            Some(true),
            Some(true),
        ),
        // A public metadata-only fallback answers both GC requests with the
        // cache-config JSON. Reachability is real; authentication is unproven.
        (
            native_metadata(),
            vec![native_metadata(), native_metadata()],
            Some(true),
            None,
        ),
        (
            native_metadata(),
            vec![reply(400, b"Bad Request\n"), unauthorized()],
            Some(true),
            None,
        ),
        (
            Reply {
                status: 200,
                media: "application/json",
                body: br#"{"ok":true}"#.to_vec(),
            },
            vec![native_get(), unauthorized(), native_post(), unauthorized()],
            Some(false),
            Some(true),
        ),
        (
            Reply {
                media: "text/html",
                ..native_metadata()
            },
            vec![native_get(), unauthorized(), native_post(), unauthorized()],
            Some(false),
            Some(true),
        ),
        (
            Reply {
                status: 201,
                ..native_metadata()
            },
            vec![native_get(), unauthorized(), native_post(), unauthorized()],
            Some(false),
            Some(true),
        ),
    ] {
        let mut replies = vec![metadata];
        replies.extend(protected);
        let fake = fake(replies).await;
        let create = CreateCacheDestination {
            cache_type: "Niks3".into(),
            ..Default::default()
        };
        let mut stages = Niks3ConnectionTestResult::default();
        let mut status = None;
        let metadata_result = probe_niks3_metadata(
            &create,
            Niks3ProbeScope::Write,
            &fake.client,
            &fake.url,
            false,
            &mut stages,
            &mut status,
        )
        .await;
        assert_eq!(metadata_result.is_ok(), api_expected == Some(true));
        let auth = Niks3WriteAuth::Token {
            token: "synthetic-probe-token".into(),
        };
        let result = probe_with_clients(
            &fake.client,
            &fake.client,
            &auth,
            &fake.url,
            Instant::now() + Duration::from_secs(2),
            &mut stages,
            &mut status,
        )
        .await;
        assert_eq!(stages.write_api_reachable, api_expected);
        assert_eq!(stages.write_authn_valid, auth_expected);
        assert_eq!(stages.write_authorization_valid, auth_expected);
        assert_eq!(stages.write_auth_valid, auth_expected);
        assert_eq!(result.is_ok(), auth_expected == Some(true));
        let fixture = fake.state.lock().unwrap();
        assert_eq!(fixture.requests[0].path, "/prefix/api/cache-config");
        assert!(!fixture.requests[0].authorization_present);
        assert_eq!(
            fixture
                .requests
                .iter()
                .filter(|request| request.path.ends_with("cache-config"))
                .count(),
            1
        );
        assert_eq!(
            fixture.requests.len(),
            if auth_expected == Some(true) { 5 } else { 3 }
        );
        if auth_expected != Some(true) {
            assert!(
                fixture
                    .requests
                    .iter()
                    .all(|request| request.method != "POST")
            );
        }
    }
}

#[tokio::test]
async fn all_metadata_key_mismatch_keeps_api_reachable_and_does_not_skip_authentication() {
    use super::super::{Niks3ProbeScope, probe_niks3_metadata};
    let fake = fake(vec![
        native_metadata(),
        native_get(),
        unauthorized(),
        native_post(),
        unauthorized(),
    ])
    .await;
    let create = CreateCacheDestination {
        cache_type: "Niks3".into(),
        push_to: Some("https://93.184.216.34/".into()),
        niks3_public_keys: vec![crate::models::cache_destination::nix_public_key_fixture(
            "configured",
        )],
        ..Default::default()
    };
    let mut stages = Niks3ConnectionTestResult::default();
    let mut status = None;
    assert_eq!(
        probe_niks3_metadata(
            &create,
            Niks3ProbeScope::All,
            &fake.client,
            &fake.url,
            false,
            &mut stages,
            &mut status
        )
        .await
        .unwrap_err(),
        "Advertised signing keys do not match configured trust"
    );
    assert_eq!(stages.write_api_reachable, Some(true));
    assert!(!stages.discovery_valid);
    assert_eq!(stages.signing_keys_valid, Some(false));
    let auth = Niks3WriteAuth::Token {
        token: "synthetic-probe-token".into(),
    };
    assert_eq!(
        probe_with_clients(
            &fake.client,
            &fake.client,
            &auth,
            &fake.url,
            Instant::now() + Duration::from_secs(2),
            &mut stages,
            &mut status
        )
        .await
        .unwrap(),
        "write_route_verified"
    );
    assert_eq!(stages.write_authn_valid, Some(true));
    assert_eq!(stages.write_authorization_valid, Some(true));
    assert_eq!(stages.write_api_reachable, Some(true));
    assert_eq!(fake.state.lock().unwrap().requests.len(), 5);
}

#[test]
fn all_scope_missing_token_keeps_existing_local_validation_rejection() {
    use super::super::Niks3ProbeScope;
    let create = CreateCacheDestination {
        name: "local credential validation fixture".into(),
        cache_type: "Niks3".into(),
        niks3_server_url: Some("https://write.example/".into()),
        push_to: Some("https://read.example/".into()),
        niks3_public_keys: vec![crate::models::cache_destination::nix_public_key_fixture(
            "fixture",
        )],
        niks3_write_auth_mode: Some("token".into()),
        niks3_read_auth_mode: Some("none".into()),
        ..Default::default()
    };
    assert_eq!(
        create.validate_probe(Niks3ProbeScope::All).unwrap_err(),
        "niks3_write_auth_mode requires token with only a token, or mtls with cert/key"
    );
    // Discovery selects transport only and still accepts token mode without a
    // token. Protected API credential requirements must not alter that contract.
    let transport = super::super::niks3_write_transport(Some("token"), None, None, None).unwrap();
    assert!(transport.cert.is_none() && transport.key.is_none() && transport.ca.is_none());
}

#[tokio::test]
async fn generic_post_errors_and_contradictory_controls_never_verify_authorization() {
    for (post, control, expected) in [
        (reply(400, b"Bad Request\n"), unauthorized(), None),
        (reply(403, b"Forbidden\n"), unauthorized(), Some(false)),
        (reply(404, b"Not Found\n"), unauthorized(), None),
        (reply(405, b"Method Not Allowed\n"), unauthorized(), None),
        (reply(302, b"Redirect\n"), unauthorized(), None),
        (native_post(), native_post(), None),
        (native_post(), reply(400, b"Bad Request\n"), None),
    ] {
        let fake = fake(vec![native_get(), unauthorized(), post, control]).await;
        let (result, stages, _) = probe(&fake).await;
        assert!(result.is_err());
        assert_eq!(stages.write_authn_valid, Some(true));
        assert_eq!(stages.write_authorization_valid, expected);
        assert_eq!(stages.write_auth_valid, expected);
        assert_eq!(fake.state.lock().unwrap().requests.len(), 4);
    }
}

#[tokio::test]
async fn unexpected_success_stops_without_control_retry_or_valid_request() {
    for status in [200, 201, 204] {
        let fake = fake(vec![
            native_get(),
            unauthorized(),
            reply(status, b"unexpected"),
        ])
        .await;
        let (result, stages, _) = probe(&fake).await;
        assert_eq!(result.unwrap_err(), "unexpected_write_response");
        assert_eq!(stages.write_authorization_valid, None);
        let fixture = fake.state.lock().unwrap();
        assert_eq!(fixture.requests.len(), 3);
        assert_eq!(fixture.requests[2].body, b"{}");
    }
    let fake = fake(vec![
        native_get(),
        unauthorized(),
        reply(200, &vec![b'x'; MAX_RESPONSE_BYTES + 1]),
    ])
    .await;
    let (result, stages, _) = probe(&fake).await;
    assert_eq!(result.unwrap_err(), "unexpected_write_response");
    assert_eq!(stages.write_authorization_valid, None);
    assert_eq!(fake.state.lock().unwrap().requests.len(), 3);
}

#[test]
fn metadata_and_read_requests_do_not_receive_write_bearer() {
    let client = reqwest::Client::new();
    let url = Url::parse("https://cache.example/prefix/").unwrap();
    let auth = Niks3WriteAuth::Token {
        token: "synthetic-probe-token".into(),
    };
    let metadata = super::super::niks3_metadata_request(&client, &url)
        .unwrap()
        .build()
        .unwrap();
    assert!(metadata.headers().get("authorization").is_none());
    let create = CreateCacheDestination {
        cache_type: "Niks3".into(),
        niks3_auth_token: Some("synthetic-probe-token".into()),
        ..Default::default()
    };
    let read =
        super::super::niks3_read_request(&client, url.join("nix-cache-info").unwrap(), &create)
            .unwrap()
            .build()
            .unwrap();
    assert!(read.headers().get("authorization").is_none());
    let control = protected_request(&client, url.join("api/gc/status").unwrap(), false, None)
        .build()
        .unwrap();
    assert!(control.headers().get("authorization").is_none());
    let selected = protected_request(
        &client,
        url.join("api/gc/status").unwrap(),
        false,
        Some(&auth),
    )
    .build()
    .unwrap();
    assert!(selected.headers()["authorization"].is_sensitive());
}

#[tokio::test]
async fn chunked_bodies_are_bounded_and_a_shared_deadline_covers_stream_consumption() {
    let app = Router::new().fallback(|| async {
        let chunks = futures::stream::iter([
            Ok::<_, std::io::Error>(vec![b'x'; MAX_RESPONSE_BYTES]),
            Ok(vec![b'x']),
        ]);
        Response::builder()
            .status(200)
            .header("content-type", "application/json")
            .body(Body::from_stream(chunks))
            .unwrap()
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let result = observe(client.get(&url), Instant::now() + Duration::from_secs(2)).await;
    task.abort();
    assert_eq!(
        result,
        Observation::ResponseFailure {
            status: 200,
            code: "probe_response_too_large"
        }
    );

    let app = Router::new().fallback(|| async {
        let chunks = futures::stream::once(async {
            tokio::time::sleep(Duration::from_secs(2)).await;
            Ok::<_, std::io::Error>(b"partial".to_vec())
        });
        Response::builder()
            .status(200)
            .header("content-type", "application/json")
            .body(Body::from_stream(chunks))
            .unwrap()
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let deadline = Instant::now() + Duration::from_millis(50);
    assert!(matches!(
        observe(client.get(&url), deadline).await,
        Observation::Transport("probe_timeout")
            | Observation::ResponseFailure {
                code: "probe_timeout",
                ..
            }
    ));
    // Reusing the expired operation deadline cannot grant another request a new
    // body-stream budget, even when the HTTP client's own timeout is unlimited.
    assert_eq!(
        observe(client.get(&url), deadline).await,
        Observation::Transport("probe_timeout")
    );
    task.abort();
}
