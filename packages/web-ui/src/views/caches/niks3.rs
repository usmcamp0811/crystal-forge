//! Dedicated Niks3 presentation and request projections.
//!
//! Write API transport and Nix read identity are independent. Stored secrets
//! never seed inputs. Discovery and scoped Test operate on snapshots only;
//! Save commits complete configuration and scope in one request.
//!
//! A fresh destination and loaded scope establish one immutable draft baseline.
//! Only editable values, credential selection and explicit clear intent affect
//! dirty state. Plane-specific input generations and operation sequences fence
//! stale successes, failures and busy completion. Public discovery never proves
//! authentication. API authorization does not prove S3 upload or publication.
//! Discovery responses are proposals only. Explicit application changes read
//! settings; matching metadata and stale responses never change the draft.

use super::*;
use crate::api::models::{Niks3DiscoverRequest, Niks3Discovery, Niks3ProbeScope};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

const CSS: &str = r#"
.n3-shell .input,.n3-credential .input{font-size:12px}
.n3-shell .n3-key-row .input{font-size:11.5px}
.n3-shell .pe-head-sub{max-width:760px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.n3-title-row{display:flex;align-items:center;gap:9px;min-width:0}
.n3-title-row .chip,.n3-title-row svg{flex-shrink:0}
.n3-toggle{display:flex;align-items:flex-start;gap:9px;font-size:13px;font-weight:600;cursor:pointer}
.n3-toggle input{accent-color:var(--cf-brand-purple);margin-top:2px}
.n3-shell .seg{width:fit-content;max-width:100%;flex-wrap:wrap}
.n3-shell .n3-key-row+.n3-key-row{margin-top:6px}
.n3-shell .n3-cred{margin-top:8px;margin-bottom:14px}
.n3-env{padding:6px 12px;border-radius:99px;font-size:12px;font-weight:600;border:1px solid var(--cf-card-border);background:transparent;color:var(--cf-text-muted);cursor:pointer;display:inline-flex;align-items:center;gap:7px;font-family:inherit}
.n3-env[aria-pressed=true]{border-color:var(--n3-env-color);background:color-mix(in oklab,var(--n3-env-color) 14%,var(--cf-card-bg));color:var(--cf-text-primary)}
.n3-env-dot{width:8px;height:8px;border-radius:50%;background:var(--n3-env-color);flex-shrink:0}
.n3-credential.modal{width:min(560px,calc(100vw - 16px));max-height:min(88vh,760px);display:flex;flex-direction:column;overflow:hidden;min-width:0}
.n3-credential .modal-head,.n3-credential .modal-foot{flex-shrink:0}
.n3-credential .modal-head{position:sticky;top:0;z-index:1;background:var(--cf-card-bg)}
.n3-credential .modal-foot{position:sticky;bottom:0;z-index:1;background:var(--cf-card-bg)}
.n3-credential .modal-body{overflow-y:auto;min-height:0}
.n3-credential textarea.input{min-height:92px}
.n3-credential .modal-head h2{overflow-wrap:anywhere}
@media(max-width:860px){.n3-shell .pe-rail-item{grid-template-columns:15px minmax(0,1fr) auto}.n3-shell .pe-head>div{min-width:0;flex:1}}
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
@media(max-width:600px){.n3-advanced{grid-template-columns:1fr}.n3-plane-head,.n3-title-row{flex-wrap:wrap}}
"#;

const CA_HELP: &str = "Trust bundle used by the client to verify remote HTTPS servers, not the CA validating your client certificate. Multiple CA certificates are supported.";
const WRITE_CA_HELP: &str = "The Niks3 client uses this bundle for the API and presigned HTTPS S3 targets. Its custom bundle replaces system roots; include public roots when those targets require them.";

#[derive(Clone, Copy, PartialEq)]
enum Choice {
    Current,
    Draft,
    None,
}

fn auth_label(mode: &str) -> &'static str {
    match mode {
        "token" => "API token",
        "mtls" => "mTLS",
        "none" => "Public",
        "basic" => "Basic",
        _ => "Unknown",
    }
}

// SECURITY: Use the shared empty-color fallback, but accept only CSS hex colors
// in Niks3's custom property. API color text cannot introduce CSS declarations.
fn environment_color(raw: &str) -> &str {
    let color = normalize_env_color(raw);
    if color.strip_prefix('#').is_some_and(|hex| {
        matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit())
    }) {
        color
    } else {
        normalize_env_color("")
    }
}

// SECURITY: Drafts include secrets for equality only. Do not derive Debug or
// serialization, log these values, or put them in diagnostic DOM attributes.
#[derive(Clone, PartialEq)]
struct PureDraft {
    form: Niks3FormState,
    environments: Vec<Uuid>,
    write: Choice,
    read: Choice,
    // Raw parallel-upload, retry and timeout inputs include invalid drafts.
    numbers: [String; 3],
}

impl PureDraft {
    fn new(
        form: Niks3FormState,
        mut environments: Vec<Uuid>,
        write: Choice,
        read: Choice,
        numbers: [String; 3],
    ) -> Self {
        environments.sort_unstable();
        environments.dedup();
        Self {
            form,
            environments,
            write,
            read,
            numbers,
        }
    }
}

fn initialize_baseline(initial: &mut Option<PureDraft>, ready: bool, draft: PureDraft) {
    if ready && initial.is_none() {
        *initial = Some(draft);
    }
}

fn draft_is_dirty(initial: Option<&PureDraft>, draft: &PureDraft) -> bool {
    initial.is_some_and(|initial| initial != draft)
}

// Only the selected plane's effective inputs invalidate its evidence. Retained
// identity and draft identity are different inputs even when fields are blank.
#[derive(Clone, PartialEq)]
struct PlaneInput {
    values: Vec<String>,
    choice: Choice,
    clear_ca: bool,
}

fn plane_input(form: &Niks3FormState, write: bool, choice: Choice) -> PlaneInput {
    let mut values = if write {
        vec![form.server_url.clone(), form.write_mode.clone()]
    } else {
        vec![
            form.read_url.clone(),
            form.read_mode.clone(),
            form.keys.clone(),
            form.require_sigs.to_string(),
        ]
    };
    let mtls = if write {
        form.write_mode == "mtls"
    } else {
        form.read_mode == "mtls"
    };
    if write && form.write_mode == "token" {
        values.push(form.token.clone());
    } else if write && mtls {
        values.extend([
            form.write_cert.clone(),
            form.write_key.clone(),
            form.write_ca.clone(),
        ]);
    } else if !write && form.read_mode == "basic" {
        values.extend([form.basic_user.clone(), form.basic_password.clone()]);
    } else if !write && mtls {
        values.extend([
            form.read_cert.clone(),
            form.read_key.clone(),
            form.read_ca.clone(),
        ]);
    }
    PlaneInput {
        values,
        choice: if !write && form.read_mode == "none" {
            Choice::None
        } else {
            choice
        },
        clear_ca: if !mtls {
            false
        } else if write {
            form.clear_write_ca
        } else {
            form.clear_read_ca
        },
    }
}

fn discovery_input(form: &Niks3FormState, choice: Choice) -> PlaneInput {
    // Public discovery never sends Bearer credentials or proves token validity.
    if form.write_mode == "mtls" {
        plane_input(form, true, choice)
    } else {
        PlaneInput {
            values: vec![form.server_url.clone(), form.write_mode.clone()],
            choice: Choice::None,
            clear_ca: false,
        }
    }
}

#[derive(Clone, Copy)]
struct Operation {
    generation: u64,
    sequence: u64,
}

#[derive(Clone)]
struct InputStamp {
    generation: u64,
    input: PlaneInput,
}

#[derive(Clone)]
struct DiscoveryProposal {
    metadata: Niks3Discovery,
    write_operation: Operation,
    read_stamp: InputStamp,
}

// CONCURRENCY: Every observed input transition advances generation, including
// edit-away/revert. Sequence fences overlapping operations on the same inputs.
// Success, failure and busy completion all use the same acceptance predicate.
#[derive(Default)]
struct PlaneOwner {
    input: Option<PlaneInput>,
    generation: u64,
    sequence: u64,
}

impl PlaneOwner {
    fn observe(&mut self, input: PlaneInput) -> bool {
        if self.input.as_ref() == Some(&input) {
            return false;
        }
        self.input = Some(input);
        self.generation += 1;
        true
    }
    fn start(&mut self, input: PlaneInput) -> Operation {
        self.observe(input);
        self.sequence += 1;
        Operation {
            generation: self.generation,
            sequence: self.sequence,
        }
    }
    fn accepts(&self, operation: Operation, input: PlaneInput) -> bool {
        self.input.as_ref() == Some(&input)
            && operation.generation == self.generation
            && operation.sequence == self.sequence
    }
    fn stamp(&self, input: PlaneInput) -> InputStamp {
        InputStamp {
            generation: self.generation,
            input,
        }
    }
    fn accepts_stamp(&self, stamp: &InputStamp, input: PlaneInput) -> bool {
        self.generation == stamp.generation
            && self.input.as_ref() == Some(&input)
            && stamp.input == input
    }
}

// Observation never mutates editable values or rebases the initial snapshot.
// A matching response needs no proposal, including equivalent trimmed inputs.
fn discovery_proposal(
    form: &Niks3FormState,
    metadata: Niks3Discovery,
    write_operation: Operation,
    read_stamp: InputStamp,
) -> Option<DiscoveryProposal> {
    if form.read_url.trim() == metadata.substituter_url.trim()
        && form.request().niks3_public_keys == metadata.public_keys
    {
        None
    } else {
        Some(DiscoveryProposal {
            metadata,
            write_operation,
            read_stamp,
        })
    }
}

// CONCURRENCY: Recheck both input stamps at the explicit application boundary.
// A read test on unchanged inputs does not stale a proposal; read edits do,
// including edit-away/revert. A newer discovery supersedes its older proposal.
fn apply_discovery_proposal(
    form: &mut Niks3FormState,
    proposal: &DiscoveryProposal,
    write_owner: &PlaneOwner,
    read_owner: &PlaneOwner,
    write: Choice,
    read: Choice,
) -> bool {
    if !write_owner.accepts(proposal.write_operation, discovery_input(form, write))
        || !read_owner.accepts_stamp(&proposal.read_stamp, plane_input(form, false, read))
    {
        return false;
    }
    form.read_url = proposal.metadata.substituter_url.clone();
    form.keys = proposal.metadata.public_keys.join("\n");
    true
}

fn select_mode(
    form: &mut Niks3FormState,
    write: bool,
    mode: &str,
    original_mode: Option<&str>,
    choice: &mut Choice,
) -> bool {
    let current = if write {
        &mut form.write_mode
    } else {
        &mut form.read_mode
    };
    if current.as_str() == mode {
        return false;
    }
    *current = mode.into();
    let drafted = match (write, mode) {
        (true, "token") => !form.token.is_empty(),
        (true, _) => {
            !form.write_cert.is_empty()
                || !form.write_key.is_empty()
                || !form.write_ca.is_empty()
                || form.clear_write_ca
        }
        (false, "basic") => !form.basic_user.is_empty() || !form.basic_password.is_empty(),
        (false, "mtls") => {
            !form.read_cert.is_empty()
                || !form.read_key.is_empty()
                || !form.read_ca.is_empty()
                || form.clear_read_ca
        }
        _ => false,
    };
    *choice = if drafted {
        Choice::Draft
    } else if original_mode == Some(mode) {
        Choice::Current
    } else {
        Choice::None
    };
    true
}

// Confirmation changes only the selected plane. Cancel leaves both selections,
// secret drafts, public fields and explicit trust removal intents unchanged.
fn confirm_credential(
    form: &mut Niks3FormState,
    write: bool,
    value: Option<Niks3FormState>,
    choice: &mut Choice,
) {
    let Some(value) = value else {
        return;
    };
    if write {
        form.token = value.token;
        form.write_cert = value.write_cert;
        form.write_key = value.write_key;
        form.write_ca = value.write_ca;
        form.clear_write_ca = value.clear_write_ca;
    } else {
        form.read_cert = value.read_cert;
        form.read_key = value.read_key;
        form.read_ca = value.read_ca;
        form.clear_read_ca = value.clear_read_ca;
        form.basic_user = value.basic_user;
        form.basic_password = value.basic_password;
    }
    *choice = Choice::Draft;
}

#[derive(Clone, Copy, Default)]
struct CredentialFocusOwner {
    open_plane: Option<bool>,
    generation: u32,
}

impl CredentialFocusOwner {
    // Each modal transition invalidates the preceding frame. Closing returns
    // the plane whose trigger must be resolved after the replacement renders.
    fn transition(&mut self, open_plane: Option<bool>) -> Option<bool> {
        self.generation = self.generation.wrapping_add(1);
        let restore = if open_plane.is_none() {
            self.open_plane
        } else {
            None
        };
        self.open_plane = open_plane;
        restore
    }
}

fn restore_credential_focus(plane: Option<bool>, generation: u32) {
    #[cfg(target_arch = "wasm32")]
    {
        let trigger = match plane {
            Some(true) => "niks3-write-credential-trigger",
            Some(false) => "niks3-read-credential-trigger",
            None => "",
        };
        // CONCURRENCY: Unmount-time focus can run before the parent loses inert,
        // and confirming a draft can replace the opener node. One frame after
        // the parent's committed transition resolves the current trigger. The
        // DOM owner and generation prevent a reopen or remount stealing focus.
        let _ = js_sys::eval(&format!(
            r#"(() => {{
                const parent = document.getElementById('cache-destination-dialog');
                if (!parent) return;
                cancelAnimationFrame(parent.__cfCredentialFocusFrame || 0);
                parent.__cfCredentialFocusGeneration = {generation};
                parent.__cfCredentialFocusFrame = 0;
                if (!'{trigger}') return;
                parent.__cfCredentialFocusFrame = requestAnimationFrame(() => {{
                    if (!parent.isConnected ||
                        document.getElementById('cache-destination-dialog') !== parent ||
                        parent.__cfCredentialFocusGeneration !== {generation}) return;
                    parent.__cfCredentialFocusFrame = 0;
                    if (document.getElementById('niks3-credential-dialog')) return;
                    const target = parent.querySelector('button[data-testid="{trigger}"]');
                    if (!target || target.matches(':disabled') ||
                        target.closest('[inert], [hidden], [aria-hidden="true"]') ||
                        !target.getClientRects().length) return;
                    target.focus();
                }});
            }})()"#
        ));
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (plane, generation);
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

fn pair(cert: &str, key: &str, _ca: &str, require: bool) -> Result<(), String> {
    let entered = !cert.trim().is_empty() || !key.trim().is_empty();
    if require || entered {
        if cert.trim().is_empty() || key.trim().is_empty() {
            return Err("Enter a client certificate and private key together.".into());
        }
        if !cert.contains("-----BEGIN CERTIFICATE-----") || !key.contains("PRIVATE KEY-----") {
            return Err("Client identity must contain a PEM certificate and private key.".into());
        }
    }
    // COMPATIBILITY: The server owns CA framing, X.509 and trust-annotation
    // validation. Do not duplicate its ordinary/TRUSTED bundle parser here.
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

fn validate_test_selection(
    form: &Niks3FormState,
    scope: Niks3ProbeScope,
    existing: bool,
    choice: Choice,
) -> Result<(), String> {
    validate_plane(form, scope, existing)?;
    if scope == Niks3ProbeScope::Write
        && form.write_mode == "token"
        && (!existing || choice == Choice::Draft)
        && form.token.trim().is_empty()
    {
        return Err("Enter an API token or explicitly use the current credential.".into());
    }
    Ok(())
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
            "{evidence_text(value, running)}"
        }
    } }
}

fn evidence_text(value: Option<bool>, running: bool) -> &'static str {
    if running {
        "Testing…"
    } else {
        match value {
            Some(true) => "Verified",
            Some(false) => "Failed",
            None => "Untested",
        }
    }
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
    let focus_owner = use_hook(|| Rc::new(Cell::new(CredentialFocusOwner::default())));
    use_effect(move || {
        let mut owner = focus_owner.get();
        let restore = owner.transition(modal());
        focus_owner.set(owner);
        restore_credential_focus(restore, owner.generation);
    });
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
    let mut pending = use_signal(|| None::<DiscoveryProposal>);
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
    let initial_draft = use_hook(move || {
        PureDraft::new(
            form(),
            Vec::new(),
            write_choice(),
            read_choice(),
            [parallel_input(), retries_input(), timeout_input()],
        )
    });
    let mut baseline = use_signal(|| None::<PureDraft>);
    let write_owner = use_hook(|| Rc::new(RefCell::new(PlaneOwner::default())));
    let read_owner = use_hook(|| Rc::new(RefCell::new(PlaneOwner::default())));
    let discovery_owner = use_hook(|| Rc::new(RefCell::new(PlaneOwner::default())));
    let id = destination.as_ref().map(|d| d.id);
    let scope_ready = environment_ready() && matches!(environments.read().as_ref(), Some(Ok(_)));
    let draft = PureDraft::new(
        form(),
        environment_ids(),
        write_choice(),
        read_choice(),
        [parallel_input(), retries_input(), timeout_input()],
    );
    let dirty = draft_is_dirty(baseline.read().as_ref(), &draft);
    // INVARIANT: The fresh GET and asynchronous scope fill establish one clean
    // baseline. Catalogue refresh, navigation and probe responses never rebase it.
    use_effect(move || {
        if destination_ready
            && environment_ready()
            && matches!(environments.read().as_ref(), Some(Ok(_)))
            && baseline.peek().is_none()
        {
            let mut initial = None;
            initialize_baseline(
                &mut initial,
                true,
                PureDraft::new(
                    initial_draft.form.clone(),
                    environment_ids(),
                    initial_draft.write,
                    initial_draft.read,
                    initial_draft.numbers.clone(),
                ),
            );
            baseline.set(initial);
        }
    });
    let observed_write = write_owner.clone();
    let observed_read = read_owner.clone();
    let observed_discovery = discovery_owner.clone();
    let write_input = use_memo(move || plane_input(&form(), true, write_choice()));
    let read_input = use_memo(move || plane_input(&form(), false, read_choice()));
    let public_input = use_memo(move || discovery_input(&form(), write_choice()));
    // CONCURRENCY: Observe after each bubbled edit event and credential commit,
    // before rendering can coalesce edit-away/revert into an unchanged memo.
    // The effect also observes external input updates. Peek keeps its
    // subscriptions limited to the plane projections, not the complete form.
    let invalidate_planes = EventHandler::new(move |_: ()| {
        let state = form.peek();
        if observed_write
            .borrow_mut()
            .observe(plane_input(&state, true, *write_choice.peek()))
        {
            write_result.set(None);
            if *busy.peek() == Some("write-test") {
                busy.set(None);
            }
        }
        if observed_read
            .borrow_mut()
            .observe(plane_input(&state, false, *read_choice.peek()))
        {
            read_result.set(None);
            pending.set(None);
            discovery_note.set(None);
            if *busy.peek() == Some("read-test") {
                busy.set(None);
            }
        }
        if observed_discovery
            .borrow_mut()
            .observe(discovery_input(&state, *write_choice.peek()))
        {
            pending.set(None);
            discovery_note.set(None);
            if *busy.peek() == Some("discover") {
                busy.set(None);
            }
        }
    });
    use_effect(move || {
        let _ = (write_input(), read_input(), public_input());
        invalidate_planes.call(());
    });
    let original_test = destination.clone();
    let test_write_owner = write_owner.clone();
    let test_read_owner = read_owner.clone();
    let test = EventHandler::new(move |scope: Niks3ProbeScope| {
        error.set(None);
        let ready = environment_ready() && matches!(environments.read().as_ref(), Some(Ok(_)));
        if let Err(message) = cache_action_ready(destination_ready, ready, busy().is_some())
            .and_then(|_| {
                validate_test_selection(
                    &form(),
                    scope,
                    id.is_some(),
                    if scope == Niks3ProbeScope::Write {
                        write_choice()
                    } else {
                        read_choice()
                    },
                )
            })
        {
            error.set(Some(format!("Test not run: {message}")));
            return;
        }
        let write = scope == Niks3ProbeScope::Write;
        let owner = if write {
            test_write_owner.clone()
        } else {
            test_read_owner.clone()
        };
        let operation = owner.borrow_mut().start(plane_input(
            &form(),
            write,
            if write { write_choice() } else { read_choice() },
        ));
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
            if !owner.borrow_mut().accepts(
                operation,
                plane_input(
                    &form(),
                    write,
                    if write { write_choice() } else { read_choice() },
                ),
            ) {
                return;
            }
            match response {
                Ok(value) => {
                    // A response for another plane cannot establish evidence.
                    if value.probe_scope != Some(scope) {
                        error.set(Some(
                            "Test returned no evidence for the selected plane.".into(),
                        ));
                        busy.set(None);
                        return;
                    }
                    if !value.ok {
                        error.set(Some(value.failure_message().into()));
                    }
                    if scope == Niks3ProbeScope::Write {
                        write_result.set(Some(value));
                    } else {
                        read_result.set(Some(value));
                    }
                }
                Err(failure) => error.set(Some(client::cache_probe_error_message(&failure).into())),
            }
            busy.set(None);
        });
    });
    let original_discovery = destination.clone();
    let discover_read_owner = read_owner.clone();
    let proposal_write_owner = discovery_owner.clone();
    let proposal_read_owner = read_owner.clone();
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
        let owner = discovery_owner.clone();
        let operation = owner
            .borrow_mut()
            .start(discovery_input(&form(), write_choice()));
        let read_owner = discover_read_owner.clone();
        // Discovery does not own the read probe sequence. Read observations on
        // unchanged inputs must not invalidate the resulting settings proposal.
        let read_stamp = read_owner
            .borrow()
            .stamp(plane_input(&form(), false, read_choice()));
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
        patch.niks3_auth_token = None;
        busy.set(Some("discover"));
        spawn(async move {
            let response = if let Some(id) = id {
                client::discover_stored_niks3(id, &patch).await
            } else {
                client::discover_niks3(&request).await
            };
            if !owner
                .borrow_mut()
                .accepts(operation, discovery_input(&form(), write_choice()))
            {
                return;
            }
            match response {
                Ok(value) => {
                    if !https(&value.substituter_url)
                        || value.public_keys.is_empty()
                        || value.public_keys.iter().any(|key| !valid_signing_key(key))
                    {
                        error.set(Some("Discovery returned invalid public metadata. Review the API URL and proxy configuration.".into()));
                    } else {
                        if !read_owner
                            .borrow()
                            .accepts_stamp(&read_stamp, plane_input(&form(), false, read_choice()))
                        {
                            pending.set(None);
                            discovery_note.set(Some("Discovery response ignored because read settings changed. Discover again to review current settings.".into()));
                        } else {
                            let proposal =
                                discovery_proposal(&form(), value, operation, read_stamp);
                            discovery_note.set(Some(if proposal.is_some() {
                                "Discovery successful. Review the proposed read URL and signing keys, then apply them explicitly. Draft settings are unchanged; nothing has been saved."
                            } else {
                                "Discovery successful. Public metadata matches the current read settings. Draft settings are unchanged; no write permission was tested."
                            }.into()));
                            pending.set(proposal);
                        }
                    }
                }
                Err(failure) => error.set(Some(format!(
                    "Discovery failed. {}",
                    client::cache_probe_error_message(&failure)
                ))),
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
    let write_original_mode = destination
        .as_ref()
        .and_then(|d| d.niks3_write_auth_mode.clone());
    let read_original_mode = destination
        .as_ref()
        .and_then(|d| d.niks3_read_auth_mode.clone());
    rsx! {
        div { class: "modal-backdrop", style: "padding:8px;", onclick: move |_| { if busy().is_none() && modal().is_none() { on_close.call(()); } },
            style { "{CACHE_FORM_CSS}" } style { "{CSS}" }
            div { id: "cache-destination-dialog", class: "pe-shell cache-form-shell n3-shell", role: "dialog", aria_modal: if nested { "false" } else { "true" }, aria_hidden: if nested { "true" } else { "false" }, "inert": if nested { Some("") } else { None }, aria_label: "Cache destination", tabindex: "-1", onclick: move |e| { e.stop_propagation(); invalidate_planes.call(()); }, oninput: move |_| invalidate_planes.call(()), onchange: move |_| invalidate_planes.call(()), onkeydown: move |e| { e.stop_propagation(); if e.key() == Key::Escape { e.prevent_default(); if busy().is_none() && modal().is_none() { on_close.call(()); } } else if e.key() == Key::Tab && busy().is_some() { e.prevent_default(); } },
                if restore_focus { DialogFocusRestore {} }
                DialogInitialFocus { dialog_id: "cache-destination-dialog".to_string() }
                DialogFocusSentinel { dialog_id: "cache-destination-dialog".to_string(), boundary: DialogFocusBoundary::Last }
                if busy().is_some() { DialogInitialFocus { dialog_id: "niks3-operation-status".to_string() } }
                header { class: "pe-head",
                    div { style: "min-width:0;flex:1;", div { class: "n3-title-row",
                        Icon { name: if id.is_some() { IconName::Gear } else { IconName::Plus }, size: 15 }
                        h2 { class: "pe-head-title", if form().name.is_empty() { "Add cache destination" } else { "{form().name}" } }
                        span { class: "chip chip-info", "Niks3" } if !form().enabled { span { class: "chip chip-unknown", "disabled" } }
                        if dirty { span { class: "chip chip-unknown", "Unsaved changes" } }
                        span { id: "niks3-operation-status", role: "status", aria_live: "polite", tabindex: "-1", style: if busy().is_some() { "" } else { "display:none;" }, class: "chip chip-unknown", if busy().is_some() { "Working" } }
                    } p { class: "pe-head-sub", if id.is_some() { "Update Niks3 destination." } else { "Register a Niks3 destination. Write and read are configured separately." } } }
                    button { class: "btn-icon focus-ring", aria_label: "Close", disabled: busy().is_some(), onclick: move |_| on_close.call(()), Icon { name: IconName::X, size: 16 } }
                }
                nav { class: "pe-rail", aria_label: "Cache form sections",
                    for (value, label, icon) in [("dest", "Destination", IconName::Download), ("write", "Write / API", IconName::Download), ("read", "Read / Pull", IconName::Link), ("trust", "Trust", IconName::Shield), ("adv", "Advanced", IconName::Gear)] {
                        button { class: if section() == value { "pe-rail-item focus-ring active" } else { "pe-rail-item focus-ring" }, aria_label: label, aria_current: section() == value, disabled: busy().is_some(), onclick: move |_| section.set(value),
                            if value == "write" { svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", path { d: "M12 16V3m-5 5 5-5 5 5M3 16v5h18v-5" } } } else { Icon { name: icon, size: 13 } }
                            span { class: "pe-rail-label", "{label}" }
                            if value == "write" { span { class: "pe-rail-badge", "{auth_label(&form().write_mode)}" } }
                            if value == "read" { span { class: "pe-rail-badge", "{auth_label(&form().read_mode)}" } }
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
                                    Some(Ok(values)) => rsx! { div { style: "display:flex;gap:6px;flex-wrap:wrap;", for env in values { button { key: "{env.id}", class: "n3-env focus-ring", style: format!("--n3-env-color:{};", environment_color(&env.color_hex)), aria_label: "{env.name}", aria_pressed: environment_ids().contains(&env.id), onclick: { let id = env.id; move |_| { let mut ids = environment_ids.write(); if ids.contains(&id) { ids.retain(|v| *v != id); } else { ids.push(id); } } }, span { class: "n3-env-dot" } "{env.name}" if environment_ids().contains(&env.id) { Icon { name: IconName::Check, size: 11 } } } } } },
                                    Some(Err(_)) => rsx! { p { role: "alert", "Environment list unavailable. Close and reopen." } },
                                    None => rsx! { p { "Loading environments…" } },
                                }
                            }
                            p { class: "help", if !scope_ready { "Scope not loaded. Saving is disabled." } else if environment_ids().is_empty() { "Global scope: no environment restriction. Select environments to restrict this destination." } else { "Configuration and selected environments are saved together." } }
                            label { class: "n3-toggle", input { r#type: "checkbox", checked: form().enabled, onchange: move |e| form.write().enabled = e.checked() } "Enabled" }
                        }
                        if section() == "write" {
                            div { class: "pe-sec-head", h3 { "Write / API" } p { "Where the Niks3 client publishes store paths. Presigned upload targets have their own HTTPS trust requirements." } }
                            div { class: "field", label { r#for: "n3-write-url", "Write / API URL" } div { style: "display:flex;gap:8px;",
                                input { id: "n3-write-url", class: "input focus-ring mono", style: "min-width:0;flex:1;", value: form().server_url, placeholder: "https://push.niks3.example.com", oninput: move |e| { form.write().server_url = e.value(); pending.set(None); discovery_note.set(None); } }
                                button { class: "btn btn-ghost focus-ring xs", disabled: form().server_url.trim().is_empty(), onclick: discover, if busy() == Some("discover") { "Discovering…" } else { "Discover" } }
                            } p { class: "help", "Discover uses selected write TLS, never a bearer token. It proposes read settings without changing the draft. Apply discovered settings explicitly before saving." } }
                            if let Some(note) = discovery_note() { div { class: "n3-disc", role: "status", "{note}" } }
                            if let Some(value) = pending() { div { class: "n3-disc", h4 { "Review discovered public metadata" }
                                p { class: "n3-public-value", "Read URL: {value.metadata.substituter_url}" }
                                for key in &value.metadata.public_keys { p { class: "n3-public-value mono", "{key}" } }
                                button { class: "btn btn-ghost focus-ring xs", onclick: move |_| {
                                    let Some(value) = pending() else { return };
                                    let applied = apply_discovery_proposal(&mut form.write(), &value,
                                        &proposal_write_owner.borrow(), &proposal_read_owner.borrow(), write_choice(), read_choice());
                                    pending.set(None);
                                    if applied {
                                        key_rows.set(value.metadata.public_keys.into_iter().enumerate().map(|(i, key)| (i as u64 + 1, key)).collect());
                                        invalidate_planes.call(());
                                        discovery_note.set(Some("Discovered settings applied to the draft. Review before saving.".into()));
                                    } else {
                                        discovery_note.set(Some("Proposal ignored because its inputs changed. Discover again before applying settings.".into()));
                                    }
                                }, "Apply discovered settings" }
                            } }
                            div { class: "field", label { "Write authentication" } div { class: "seg", role: "group", aria_label: "Write authentication",
                                for (value, label) in [("token", "API token"), ("mtls", "mTLS")] { button { class: if form().write_mode == value { "active focus-ring" } else { "focus-ring" }, aria_pressed: form().write_mode == value, onclick: { let original = write_original_mode.clone(); move |_| { let mut choice = write_choice(); if select_mode(&mut form.write(), true, value, original.as_deref(), &mut choice) { write_choice.set(choice); } } }, "{label}" } }
                            } }
                            CredentialRow { write: true, form, choice: write_choice, destination: destination.clone(), on_open: move |_| modal.set(Some(true)) }
                            div { class: "n3-plane", div { class: "n3-plane-head", strong { "Write API" } button { class: "btn btn-ghost focus-ring xs", disabled: !https(&form().server_url), onclick: move |_| test.call(Niks3ProbeScope::Write), if busy() == Some("write-test") { "Testing…" } else { "Test write API" } } }
                                Evidence { label: "Write API access", value: write_result().and_then(|r| r.write_api_reachable), running: busy() == Some("write-test") }
                                Evidence { label: "Authentication", value: write_result().and_then(|r| r.write_authn_valid), running: busy() == Some("write-test") }
                                Evidence { label: "Write authorization", value: write_result().and_then(|r| r.write_authorization_valid), running: busy() == Some("write-test") }
                                Evidence { label: "Object publication", value: None, running: false }
                            }
                            if let Some(message) = write_result().and_then(|r| r.write_probe_diagnostic()) {
                                p { class: "help", role: "status", "{message}" }
                            }
                            p { class: "help", "Write API access checks the API route and method. Each typed result reports its own evidence. This test does not validate S3 upload, signing or object publication; publication remains Untested. Public discovery alone does not prove authentication or authorization." }
                        }
                        if section() == "read" {
                            div { class: "pe-sec-head", h3 { "Read / Pull" } p { "Nix client → Niks3 read endpoint → storage. This endpoint can have a different hostname from the write API." } }
                            div { class: "field", label { r#for: "n3-read-url", "Read / substituter URL" } input { id: "n3-read-url", class: "input focus-ring mono", value: form().read_url, placeholder: "https://cache.example.com", oninput: move |e| form.write().read_url = e.value() } p { class: "help", "Never embed a username or password in the URL." } }
                            div { class: "field", label { "Read authentication" } div { class: "seg", role: "group", aria_label: "Read authentication",
                                for (value, label) in [("none", "Public"), ("basic", "Basic"), ("mtls", "mTLS")] { button { class: if form().read_mode == value { "active focus-ring" } else { "focus-ring" }, aria_pressed: form().read_mode == value, onclick: { let original = read_original_mode.clone(); move |_| { let mut choice = read_choice(); if select_mode(&mut form.write(), false, value, original.as_deref(), &mut choice) { read_choice.set(choice); } } }, "{label}" } }
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
                            label { class: "n3-toggle", input { r#type: "checkbox", checked: form().require_sigs, onchange: move |e| form.write().require_sigs = e.checked() } "Require signatures" }
                            p { class: "help", "Only pull paths accepted by the selected Nix signature policy and trusted keys. Save requires at least one valid signing key." }
                        }
                        if let Some(message) = error() { p { role: "alert", "{message}" } }
                    }
                }
                footer { class: "pe-foot",
                    div { class: "pe-foot-state", style: CACHE_FORM_FOOT_STATE_STYLE,
                        span { "{form().name}" span { class: "pe-foot-dot", "·" } "write {auth_label(&form().write_mode)}" span { class: "pe-foot-dot", "·" } "read {auth_label(&form().read_mode)}" span { class: "pe-foot-dot", "·" }
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
                    let mut choice = if write { write_choice() } else { read_choice() };
                    confirm_credential(&mut form.write(), write, Some(value), &mut choice);
                    if write { write_choice.set(choice); } else { read_choice.set(choice); }
                    invalidate_planes.call(());
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
                button { class: "btn btn-ghost focus-ring xs", "data-testid": if write { "niks3-write-credential-trigger" } else { "niks3-read-credential-trigger" }, onclick: move |_| on_open.call(()), if choice() == Choice::Draft { "Edit replacement" } else if has_current { "Replace" } else { "Add credential" } }
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
        div { id: "niks3-credential-dialog", class: "modal n3-credential", role: "dialog", aria_modal: "true", aria_label: "{plane} credential", tabindex: "-1", onclick: move |e| e.stop_propagation(), onkeydown: move |e| { e.stop_propagation(); if e.key() == Key::Escape { e.prevent_default(); on_close.call(None); } },
            style { "{CSS}" }
            // The parent restores the current trigger after its inert state and
            // credential selection commit. Do not capture the old opener node.
            DialogInitialFocus { dialog_id: "niks3-credential-dialog".to_string() }
            DialogFocusSentinel { dialog_id: "niks3-credential-dialog".to_string(), boundary: DialogFocusBoundary::Last }
            div { class: "modal-head", h2 { Icon { name: IconName::Key, size: 14 } if has_current { " Replace" } else { " Add" } " {plane} credential · {auth_label(&mode)}" } p { "Stored secrets are never shown. This local draft is not persisted until the cache Save action." } }
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
            dt { "Authentication" } dd { "{auth_label(write_mode)}" span { class: "chip chip-unknown", if write_configured { "Current configured credential" } else { "Stored credential status unavailable" } } }
        } }
        section { class: "panel-section", h3 { "Read / Pull" } dl { class: "kv-grid",
            dt { "Substituter URL" } dd { class: "mono", style: "overflow-wrap:anywhere;", "{read_url}" }
            dt { "Authentication" } dd { if read_mode == "none" { "Public · no credential needed" } else { "{auth_label(read_mode)}" span { class: "chip chip-unknown", if read_configured { "Current configured credential" } else { "Stored credential status unavailable" } } } }
        } }
        section { class: "panel-section", h3 { "Trusted signing public keys ({destination.niks3_public_keys.len()})" }
            for key in &destination.niks3_public_keys { p { class: "mono", style: "font-size:11px;overflow-wrap:anywhere;", "{key}" } }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(form: Niks3FormState, ids: Vec<Uuid>) -> PureDraft {
        let numbers = [
            form.parallel_uploads.to_string(),
            form.max_retries.to_string(),
            form.push_timeout_seconds.to_string(),
        ];
        PureDraft::new(form, ids, Choice::Current, Choice::Current, numbers)
    }

    fn public_metadata() -> Niks3Discovery {
        Niks3Discovery {
            server_url: "https://write.example.test".into(),
            substituter_url: "https://read.example.test".into(),
            public_keys: vec!["fixture:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into()],
            oidc_audience: None,
        }
    }

    #[test]
    fn discovery_observation_is_clean_for_blank_different_and_matching_read_settings() {
        for existing in [false, true] {
            let mut form = Niks3FormState::from_destination(None);
            form.name = "actual draft name".into();
            form.token = "retained local replacement".into();
            if existing {
                form.read_url = "https://existing.example.test".into();
                form.keys = "existing key".into();
            }
            let initial = snapshot(form.clone(), vec![]);
            let mut write_owner = PlaneOwner::default();
            let mut read_owner = PlaneOwner::default();
            let operation = write_owner.start(discovery_input(&form, Choice::Current));
            read_owner.observe(plane_input(&form, false, Choice::Current));
            let stamp = read_owner.stamp(plane_input(&form, false, Choice::Current));
            let proposal = discovery_proposal(&form, public_metadata(), operation, stamp).unwrap();
            assert!(!draft_is_dirty(
                Some(&initial),
                &snapshot(form.clone(), vec![])
            ));
            assert!(form == initial.form);
            // Testing unchanged read inputs changes operation sequence only.
            read_owner.start(plane_input(&form, false, Choice::Current));
            assert!(apply_discovery_proposal(
                &mut form,
                &proposal,
                &write_owner,
                &read_owner,
                Choice::Current,
                Choice::Current
            ));
            assert!(draft_is_dirty(
                Some(&initial),
                &snapshot(form.clone(), vec![])
            ));
            assert_eq!(form.name, initial.form.name);
            assert!(form.token == initial.form.token);
            read_owner.observe(plane_input(&form, false, Choice::Current));
            let applied = snapshot(form.clone(), vec![]);
            assert!(
                discovery_proposal(
                    &form,
                    public_metadata(),
                    operation,
                    read_owner.stamp(plane_input(&form, false, Choice::Current))
                )
                .is_none()
            );
            assert!(!draft_is_dirty(Some(&applied), &snapshot(form, vec![])));
        }
    }

    #[test]
    fn stale_discovery_proposals_cannot_overwrite_new_or_reverted_read_settings() {
        for edit in ["read", "read_revert", "write", "new_discovery"] {
            let mut form = Niks3FormState::from_destination(None);
            let mut write_owner = PlaneOwner::default();
            let mut read_owner = PlaneOwner::default();
            let operation = write_owner.start(discovery_input(&form, Choice::Current));
            read_owner.observe(plane_input(&form, false, Choice::Current));
            let stamp = read_owner.stamp(plane_input(&form, false, Choice::Current));
            let proposal =
                discovery_proposal(&form, public_metadata(), operation, stamp.clone()).unwrap();
            match edit {
                "read" | "read_revert" => {
                    form.read_url = "https://new-draft.example.test".into();
                    read_owner.observe(plane_input(&form, false, Choice::Current));
                    if edit == "read_revert" {
                        form.read_url.clear();
                        read_owner.observe(plane_input(&form, false, Choice::Current));
                    }
                    // The response boundary rejects the captured read stamp.
                    assert!(
                        !read_owner
                            .accepts_stamp(&stamp, plane_input(&form, false, Choice::Current))
                    );
                }
                "write" => {
                    form.server_url = "https://new-write.example.test".into();
                    write_owner.observe(discovery_input(&form, Choice::Current));
                }
                _ => {
                    write_owner.start(discovery_input(&form, Choice::Current));
                }
            }
            let before = form.clone();
            assert!(!apply_discovery_proposal(
                &mut form,
                &proposal,
                &write_owner,
                &read_owner,
                Choice::Current,
                Choice::Current
            ));
            assert!(form == before);
        }
    }

    #[test]
    fn initial_scope_fill_is_clean_and_catalogue_refresh_never_rebases_edits() {
        let form = Niks3FormState::from_destination(None);
        let mut initial = None;
        let before_scope = snapshot(form.clone(), vec![]);
        initialize_baseline(&mut initial, false, before_scope.clone());
        assert!(initial.is_none());
        assert!(!draft_is_dirty(initial.as_ref(), &before_scope));
        let loaded = snapshot(form, vec![Uuid::from_u128(2), Uuid::from_u128(1)]);
        initialize_baseline(&mut initial, true, loaded.clone());
        assert!(!draft_is_dirty(initial.as_ref(), &loaded));
        let mut edited = loaded.clone();
        edited.form.name.push_str("changed");
        initialize_baseline(&mut initial, true, edited.clone());
        assert!(draft_is_dirty(initial.as_ref(), &edited));
        edited.form.name = loaded.form.name.clone();
        assert!(!draft_is_dirty(initial.as_ref(), &edited));
        let reordered = snapshot(
            loaded.form.clone(),
            vec![Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(1)],
        );
        assert!(!draft_is_dirty(initial.as_ref(), &reordered));
        assert!(draft_is_dirty(
            initial.as_ref(),
            &snapshot(loaded.form, vec![])
        ));
    }

    #[test]
    fn draft_comparison_includes_secrets_selection_clear_intent_and_raw_numbers() {
        let initial = snapshot(Niks3FormState::from_destination(None), vec![]);
        let mut edited = initial.clone();
        edited.form.token = "local replacement".into();
        edited.write = Choice::Draft;
        assert!(draft_is_dirty(Some(&initial), &edited));
        clear_identity(&mut edited.form, true);
        edited.write = Choice::Current;
        assert!(!draft_is_dirty(Some(&initial), &edited));
        edited.read = Choice::None;
        assert!(draft_is_dirty(Some(&initial), &edited));
        edited.read = Choice::Current;
        edited.form.clear_read_ca = true;
        assert!(draft_is_dirty(Some(&initial), &edited));
        edited.form.clear_read_ca = false;
        for invalid in ["", "1.5", "2147483648"] {
            edited.numbers[0] = invalid.into();
            assert!(draft_is_dirty(Some(&initial), &edited));
            assert!(
                validate_advanced_input(&edited.numbers[0], &edited.numbers[1], &edited.numbers[2])
                    .is_err()
            );
        }
        edited.numbers = initial.numbers.clone();
        assert!(!draft_is_dirty(Some(&initial), &edited));
    }

    #[test]
    fn credential_dialog_cancel_and_observation_state_do_not_edit_the_baseline() {
        let initial = snapshot(Niks3FormState::from_destination(None), vec![]);
        let mut live = initial.clone();
        let mut focus = CredentialFocusOwner::default();
        for plane in [true, false] {
            focus.transition(Some(plane));
            let mut unconfirmed = live.form.clone();
            if plane {
                unconfirmed.token = "unconfirmed".into();
            } else {
                unconfirmed.basic_password = "unconfirmed".into();
            }
            let mut choice = if plane { live.write } else { live.read };
            confirm_credential(&mut live.form, plane, None, &mut choice);
            assert!(choice == Choice::Current);
            focus.transition(None);
            assert!(!draft_is_dirty(Some(&initial), &live));
            // A confirmed replacement changes only its plane, not public fields.
            let mut confirmed = live.form.clone();
            confirm_credential(&mut confirmed, plane, Some(unconfirmed), &mut choice);
            assert!(choice == Choice::Draft);
            assert_eq!(confirmed.name, live.form.name);
            if plane {
                assert!(confirmed.basic_password.is_empty());
            } else {
                assert!(confirmed.token.is_empty());
            }
        }
        let mut owner = PlaneOwner::default();
        let operation = owner.start(plane_input(&live.form, true, live.write));
        assert!(owner.accepts(operation, plane_input(&live.form, true, live.write)));
        assert!(!draft_is_dirty(Some(&initial), &live));
    }

    #[test]
    fn same_mode_is_noop_and_reverting_restores_retention_without_erasing_drafts() {
        for (write, mode, other) in [
            (true, "token", "mtls"),
            (false, "basic", "mtls"),
            (false, "mtls", "none"),
        ] {
            let mut form = Niks3FormState::from_destination(None);
            if write {
                form.write_mode = mode.into();
            } else {
                form.read_mode = mode.into();
            }
            let original = form.clone();
            let mut choice = Choice::Current;
            assert!(!select_mode(
                &mut form,
                write,
                mode,
                Some(mode),
                &mut choice
            ));
            assert!(form == original && choice == Choice::Current);
            assert!(select_mode(
                &mut form,
                write,
                other,
                Some(mode),
                &mut choice
            ));
            assert!(select_mode(&mut form, write, mode, Some(mode), &mut choice));
            assert!(form == original && choice == Choice::Current);
        }
        let mut form = Niks3FormState::from_destination(None);
        form.token = "replacement".into();
        let mut choice = Choice::Draft;
        assert!(!select_mode(
            &mut form,
            true,
            "token",
            Some("token"),
            &mut choice
        ));
        assert!(choice == Choice::Draft && form.token == "replacement");
        select_mode(&mut form, true, "mtls", Some("token"), &mut choice);
        select_mode(&mut form, true, "token", Some("token"), &mut choice);
        assert!(choice == Choice::Draft && form.token == "replacement");
        let retained = Niks3FormState::from_destination(None).update_request();
        assert!(retained.niks3_auth_token.is_none());
        assert!(
            !retained.clear_niks3_auth_token
                && !retained.clear_niks3_write_client_key
                && !retained.clear_niks3_read_client_key
        );
    }

    #[test]
    fn plane_inputs_isolate_evidence_from_name_scope_numbers_and_other_plane() {
        let mut form = Niks3FormState::from_destination(None);
        let write = plane_input(&form, true, Choice::Current);
        let read = plane_input(&form, false, Choice::Current);
        form.name = "new name".into();
        form.enabled = false;
        form.parallel_uploads += 1;
        assert!(write == plane_input(&form, true, Choice::Current));
        assert!(read == plane_input(&form, false, Choice::Current));
        form.keys = "changed key".into();
        assert!(write == plane_input(&form, true, Choice::Current));
        assert!(read != plane_input(&form, false, Choice::Current));
        let public = discovery_input(&form, Choice::Current);
        form.token = "new token".into();
        assert!(public == discovery_input(&form, Choice::Draft));
        assert!(write != plane_input(&form, true, Choice::Draft));
    }

    #[test]
    fn stale_operations_cannot_publish_success_failure_or_busy_completion() {
        for write in [true, false] {
            let mut owner = PlaneOwner::default();
            let mut form = Niks3FormState::from_destination(None);
            let input = plane_input(&form, write, Choice::Current);
            let old = owner.start(input.clone());
            assert!(owner.accepts(old, input.clone()));
            if write {
                form.server_url.push_str("/changed");
            } else {
                form.keys.push_str("changed");
            }
            let changed = plane_input(&form, write, Choice::Current);
            // A response between the edit and its effect must also be rejected.
            assert!(!owner.accepts(old, changed.clone()));
            owner.observe(changed);
            owner.observe(input.clone());
            for _outcome in ["success", "failure", "busy completion"] {
                assert!(!owner.accepts(old, input.clone()));
            }
            let next = owner.start(input.clone());
            let latest = owner.start(input.clone());
            assert!(!owner.accepts(next, input.clone()));
            assert!(owner.accepts(latest, input));
        }
    }

    #[test]
    fn selected_tls_identity_trust_and_mode_transitions_fence_reverted_results() {
        for write in [true, false] {
            let mut original = Niks3FormState::from_destination(None);
            original.write_mode = "mtls".into();
            original.read_mode = "mtls".into();
            for field in ["endpoint", "mode", "certificate", "key", "ca", "clear"] {
                let input = plane_input(&original, write, Choice::Current);
                let other = plane_input(&original, !write, Choice::Current);
                let mut owner = PlaneOwner::default();
                let operation = owner.start(input.clone());
                let mut edited = original.clone();
                match (write, field) {
                    (true, "endpoint") => edited.server_url.push_str("changed"),
                    (false, "endpoint") => edited.read_url.push_str("changed"),
                    (true, "mode") => edited.write_mode = "token".into(),
                    (false, "mode") => edited.read_mode = "basic".into(),
                    (true, "certificate") => edited.write_cert = "local".into(),
                    (false, "certificate") => edited.read_cert = "local".into(),
                    (true, "key") => edited.write_key = "local".into(),
                    (false, "key") => edited.read_key = "local".into(),
                    (true, "ca") => edited.write_ca = "local".into(),
                    (false, "ca") => edited.read_ca = "local".into(),
                    (true, _) => edited.clear_write_ca = true,
                    (false, _) => edited.clear_read_ca = true,
                }
                assert!(other == plane_input(&edited, !write, Choice::Current));
                assert!(owner.observe(plane_input(&edited, write, Choice::Current)));
                owner.observe(input.clone());
                assert!(!owner.accepts(operation, input));
            }
        }
    }

    #[test]
    fn typed_api_evidence_never_infers_authentication_or_publication() {
        for (json, expected) in [
            (serde_json::json!(true), "Verified"),
            (serde_json::json!(false), "Failed"),
            (serde_json::Value::Null, "Untested"),
        ] {
            let result: CacheCredentialTestResult = serde_json::from_value(serde_json::json!({
                "ok": true, "message": "untrusted response", "probe_scope": "write",
                "write_api_reachable": json, "write_authn_valid": json,
                "write_authorization_valid": json
            }))
            .unwrap();
            assert_eq!(evidence_text(result.write_api_reachable, false), expected);
            assert_eq!(evidence_text(result.write_authn_valid, false), expected);
            assert_eq!(
                evidence_text(result.write_authorization_valid, false),
                expected
            );
            assert_eq!(evidence_text(None, false), "Untested");
        }
        let legacy: CacheCredentialTestResult = serde_json::from_value(serde_json::json!({
            "ok": true, "message": "legacy metadata success", "server_reachable": true,
            "discovery_valid": true
        }))
        .unwrap();
        assert_eq!(evidence_text(legacy.write_api_reachable, false), "Untested");
        assert_eq!(evidence_text(legacy.write_authn_valid, false), "Untested");
        assert_eq!(
            evidence_text(legacy.write_authorization_valid, false),
            "Untested"
        );
        assert_eq!(evidence_text(Some(true), true), "Testing…");
        for invalid in [serde_json::json!("true"), serde_json::json!(1)] {
            assert!(
                serde_json::from_value::<CacheCredentialTestResult>(serde_json::json!({
                    "ok": true, "message": "", "write_api_reachable": invalid
                }))
                .is_err()
            );
        }
    }

    #[test]
    fn write_diagnostic_allowlist_never_uses_server_prose_or_decorated_codes() {
        use crate::api::models::cache_diagnostic_message;
        for code in [
            "token_rejected",
            "mtls_subject_rejected",
            "proxy_access_denied",
            "method_rejected",
            "route_unsupported",
            "unexpected_write_response",
            "auth_control_inconclusive",
            "write_route_verified",
            "write_route_verified_certificate_gate",
            "probe_response_too_large",
            "read_access_verified",
            "write_credentials_missing",
            "write_credentials_invalid",
        ] {
            assert!(cache_diagnostic_message(code).is_some());
            assert!(cache_diagnostic_message(&format!("{code} secret")).is_none());
            assert!(cache_diagnostic_message(&code.to_uppercase()).is_none());
        }
    }

    #[test]
    fn per_write_diagnostic_requires_write_provenance_and_ignores_aggregate_read_codes() {
        use crate::api::models::cache_diagnostic_message;
        let decode = |scope: serde_json::Value, write: serde_json::Value| {
            serde_json::from_value::<CacheCredentialTestResult>(serde_json::json!({
                "ok": false, "message": "forbidden upstream prose", "probe_scope": scope,
                "diagnostic_code": "read_access_verified", "write_probe_diagnostic_code": write,
                "write_api_reachable": true, "write_authn_valid": false,
                "write_authorization_valid": false
            }))
            .unwrap()
        };
        for scope in ["write", "all"] {
            let result = decode(
                serde_json::json!(scope),
                serde_json::json!("token_rejected"),
            );
            assert_eq!(
                result.write_probe_diagnostic(),
                cache_diagnostic_message("token_rejected")
            );
            assert_eq!(evidence_text(result.write_api_reachable, false), "Verified");
            assert_eq!(evidence_text(result.write_authn_valid, false), "Failed");
        }
        for scope in [serde_json::json!("read"), serde_json::Value::Null] {
            assert!(
                decode(scope, serde_json::json!("token_rejected"))
                    .write_probe_diagnostic()
                    .is_none()
            );
        }
        for invalid in [
            serde_json::json!("read_access_verified"),
            serde_json::json!("token_rejected secret"),
            serde_json::json!("TOKEN_REJECTED"),
            serde_json::json!(true),
            serde_json::json!(27),
            serde_json::Value::Null,
        ] {
            let result = decode(serde_json::json!("all"), invalid);
            assert!(result.write_probe_diagnostic_code.is_none());
            assert!(result.write_probe_diagnostic().is_none());
        }
        let mut standalone: CacheCredentialTestResult = serde_json::from_value(serde_json::json!({
            "ok": false, "message": "", "probe_scope": "write", "diagnostic_code": "method_rejected"
        }))
        .unwrap();
        assert!(standalone.write_probe_diagnostic_code.is_none());
        assert_eq!(
            standalone.write_probe_diagnostic(),
            cache_diagnostic_message("method_rejected")
        );
        standalone.probe_scope = Some(Niks3ProbeScope::All);
        assert!(standalone.write_probe_diagnostic().is_none());
    }

    #[test]
    fn local_credential_failure_keeps_api_reachability_untested_and_never_claims_upstream_rejection()
     {
        for code in ["write_credentials_missing", "write_credentials_invalid"] {
            let result: CacheCredentialTestResult = serde_json::from_value(serde_json::json!({
                "ok": false, "message": "", "probe_scope": "write", "diagnostic_code": code,
                "write_api_reachable": null, "status_code": null,
                "write_authn_valid": false, "write_authorization_valid": false
            }))
            .unwrap();
            assert_eq!(evidence_text(result.write_api_reachable, false), "Untested");
            assert_eq!(evidence_text(result.write_authn_valid, false), "Failed");
            assert_eq!(
                evidence_text(result.write_authorization_valid, false),
                "Failed"
            );
            assert!(result.status_code.is_none());
            assert!(
                result
                    .write_probe_diagnostic()
                    .unwrap()
                    .contains("endpoint was not contacted")
            );
        }
        let gate: CacheCredentialTestResult = serde_json::from_value(serde_json::json!({
            "ok": true, "message": "", "probe_scope": "write",
            "diagnostic_code": "write_route_verified_certificate_gate"
        }))
        .unwrap();
        let message = gate.write_probe_diagnostic().unwrap();
        assert!(message.contains("Native Go subject enforcement is not proved"));
        assert!(message.contains("publication remain untested"));
    }

    #[test]
    fn labels_and_environment_colors_use_wire_modes_and_safe_real_metadata() {
        for (mode, label) in [
            ("token", "API token"),
            ("mtls", "mTLS"),
            ("none", "Public"),
            ("basic", "Basic"),
        ] {
            assert_eq!(auth_label(mode), label);
        }
        assert_eq!(environment_color(" #A1b2C3 "), "#A1b2C3");
        for invalid in ["", "red", "#12", "#abcdef;display:none", "url(secret)"] {
            assert_eq!(environment_color(invalid), normalize_env_color(""));
        }
    }

    #[test]
    fn credential_focus_owner_restores_only_the_closed_plane_and_invalidates_reopens() {
        let mut owner = CredentialFocusOwner::default();
        assert_eq!(owner.transition(None), None);
        for plane in [true, false] {
            assert_eq!(owner.transition(Some(plane)), None);
            assert_eq!(owner.transition(None), Some(plane));
            let pending_generation = owner.generation;
            assert_eq!(owner.transition(Some(!plane)), None);
            assert_ne!(owner.generation, pending_generation);
            assert_eq!(owner.transition(None), Some(!plane));
            assert_eq!(owner.transition(None), None);
        }
    }

    #[test]
    fn ca_payloads_preserve_delimiter_columns_and_line_endings_in_every_projection() {
        for raw in [
            " -----BEGIN CERTIFICATE-----\r\nAQ==\r\n-----END CERTIFICATE-----\r\n",
            "\t-----BEGIN CERTIFICATE-----\nAQ==\n-----END CERTIFICATE-----\n ",
            "\r\n-----BEGIN CERTIFICATE-----\r\nAQ==\r\n-----END CERTIFICATE-----\r\n\t",
        ] {
            let mut form = Niks3FormState::from_destination(None);
            form.write_mode = "mtls".into();
            form.read_mode = "mtls".into();
            form.write_ca = raw.into();
            form.read_ca = raw.into();
            form.server_url = " https://write.example.com/ ".into();
            form.read_url = " https://read.example.com/ ".into();
            form.write_cert = " certificate ".into();
            form.write_key = " key ".into();
            let create = form.request();
            assert_eq!(create.niks3_write_ca_cert.as_deref(), Some(raw));
            assert_eq!(create.niks3_read_ca_cert.as_deref(), Some(raw));
            assert_eq!(
                create.niks3_server_url.as_deref(),
                Some("https://write.example.com/")
            );
            assert_eq!(
                create.niks3_write_client_cert.as_deref(),
                Some("certificate")
            );
            assert_eq!(create.niks3_write_client_key.as_deref(), Some("key"));
            let save = cache_update_patch(
                "niks3",
                &form,
                &CacheTypeDrafts::default(),
                false,
                None,
                vec![],
            );
            for wire in [
                serde_json::to_value(&create).unwrap(),
                serde_json::to_value(&save).unwrap(),
            ] {
                assert_eq!(wire["niks3_write_ca_cert"], raw);
                assert_eq!(wire["niks3_read_ca_cert"], raw);
            }
            for scope in [Niks3ProbeScope::Write, Niks3ProbeScope::Read] {
                let field = if scope == Niks3ProbeScope::Write {
                    "niks3_write_ca_cert"
                } else {
                    "niks3_read_ca_cert"
                };
                for wire in [
                    serde_json::to_value(project_create(form.request(), scope)).unwrap(),
                    serde_json::to_value(project_update(save.clone(), scope)).unwrap(),
                ] {
                    assert_eq!(wire[field], raw);
                }
            }
            let discovery = serde_json::to_value(discovery_request(&form)).unwrap();
            assert_eq!(discovery["niks3_write_ca_cert"], raw);
            let stored_discovery =
                serde_json::to_value(project_update(save, Niks3ProbeScope::Write)).unwrap();
            assert_eq!(stored_discovery["niks3_write_ca_cert"], raw);
        }
    }

    #[test]
    fn blank_inactive_and_retained_ca_drafts_are_omitted_without_implicit_clear() {
        let original: CacheDestination = serde_json::from_value(serde_json::json!({
            "id": 1, "name": "fixture", "cache_type": "Niks3", "enabled": true,
            "created_at": "2026-10-02T00:00:00Z", "updated_at": "2026-10-02T00:00:00Z",
            "niks3_write_auth_mode": "mtls", "niks3_read_auth_mode": "mtls",
            "niks3_write_ca_cert": "stored-write-ca", "niks3_read_ca_cert": "stored-read-ca"
        }))
        .unwrap();
        let mut form = Niks3FormState::from_destination(Some(&original));
        assert!(form.write_ca.is_empty());
        assert!(form.read_ca.is_empty());
        for blank in ["", " \t\r\n"] {
            form.write_ca = blank.into();
            form.read_ca = blank.into();
            let patch = form.update_request();
            assert!(patch.niks3_write_ca_cert.is_none());
            assert!(patch.niks3_read_ca_cert.is_none());
            assert!(!patch.clear_niks3_write_ca_cert);
            assert!(!patch.clear_niks3_read_ca_cert);
        }
        form.write_ca = " raw inactive write CA ".into();
        form.read_ca = " raw inactive read CA ".into();
        form.write_mode = "token".into();
        form.read_mode = "none".into();
        let create = form.request();
        assert!(create.niks3_write_ca_cert.is_none());
        assert!(create.niks3_read_ca_cert.is_none());
    }

    #[test]
    fn ca_drafts_use_server_validation_without_weakening_client_pair_markers() {
        for ca in [
            "-----BEGIN CERTIFICATE-----\nordinary\n-----END CERTIFICATE-----\n-----BEGIN TRUSTED CERTIFICATE-----\ntrusted\n-----END TRUSTED CERTIFICATE-----",
            "untrusted malformed CA draft",
        ] {
            assert!(pair("", "", ca, false).is_ok());
        }
        assert!(
            pair(
                "-----BEGIN CERTIFICATE-----",
                "-----BEGIN PRIVATE KEY-----",
                "",
                true
            )
            .is_ok()
        );
        assert!(
            pair(
                "-----BEGIN TRUSTED CERTIFICATE-----",
                "-----BEGIN PRIVATE KEY-----",
                "",
                true
            )
            .is_err()
        );
        assert!(pair("-----BEGIN CERTIFICATE-----", "", "", true).is_err());
    }

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
    fn discovery_excludes_tokens_and_tests_exclude_untested_plane_material() {
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
        form.write_mode = "token".into();
        for wire in [
            serde_json::to_value(project_update(
                form.update_request(),
                Niks3ProbeScope::Write,
            ))
            .unwrap(),
            serde_json::to_value(project_create(form.request(), Niks3ProbeScope::Write)).unwrap(),
        ] {
            assert_eq!(wire["niks3_auth_token"], "fixture-write-token");
            assert!(wire.get("niks3_read_basic_password").is_none());
        }
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
