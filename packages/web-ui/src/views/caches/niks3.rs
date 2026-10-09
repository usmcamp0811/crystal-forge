//! Dedicated Niks3 presentation and request projections.
//!
//! Write API transport and Nix read identity are independent. Stored secrets
//! never seed inputs. Discovery and scoped Test operate on snapshots only;
//! Save commits complete configuration and scope in one request.

use super::*;
use crate::api::models::{Niks3DiscoverRequest, Niks3Discovery, Niks3ProbeScope};
use base64::{Engine as _, engine::general_purpose::STANDARD};

const CSS: &str = r#"
.n3-cred{border:1px solid var(--cf-divider);border-radius:9px;padding:9px 12px;background:var(--cf-subtle-bg)}
.n3-cred-main{display:flex;align-items:center;justify-content:space-between;gap:10px;flex-wrap:wrap}
.n3-cred-state{display:inline-flex;align-items:center;gap:7px;font-size:12px;color:var(--cf-text-primary);flex-wrap:wrap}
.n3-cred-sub{font-size:11px;color:var(--cf-text-muted)}
.n3-cred-warn{margin-top:7px;font-size:11.5px;color:var(--cf-amber)}
.n3-plane{border:1px solid var(--cf-divider);border-radius:10px;overflow:hidden;margin-top:6px}
.n3-plane-head{display:flex;justify-content:space-between;align-items:center;gap:8px;padding:8px 12px;background:var(--cf-subtle-bg);border-bottom:1px solid var(--cf-divider);font-size:12px}
.n3-plane-row{display:flex;justify-content:space-between;align-items:center;gap:10px;padding:7px 12px;font-size:12px;color:var(--cf-text-secondary);border-top:1px solid var(--cf-divider)}
.n3-plane-head+.n3-plane-row{border-top:0}.n3-plane-row .chip{font-size:10px}
.n3-disc{margin-top:8px;padding:8px 10px;border-radius:8px;font-size:12px;line-height:1.5;border:1px solid var(--cf-divider);overflow-wrap:anywhere}
.n3-advanced{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:12px}
.n3-key-row{display:flex;gap:6px;min-width:0}.n3-key-row input{min-width:0;flex:1}
.n3-public-value{overflow-wrap:anywhere;white-space:pre-wrap;font-size:11.5px}
@media(max-width:600px){.n3-advanced{grid-template-columns:1fr}.n3-plane-head{flex-wrap:wrap}}
"#;

const CA_HELP: &str = "Trust bundle used by the client to verify remote HTTPS servers, not the CA validating your client certificate. Multiple CA certificates are supported.";
const WRITE_CA_HELP: &str = "The Niks3 client uses this bundle for the API and presigned HTTPS S3 targets. Its custom bundle replaces system roots; include public roots when those targets require them.";

#[derive(Clone, Copy, PartialEq)]
enum Choice {
    Current,
    Draft,
    None,
}

/// Checks the same wire-format constraints as the shared protocol validator.
/// This does not prove ownership or verify any artifact signature.
pub(super) fn valid_signing_key(value: &str) -> bool {
    value.split_once(':').is_some_and(|(name, encoded)| {
        !name.is_empty()
            && !value.chars().any(char::is_whitespace)
            && STANDARD
                .decode(encoded)
                .is_ok_and(|decoded| decoded.len() == 32)
    })
}

/// Validates a complete Basic pair without trimming password or username data.
/// ASCII control characters and username colons are prohibited by the server.
pub(super) fn validate_basic(user: &str, password: &str) -> Result<(), String> {
    if user.is_empty() || password.is_empty() {
        return Err("Enter a Basic username and password together.".into());
    }
    if user.contains(':')
        || user.chars().any(|c| c.is_ascii_control())
        || password.chars().any(|c| c.is_ascii_control())
    {
        return Err(
            "Basic credentials must not contain control characters or a username colon.".into(),
        );
    }
    Ok(())
}

fn https(value: &str) -> bool {
    web_sys::Url::new(value.trim()).is_ok_and(|url| {
        url.protocol() == "https:"
            && !url.hostname().is_empty()
            && url.username().is_empty()
            && url.password().is_empty()
            && url.search().is_empty()
            && url.hash().is_empty()
    })
}

fn same_authority(old: &str, new: &str) -> bool {
    web_sys::Url::new(old)
        .ok()
        .zip(web_sys::Url::new(new).ok())
        .is_some_and(|(old, new)| {
            old.protocol() == new.protocol()
                && old.hostname() == new.hostname()
                && old.port() == new.port()
        })
}

// Empty, fractional and overflowing input must block Save rather than silently
// persisting the last successfully parsed value behind the visible draft.
fn validate_advanced_input(parallel: &str, retries: &str, timeout: &str) -> Result<(), String> {
    if parallel.parse::<i32>().is_ok_and(|value| value > 0)
        && retries.parse::<i32>().is_ok_and(|value| value >= 0)
        && timeout.parse::<i64>().is_ok_and(|value| value > 0)
    {
        Ok(())
    } else {
        Err("Enter whole-number Advanced values: positive parallel uploads and timeout, and nonnegative retries.".into())
    }
}

fn clear_identity(form: &mut Niks3FormState, write: bool) {
    if write {
        form.token.clear();
        form.write_cert.clear();
        form.write_key.clear();
        form.write_ca.clear();
        form.clear_write_ca = false;
    } else {
        form.read_cert.clear();
        form.read_key.clear();
        form.read_ca.clear();
        form.clear_read_ca = false;
        form.basic_user.clear();
        form.basic_password.clear();
    }
}

fn configured(write: bool, form: &Niks3FormState, original: Option<&CacheDestination>) -> bool {
    original.is_some_and(|d| {
        if write {
            d.niks3_write_auth_mode.as_deref() == Some(form.write_mode.as_str())
                && if form.write_mode == "token" {
                    d.niks3_write_token_configured
                } else {
                    d.niks3_write_mtls_configured
                }
        } else {
            d.niks3_read_auth_mode.as_deref() == Some(form.read_mode.as_str())
                && match form.read_mode.as_str() {
                    "basic" => d.niks3_read_basic_configured,
                    "mtls" => d.niks3_read_mtls_configured,
                    _ => false,
                }
        }
    })
}

fn pair(cert: &str, key: &str, ca: &str, require: bool) -> Result<(), String> {
    let entered = !cert.trim().is_empty() || !key.trim().is_empty();
    if require || entered {
        if cert.trim().is_empty() || key.trim().is_empty() {
            return Err("Enter a client certificate and private key together.".into());
        }
        if !cert.contains("-----BEGIN CERTIFICATE-----") || !key.contains("PRIVATE KEY-----") {
            return Err("Client identity must contain a PEM certificate and private key.".into());
        }
    }
    if !ca.trim().is_empty() && !ca.contains("-----BEGIN CERTIFICATE-----") {
        return Err("Server CA bundle requires certificate-only PEM.".into());
    }
    Ok(())
}

fn validate_plane(
    form: &Niks3FormState,
    scope: Niks3ProbeScope,
    existing: bool,
) -> Result<(), String> {
    if scope == Niks3ProbeScope::Write {
        if !https(&form.server_url) {
            return Err(
                "Enter an HTTPS Write / API URL without userinfo, query or fragment.".into(),
            );
        }
        match form.write_mode.as_str() {
            "token" => Ok(()), // Public metadata never receives a bearer token.
            "mtls" => pair(&form.write_cert, &form.write_key, &form.write_ca, !existing),
            _ => Err("Select API token or mTLS write authentication.".into()),
        }
    } else {
        if !https(&form.read_url) {
            return Err(
                "Enter an HTTPS Read / substituter URL without userinfo, query or fragment.".into(),
            );
        }
        if form
            .request()
            .niks3_public_keys
            .iter()
            .any(|key| !valid_signing_key(key))
        {
            return Err(
                "Signing keys require name:standard-base64 with exactly 32 decoded bytes.".into(),
            );
        }
        match form.read_mode.as_str() {
            "none" => Ok(()),
            "basic"
                if !existing || !form.basic_user.is_empty() || !form.basic_password.is_empty() =>
            {
                validate_basic(&form.basic_user, &form.basic_password)
            }
            "basic" => Ok(()), // Stored-ID authority does not depend on configured flags.
            "mtls" => pair(&form.read_cert, &form.read_key, &form.read_ca, !existing),
            _ => Err("Select Public, Basic or mTLS read authentication.".into()),
        }
    }
}

fn discovery_request(form: &Niks3FormState) -> Niks3DiscoverRequest {
    let req = form.request();
    Niks3DiscoverRequest {
        server_url: form.server_url.trim().into(),
        niks3_write_auth_mode: (form.write_mode == "mtls").then(|| "mtls".into()),
        niks3_write_client_cert: req.niks3_write_client_cert,
        niks3_write_client_key: req.niks3_write_client_key,
        niks3_write_ca_cert: req.niks3_write_ca_cert,
    }
}

fn validate_save(
    form: &Niks3FormState,
    original: Option<&CacheDestination>,
    write: Choice,
    read: Choice,
) -> Result<(), String> {
    form.validate_destination()?;
    if form.parallel_uploads < 1 || form.max_retries < 0 || form.push_timeout_seconds < 1 {
        return Err("Advanced values require positive parallel uploads and timeout, and nonnegative retries.".into());
    }
    for plane in [true, false] {
        let mode = if plane {
            form.write_mode.as_str()
        } else {
            form.read_mode.as_str()
        };
        let choice = if plane { write } else { read };
        let retained = choice == Choice::Current
            && original.is_some_and(|d| {
                if plane {
                    d.niks3_write_auth_mode.as_deref() == Some(mode)
                } else {
                    d.niks3_read_auth_mode.as_deref() == Some(mode)
                }
            });
        if retained {
            if !plane
                && mode == "basic"
                && original
                    .and_then(|d| d.push_to.as_deref())
                    .is_some_and(|old| !same_authority(old, &form.read_url))
            {
                return Err("Read authority changed. Enter a complete Basic replacement or select Public; stored Basic credentials cannot be forwarded.".into());
            }
            continue; // Server persistence validates the retained encrypted identity.
        }
        match (plane, mode) {
            (true, "token") if !form.token.trim().is_empty() => {}
            (false, "none") => {}
            (false, "basic") => validate_basic(&form.basic_user, &form.basic_password)?,
            (true, "mtls") => {
                let ca_only = original
                    .is_some_and(|d| d.niks3_write_auth_mode.as_deref() == Some("mtls"))
                    && form.write_cert.trim().is_empty()
                    && form.write_key.trim().is_empty()
                    && (!form.write_ca.trim().is_empty() || form.clear_write_ca);
                pair(&form.write_cert, &form.write_key, &form.write_ca, !ca_only)?;
            }
            (false, "mtls") => {
                let ca_only = original
                    .is_some_and(|d| d.niks3_read_auth_mode.as_deref() == Some("mtls"))
                    && form.read_cert.trim().is_empty()
                    && form.read_key.trim().is_empty()
                    && (!form.read_ca.trim().is_empty() || form.clear_read_ca);
                pair(&form.read_cert, &form.read_key, &form.read_ca, !ca_only)?;
            }
            _ => {
                return Err(
                    "Enter or explicitly retain the selected write and read credentials.".into(),
                );
            }
        }
    }
    Ok(())
}

// SECURITY: Untested-plane drafts must not enter an effective probe candidate.
// The shared Save builder remains authoritative for omissions and clear flags.
fn project_update(
    mut req: UpdateCacheDestination,
    scope: Niks3ProbeScope,
) -> UpdateCacheDestination {
    req.probe_scope = Some(scope);
    if scope == Niks3ProbeScope::Write {
        req.niks3_auth_token = None;
        req.push_to = None;
        req.niks3_public_keys.clear();
        req.niks3_read_auth_mode = None;
        req.niks3_read_client_cert = None;
        req.niks3_read_client_key = None;
        req.niks3_read_ca_cert = None;
        req.niks3_read_basic_username = None;
        req.niks3_read_basic_password = None;
        req.clear_niks3_read_client_key = false;
        req.clear_niks3_read_ca_cert = false;
    } else if scope == Niks3ProbeScope::Read {
        req.niks3_server_url = None;
        req.niks3_write_auth_mode = None;
        req.niks3_auth_token = None;
        req.niks3_write_client_cert = None;
        req.niks3_write_client_key = None;
        req.niks3_write_ca_cert = None;
        req.clear_niks3_auth_token = false;
        req.clear_niks3_write_client_key = false;
        req.clear_niks3_write_ca_cert = false;
    }
    req
}

fn project_create(
    mut req: CreateCacheDestination,
    scope: Niks3ProbeScope,
) -> CreateCacheDestination {
    req.probe_scope = Some(scope);
    if scope == Niks3ProbeScope::Write {
        req.niks3_auth_token = None;
        req.push_to = None;
        req.niks3_public_keys.clear();
        req.niks3_read_auth_mode = None;
        req.niks3_read_client_cert = None;
        req.niks3_read_client_key = None;
        req.niks3_read_ca_cert = None;
        req.niks3_read_basic_username = None;
        req.niks3_read_basic_password = None;
    } else {
        req.niks3_server_url = None;
        req.niks3_write_auth_mode = None;
        req.niks3_auth_token = None;
        req.niks3_write_client_cert = None;
        req.niks3_write_client_key = None;
        req.niks3_write_ca_cert = None;
    }
    req
}

#[component]
fn Evidence(label: String, value: Option<bool>, running: bool) -> Element {
    rsx! { div { class: "n3-plane-row", span { "{label}" }
        span { class: if running || value.is_none() { "chip chip-unknown" } else if value == Some(true) { "chip chip-healthy" } else { "chip chip-critical" },
            if running { "Testing…" } else { match value { Some(true) => "Verified", Some(false) => "Failed", None => "Untested" } }
        }
    } }
}

/// Renders the dedicated five-section Niks3 editor using a fresh parent snapshot.
/// Secret inputs exist only in a nested dialog. Test and discovery do not Save;
/// stored-ID retention is resolved on the server independently of metadata flags.
#[component]
pub(super) fn Niks3DestinationForm(
    mut form: Signal<Niks3FormState>,
    destination: Option<CacheDestination>,
    destination_ready: bool,
    restore_focus: bool,
    mut environment_ids: Signal<Vec<Uuid>>,
    environment_ready: Signal<bool>,
    environments: Resource<Result<Vec<EnvironmentSummary>, ApiClientError>>,
    mut busy: Signal<Option<&'static str>>,
    mut error: Signal<Option<String>>,
    on_close: EventHandler<()>,
    on_saved: EventHandler<CacheDestination>,
    on_type: EventHandler<String>,
) -> Element {
    let mut section = use_signal(|| "dest");
    let mut modal = use_signal(|| None::<bool>);
    let mut write_choice = use_signal(|| {
        if destination.is_some() {
            Choice::Current
        } else {
            Choice::None
        }
    });
    let mut read_choice = use_signal(|| {
        if destination.is_some() {
            Choice::Current
        } else {
            Choice::None
        }
    });
    let mut write_result = use_signal(|| None::<CacheCredentialTestResult>);
    let mut read_result = use_signal(|| None::<CacheCredentialTestResult>);
    let mut discovery_note = use_signal(|| None::<String>);
    let mut pending = use_signal(|| None::<Niks3Discovery>);
    let initial_keys = form().request().niks3_public_keys;
    let mut key_rows = use_signal(move || {
        if initial_keys.is_empty() {
            vec![(1_u64, String::new())]
        } else {
            initial_keys
                .into_iter()
                .enumerate()
                .map(|(i, key)| (i as u64 + 1, key))
                .collect()
        }
    });
    let mut next_key = use_signal(|| 1000_u64);
    let mut parallel_input = use_signal(|| form().parallel_uploads.to_string());
    let mut retries_input = use_signal(|| form().max_retries.to_string());
    let mut timeout_input = use_signal(|| form().push_timeout_seconds.to_string());
    let id = destination.as_ref().map(|d| d.id);
    let scope_ready = environment_ready() && matches!(environments.read().as_ref(), Some(Ok(_)));
    use_effect(move || {
        let _state = form();
        write_result.set(None);
        read_result.set(None);
    });
    let original_test = destination.clone();
    let test = EventHandler::new(move |scope: Niks3ProbeScope| {
        error.set(None);
        let ready = environment_ready() && matches!(environments.read().as_ref(), Some(Ok(_)));
        if let Err(message) = cache_action_ready(destination_ready, ready, busy().is_some())
            .and_then(|_| validate_plane(&form(), scope, id.is_some()))
        {
            error.set(Some(format!("Test not run: {message}")));
            return;
        }
        let patch = project_update(
            cache_update_patch(
                "niks3",
                &form(),
                &CacheTypeDrafts::default(),
                false,
                original_test.as_ref(),
                environment_ids(),
            ),
            scope,
        );
        let mut create = project_create(form().request(), scope);
        create.environment_ids = Some(environment_ids());
        if scope == Niks3ProbeScope::Write {
            write_result.set(None);
        } else {
            read_result.set(None);
        }
        busy.set(Some(if scope == Niks3ProbeScope::Write {
            "write-test"
        } else {
            "read-test"
        }));
        spawn(async move {
            let response = if let Some(id) = id {
                client::test_stored_cache_destination_credentials(id, &patch).await
            } else {
                client::test_cache_destination_credentials(&create).await
            };
            match response {
                Ok(value) => { if scope == Niks3ProbeScope::Write { write_result.set(Some(value)); } else { read_result.set(Some(value)); } }
                Err(_) => error.set(Some("Test failed. Review the selected endpoint, identity, server trust and permitted target policy.".into())),
            }
            busy.set(None);
        });
    });
    let original_discovery = destination.clone();
    let discover = move |_| {
        error.set(None);
        discovery_note.set(None);
        pending.set(None);
        let ready = environment_ready() && matches!(environments.read().as_ref(), Some(Ok(_)));
        if let Err(message) = cache_action_ready(destination_ready, ready, busy().is_some())
            .and_then(|_| validate_plane(&form(), Niks3ProbeScope::Write, id.is_some()))
        {
            error.set(Some(format!("Discovery not run: {message}")));
            return;
        }
        let request = discovery_request(&form());
        let mut patch = project_update(
            cache_update_patch(
                "niks3",
                &form(),
                &CacheTypeDrafts::default(),
                false,
                original_discovery.as_ref(),
                environment_ids(),
            ),
            Niks3ProbeScope::Write,
        );
        patch.probe_scope = None;
        busy.set(Some("discover"));
        spawn(async move {
            let response = if let Some(id) = id {
                client::discover_stored_niks3(id, &patch).await
            } else {
                client::discover_niks3(&request).await
            };
            match response {
                Ok(value) => {
                    if !https(&value.substituter_url) || value.public_keys.is_empty() || value.public_keys.iter().any(|key| !valid_signing_key(key)) {
                        error.set(Some("Discovery returned invalid public metadata. Review the API URL and proxy configuration.".into()));
                    } else {
                        let mut state = form.write();
                        let conflict = (!state.read_url.trim().is_empty() && state.read_url.trim() != value.substituter_url)
                            || (!state.request().niks3_public_keys.is_empty() && state.request().niks3_public_keys != value.public_keys);
                        if state.read_url.trim().is_empty() { state.read_url = value.substituter_url.clone(); }
                        if state.request().niks3_public_keys.is_empty() {
                            state.keys = value.public_keys.join("\n");
                            key_rows.set(value.public_keys.iter().enumerate().map(|(i, key)| (i as u64 + 1, key.clone())).collect());
                        }
                        drop(state);
                        if conflict { pending.set(Some(value)); discovery_note.set(Some("Discovery successful. Existing read URL or keys differ; review the public values below before applying. Nothing has been saved.".into())); }
                        else { discovery_note.set(Some("Discovery successful. Empty read URL and signing-key fields were populated. Review before saving; no write permission was tested.".into())); }
                    }
                }
                Err(_) => error.set(Some("Discovery failed. Check the write URL, selected mTLS identity, server CA bundle and permitted target policy.".into())),
            }
            busy.set(None);
        });
    };
    let save_original = destination.clone();
    let save_validation =
        validate_advanced_input(&parallel_input(), &retries_input(), &timeout_input()).and_then(
            |_| validate_save(&form(), destination.as_ref(), write_choice(), read_choice()),
        );
    let blocked = cache_action_ready(destination_ready, scope_ready, busy().is_some())
        .err()
        .or_else(|| save_validation.err());
    let nested = modal().is_some();
    rsx! {
        div { class: "modal-backdrop", style: "padding:8px;", onclick: move |_| { if busy().is_none() && modal().is_none() { on_close.call(()); } },
            style { "{CACHE_FORM_CSS}" } style { "{CSS}" }
            div { id: "cache-destination-dialog", class: "pe-shell cache-form-shell", role: "dialog", aria_modal: if nested { "false" } else { "true" }, aria_hidden: if nested { "true" } else { "false" }, "inert": if nested { Some("") } else { None }, aria_label: "Cache destination", tabindex: "-1", onclick: move |e| e.stop_propagation(), onkeydown: move |e| { e.stop_propagation(); if e.key() == Key::Escape { e.prevent_default(); if busy().is_none() && modal().is_none() { on_close.call(()); } } else if e.key() == Key::Tab && busy().is_some() { e.prevent_default(); } },
                if restore_focus { DialogFocusRestore {} }
                DialogInitialFocus { dialog_id: "cache-destination-dialog".to_string() }
                DialogFocusSentinel { dialog_id: "cache-destination-dialog".to_string(), boundary: DialogFocusBoundary::Last }
                if busy().is_some() { DialogInitialFocus { dialog_id: "niks3-operation-status".to_string() } }
                header { class: "pe-head",
                    div { style: "min-width:0;", div { style: "display:flex;align-items:center;gap:8px;flex-wrap:wrap;",
                        Icon { name: if id.is_some() { IconName::Gear } else { IconName::Plus }, size: 15 }
                        h2 { class: "pe-head-title", if form().name.is_empty() { "Add cache destination" } else { "{form().name}" } }
                        span { class: "chip chip-info", "Niks3" } if !form().enabled { span { class: "chip chip-unknown", "disabled" } }
                        span { id: "niks3-operation-status", role: "status", aria_live: "polite", tabindex: "-1", class: "chip chip-unknown", if busy().is_some() { "Working" } else { "Unsaved changes" } }
                    } p { class: "pe-head-sub", if id.is_some() { "Update Niks3 destination." } else { "Register a Niks3 destination. Write and read are configured separately." } } }
                    button { class: "btn-icon focus-ring", aria_label: "Close", disabled: busy().is_some(), onclick: move |_| on_close.call(()), Icon { name: IconName::X, size: 16 } }
                }
                nav { class: "pe-rail", aria_label: "Cache form sections",
                    for (value, label, icon) in [("dest", "Destination", IconName::Download), ("write", "Write / API", IconName::Download), ("read", "Read / Pull", IconName::Link), ("trust", "Trust", IconName::Shield), ("adv", "Advanced", IconName::Gear)] {
                        button { class: if section() == value { "pe-rail-item focus-ring active" } else { "pe-rail-item focus-ring" }, aria_label: label, aria_current: section() == value, disabled: busy().is_some(), onclick: move |_| section.set(value),
                            if value == "write" { svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", path { d: "M12 16V3m-5 5 5-5 5 5M3 16v5h18v-5" } } } else { Icon { name: icon, size: 13 } }
                            span { class: "pe-rail-label", "{label}" }
                            if value == "trust" { span { class: "pe-rail-badge", "{form().request().niks3_public_keys.len()}" } }
                        }
                    }
                }
                div { class: "pe-body",
                    fieldset { disabled: busy().is_some() || !scope_ready || !destination_ready, style: "border:0;padding:0;margin:0;min-width:0;",
                        if section() == "dest" {
                            div { class: "pe-sec-head", h3 { "Destination" } p { "Name, type and which environments push here." } }
                            div { class: "field", label { r#for: "n3-name", "Name" } input { id: "n3-name", class: "input focus-ring", value: form().name, oninput: move |e| form.write().name = e.value() } }
                            div { class: "field", label { "Cache type" } div { class: "seg", style: "width:fit-content;flex-wrap:wrap;",
                                for (value, label) in [("s3", "S3-compatible"), ("attic", "Attic"), ("nix", "Nix HTTPS"), ("niks3", "Niks3")] {
                                    button { class: if value == "niks3" { "active focus-ring" } else { "focus-ring" }, aria_pressed: value == "niks3", disabled: id.is_some(), onclick: move |_| { if value != "niks3" { on_type.call(value.into()); } }, "{label}" }
                                }
                            } p { class: "help", "Niks3 publishes through its API and serves Nix reads independently. Do not enter underlying S3 or Garage credentials." } }
                            div { class: "field", label { "Environment scope" }
                                match environments.read().as_ref() {
                                    Some(Ok(values)) => rsx! { div { style: "display:flex;gap:6px;flex-wrap:wrap;", for env in values { button { class: "btn btn-ghost focus-ring xs", aria_label: "{env.name}", aria_pressed: environment_ids().contains(&env.id), onclick: { let id = env.id; move |_| { let mut ids = environment_ids.write(); if ids.contains(&id) { ids.retain(|v| *v != id); } else { ids.push(id); } } }, "{env.name}" if environment_ids().contains(&env.id) { Icon { name: IconName::Check, size: 11 } } } } } },
                                    Some(Err(_)) => rsx! { p { role: "alert", "Environment list unavailable. Close and reopen." } },
                                    None => rsx! { p { "Loading environments…" } },
                                }
                            }
                            p { class: "help", if !scope_ready { "Scope not loaded. Saving is disabled." } else if environment_ids().is_empty() { "Global scope: no environment restriction. Select environments to restrict this destination." } else { "Configuration and selected environments are saved together." } }
                            label { input { r#type: "checkbox", checked: form().enabled, onchange: move |e| form.write().enabled = e.checked() } " Enabled" }
                        }
                        if section() == "write" {
                            div { class: "pe-sec-head", h3 { "Write / API" } p { "Where the Niks3 client publishes store paths. Presigned upload targets have their own HTTPS trust requirements." } }
                            div { class: "field", label { r#for: "n3-write-url", "Write / API URL" } div { style: "display:flex;gap:8px;",
                                input { id: "n3-write-url", class: "input focus-ring mono", style: "min-width:0;flex:1;", value: form().server_url, placeholder: "https://push.niks3.example.com", oninput: move |e| { form.write().server_url = e.value(); pending.set(None); discovery_note.set(None); } }
                                button { class: "btn btn-ghost focus-ring xs", disabled: form().server_url.trim().is_empty(), onclick: discover, if busy() == Some("discover") { "Discovering…" } else { "Discover" } }
                            } p { class: "help", "Discover uses selected write TLS, never a bearer token. Empty read URL and signing-key fields are populated without saving." } }
                            if let Some(note) = discovery_note() { div { class: "n3-disc", role: "status", "{note}" } }
                            if let Some(value) = pending() { div { class: "n3-disc", h4 { "Review discovered public metadata" }
                                p { class: "n3-public-value", "Read URL: {value.substituter_url}" }
                                for key in &value.public_keys { p { class: "n3-public-value mono", "{key}" } }
                                button { class: "btn btn-ghost focus-ring xs", onclick: move |_| { let Some(value) = pending() else { return }; form.write().read_url = value.substituter_url; form.write().keys = value.public_keys.join("\n"); key_rows.set(value.public_keys.into_iter().enumerate().map(|(i, key)| (i as u64 + 1, key)).collect()); pending.set(None); discovery_note.set(Some("Discovered public metadata applied to the draft. Review before saving.".into())); }, "Apply discovered metadata" }
                            } }
                            div { class: "field", label { "Write authentication" } div { class: "seg", role: "group", aria_label: "Write authentication",
                                for (value, label) in [("token", "API token"), ("mtls", "mTLS")] { button { class: if form().write_mode == value { "active focus-ring" } else { "focus-ring" }, aria_pressed: form().write_mode == value, onclick: move |_| { clear_identity(&mut form.write(), true); form.write().write_mode = value.into(); write_choice.set(Choice::None); pending.set(None); discovery_note.set(None); }, "{label}" } }
                            } }
                            CredentialRow { write: true, form, choice: write_choice, destination: destination.clone(), on_open: move |_| modal.set(Some(true)) }
                            div { class: "n3-plane", div { class: "n3-plane-head", strong { "Write API" } button { class: "btn btn-ghost focus-ring xs", disabled: !https(&form().server_url), onclick: move |_| test.call(Niks3ProbeScope::Write), if busy() == Some("write-test") { "Testing…" } else { "Test write API" } } }
                                Evidence { label: "API reachable", value: write_result().and_then(|r| r.write_api_reachable), running: busy() == Some("write-test") }
                                Evidence { label: "Authentication", value: write_result().and_then(|r| r.write_authn_valid), running: busy() == Some("write-test") }
                                Evidence { label: "Write authorization", value: write_result().and_then(|r| r.write_authorization_valid), running: false }
                            }
                            p { class: "help", "Public metadata and TLS reachability do not prove token validity or upload permission. Write authorization remains Untested unless independently established." }
                        }
                        if section() == "read" {
                            div { class: "pe-sec-head", h3 { "Read / Pull" } p { "Nix client → Niks3 read endpoint → storage. This endpoint can have a different hostname from the write API." } }
                            div { class: "field", label { r#for: "n3-read-url", "Read / substituter URL" } input { id: "n3-read-url", class: "input focus-ring mono", value: form().read_url, placeholder: "https://cache.example.com", oninput: move |e| form.write().read_url = e.value() } p { class: "help", "Never embed a username or password in the URL." } }
                            div { class: "field", label { "Read authentication" } div { class: "seg", role: "group", aria_label: "Read authentication",
                                for (value, label) in [("none", "Public"), ("basic", "Basic"), ("mtls", "mTLS")] { button { class: if form().read_mode == value { "active focus-ring" } else { "focus-ring" }, aria_pressed: form().read_mode == value, onclick: move |_| { clear_identity(&mut form.write(), false); form.write().read_mode = value.into(); read_choice.set(Choice::None); }, "{label}" } }
                            } }
                            CredentialRow { write: false, form, choice: read_choice, destination: destination.clone(), on_open: move |_| modal.set(Some(false)) }
                            if form().read_mode == "basic" { p { class: "help", "Basic values remain outside URLs and global configuration. Native consumers use protected authority-bound mode-0600 netrc files. Basic reads use system CA trust." } }
                            div { class: "n3-plane", div { class: "n3-plane-head", strong { "Read endpoint" } button { class: "btn btn-ghost focus-ring xs", disabled: !https(&form().read_url), onclick: move |_| test.call(Niks3ProbeScope::Read), if busy() == Some("read-test") { "Testing…" } else { "Test read endpoint" } } }
                                Evidence { label: "Read access", value: read_result().and_then(|r| r.read_access_valid), running: busy() == Some("read-test") }
                                Evidence { label: "Signing keys", value: read_result().and_then(|r| r.signing_keys_valid), running: busy() == Some("read-test") }
                            }
                            p { class: "help", "Signing-key evidence validates metadata configuration, not an artifact signature. Nix consumers enforce the selected signature policy when pulling paths." }
                        }
                        if section() == "trust" {
                            div { class: "pe-sec-head", h3 { "Trust" } p { "Signing keys are independent of Basic or mTLS network authentication." } }
                            label { "Trusted signing public keys" }
                            for (key_id, value) in key_rows() { div { key: "n3-key-{key_id}", class: "n3-key-row",
                                input { aria_label: "Signing public key {key_id}", class: "input focus-ring mono", value: value.clone(), placeholder: "cache.example.com-1:base64-ed25519-public-key", oninput: move |e| { if let Some(row) = key_rows.write().iter_mut().find(|row| row.0 == key_id) { row.1 = e.value(); } form.write().keys = key_rows().iter().map(|row| row.1.clone()).collect::<Vec<_>>().join("\n"); } }
                                button { class: "btn-icon focus-ring", aria_label: "Remove key {key_id}", disabled: key_rows().len() == 1 && value.is_empty(), onclick: move |_| { if key_rows().len() == 1 { key_rows.set(vec![(key_id, String::new())]); } else { key_rows.write().retain(|row| row.0 != key_id); } form.write().keys = key_rows().iter().map(|row| row.1.clone()).collect::<Vec<_>>().join("\n"); }, Icon { name: IconName::X, size: 13 } }
                            } }
                            if key_rows().iter().any(|row| !row.1.trim().is_empty() && !valid_signing_key(row.1.trim())) { p { class: "help", role: "alert", "Keys require a nonempty name and standard padded Base64 with exactly 32 decoded bytes, without whitespace." } }
                            button { class: "btn btn-ghost focus-ring xs", style: "margin-top:8px;", onclick: move |_| { next_key += 1; key_rows.write().push((next_key(), String::new())); }, Icon { name: IconName::Plus, size: 11 } " Add key" }
                            p { class: "help", "Keep both old and new keys during rotation. Connection authentication never replaces artifact signature verification." }
                        }
                        if section() == "adv" {
                            div { class: "pe-sec-head", h3 { "Advanced" } p { "Production defaults: 1 parallel upload, 3 retries and a 3600-second push timeout." } }
                            div { class: "n3-advanced",
                                div { class: "field", label { r#for: "n3-parallel", "Parallel uploads" } input { id: "n3-parallel", r#type: "number", min: "1", max: "2147483647", class: "input focus-ring", value: parallel_input(), oninput: move |e| { parallel_input.set(e.value()); if let Ok(value) = e.value().parse() { form.write().parallel_uploads = value; } } } p { class: "help", "concurrent" } }
                                div { class: "field", label { r#for: "n3-retries", "Retry attempts" } input { id: "n3-retries", r#type: "number", min: "0", max: "2147483647", class: "input focus-ring", value: retries_input(), oninput: move |e| { retries_input.set(e.value()); if let Ok(value) = e.value().parse() { form.write().max_retries = value; } } } p { class: "help", "per path" } }
                                div { class: "field", label { r#for: "n3-timeout", "Push timeout" } input { id: "n3-timeout", r#type: "number", min: "1", class: "input focus-ring", value: timeout_input(), oninput: move |e| { timeout_input.set(e.value()); if let Ok(value) = e.value().parse() { form.write().push_timeout_seconds = value; } } } p { class: "help", "seconds" } }
                            }
                            label { input { r#type: "checkbox", checked: form().require_sigs, onchange: move |e| form.write().require_sigs = e.checked() } " Require signatures" }
                            p { class: "help", "Only pull paths accepted by the selected Nix signature policy and trusted keys. Save requires at least one valid signing key." }
                        }
                        if let Some(message) = error() { p { role: "alert", "{message}" } }
                    }
                }
                footer { class: "pe-foot",
                    div { class: "pe-foot-state", style: CACHE_FORM_FOOT_STATE_STYLE,
                        span { "{form().name}" span { class: "pe-foot-dot", "·" } "write {form().write_mode}" span { class: "pe-foot-dot", "·" } "read {form().read_mode}" span { class: "pe-foot-dot", "·" }
                            if !scope_ready { "Scope not loaded" } else if environment_ids().is_empty() { "Global scope" } else { "{environment_ids().len()} selected" }
                        }
                        if let Some(reason) = blocked.as_ref() { p { "data-testid": "cache-save-blocked", role: "status", aria_live: "polite", "{reason}" } }
                    }
                    div { style: "display:flex;gap:8px;flex-shrink:0;",
                        button { class: "btn btn-ghost focus-ring", disabled: busy().is_some(), onclick: move |_| on_close.call(()), "Cancel" }
                        button { class: "btn btn-primary focus-ring", disabled: blocked.is_some(), onclick: move |_| {
                            let ready = environment_ready() && matches!(environments.read().as_ref(), Some(Ok(_)));
                            if let Err(message) = cache_action_ready(destination_ready, ready, busy().is_some())
                                .and_then(|_| validate_advanced_input(&parallel_input(), &retries_input(), &timeout_input()))
                                .and_then(|_| validate_save(&form(), save_original.as_ref(), write_choice(), read_choice())) { error.set(Some(format!("Save not run: {message}"))); return; }
                            let mut create = form().request(); create.environment_ids = Some(environment_ids());
                            let patch = cache_update_patch("niks3", &form(), &CacheTypeDrafts::default(), false, save_original.as_ref(), environment_ids());
                            busy.set(Some("save")); error.set(None);
                            spawn(async move { let response = if let Some(id) = id { client::update_cache_destination(id, &patch).await } else { client::create_cache_destination(&create).await };
                                match response { Ok(value) => on_saved.call(value), Err(_) => error.set(Some("Cache save failed. Check the selected configuration, identity and scope, then retry.".into())) }
                                busy.set(None);
                            });
                        }, Icon { name: IconName::Check, size: 13 } if busy() == Some("save") { "Saving…" } else if id.is_some() { "Save changes" } else { "Add cache" } }
                    }
                }
                DialogFocusSentinel { dialog_id: "cache-destination-dialog".to_string(), boundary: DialogFocusBoundary::First }
            }
        }
        if let Some(write) = modal() {
            CredentialModal { write, initial: form(), original: destination.clone(), on_close: move |value: Option<Niks3FormState>| {
                modal.set(None);
                if let Some(value) = value {
                    let mut state = form.write();
                    if write { state.token = value.token; state.write_cert = value.write_cert; state.write_key = value.write_key; state.write_ca = value.write_ca; state.clear_write_ca = value.clear_write_ca; write_choice.set(Choice::Draft); }
                    else { state.read_cert = value.read_cert; state.read_key = value.read_key; state.read_ca = value.read_ca; state.clear_read_ca = value.clear_read_ca; state.basic_user = value.basic_user; state.basic_password = value.basic_password; read_choice.set(Choice::Draft); }
                }
            } }
        }
    }
}

#[component]
fn CredentialRow(
    write: bool,
    mut form: Signal<Niks3FormState>,
    mut choice: Signal<Choice>,
    destination: Option<CacheDestination>,
    on_open: EventHandler<()>,
) -> Element {
    let mode = if write {
        form().write_mode
    } else {
        form().read_mode
    };
    let has_current = configured(write, &form(), destination.as_ref());
    let old_mode = destination.as_ref().and_then(|d| {
        if write {
            d.niks3_write_auth_mode.clone()
        } else {
            d.niks3_read_auth_mode.clone()
        }
    });
    // SECURITY: Redacted configured flags affect labels only. Returning to
    // retained identity must remain possible when metadata is absent or stale.
    let can_retain = old_mode.as_deref() == Some(mode.as_str());
    rsx! { div { class: "n3-cred",
        div { class: "n3-cred-main",
            span { class: "n3-cred-state", "data-testid": if write { "niks3-write-credential-state" } else { "niks3-read-credential-state" },
                if mode == "none" { "No credential needed" }
                else if choice() == Choice::Draft { Icon { name: IconName::Key, size: 12 } "Replacement entered · not saved" }
                else if choice() == Choice::Current && has_current { Icon { name: IconName::Check, size: 12 } "Current configured credential" span { class: "n3-cred-sub", "stored encrypted · never shown" } }
                else if destination.is_some() { "Stored credential status unavailable" }
                else { "Not configured" }
            }
            if mode != "none" { div { style: "display:flex;gap:6px;flex-wrap:wrap;",
                if can_retain && choice() != Choice::Current { button { class: "btn btn-ghost focus-ring xs", onclick: move |_| { clear_identity(&mut form.write(), write); choice.set(Choice::Current); }, "Use current credential" } }
                if choice() == Choice::Draft { button { class: "btn btn-ghost focus-ring xs", onclick: move |_| { clear_identity(&mut form.write(), write); choice.set(if can_retain { Choice::Current } else { Choice::None }); }, "Discard" } }
                button { class: "btn btn-ghost focus-ring xs", onclick: move |_| on_open.call(()), if choice() == Choice::Draft { "Edit replacement" } else if has_current { "Replace" } else { "Add credential" } }
            } }
        }
        if old_mode.as_deref().is_some_and(|old| old != mode) { p { class: "n3-cred-warn", "Saving this authentication mode clears the previous stored identity. Inactive credentials are never borrowed." } }
        if choice() == Choice::Draft { p { class: "help", "Test uses this replacement. Cancel discards it; Save persists it." } }
    } }
}

#[component]
fn CredentialModal(
    write: bool,
    initial: Niks3FormState,
    original: Option<CacheDestination>,
    on_close: EventHandler<Option<Niks3FormState>>,
) -> Element {
    let mut form = use_signal(move || initial);
    let mode = if write {
        form().write_mode
    } else {
        form().read_mode
    };
    let plane = if write { "Write" } else { "Read" };
    let has_current = configured(write, &form(), original.as_ref());
    let same_mode = original.as_ref().is_some_and(|d| {
        if write {
            d.niks3_write_auth_mode.as_deref() == Some(mode.as_str())
        } else {
            d.niks3_read_auth_mode.as_deref() == Some(mode.as_str())
        }
    });
    let has_ca = original.as_ref().is_some_and(|d| {
        if write {
            d.niks3_write_ca_cert.is_some()
        } else {
            d.niks3_read_ca_cert.is_some()
        }
    });
    let state = form();
    let validation = match mode.as_str() {
        "token" => {
            if state.token.trim().is_empty() {
                Err("Enter an API token or Cancel to retain the current selection.".into())
            } else {
                Ok(())
            }
        }
        "basic" => validate_basic(&state.basic_user, &state.basic_password),
        _ => {
            let (cert, key, ca, clear) = if write {
                (
                    &state.write_cert,
                    &state.write_key,
                    &state.write_ca,
                    state.clear_write_ca,
                )
            } else {
                (
                    &state.read_cert,
                    &state.read_key,
                    &state.read_ca,
                    state.clear_read_ca,
                )
            };
            if cert.trim().is_empty() && key.trim().is_empty() && ca.trim().is_empty() && !clear {
                Err("Enter a complete identity, change server trust, or Cancel to retain the current selection.".into())
            } else {
                pair(cert, key, ca, !same_mode)
            }
        }
    };
    rsx! { div { class: "modal-backdrop modal-backdrop-above-drawer", style: CACHE_CREDENTIAL_BACKDROP_STYLE, onclick: move |e| { e.stop_propagation(); on_close.call(None); },
        div { id: "niks3-credential-dialog", class: "modal", style: "width:min(560px,calc(100vw - 16px));max-height:92vh;overflow-y:auto;", role: "dialog", aria_modal: "true", aria_label: "{plane} credential", tabindex: "-1", onclick: move |e| e.stop_propagation(), onkeydown: move |e| { e.stop_propagation(); if e.key() == Key::Escape { e.prevent_default(); on_close.call(None); } },
            DialogFocusRestore {} DialogInitialFocus { dialog_id: "niks3-credential-dialog".to_string() }
            DialogFocusSentinel { dialog_id: "niks3-credential-dialog".to_string(), boundary: DialogFocusBoundary::Last }
            div { class: "modal-head", h2 { Icon { name: IconName::Key, size: 14 } if has_current { " Replace" } else { " Add" } " {plane} credential · {mode}" } p { "Stored secrets are never shown. This local draft is not persisted until the cache Save action." } }
            div { class: "modal-body",
                if mode == "token" { div { class: "field", label { r#for: "n3-token", "API token" } input { id: "n3-token", r#type: "password", autocomplete: "off", class: "input focus-ring mono", value: form().token, oninput: move |e| form.write().token = e.value() } } }
                else if mode == "basic" {
                    div { class: "field", label { r#for: "n3-basic-user", "Username" } input { id: "n3-basic-user", autocomplete: "off", class: "input focus-ring", value: form().basic_user, oninput: move |e| form.write().basic_user = e.value() } }
                    div { class: "field", label { r#for: "n3-basic-password", "Password" } input { id: "n3-basic-password", r#type: "password", autocomplete: "new-password", class: "input focus-ring", value: form().basic_password, oninput: move |e| form.write().basic_password = e.value() } }
                    p { class: "help", "Username and password are replaced together. Allowed whitespace is preserved. They never enter the URL or global Nix configuration." }
                } else {
                    for (field, label, secret) in [("cert", "Client certificate", false), ("key", "Private key", true), ("ca", "Server CA bundle (optional)", false)] {
                        div { class: "field", label { r#for: "n3-identity-{field}", "{label}" }
                            textarea { id: "n3-identity-{field}", class: "input focus-ring mono", rows: "4", autocomplete: "off", spellcheck: "false", style: if secret { "font-size:11.5px;resize:vertical;-webkit-text-security:disc;white-space:pre;" } else { "font-size:11.5px;resize:vertical;white-space:pre;" },
                                value: match (write, field) { (true,"cert") => form().write_cert, (true,"key") => form().write_key, (true,_) => form().write_ca, (false,"cert") => form().read_cert, (false,"key") => form().read_key, _ => form().read_ca },
                                oninput: move |e| { let mut state = form.write(); match (write,field) { (true,"cert") => state.write_cert = e.value(), (true,"key") => state.write_key = e.value(), (true,_) => state.write_ca = e.value(), (false,"cert") => state.read_cert = e.value(), (false,"key") => state.read_key = e.value(), _ => state.read_ca = e.value() } }
                            }
                            if field == "ca" { p { class: "help", "{CA_HELP}" } if write { p { class: "help", "{WRITE_CA_HELP}" } } p { class: "help", if has_ca { "Blank retains the stored bundle. Explicit removal restores system trust." } else { "Blank on a new identity uses system trust." } } }
                        }
                    }
                    if has_ca { label { input { r#type: "checkbox", checked: if write { form().clear_write_ca } else { form().clear_read_ca }, onchange: move |e| { if write { form.write().clear_write_ca = e.checked(); } else { form.write().clear_read_ca = e.checked(); } } } " Remove {plane} server CA bundle on Save" } }
                }
                if let Err(message) = validation.as_ref() { p { role: "status", "{message}" } }
            }
            div { class: "modal-foot", button { class: "btn btn-ghost focus-ring", onclick: move |_| on_close.call(None), "Cancel" } button { class: "btn btn-primary focus-ring", disabled: validation.is_err(), onclick: move |_| on_close.call(Some(form())), Icon { name: IconName::Check, size: 13 } " Use for this cache" } }
            DialogFocusSentinel { dialog_id: "niks3-credential-dialog".to_string(), boundary: DialogFocusBoundary::First }
        }
    } }
}

/// Renders public Niks3 endpoint, authentication and trust metadata only.
/// False/absent configured flags are unavailable evidence, never secret values.
pub(super) fn details(destination: &CacheDestination) -> Element {
    let write_url = destination
        .niks3_server_url
        .as_deref()
        .unwrap_or("Unavailable");
    let read_url = destination.push_to.as_deref().unwrap_or("Unavailable");
    let write_mode = destination
        .niks3_write_auth_mode
        .as_deref()
        .unwrap_or("unknown");
    let read_mode = destination
        .niks3_read_auth_mode
        .as_deref()
        .unwrap_or("unknown");
    let write_configured = if write_mode == "mtls" {
        destination.niks3_write_mtls_configured
    } else {
        destination.niks3_write_token_configured
    };
    let read_configured = if read_mode == "basic" {
        destination.niks3_read_basic_configured
    } else {
        destination.niks3_read_mtls_configured
    };
    rsx! {
        section { class: "panel-section", h3 { "Write / API" } dl { class: "kv-grid",
            dt { "URL" } dd { class: "mono", style: "overflow-wrap:anywhere;", "{write_url}" }
            dt { "Authentication" } dd { "{write_mode}" span { class: "chip chip-unknown", if write_configured { "Current configured credential" } else { "Stored credential status unavailable" } } }
        } }
        section { class: "panel-section", h3 { "Read / Pull" } dl { class: "kv-grid",
            dt { "Substituter URL" } dd { class: "mono", style: "overflow-wrap:anywhere;", "{read_url}" }
            dt { "Authentication" } dd { if read_mode == "none" { "Public · no credential needed" } else { "{read_mode}" span { class: "chip chip-unknown", if read_configured { "Current configured credential" } else { "Stored credential status unavailable" } } } }
        } }
        section { class: "panel-section", h3 { "Trusted signing public keys ({destination.niks3_public_keys.len()})" }
            for key in &destination.niks3_public_keys { p { class: "mono", style: "font-size:11px;overflow-wrap:anywhere;", "{key}" } }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advanced_drafts_reject_empty_fractional_and_overflowing_values() {
        assert!(validate_advanced_input("1", "0", "3600").is_ok());
        for (parallel, retries, timeout) in [
            ("", "3", "3600"),
            ("0", "3", "3600"),
            ("2147483648", "3", "3600"),
            ("1", "-1", "3600"),
            ("1", "1.5", "3600"),
            ("1", "3", ""),
            ("1", "3", "9223372036854775808"),
        ] {
            assert!(validate_advanced_input(parallel, retries, timeout).is_err());
        }
    }

    #[test]
    fn signing_key_validation_matches_protocol_format_not_mock_lengths() {
        assert!(valid_signing_key(
            "fixture:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
        ));
        for value in [
            "fixture:YWJj",
            ":AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "bad name:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "fixture:invalid_",
        ] {
            assert!(!valid_signing_key(value));
        }
    }

    #[test]
    fn basic_pair_preserves_spaces_and_rejects_colons_controls_and_partial_pairs() {
        assert!(validate_basic("user name", "  password  ").is_ok());
        for (user, password) in [
            ("", "value"),
            ("user", ""),
            ("user:name", "value"),
            ("user\n", "value"),
            ("user", "value\r"),
        ] {
            assert!(validate_basic(user, password).is_err());
        }
        let mut form = Niks3FormState::from_destination(None);
        form.read_mode = "basic".into();
        form.basic_user = "user name".into();
        form.basic_password = "  password  ".into();
        let request = form.request();
        assert_eq!(
            request.niks3_read_basic_password.as_deref(),
            Some("  password  ")
        );
        assert!(request.niks3_read_client_key.is_none());
        assert!(request.niks3_read_ca_cert.is_none());
    }

    #[test]
    fn discovery_and_scoped_tests_exclude_tokens_and_untested_plane_material() {
        let mut form = Niks3FormState::from_destination(None);
        form.token = "fixture-write-token".into();
        form.read_mode = "basic".into();
        form.basic_user = "fixture-user".into();
        form.basic_password = "fixture-read-password".into();
        form.server_url = "https://write.example.com".into();
        let public = serde_json::to_value(discovery_request(&form)).unwrap();
        assert_eq!(
            public,
            serde_json::json!({"server_url":"https://write.example.com"})
        );
        form.write_mode = "mtls".into();
        form.write_cert = "fixture-cert".into();
        form.write_key = "fixture-key".into();
        form.write_ca = "fixture-ca".into();
        let write = project_update(form.update_request(), Niks3ProbeScope::Write);
        assert!(write.niks3_auth_token.is_none());
        assert!(write.niks3_read_basic_password.is_none());
        assert!(write.niks3_read_auth_mode.is_none());
        assert_eq!(write.niks3_write_client_key.as_deref(), Some("fixture-key"));
        let read = project_update(form.update_request(), Niks3ProbeScope::Read);
        assert!(read.niks3_write_client_key.is_none());
        assert!(read.niks3_auth_token.is_none());
        assert!(read.niks3_write_auth_mode.is_none());
        assert_eq!(
            read.niks3_read_basic_password.as_deref(),
            Some("fixture-read-password")
        );
        for wire in [
            serde_json::to_value(read).unwrap(),
            serde_json::to_value(project_create(form.request(), Niks3ProbeScope::Read)).unwrap(),
        ] {
            for field in [
                "niks3_auth_token",
                "niks3_write_client_cert",
                "niks3_write_client_key",
                "niks3_write_ca_cert",
            ] {
                assert!(wire.get(field).is_none(), "read wire body excludes {field}");
            }
        }
        for wire in [
            serde_json::to_value(write).unwrap(),
            serde_json::to_value(project_create(form.request(), Niks3ProbeScope::Write)).unwrap(),
        ] {
            for field in [
                "niks3_auth_token",
                "niks3_read_basic_username",
                "niks3_read_basic_password",
                "niks3_read_client_cert",
                "niks3_read_client_key",
                "niks3_read_ca_cert",
            ] {
                assert!(
                    wire.get(field).is_none(),
                    "write wire body excludes {field}"
                );
            }
        }
        let mtls = serde_json::to_value(discovery_request(&form)).unwrap();
        assert!(mtls.get("niks3_auth_token").is_none());
        assert!(mtls.get("niks3_read_basic_password").is_none());
    }

    #[test]
    fn production_defaults_and_clear_flags_are_not_design_mock_defaults() {
        let mut form = Niks3FormState::from_destination(None);
        assert_eq!(
            (
                form.parallel_uploads,
                form.max_retries,
                form.push_timeout_seconds,
                form.require_sigs
            ),
            (1, 3, 3600, true)
        );
        form.write_mode = "mtls".into();
        form.write_ca = "replacement".into();
        form.clear_write_ca = true;
        let projected = project_update(form.update_request(), Niks3ProbeScope::Write);
        assert!(projected.clear_niks3_write_ca_cert);
        assert!(projected.niks3_write_ca_cert.is_none());
        assert!(form.basic_password.is_empty());
    }
}
