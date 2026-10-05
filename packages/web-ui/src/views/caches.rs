//! Cache management view - configure cache destinations and monitor push jobs.
//!
//! Niks3 keeps write API and Nix read authentication independent. Edit forms
//! contain replacements only; configured flags describe retained server secrets.
//! Discovery populates public metadata without saving. Non-mutating probes cannot
//! establish write permission, so absent authorization results remain untested.
//! Cache forms save configuration and environment scope in one API transaction. A
//! failed save retains the draft and does not publish a transient global cache.

use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::api::client::{self, ApiClientError};
use crate::api::models::{
    CacheDestination, CachePushJob, CreateCacheDestination, EnvironmentSummary, SortOrder,
    SystemSummary, SystemsListParams, UpdateCacheDestination,
};
use crate::components::dialog_focus::{
    DialogFocusBoundary, DialogFocusRestore, DialogFocusSentinel, DialogInitialFocus,
};
use crate::components::icon::{Icon, IconName};
use crate::routes::Route;
use crate::theme;

// Override the production overlay's 32px padding so the nested popup fits
// narrow viewports. Keep excess content scrollable without moving focus out.
const CACHE_CREDENTIAL_BACKDROP_STYLE: &str = "padding:8px;";
const CACHE_CREDENTIAL_DIALOG_STYLE: &str =
    "width:min(520px,calc(100vw - 16px));max-height:92vh;overflow-y:auto;";
const CACHE_CREDENTIAL_ICON_STYLE: &str =
    "margin-right:6px;vertical-align:text-bottom;display:inline-block;";
// INVARIANT: The mobile stylesheet owns the footer's flex-basis. An inline
// flex shorthand would override the full-width validation reason.
const CACHE_FORM_FOOT_STATE_STYLE: &str = "flex-grow:1;min-width:0;";

// The policy-editor shell is not part of the production stylesheet. Keep this
// design-parity styling local to the cache form, including its narrow layout.
const CACHE_FORM_CSS: &str = r#"
.cache-form-shell { width:min(1120px,96vw);height:min(88vh,900px);display:grid;grid-template-columns:236px minmax(0,1fr);grid-template-rows:auto minmax(0,1fr) auto;grid-template-areas:"head head" "rail body" "foot foot";background:var(--cf-card-bg);border:1px solid var(--cf-card-border);border-radius:14px;overflow:hidden;box-shadow:0 24px 64px rgba(0,0,0,.45); }
.cache-form-shell .pe-head { grid-area:head;display:flex;align-items:flex-start;justify-content:space-between;gap:12px;padding:15px 18px;border-bottom:1px solid var(--cf-divider);background:color-mix(in oklab,var(--cf-page-bg) 45%,var(--cf-card-bg)); }
.cache-form-shell .pe-head-title { margin:0;font-size:15px;font-weight:700;letter-spacing:-.01em;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap; }
.cache-form-shell .pe-head-sub { margin:3px 0 0;font-size:12px;color:var(--cf-text-muted); }
.cache-form-shell .pe-rail { grid-area:rail;border-right:1px solid var(--cf-divider);padding:12px 10px;display:flex;flex-direction:column;gap:2px;overflow:auto;background:color-mix(in oklab,var(--cf-page-bg) 30%,var(--cf-card-bg)); }
.cache-form-shell .pe-rail-item { all:unset;cursor:pointer;box-sizing:border-box;width:100%;display:grid;grid-template-columns:15px 1fr auto;align-items:center;gap:8px;padding:7px 9px;border-radius:7px;font-size:12.5px;color:var(--cf-text-secondary); }
.cache-form-shell .pe-rail-item:hover { background:var(--cf-hover-bg);color:var(--cf-text-primary); }
.cache-form-shell .pe-rail-item.active { background:color-mix(in oklab,var(--cf-brand-purple) 14%,transparent);color:var(--cf-text-primary);font-weight:600; }
.cache-form-shell .pe-rail-item:focus-visible { outline:2px solid var(--cf-brand-purple);outline-offset:-2px; }
.cache-form-shell .pe-rail-item:disabled { opacity:.5;cursor:default; }
.cache-form-shell .pe-rail-label { overflow:hidden;text-overflow:ellipsis;white-space:nowrap; }
.cache-form-shell .pe-rail-badge { font-size:10px;font-variant-numeric:tabular-nums;color:var(--cf-text-muted);background:var(--cf-subtle-bg);border-radius:999px;padding:1px 7px;white-space:nowrap; }
.cache-form-shell .pe-rail-badge.warn { color:var(--cf-amber);background:color-mix(in oklab,var(--cf-amber) 16%,transparent); }
.cache-form-shell .pe-body { grid-area:body;overflow-y:auto;padding:18px 22px 24px;min-width:0; }
.cache-form-shell .pe-foot { grid-area:foot;display:flex;align-items:center;justify-content:space-between;gap:12px;padding:12px 18px;border-top:1px solid var(--cf-divider);background:color-mix(in oklab,var(--cf-page-bg) 45%,var(--cf-card-bg)); }
.cache-form-shell .pe-foot-state { font-size:11.5px;color:var(--cf-text-muted);min-width:0;overflow-wrap:anywhere; }
.cache-form-shell .pe-foot-dot { margin:0 7px;opacity:.55; }
.cache-form-shell .pe-sec-head { margin-bottom:14px; }
.cache-form-shell .pe-sec-head h3 { font-size:14px;font-weight:700;margin:0 0 4px; }
.cache-form-shell .pe-sec-head p { font-size:11.5px;color:var(--cf-text-muted);margin:0;line-height:1.5;max-width:640px; }
@media(max-width:860px) { .cache-form-shell { grid-template-columns:minmax(0,1fr);grid-template-rows:auto auto minmax(0,1fr) auto;grid-template-areas:"head" "rail" "body" "foot"; } .cache-form-shell .pe-rail { flex-direction:row;border-right:0;border-bottom:1px solid var(--cf-divider); } .cache-form-shell .pe-rail-item { width:auto;flex-shrink:0; } .cache-form-shell .pe-foot { flex-wrap:wrap; } .cache-form-shell .pe-foot-state { flex-basis:100%; } .cache-form-shell .pe-foot>div { margin-left:auto; } }
"#;

// INVARIANT: Edit state contains replacements only. Configured flags permit
// retention on save and by-ID probes; the browser never substitutes secrets.
#[derive(Clone, PartialEq)]
struct Niks3FormState {
    name: String,
    server_url: String,
    read_url: String,
    keys: String,
    write_mode: String,
    read_mode: String,
    token: String,
    write_cert: String,
    write_key: String,
    write_ca: String,
    read_cert: String,
    read_key: String,
    read_ca: String,
    clear_write_ca: bool,
    clear_read_ca: bool,
}

impl Niks3FormState {
    fn from_destination(destination: Option<&CacheDestination>) -> Self {
        Self {
            name: destination.map(|d| d.name.clone()).unwrap_or_default(),
            server_url: destination
                .and_then(|d| d.niks3_server_url.clone())
                .unwrap_or_default(),
            read_url: destination
                .and_then(|d| d.push_to.clone())
                .unwrap_or_default(),
            keys: destination
                .map(|d| d.niks3_public_keys.join("\n"))
                .unwrap_or_default(),
            write_mode: destination
                .and_then(|d| d.niks3_write_auth_mode.clone())
                .unwrap_or_else(|| "token".into()),
            read_mode: destination
                .and_then(|d| d.niks3_read_auth_mode.clone())
                .unwrap_or_else(|| "none".into()),
            token: String::new(),
            write_cert: String::new(),
            write_key: String::new(),
            write_ca: String::new(),
            read_cert: String::new(),
            read_key: String::new(),
            read_ca: String::new(),
            clear_write_ca: false,
            clear_read_ca: false,
        }
    }

    fn request(&self) -> CreateCacheDestination {
        let value = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_string());
        let write_mtls = self.write_mode == "mtls";
        let read_mtls = self.read_mode == "mtls";
        CreateCacheDestination {
            name: self.name.trim().into(),
            cache_type: "Niks3".into(),
            push_to: value(&self.read_url),
            niks3_server_url: value(&self.server_url),
            niks3_public_keys: self
                .keys
                .lines()
                .map(str::trim)
                .filter(|k| !k.is_empty())
                .map(str::to_string)
                .collect(),
            niks3_write_auth_mode: Some(self.write_mode.clone()),
            niks3_read_auth_mode: Some(self.read_mode.clone()),
            niks3_auth_token: if write_mtls { None } else { value(&self.token) },
            niks3_write_client_cert: if write_mtls {
                value(&self.write_cert)
            } else {
                None
            },
            niks3_write_client_key: if write_mtls {
                value(&self.write_key)
            } else {
                None
            },
            niks3_write_ca_cert: if write_mtls && !self.clear_write_ca {
                value(&self.write_ca)
            } else {
                None
            },
            niks3_read_client_cert: if read_mtls {
                value(&self.read_cert)
            } else {
                None
            },
            niks3_read_client_key: if read_mtls {
                value(&self.read_key)
            } else {
                None
            },
            niks3_read_ca_cert: if read_mtls && !self.clear_read_ca {
                value(&self.read_ca)
            } else {
                None
            },
            enabled: Some(true),
            require_sigs: Some(true),
            ..Default::default()
        }
    }

    // INVARIANT: Empty replacements retain server-side identities. Clear flags
    // remove only inactive credentials or a CA explicitly selected for removal.
    fn update_request(&self) -> UpdateCacheDestination {
        let req = self.request();
        UpdateCacheDestination {
            name: Some(req.name),
            push_to: req.push_to,
            niks3_server_url: req.niks3_server_url,
            niks3_public_keys: req.niks3_public_keys,
            niks3_write_auth_mode: req.niks3_write_auth_mode,
            niks3_read_auth_mode: req.niks3_read_auth_mode,
            niks3_auth_token: req.niks3_auth_token,
            niks3_write_client_cert: req.niks3_write_client_cert,
            niks3_write_client_key: req.niks3_write_client_key,
            niks3_write_ca_cert: req.niks3_write_ca_cert,
            niks3_read_client_cert: req.niks3_read_client_cert,
            niks3_read_client_key: req.niks3_read_client_key,
            niks3_read_ca_cert: req.niks3_read_ca_cert,
            clear_niks3_auth_token: self.write_mode == "mtls",
            clear_niks3_write_client_key: self.write_mode == "token",
            clear_niks3_read_client_key: self.read_mode == "none",
            clear_niks3_write_ca_cert: self.clear_write_ca,
            clear_niks3_read_ca_cert: self.clear_read_ca,
            ..Default::default()
        }
    }

    fn validate(&self, retained: Option<&CacheDestination>) -> Result<(), String> {
        self.validate_destination()?;
        self.validate_credentials(retained)
    }

    fn validate_destination(&self) -> Result<(), String> {
        let req = self.request();
        if req.name.is_empty() {
            return Err("Enter a cache name in Destination.".into());
        }
        for (label, url) in [
            ("Write / API", &self.server_url),
            ("Read / substituter", &self.read_url),
        ] {
            let valid = web_sys::Url::new(url.trim()).is_ok_and(|u| {
                u.protocol() == "https:"
                    && !u.hostname().is_empty()
                    && u.username().is_empty()
                    && u.password().is_empty()
                    && u.search().is_empty()
                    && u.hash().is_empty()
            });
            if !valid {
                return Err(format!(
                    "{label} URL must be HTTPS without credentials, query or fragment."
                ));
            }
        }
        if req.niks3_public_keys.is_empty()
            || req
                .niks3_public_keys
                .iter()
                .any(|k| !is_attic_public_key(k))
        {
            return Err(
                "Enter at least one signing public key, one name:BASE64KEY per line.".into(),
            );
        }
        Ok(())
    }

    fn validate_credentials(&self, retained: Option<&CacheDestination>) -> Result<(), String> {
        let req = self.request();
        if self.write_mode == "token" {
            if req.niks3_auth_token.is_none()
                && !retained.is_some_and(|d| {
                    d.niks3_write_auth_mode.as_deref() == Some("token")
                        && d.niks3_write_token_configured
                })
            {
                return Err("Enter a write token in Credentials.".into());
            }
        } else if self.write_mode != "mtls" {
            return Err("Select token or mTLS write authentication.".into());
        }
        for (plane, mode, cert, key, configured) in [
            (
                "Write",
                self.write_mode.as_str(),
                &self.write_cert,
                &self.write_key,
                retained.is_some_and(|d| {
                    d.niks3_write_auth_mode.as_deref() == Some("mtls")
                        && d.niks3_write_mtls_configured
                }),
            ),
            (
                "Read",
                self.read_mode.as_str(),
                &self.read_cert,
                &self.read_key,
                retained.is_some_and(|d| {
                    d.niks3_read_auth_mode.as_deref() == Some("mtls")
                        && d.niks3_read_mtls_configured
                }),
            ),
        ] {
            if mode == "mtls" {
                // Replacement identities are atomic in the UI: both inputs or
                // neither. Retained identities are never loaded into form values.
                if (!configured && (cert.trim().is_empty() || key.trim().is_empty()))
                    || (cert.trim().is_empty() != key.trim().is_empty())
                {
                    return Err(format!(
                        "{plane} mTLS requires a client certificate and private key together."
                    ));
                }
                if !cert.trim().is_empty()
                    && (!cert.contains("-----BEGIN CERTIFICATE-----")
                        || !key.contains("PRIVATE KEY-----"))
                {
                    return Err(format!(
                        "{plane} mTLS identity must use PEM certificates and private keys."
                    ));
                }
            }
        }
        if !matches!(self.read_mode.as_str(), "none" | "mtls") {
            return Err("Select public or mTLS read authentication.".into());
        }
        for (plane, ca) in [("Write", &self.write_ca), ("Read", &self.read_ca)] {
            if !ca.trim().is_empty() && !ca.contains("-----BEGIN CERTIFICATE-----") {
                return Err(format!("{plane} CA certificate must use PEM."));
            }
        }
        Ok(())
    }
}

#[component]
fn Niks3TextField(
    label: String,
    field: String,
    mut form: Signal<Niks3FormState>,
    mut result: Signal<Option<crate::api::models::CacheCredentialTestResult>>,
    multiline: bool,
    secret: bool,
    hint: String,
) -> Element {
    let value = match field.as_str() {
        "name" => form().name,
        "server" => form().server_url,
        "read" => form().read_url,
        "keys" => form().keys,
        "token" => form().token,
        "write_cert" => form().write_cert,
        "write_key" => form().write_key,
        "write_ca" => form().write_ca,
        "read_cert" => form().read_cert,
        "read_key" => form().read_key,
        _ => form().read_ca,
    };
    let field_id = field.clone();
    let change = move |evt: FormEvent| {
        result.set(None);
        let mut state = form.write();
        let target = match field.as_str() {
            "name" => &mut state.name,
            "server" => &mut state.server_url,
            "read" => &mut state.read_url,
            "keys" => &mut state.keys,
            "token" => &mut state.token,
            "write_cert" => &mut state.write_cert,
            "write_key" => &mut state.write_key,
            "write_ca" => &mut state.write_ca,
            "read_cert" => &mut state.read_cert,
            "read_key" => &mut state.read_key,
            _ => &mut state.read_ca,
        };
        *target = evt.value();
    };
    rsx! { div { class: "field",
        label { r#for: "niks3-{field_id}", "{label}" }
        if multiline { textarea { id: "niks3-{field_id}", class: "input focus-ring mono", rows: "3", value, oninput: change, autocomplete: "off", spellcheck: "false", style: if secret { "-webkit-text-security:disc;" } else { "" } } }
        else { input { id: "niks3-{field_id}", class: "input focus-ring", r#type: if secret { "password" } else { "text" }, value, oninput: change, autocomplete: "off", placeholder: match field_id.as_str() { "name" => "e.g. crystal-forge-prod-cache", "server" => "https://niks3.example.com", "read" => "https://cache.nixos.org", _ => "" } } }
        if !hint.is_empty() { div { class: "help", "{hint}" } }
    } }
}

// Each type owns its configuration draft. Name and scope are common. A new HTTP
// type inherits the current URL once; later switches restore that type's URL.
// An S3 destination is separate because s3:// is not an HTTPS read endpoint.
#[derive(Clone, PartialEq, Default)]
struct CacheTypeDrafts(HashMap<String, String>);

// Http and Nix share presentation, but an unrelated edit must retain the exact
// stored wire type. Only an explicit legacy type selection requests conversion.
fn cache_form_kind(cache_type: &str) -> &'static str {
    match cache_type.to_ascii_lowercase().as_str() {
        "s3" => "s3",
        "attic" => "attic",
        "nix" | "http" => "nix",
        "niks3" => "niks3",
        _ => "unknown",
    }
}

impl CacheTypeDrafts {
    fn from_destination(destination: Option<&CacheDestination>) -> Self {
        let mut state = Self::default();
        if let Some(destination) = destination {
            let kind = cache_form_kind(&destination.cache_type);
            // SECURITY: Populate public configuration only, even if a malformed
            // response includes a token, session token or secret access key.
            for (field, value) in [
                ("url", &destination.push_to),
                ("region", &destination.s3_region),
                ("profile", &destination.s3_profile),
                ("endpoint", &destination.s3_endpoint_url),
                ("cache", &destination.attic_cache_name),
                ("key", &destination.attic_public_key),
                ("signing", &destination.signing_key_path),
                ("compression", &destination.compression),
            ] {
                if let Some(value) = value {
                    state.0.insert(format!("{kind}.{field}"), value.clone());
                }
            }
        }
        state
    }

    fn get(&self, kind: &str, field: &str) -> String {
        self.0
            .get(&format!("{kind}.{field}"))
            .cloned()
            .unwrap_or_default()
    }

    fn request(&self, kind: &str, common: &Niks3FormState) -> CreateCacheDestination {
        let value = |field| {
            let text = self.get(kind, field);
            (!text.trim().is_empty()).then(|| text.trim().to_string())
        };
        CreateCacheDestination {
            name: common.name.trim().into(),
            cache_type: api_cache_type(kind),
            push_to: if kind == "s3" {
                value("url")
            } else {
                Some(common.read_url.trim().into())
            },
            enabled: Some(true),
            s3_region: if kind == "s3" { value("region") } else { None },
            s3_profile: if kind == "s3" { value("profile") } else { None },
            s3_endpoint_url: if kind == "s3" {
                value("endpoint")
            } else {
                None
            },
            s3_access_key_id: if kind == "s3" { value("access") } else { None },
            s3_secret_access_key: if kind == "s3" { value("secret") } else { None },
            s3_session_token: if kind == "s3" { value("session") } else { None },
            attic_cache_name: if kind == "attic" {
                value("cache")
            } else {
                None
            },
            attic_public_key: if kind == "attic" { value("key") } else { None },
            attic_token: if kind == "attic" {
                value("token")
            } else {
                None
            },
            signing_key_path: if kind != "attic" {
                value("signing")
            } else {
                None
            },
            compression: value("compression"),
            ..Default::default()
        }
    }

    fn validate(
        &self,
        kind: &str,
        common: &Niks3FormState,
        credentials: bool,
        retained: Option<&CacheDestination>,
    ) -> Result<(), String> {
        if !matches!(kind, "s3" | "attic" | "nix") {
            return Err("Select a supported cache type.".into());
        }
        let mut req = self.request(kind, common);
        let selected = self.get(kind, "credential");
        let retaining = (selected.is_empty() || selected == "__current__")
            && retained.is_some_and(|d| retained_credential_configured(kind, d));
        if retaining && let Some(d) = retained {
            req.push_to = req
                .push_to
                .filter(|v| !v.trim().is_empty())
                .or_else(|| d.push_to.clone());
            req.s3_region = req.s3_region.or_else(|| d.s3_region.clone());
            req.s3_endpoint_url = req.s3_endpoint_url.or_else(|| d.s3_endpoint_url.clone());
            req.attic_cache_name = req.attic_cache_name.or_else(|| d.attic_cache_name.clone());
            req.attic_public_key = req.attic_public_key.or_else(|| d.attic_public_key.clone());
        }
        if credentials {
            if kind == "attic" && req.attic_token.is_none() && !retaining {
                return Err("Enter an Attic token in Credentials.".into());
            }
            if kind == "s3"
                && ((!retaining
                    && (req.s3_access_key_id.is_none() || req.s3_secret_access_key.is_none()))
                    || (req.s3_access_key_id.is_some() != req.s3_secret_access_key.is_some()))
            {
                return Err("Enter AWS access credentials in Credentials. The current API requires access keys even with a profile.".into());
            }
        } else {
            if req.name.is_empty() {
                return Err("Enter a cache name in Destination.".into());
            }
            if !req.push_to.as_deref().is_some_and(|url| {
                if kind == "s3" {
                    is_s3_url(url)
                } else {
                    is_http_url(url)
                }
            }) {
                return Err(if kind == "s3" {
                    "Enter an s3://bucket destination URL."
                } else {
                    "Enter an HTTP or HTTPS URL in Destination."
                }
                .into());
            }
            if kind == "attic"
                && (req.attic_cache_name.is_none()
                    || !req
                        .attic_public_key
                        .as_deref()
                        .is_some_and(is_attic_public_key))
            {
                return Err(
                    "Enter an Attic cache name and signing public key in Destination.".into(),
                );
            }
            if kind == "s3"
                && (req.s3_region.is_none()
                    || !req.s3_endpoint_url.as_deref().is_some_and(is_http_url))
            {
                return Err("Enter an S3 region and HTTP or HTTPS endpoint in Destination.".into());
            }
        }
        Ok(())
    }

    // COMPATIBILITY: Same-type omitted fields retain stored configuration
    // and ciphertext. Type conversions require target-type credentials and
    // clear incompatible secrets. Unrelated edits never enable a disabled cache.
    fn update_request(
        &self,
        kind: &str,
        common: &Niks3FormState,
        convert: bool,
        original: Option<&CacheDestination>,
    ) -> UpdateCacheDestination {
        let req = self.request(kind, common);
        // SECURITY: A new S3 identity must not borrow the previous session token.
        // An omitted token retains it only when the entire identity is retained.
        let replacement_session =
            if req.s3_access_key_id.is_some() && req.s3_secret_access_key.is_some() {
                Some(req.s3_session_token.clone().unwrap_or_default())
            } else {
                req.s3_session_token.clone()
            };
        let mut update = UpdateCacheDestination {
            name: Some(req.name),
            cache_type: convert.then_some(req.cache_type),
            push_to: req.push_to.filter(|v| !v.trim().is_empty()),
            signing_key_path: req.signing_key_path,
            compression: req.compression,
            s3_region: req.s3_region,
            s3_profile: req.s3_profile,
            s3_endpoint_url: req.s3_endpoint_url,
            s3_access_key_id: req.s3_access_key_id,
            s3_secret_access_key: req.s3_secret_access_key,
            s3_session_token: replacement_session,
            attic_cache_name: req.attic_cache_name,
            attic_public_key: req.attic_public_key,
            attic_token: req.attic_token,
            ..Default::default()
        };
        if let Some(original) = original {
            // GET sanitizes URI credentials. An unchanged displayed URL must
            // not overwrite the stored URI or silently discard its credentials.
            // Apply the same partial-update rule to unchanged public settings.
            let changed = |value: Option<String>, previous: &Option<String>| {
                value.filter(|v| Some(v) != previous.as_ref())
            };
            update.name = update.name.filter(|name| name != &original.name);
            if common.name == original.name {
                update.name = None;
            }
            update.push_to = changed(update.push_to, &original.push_to);
            update.signing_key_path = changed(update.signing_key_path, &original.signing_key_path);
            update.compression = changed(update.compression, &original.compression);
            update.s3_region = changed(update.s3_region, &original.s3_region);
            update.s3_profile = changed(update.s3_profile, &original.s3_profile);
            update.s3_endpoint_url = changed(update.s3_endpoint_url, &original.s3_endpoint_url);
            update.attic_cache_name = changed(update.attic_cache_name, &original.attic_cache_name);
            update.attic_public_key = changed(update.attic_public_key, &original.attic_public_key);
        }
        update
    }
}

// Configured entries describe only the active server identity, never inventory.
fn retained_credential_configured(kind: &str, destination: &CacheDestination) -> bool {
    cache_form_kind(&destination.cache_type) == kind
        && match kind {
            "attic" => destination.attic_token_configured,
            "s3" => destination.s3_credentials_configured,
            "nix" => {
                destination.http_basic_auth_configured
                    && !destination.legacy_query_credentials_configured
            }
            _ => false,
        }
}

// INVARIANT: Save and Test submit the same patch. Tests merge retained material
// only on the server; public sanitized URLs remain omitted when unchanged.
fn cache_update_patch(
    kind: &str,
    form: &Niks3FormState,
    drafts: &CacheTypeDrafts,
    convert: bool,
    original: Option<&CacheDestination>,
    ids: Vec<Uuid>,
) -> UpdateCacheDestination {
    let mut update = if kind == "niks3" {
        form.update_request()
    } else {
        drafts.update_request(kind, form, convert, original)
    };
    update.environment_ids = Some(ids);
    update
}

#[component]
fn CacheDraftField(
    kind: String,
    field: String,
    label: String,
    mut drafts: Signal<CacheTypeDrafts>,
    mut result: Signal<Option<crate::api::models::CacheCredentialTestResult>>,
    secret: bool,
) -> Element {
    let value = drafts().get(&kind, &field);
    let id = format!("cache-{kind}-{field}");
    rsx! { div { class: "field",
        label { r#for: "{id}", "{label}" }
        input { id, class: "input focus-ring mono", r#type: if secret { "password" } else { "text" }, value, autocomplete: "off",
            oninput: move |e| { result.set(None); drafts.write().0.insert(format!("{kind}.{field}"), e.value()); }
        }
    } }
}

/// Renders one keyboard-contained Add and Edit shell for every cache type.
/// Type changes retain drafts without unmounting the dialog. Scope and cache
/// configuration commit together; stored secrets are never loaded into inputs.
/// Legacy Edit conversions remain explicit; Niks3 Edit keeps its type fixed.
#[component]
fn CacheDestinationForm(
    destination: Option<CacheDestination>,
    add_draft: Niks3FormState,
    add_environment_ids: Vec<Uuid>,
    on_close: EventHandler<()>,
    on_saved: EventHandler<CacheDestination>,
) -> Element {
    let initial = destination.clone();
    let mut form = use_signal(move || {
        initial
            .as_ref()
            .map(|d| Niks3FormState::from_destination(Some(d)))
            .unwrap_or(add_draft)
    });
    let mut section = use_signal(|| "dest");
    let initial_kind = destination
        .as_ref()
        .map(|d| cache_form_kind(&d.cache_type))
        .unwrap_or("s3");
    let mut initial_drafts = CacheTypeDrafts::from_destination(destination.as_ref());
    if destination
        .as_ref()
        .is_some_and(|d| retained_credential_configured(initial_kind, d))
    {
        initial_drafts
            .0
            .insert(format!("{initial_kind}.credential"), "__current__".into());
    }
    let mut kind = use_signal(move || initial_kind);
    let mut type_changed = use_signal(|| false);
    let mut drafts = use_signal(move || initial_drafts);
    let mut credentials = use_signal(Vec::<LocalCredential>::new);
    let mut show_credential = use_signal(|| false);
    let mut credential_modal_new = use_signal(|| true);
    let mut identity_dialog = use_signal(|| None::<String>);
    let mut busy = use_signal(|| None::<&'static str>);
    let mut error = use_signal(|| None::<String>);
    let mut discovery_note = use_signal(|| None::<String>);
    let mut result = use_signal(|| None::<crate::api::models::CacheCredentialTestResult>);
    let adding = destination.is_none();
    let mut environment_ids = use_signal(move || {
        if adding {
            add_environment_ids
        } else {
            Vec::new()
        }
    });
    let mut environment_ready = use_signal(|| destination.is_none());
    let environments = use_resource(|| async { client::fetch_environments().await });
    let editing_id = destination.as_ref().map(|d| d.id);
    use_future(move || async move {
        if let Some(id) = editing_id {
            match client::get_cache_environments(id).await {
                Ok(ids) => {
                    environment_ids.set(ids);
                    environment_ready.set(true);
                }
                Err(_) => error.set(Some(
                    "Environment assignments could not be loaded. Close and reopen before saving."
                        .into(),
                )),
            }
        }
    });
    let retained = destination.clone();
    let type_label = match kind() {
        "s3" => "S3",
        "attic" => "Attic",
        "niks3" => "Niks3",
        "nix" => "Nix HTTPS",
        _ => "Unsupported",
    };
    let destination_valid = if kind() == "niks3" {
        form().validate_destination()
    } else {
        drafts().validate(kind(), &form(), false, destination.as_ref())
    };
    let credentials_valid = if kind() == "niks3" {
        form().validate_credentials(destination.as_ref())
    } else {
        drafts().validate(kind(), &form(), true, destination.as_ref())
    };
    let scope_ready = environment_ready() && matches!(environments.read().as_ref(), Some(Ok(_)));
    let save_validation = if kind() == "niks3" {
        form().validate(destination.as_ref())
    } else {
        destination_valid.clone().and(credentials_valid.clone())
    };
    let blocked_reason = if busy().is_some() {
        Some("Wait for the current operation to finish.".to_string())
    } else if !scope_ready {
        Some("Environment scope is not loaded. Wait for loading; if loading failed, close and reopen.".to_string())
    } else {
        save_validation.as_ref().err().cloned()
    };
    let test_original = destination.clone();
    let test_connection = EventHandler::new(move |_: MouseEvent| {
        error.set(None);
        result.set(None);
        let validation = if kind() == "niks3" {
            form().validate(test_original.as_ref())
        } else {
            drafts()
                .validate(kind(), &form(), false, test_original.as_ref())
                .and_then(|_| drafts().validate(kind(), &form(), true, test_original.as_ref()))
        };
        if let Err(message) = validation {
            error.set(Some(format!("Test not run: {message}")));
            return;
        }
        let patch = cache_update_patch(
            kind(),
            &form(),
            &drafts(),
            type_changed(),
            test_original.as_ref(),
            environment_ids(),
        );
        let mut create = if kind() == "niks3" {
            form().request()
        } else {
            drafts().request(kind(), &form())
        };
        let legacy_query = kind() == "nix"
            && patch.push_to.is_none()
            && test_original
                .as_ref()
                .is_some_and(|d| d.legacy_query_credentials_configured);
        create.environment_ids = Some(environment_ids());
        busy.set(Some("test"));
        spawn(async move {
            let tested = if let Some(id) = editing_id {
                client::test_stored_cache_destination_credentials(id, &patch).await
            } else {
                client::test_cache_destination_credentials(&create).await
            };
            match tested {
                Ok(value) => result.set(Some(value)),
                Err(ApiClientError::Status { code: 400, .. }) => error.set(Some(if legacy_query {
                    "Legacy credential queries require migration before testing. The endpoint was not contacted."
                } else {
                    "Connection test rejected. Check destination values and credentials."
                }.into())),
                Err(_) => error.set(Some("Connection test failed. Check endpoint policy and credentials.".into())),
            }
            busy.set(None);
        });
    });
    let current_configured = destination
        .as_ref()
        .is_some_and(|d| retained_credential_configured(kind(), d));
    let credential_draft = drafts().get(kind(), "credential");
    let selected_credential = credentials().into_iter().find(|c| c.id == credential_draft);
    let selected_local_credential = selected_credential.is_some();
    let url_changed = destination
        .as_ref()
        .is_some_and(|d| kind() == "nix" && d.push_to.as_deref() != Some(form().read_url.trim()));
    let migration_required = kind() == "nix"
        && !url_changed
        && destination
            .as_ref()
            .is_some_and(|d| d.legacy_query_credentials_configured);
    rsx! {
        div { class: "modal-backdrop", style: "padding:8px;", onclick: move |_| { if busy().is_none() { on_close.call(()); } },
            style { "{CACHE_FORM_CSS}" }
            div { id: "cache-destination-dialog", class: "pe-shell cache-form-shell", role: "dialog", aria_modal: "true", aria_label: "Cache destination", aria_describedby: "cache-destination-description", tabindex: "-1", onclick: move |e| e.stop_propagation(),
                onkeydown: move |event| {
                    event.stop_propagation();
                    if event.key() == Key::Escape {
                        event.prevent_default();
                        if busy().is_none() && !show_credential() && identity_dialog().is_none() { on_close.call(()); }
                    } else if event.key() == Key::Tab && busy().is_some() {
                        event.prevent_default();
                    }
                },
                DialogFocusRestore {}
                DialogInitialFocus { dialog_id: "cache-destination-dialog".to_string() }
                DialogFocusSentinel { dialog_id: "cache-destination-dialog".to_string(), boundary: DialogFocusBoundary::Last }
                // Disabled controls cannot receive focus during an operation.
                // Focus the status chip until the frozen snapshot is released.
                if busy().is_some() { DialogInitialFocus { dialog_id: "cache-destination-busy".to_string() } }
                header { class: "pe-head",
                    div { style: "min-width:0;",
                        div { style: "display:flex;align-items:center;gap:8px;flex-wrap:wrap;",
                            span { style: "color:var(--cf-brand-purple);display:flex;", Icon { name: if destination.is_some() { IconName::Gear } else { IconName::Plus }, size: 15 } }
                            h2 { class: "pe-head-title", if form().name.trim().is_empty() { if let Some(d) = destination.as_ref() { "{d.name}" } else { "Add cache destination" } } else { "{form().name}" } }
                            span { class: "chip chip-info", "{type_label}" }
                            span { id: "cache-destination-busy", tabindex: "-1", class: "chip", if busy().is_some() { "Working" } else if migration_required { "Migration required" } else if error().is_some() { "Needs attention" } else if save_validation.is_ok() { "Set" } else { "Unsaved draft" } }
                        }
                        p { id: "cache-destination-description", class: "pe-head-sub", if destination.is_some() { "Update binary cache destination." } else { "Register a new binary cache destination." } }
                    }
                    button { class: "btn-icon focus-ring", aria_label: "Close", disabled: busy().is_some(), onclick: move |_| on_close.call(()), Icon { name: IconName::X, size: 16 } }
                }
                nav { class: "pe-rail", aria_label: "Cache form sections",
                    for (id, label, icon) in [("dest", "Destination", IconName::Download), ("auth", "Credentials", IconName::Key), ("envs", "Environments", IconName::Grid)] {
                        button { aria_label: label, aria_current: if section() == id { "true" } else { "false" }, disabled: busy().is_some(), class: if section() == id { "pe-rail-item focus-ring active" } else { "pe-rail-item focus-ring" }, onclick: move |_| section.set(id),
                            Icon { name: icon, size: 13 }
                            span { class: "pe-rail-label", "{label}" }
                            span { class: if (id == "dest" && destination_valid.is_err()) || (id == "auth" && (credentials_valid.is_err() || migration_required)) { "pe-rail-badge warn" } else { "pe-rail-badge" }, title: "Draft validation and scope; not connection verification",
                                if id == "envs" { if scope_ready { "{environment_ids().len()}" } else { "!" } }
                                else if id == "dest" { if destination_valid.is_ok() { "Set" } else { "!" } }
                                else { if credentials_valid.is_ok() && !migration_required { "Set" } else { "Review" } }
                            }
                        }
                    }
                }
                div { class: "pe-body", style: "min-height:0;overflow-y:auto;",
                    // Freeze the submitted snapshot until discovery, probe or
                    // save completes, so responses cannot overwrite newer input.
                    fieldset { disabled: busy().is_some(), style: "border:0;padding:0;margin:0;min-width:0;",
                    if section() == "dest" {
                        div { class: "pe-sec-head", h3 { "Destination" } p { "What this cache is called, what kind of store it is, and where it lives." } }
                        Niks3TextField { label: "Name", field: "name", form, result, multiline: false, secret: false, hint: String::new() }
                        div { class: "field", label { "Type" }
                            div { class: "seg", style: "width:fit-content;flex-wrap:wrap;",
                                for (value, label) in [("s3", "S3-compatible"), ("attic", "Attic"), ("nix", "Nix HTTPS"), ("niks3", "Niks3")] {
                                    button { class: if kind() == value { "active focus-ring" } else { "focus-ring" }, disabled: destination.as_ref().is_some_and(|d| cache_form_kind(&d.cache_type) == "niks3" || value == "niks3"), aria_pressed: kind() == value,
                                        onclick: { let original_type = destination.as_ref().map(|d| d.cache_type.clone()); move |_| {
                                            type_changed.set(original_type.as_ref().is_some_and(|wire| wire != &api_cache_type(value)));
                                            // Discovery may change the active read URL. Snapshot
                                            // it before switching so other type URLs stay intact.
                                            if kind() != value {
                                                if kind() != "s3" { drafts.write().0.insert(format!("{}.read", kind()), form().read_url); }
                                                if value != "s3" {
                                                    let saved = drafts().0.get(&format!("{value}.read")).cloned();
                                                    if let Some(url) = saved { form.write().read_url = url; }
                                                }
                                                kind.set(value);
                                            }
                                            result.set(None); error.set(None); discovery_note.set(None);
                                        } }, "{label}" }
                                }
                            }
                            if destination.as_ref().is_some_and(|d| cache_form_kind(&d.cache_type) == "niks3") { p { class: "help", "Editing retains the Niks3 destination type and stored credential boundary." } }
                             else if destination.is_some() { p { class: "help", "Type changes require a replacement identity for the selected type. An unrelated edit retains the current configured credential." } }
                        }
                        if kind() == "s3" {
                            CacheDraftField { kind: "s3", field: "url", label: "Destination URL", drafts, result, secret: false }
                            CacheDraftField { kind: "s3", field: "region", label: "S3 region", drafts, result, secret: false }
                            CacheDraftField { kind: "s3", field: "endpoint", label: "S3 endpoint URL", drafts, result, secret: false }
                        } else {
                            Niks3TextField { label: if kind() == "niks3" { "Read / substituter URL" } else if kind() == "attic" { "Attic server URL" } else { "URL" }, field: "read", form, result, multiline: false, secret: false, hint: if kind() == "niks3" { "HTTPS endpoint used by Nix reads; independent of the write API.".to_string() } else { String::new() } }
                        }
                        if kind() == "attic" {
                            CacheDraftField { kind: "attic", field: "cache", label: "Attic cache name", drafts, result, secret: false }
                            CacheDraftField { kind: "attic", field: "key", label: "Attic public key", drafts, result, secret: false }
                        }
                        if kind() != "niks3" {
                            if kind() != "attic" { CacheDraftField { kind: kind(), field: "signing", label: "Signing key path (optional)", drafts, result, secret: false } }
                            div { class: "field", label { r#for: "cache-compression", "Compression (optional)" }
                                select { id: "cache-compression", class: "input focus-ring", value: drafts().get(kind(), "compression"), onchange: move |e| { result.set(None); drafts.write().0.insert(format!("{}.compression", kind()), e.value()); },
                                    option { value: "", "Default" } option { value: "none", "None" } option { value: "xz", "XZ" } option { value: "zstd", "Zstandard" }
                                }
                            }
                        }
                        if kind() == "niks3" {
                        Niks3TextField { label: "Write / API URL", field: "server", form, result, multiline: false, secret: false, hint: "HTTPS control endpoint used for publication.".to_string() }
                        button { class: "btn btn-ghost focus-ring", disabled: busy().is_some() || form().server_url.trim().is_empty(), onclick: move |_| {
                            busy.set(Some("discover")); error.set(None); discovery_note.set(None); result.set(None);
                            let url = form().server_url;
                            spawn(async move {
                                match client::discover_niks3(&url).await {
                                    Ok(values) => {
                                        let mut state = form.write(); state.server_url = values.server_url; state.read_url = values.substituter_url; state.keys = values.public_keys.join("\n");
                                        discovery_note.set(Some(if values.oidc_audience.is_some() { "Configuration populated. Review URLs and all keys before saving. OIDC advertised; external providers are not offered." } else { "Configuration populated. Review URLs and all keys before saving." }.into()));
                                    }
                                    Err(_) => error.set(Some("Discovery failed. Check the API URL, HTTPS certificate and permitted target policy.".into())),
                                }
                                busy.set(None);
                            });
                        }, if busy() == Some("discover") { "Discovering…" } else { "Discover configuration" } }
                        if let Some(note) = discovery_note() { p { class: "help", role: "status", "{note}" } }
                        Niks3TextField { label: "Signing public keys", field: "keys", form, result, multiline: true, secret: false, hint: "One name:BASE64KEY per line. Keep both keys during rotation.".to_string() }
                        }
                    }
                    if section() == "auth" && kind() != "niks3" {
                        div { class: "pe-sec-head", h3 { "Credentials" } p { "Use the current configured credential or select a dialog-local replacement. Secrets are encrypted on save." } }
                        div { class: "field", label { style: "display:flex;gap:9px;align-items:flex-start;margin:0;text-transform:none;letter-spacing:0;",
                            input { r#type: "checkbox", checked: kind() != "nix" || (current_configured && !url_changed), disabled: true, style: "accent-color:var(--cf-brand-purple);margin-top:1px;" }
                            span { "Requires authentication" div { class: "help", if kind() == "nix" { if current_configured { "HTTP Basic credentials remain on the server." } else { "Public read access or the executing Nix store configuration." } } else { "Required by the current destination API; anonymous publication is not supported." } } }
                        } }
                        if kind() == "nix" {
                            p { class: "help", "Nix HTTPS uses the configured Nix store mechanism. A generic bearer-token provider is not supported by this destination type." }
                            if destination.as_ref().is_some_and(|d| d.legacy_query_credentials_configured) {
                                p { role: "status", "Legacy query credentials: migration required. Testing rejects credential queries before contacting the endpoint. An unrelated save preserves the existing server-only URL." }
                            }
                            div { class: "field", label { r#for: "cache-http-credential", "Read access" }
                                div { style: "display:flex;gap:8px;align-items:center;",
                                    select { id: "cache-http-credential", class: "input focus-ring", disabled: true, value: if migration_required { "__migration__" } else if current_configured && !url_changed { "__current__" } else { "__anonymous__" },
                                        option { value: if migration_required { "__migration__" } else if current_configured && !url_changed { "__current__" } else { "__anonymous__" }, selected: true, if migration_required { "Migration required" } else if current_configured && !url_changed { "Current configured credential" } else { "Anonymous read access" } }
                                    }
                                    button { class: "btn btn-ghost focus-ring xs", onclick: test_connection, if busy() == Some("test") { "Testing…" } else { "Test connection" } }
                                }
                            }
                            if current_configured { p { class: "help", "Stored HTTP Basic credentials can be retained and tested by ID. Replacing Basic credentials is not available in this form." } }
                            if url_changed && destination.as_ref().is_some_and(|d| d.http_basic_auth_configured || d.legacy_query_credentials_configured) { p { role: "status", "URL changed: stored URL credentials will not be forwarded to the new destination. The new URL must provide its own supported access configuration." } }
                        } else {
                            if current_configured { p { class: "help", if credential_draft == "__current__" { "Credentials stored. Test uses the configured identity without retrieving its secrets." } else { "Replacement selected. Save replaces the configured identity; Cancel discards this dialog's draft." } } }
                            div { class: "field", label { r#for: "cache-credential", "Credential" }
                                div { style: "display:flex;gap:8px;align-items:center;",
                                select { id: "cache-credential", class: "input focus-ring", value: drafts().get(kind(), "credential"), onchange: move |e| {
                                    result.set(None);
                                    let id = e.value();
                                    if id == "__new__" { credential_modal_new.set(true); show_credential.set(true); } else {
                                        drafts.write().0.insert(format!("{}.credential", kind()), id.clone());
                                        if let Some(cred) = credentials().iter().find(|c| c.id == id) {
                                            let (profile, access, secret, token) = credential_fields_for_request(Some(cred));
                                            for (field, value) in [("profile", profile), ("access", access), ("secret", secret), ("token", token), ("session", cred.session_token.clone())] {
                                                drafts.write().0.insert(format!("{}.{field}", kind()), value.unwrap_or_default());
                                            }
                                        } else {
                                            for field in ["access", "secret", "token", "session"] { drafts.write().0.remove(&format!("{}.{field}", kind())); }
                                        }
                                    }
                                },
                                    option { value: "", selected: !show_credential() && drafts().get(kind(), "credential").is_empty(), "Select a credential…" }
                                    if current_configured { option { value: "__current__", selected: credential_draft == "__current__", "Current configured credential" } }
                                    // Newly confirmed options can be inserted after the
                                    // select value is patched. Select the option itself
                                    // so the visible credential matches the draft ID.
                                    for cred in credentials().into_iter().filter(|c| credential_matches_cache_type(c, kind())) { option { value: "{cred.id}", selected: !show_credential() && drafts().get(kind(), "credential") == cred.id, "{credential_label(&cred)}" } }
                                    option { value: "__new__", if current_configured { "+ Replace credential…" } else { "+ Add new credential…" } }
                                }
                                button { class: "btn btn-ghost focus-ring xs", onclick: test_connection, if busy() == Some("test") { "Testing…" } else { "Test connection" } }
                                }
                                button { id: "cache-add-credential", class: "btn btn-ghost focus-ring xs", r#type: "button", style: "margin-top:8px;width:fit-content;display:inline-flex;", onclick: move |_| { credential_modal_new.set(!selected_local_credential); show_credential.set(true); }, if selected_local_credential { "Edit credential" } else if current_configured { "Replace credential" } else { "Add credential" } }
                                p { class: "help", "Credential drafts are available for this dialog only; they are not a server-side credential library." }
                            }
                            if kind() == "s3" {
                                CacheDraftField { kind: "s3", field: "profile", label: "S3 profile (optional)", drafts, result, secret: false }
                                p { class: "help", "Uses a profile already configured on the executing builder. The legacy IAM-role credential maps to this profile field; this form does not assume a role. The current API also requires access keys." }
                            }
                        }
                        if let Some(test) = result() { p { role: "status", if test.ok { if kind() == "s3" { "Bucket read access verified. Write authorization: Untested." } else { "Connection verified." } } else { "Connection failed. Check endpoint configuration." } } }
                    }
                    if section() == "auth" && kind() == "niks3" {
                        div { class: "pe-sec-head", h3 { "Credentials" } p { "Write credentials stay on builders. Read credentials go only to assigned agents." } }
                        for plane in ["Read", "Write"] {
                            section { aria_label: "{plane} identity", style: "margin-bottom:18px;",
                                if plane == "Read" {
                                    div { class: "field", label { r#for: "niks3-read-mode", "Read authentication" }
                                        select { id: "niks3-read-mode", class: "input focus-ring", value: form().read_mode, onchange: move |e| { result.set(None); let mut state = form.write(); state.read_mode = e.value(); state.read_cert.clear(); state.read_key.clear(); state.read_ca.clear(); state.clear_read_ca = false; },
                                            option { value: "none", "Public (none)" } option { value: "mtls", "mTLS" }
                                        }
                                    }
                                } else {
                                    div { class: "field", label { r#for: "niks3-write-mode", "Write authentication" }
                                        select { id: "niks3-write-mode", class: "input focus-ring", value: form().write_mode, onchange: move |e| { result.set(None); let mut state = form.write(); state.write_mode = e.value(); state.token.clear(); state.write_cert.clear(); state.write_key.clear(); state.write_ca.clear(); state.clear_write_ca = false; },
                                            option { value: "token", "Static token" } option { value: "mtls", "mTLS" }
                                        }
                                    }
                                }
                                if plane == "Write" || form().read_mode == "mtls" {
                                    Niks3CredentialControl { plane: plane.to_string(), form, result, destination: destination.clone(), on_open: move |plane| identity_dialog.set(Some(plane)), on_test: test_connection, testing: busy() == Some("test") }
                                } else { p { class: "help", "Public read access. No read credential is sent." } }
                            }
                        }
                        p { class: "help", "Changing authentication modes clears previous credentials on save. External credential providers are not offered." }
                        if let Some(test) = result() {
                            div { role: "status", "data-testid": "niks3-test-result",
                                for (label, value) in [("API reachable", test.server_reachable), ("Discovery valid", test.discovery_valid), ("Write authorization", test.write_auth_valid), ("Read endpoint reachable", test.read_endpoint_reachable), ("Signing keys found", test.signing_keys_found)] {
                                    p { style: match value { Some(true) => "color:var(--cf-emerald);", Some(false) => "color:var(--cf-red);", None => "color:var(--cf-text-muted);" }, "{label}: ", match value { Some(true) => "Verified", Some(false) => "Failed", None => "Untested" } }
                                }
                                p { class: "help", "Discovery and read connectivity do not prove write permission. No upload was attempted." }
                            }
                        }
                    }
                    if section() == "envs" {
                        div { class: "pe-sec-head", h3 { "Assigned environments" } p { "Crystal Forge pushes builds for systems in these environments to this cache." } }
                        div { class: "field", label { "Environments" } }
                        match environments.read().as_ref() {
                            Some(Ok(envs)) => rsx! { div { style: "display:flex;flex-wrap:wrap;gap:8px;",
                                for env in envs { button { class: "focus-ring", style: format!("padding:6px 12px;border-radius:99px;font-size:12px;font-weight:600;border:1px solid {};background:{};color:var(--cf-text-primary);display:inline-flex;align-items:center;gap:7px;", if environment_ids().contains(&env.id) { normalize_env_color(&env.color_hex) } else { "var(--cf-card-border)" }, if environment_ids().contains(&env.id) { format!("color-mix(in oklab, {} 14%, var(--cf-card-bg))", normalize_env_color(&env.color_hex)) } else { "transparent".into() }), aria_pressed: environment_ids().contains(&env.id), aria_label: "{env.name}", disabled: !environment_ready(), onclick: { let id = env.id; move |_| { let mut ids = environment_ids.write(); if ids.contains(&id) { ids.retain(|v| *v != id); } else { ids.push(id); } } },
                                    span { style: "width:8px;height:8px;border-radius:50%;", background: "{normalize_env_color(&env.color_hex)}" } "{env.name}"
                                    if environment_ids().contains(&env.id) { Icon { name: IconName::Check, size: 11 } }
                                } }
                            } },
                            Some(Err(_)) => rsx! { p { role: "alert", "Could not load environments. Close and reopen before assigning environments." } },
                            None => rsx! { p { "Loading environments…" } },
                        }
                        if !environment_ready() { p { "Loading assigned environments…" } }
                        p { class: "help", if !scope_ready { "Scope is not loaded. Saving is disabled." } else if environment_ids().is_empty() { "No environments selected: this cache is global. Select environments to restrict its scope." } else { "Configuration and selected environments are saved together." } }
                    }
                    if let Some(message) = error() { p { role: "alert", style: "color:var(--cf-red);", "{message}" } }
                    }
                }
                footer { class: "pe-foot",
                    div { class: "pe-foot-state", style: CACHE_FORM_FOOT_STATE_STYLE,
                    span { if form().name.trim().is_empty() { "Unnamed cache" } else { "{form().name}" } span { class: "pe-foot-dot", "·" } "{type_label}" span { class: "pe-foot-dot", "·" }
                        if kind() == "niks3" { "{form().write_mode} writes / {form().read_mode} reads" } else if migration_required { "Credential migration required" } else if kind() == "nix" { if current_configured && !url_changed { "Credentials stored" } else { "Public read access" } } else if credential_draft == "__current__" && current_configured { "Credentials stored" } else if credentials_valid.is_ok() { "Credential draft set" } else { "Credential required" }
                        span { class: "pe-foot-dot", "·" } if !scope_ready { "Scope not loaded" } else if environment_ids().is_empty() { "Global scope" } else { "{environment_ids().len()} selected" } }
                    if let Some(reason) = blocked_reason.as_ref() { p { id: "cache-save-blocked", "data-testid": "cache-save-blocked", role: "status", aria_live: "polite", style: "margin:4px 0 0;", "{reason}" } }
                    }
                    div { style: "display:flex;gap:8px;flex-shrink:0;",
                    button { class: "btn btn-ghost focus-ring", disabled: busy().is_some(), onclick: move |_| on_close.call(()), "Cancel" }
                    button { class: "btn btn-primary focus-ring", disabled: blocked_reason.is_some(), aria_describedby: if blocked_reason.is_some() { "cache-save-blocked" } else { "" }, title: blocked_reason.clone().unwrap_or_default(), onclick: move |_| {
                        error.set(None);
                        let validation = if kind() == "niks3" { form().validate(retained.as_ref()) } else { drafts().validate(kind(), &form(), false, retained.as_ref()).and_then(|_| drafts().validate(kind(), &form(), true, retained.as_ref())) };
                        if let Err(message) = validation { error.set(Some(format!("Save not run: {message}"))); return; }
                        let mut req = if kind() == "niks3" { form().request() } else { drafts().request(kind(), &form()) }; let state = form(); let ids = environment_ids(); req.environment_ids = Some(ids.clone());
                         let update = cache_update_patch(kind(), &state, &drafts(), type_changed(), retained.as_ref(), ids);
                        busy.set(Some("save"));
                        spawn(async move {
                            let saved = if let Some(id) = editing_id {
                                client::update_cache_destination(id, &update).await
                            } else { client::create_cache_destination(&req).await };
                            match saved {
                                Ok(saved) => on_saved.call(saved),
                                Err(_) => error.set(Some("Cache save failed. Check values, credentials and permissions, then retry.".into())),
                            }
                            busy.set(None);
                        });
                    }, Icon { name: IconName::Check, size: 13 } if busy() == Some("save") { "Saving…" } else if destination.is_some() { "Save changes" } else { "Add cache" } }
                    }
                }
                DialogFocusSentinel { dialog_id: "cache-destination-dialog".to_string(), boundary: DialogFocusBoundary::First }
            }
        }
        if show_credential() { CacheCredModal { cache_type: kind().to_string(), initial: if credential_modal_new() { None } else { selected_credential }, on_close: move |value: Option<LocalCredential>| {
            show_credential.set(false);
            if let Some(cred) = value {
                result.set(None);
                let (profile, access, secret, token) = credential_fields_for_request(Some(&cred));
                for (field, value) in [("profile", profile), ("access", access), ("secret", secret), ("token", token), ("session", cred.session_token.clone())] {
                    // A key draft must not erase an independently configured
                    // builder profile when that draft has no profile override.
                    if field != "profile" || value.is_some() { drafts.write().0.insert(format!("{}.{field}", kind()), value.unwrap_or_default()); }
                }
                drafts.write().0.insert(format!("{}.credential", kind()), cred.id.clone());
                if current_configured
                    && drafts().request(kind(), &form()).attic_token.is_none()
                    && drafts().request(kind(), &form()).s3_access_key_id.is_none()
                    && drafts().request(kind(), &form()).s3_secret_access_key.is_none() {
                    drafts.write().0.insert(format!("{}.credential", kind()), "__current__".into());
                }
                credentials.write().retain(|c| c.id != cred.id);
                credentials.write().push(cred);
            }
        } } }
        if let Some(plane) = identity_dialog() {
            Niks3CredentialModal { plane: plane.clone(), initial: form(), destination: destination.clone(), on_close: move |replacement: Option<Niks3FormState>| {
                identity_dialog.set(None);
                if let Some(replacement) = replacement {
                    result.set(None);
                    let mut state = form.write();
                    // Each modal owns only its plane. Confirming read auth must
                    // never overwrite write auth or the destination/scope draft.
                    if plane == "Write" {
                        state.token = replacement.token; state.write_cert = replacement.write_cert;
                        state.write_key = replacement.write_key; state.write_ca = replacement.write_ca;
                        state.clear_write_ca = replacement.clear_write_ca;
                    } else {
                        state.read_cert = replacement.read_cert; state.read_key = replacement.read_key;
                        state.read_ca = replacement.read_ca; state.clear_read_ca = replacement.clear_read_ca;
                    }
                }
            } }
        }
    }
}

fn niks3_identity_configured(
    plane: &str,
    form: &Niks3FormState,
    destination: Option<&CacheDestination>,
) -> bool {
    destination.is_some_and(|d| {
        cache_form_kind(&d.cache_type) == "niks3"
            && if plane == "Write" {
                d.niks3_write_auth_mode.as_deref() == Some(form.write_mode.as_str())
                    && if form.write_mode == "token" {
                        d.niks3_write_token_configured
                    } else {
                        d.niks3_write_mtls_configured
                    }
            } else {
                d.niks3_read_auth_mode.as_deref() == Some("mtls")
                    && form.read_mode == "mtls"
                    && d.niks3_read_mtls_configured
            }
    })
}

fn niks3_identity_draft(plane: &str, form: &Niks3FormState) -> bool {
    if plane == "Write" {
        !form.token.trim().is_empty()
            || !form.write_cert.trim().is_empty()
            || !form.write_key.trim().is_empty()
            || !form.write_ca.trim().is_empty()
            || form.clear_write_ca
    } else {
        !form.read_cert.trim().is_empty()
            || !form.read_key.trim().is_empty()
            || !form.read_ca.trim().is_empty()
            || form.clear_read_ca
    }
}

/// Renders independent configured and replacement choices for one Niks3 plane.
#[component]
fn Niks3CredentialControl(
    plane: String,
    mut form: Signal<Niks3FormState>,
    mut result: Signal<Option<crate::api::models::CacheCredentialTestResult>>,
    destination: Option<CacheDestination>,
    on_open: EventHandler<String>,
    on_test: EventHandler<MouseEvent>,
    testing: bool,
) -> Element {
    let configured = niks3_identity_configured(&plane, &form(), destination.as_ref());
    let draft = niks3_identity_draft(&plane, &form());
    let id = format!("niks3-{}-credential", plane.to_lowercase());
    let selection_plane = plane.clone();
    rsx! { div { class: "field",
        label { r#for: "{id}", "{plane} credential" }
        div { style: "display:flex;gap:8px;align-items:center;",
            select { id, class: "input focus-ring", value: if draft { "__draft__" } else if configured { "__current__" } else { "" }, onchange: move |e| {
                if e.value() == "__new__" { on_open.call(selection_plane.clone()); }
                else if e.value() == "__current__" {
                    result.set(None); let mut state = form.write();
                    if selection_plane == "Write" { state.token.clear(); state.write_cert.clear(); state.write_key.clear(); state.write_ca.clear(); state.clear_write_ca = false; }
                    else { state.read_cert.clear(); state.read_key.clear(); state.read_ca.clear(); state.clear_read_ca = false; }
                }
            },
                option { value: "", selected: !draft && !configured, "Select a credential…" }
                if configured { option { value: "__current__", selected: !draft, "Current configured credential" } }
                if draft { option { value: "__draft__", selected: true, "Replacement credential draft" } }
                option { value: "__new__", "+ Enter credential…" }
            }
            if plane == "Write" { button { class: "btn btn-ghost focus-ring xs", onclick: on_test, if testing { "Testing…" } else { "Test connection" } } }
        }
        button { class: "btn btn-ghost focus-ring xs", style: "margin-top:8px;width:fit-content;display:inline-flex;", onclick: { let plane = plane.clone(); move |_| on_open.call(plane.clone()) }, if configured { "Replace {plane} credential" } else if draft { "Edit {plane} credential" } else { "Add {plane} credential" } }
        p { class: "help", if draft { "Replacement draft selected. Cancel the cache dialog to discard it." } else if configured { "Credentials stored. Secrets remain on the server." } else { "Enter this plane's identity in its credential dialog." } }
    } }
}

/// Edits a snapshot of one Niks3 identity without persisting or mutating its parent.
/// Cancel discards this snapshot. Confirmation returns replacements and CA clears.
#[component]
fn Niks3CredentialModal(
    plane: String,
    initial: Niks3FormState,
    destination: Option<CacheDestination>,
    on_close: EventHandler<Option<Niks3FormState>>,
) -> Element {
    let mut form = use_signal(move || initial);
    let result = use_signal(|| None::<crate::api::models::CacheCredentialTestResult>);
    let write = plane == "Write";
    let configured = niks3_identity_configured(&plane, &form(), destination.as_ref());
    let has_ca = destination.as_ref().is_some_and(|d| {
        if write {
            d.niks3_write_ca_cert.is_some()
        } else {
            d.niks3_read_ca_cert.is_some()
        }
    });
    rsx! { div { class: "modal-backdrop modal-backdrop-above-drawer", style: CACHE_CREDENTIAL_BACKDROP_STYLE, onclick: move |e| { e.stop_propagation(); on_close.call(None); },
        div { id: "niks3-credential-dialog", class: "modal", style: CACHE_CREDENTIAL_DIALOG_STYLE, role: "dialog", aria_modal: "true", aria_label: "{plane} credential", tabindex: "-1", onclick: move |e| e.stop_propagation(), onkeydown: move |e| { e.stop_propagation(); if e.key() == Key::Escape { e.prevent_default(); on_close.call(None); } },
            DialogFocusRestore {} DialogInitialFocus { dialog_id: "niks3-credential-dialog".to_string() }
            DialogFocusSentinel { dialog_id: "niks3-credential-dialog".to_string(), boundary: DialogFocusBoundary::Last }
            div { class: "modal-head", h2 { "{plane} credential" } }
            div { class: "modal-body",
                p { class: "help", if configured { "Leave identity fields blank to retain stored credentials. Replace certificate and key together." } else { "Confirm a local credential draft. The cache Save action persists it." } }
                if write && form().write_mode == "token" {
                    Niks3TextField { label: "Write token", field: "token", form, result, multiline: false, secret: true, hint: "Stored tokens are never returned.".to_string() }
                } else {
                    Niks3TextField { label: "{plane} client certificate", field: if write { "write_cert" } else { "read_cert" }, form, result, multiline: true, secret: false, hint: "PEM client certificate.".to_string() }
                    Niks3TextField { label: "{plane} private key", field: if write { "write_key" } else { "read_key" }, form, result, multiline: true, secret: true, hint: "PEM private key. Stored keys are never returned.".to_string() }
                    Niks3TextField { label: "{plane} CA certificate (optional)", field: if write { "write_ca" } else { "read_ca" }, form, result, multiline: true, secret: false, hint: if has_ca { "Blank retains the configured CA.".to_string() } else { "Blank uses system trust.".to_string() } }
                    if has_ca { label { input { r#type: "checkbox", checked: if write { form().clear_write_ca } else { form().clear_read_ca }, onchange: move |e| { if write { form.write().clear_write_ca = e.checked(); } else { form.write().clear_read_ca = e.checked(); } } } " Remove {plane} custom CA on save" } }
                }
            }
            div { class: "modal-foot", button { class: "btn btn-ghost focus-ring", onclick: move |_| on_close.call(None), "Cancel" } button { class: "btn btn-primary focus-ring", onclick: move |_| on_close.call(Some(form())), "Use credential" } }
            DialogFocusSentinel { dialog_id: "niks3-credential-dialog".to_string(), boundary: DialogFocusBoundary::First }
        }
    } }
}

fn is_http_url(value: &str) -> bool {
    let trimmed = value.trim();
    (trimmed.starts_with("https://") || trimmed.starts_with("http://")) && trimmed.len() > 8
}

fn is_s3_url(value: &str) -> bool {
    let trimmed = value.trim();
    let Some(rest) = trimmed.strip_prefix("s3://") else {
        return false;
    };

    let bucket = rest.split('/').next().unwrap_or_default().trim();
    !bucket.is_empty()
}

fn is_attic_public_key(value: &str) -> bool {
    let trimmed = value.trim();
    let Some((name, key)) = trimmed.split_once(':') else {
        return false;
    };

    !name.trim().is_empty()
        && !key.trim().is_empty()
        && key
            .trim()
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '/' | '=' | '-' | '_'))
}

#[derive(Clone)]
struct CacheFormValidationInput {
    name: String,
    cache_type: String,
    push_to: String,
    attic_cache_name: String,
    attic_public_key: String,
    attic_token: String,
    s3_region: String,
    s3_access_key_id: String,
    s3_secret_access_key: String,
    s3_endpoint_url: String,
    require_attic_token: bool,
    require_s3_secret_access_key: bool,
}

fn validate_cache_destination_form(input: &CacheFormValidationInput) -> HashMap<String, String> {
    let mut errors = HashMap::new();

    if input.name.trim().is_empty() {
        errors.insert("name".to_string(), "Cache name is required".to_string());
    }

    match input.cache_type.as_str() {
        "Attic" => {
            if input.attic_cache_name.trim().is_empty() {
                errors.insert(
                    "attic_cache_name".to_string(),
                    "Attic cache name is required".to_string(),
                );
            }

            if input.push_to.trim().is_empty() {
                errors.insert(
                    "push_to".to_string(),
                    "Attic server URL is required".to_string(),
                );
            } else if !is_http_url(&input.push_to) {
                errors.insert(
                    "push_to".to_string(),
                    "Attic server URL must start with http:// or https://".to_string(),
                );
            }

            if input.attic_public_key.trim().is_empty() {
                errors.insert(
                    "attic_public_key".to_string(),
                    "Attic public key is required".to_string(),
                );
            } else if !is_attic_public_key(&input.attic_public_key) {
                errors.insert(
                    "attic_public_key".to_string(),
                    "Attic public key must look like cache-name:BASE64KEY".to_string(),
                );
            }

            if input.require_attic_token && input.attic_token.trim().is_empty() {
                errors.insert(
                    "attic_token".to_string(),
                    "Attic token is required".to_string(),
                );
            }
        }
        "S3" => {
            if input.push_to.trim().is_empty() {
                errors.insert(
                    "push_to".to_string(),
                    "Destination URL is required".to_string(),
                );
            } else if !is_s3_url(&input.push_to) {
                errors.insert(
                    "push_to".to_string(),
                    "S3 destination must look like s3://bucket or s3://bucket/prefix".to_string(),
                );
            }

            if input.s3_region.trim().is_empty() {
                errors.insert("s3_region".to_string(), "S3 region is required".to_string());
            }

            if input.s3_access_key_id.trim().is_empty() {
                errors.insert(
                    "s3_access_key_id".to_string(),
                    "AWS access key ID is required".to_string(),
                );
            }

            if input.require_s3_secret_access_key && input.s3_secret_access_key.trim().is_empty() {
                errors.insert(
                    "s3_secret_access_key".to_string(),
                    "AWS secret access key is required".to_string(),
                );
            }

            if input.s3_endpoint_url.trim().is_empty() {
                errors.insert(
                    "s3_endpoint_url".to_string(),
                    "S3 endpoint URL is required".to_string(),
                );
            } else if !is_http_url(&input.s3_endpoint_url) {
                errors.insert(
                    "s3_endpoint_url".to_string(),
                    "S3 endpoint URL must start with http:// or https://".to_string(),
                );
            }
        }
        "Nix" | "Http" => {
            if input.push_to.trim().is_empty() {
                errors.insert(
                    "push_to".to_string(),
                    "Destination URL is required".to_string(),
                );
            } else if !is_http_url(&input.push_to) {
                errors.insert(
                    "push_to".to_string(),
                    "Destination URL must start with http:// or https://".to_string(),
                );
            }
        }
        _ => {}
    }

    errors
}

fn came_from_setup() -> bool {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    {
        let flag = storage.get_item("cf.from_setup").ok().flatten();
        if flag.as_deref() == Some("1") {
            let _ = storage.remove_item("cf.from_setup");
            return true;
        }
    }
    false
}

fn query_param(name: &str) -> Option<String> {
    let window = web_sys::window()?;
    let search = window.location().search().ok()?;
    let query = search.trim_start_matches('?');
    if query.is_empty() {
        return None;
    }

    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or_default();
        let value = parts.next().unwrap_or_default();
        if key == name {
            return js_sys::decode_uri_component(value)
                .ok()
                .map(|v| v.as_string().unwrap_or_default());
        }
    }

    None
}

/// Remove one or more query parameters from the URL without reloading the page.
fn clear_url_params(names: &[&str]) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(win) = web_sys::window() else { return };
        let pathname = win.location().pathname().ok().unwrap_or_default();
        let search = win.location().search().ok().unwrap_or_default();
        let query = search.trim_start_matches('?');
        if query.is_empty() {
            return;
        }
        let remaining: Vec<&str> = query
            .split('&')
            .filter(|pair| {
                let key = pair.splitn(2, '=').next().unwrap_or("");
                !names.iter().any(|n| *n == key)
            })
            .collect();
        let new_search = if remaining.is_empty() {
            String::new()
        } else {
            format!("?{}", remaining.join("&"))
        };
        if let Ok(history) = win.history() {
            let _ = history.replace_state_with_url(
                &wasm_bindgen::JsValue::NULL,
                "",
                Some(&format!("{pathname}{new_search}")),
            );
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum CachesTab {
    Destinations,
    PushJobs,
}

#[derive(Clone, Copy, PartialEq)]
enum CacheViewMode {
    Cards,
    Table,
}

#[derive(Clone, PartialEq)]
enum LocalCredentialKind {
    AwsKey,
    AwsRole,
    AtticToken,
    NixToken,
}

#[derive(Clone, PartialEq)]
struct LocalCredential {
    id: String,
    name: String,
    kind: LocalCredentialKind,
    access_key_id: Option<String>,
    secret_access_key: Option<String>,
    role_arn: Option<String>,
    token: Option<String>,
    session_token: Option<String>,
}

fn credential_label(cred: &LocalCredential) -> String {
    let suffix = match cred.kind {
        LocalCredentialKind::AwsKey => "AWS key",
        LocalCredentialKind::AwsRole => "IAM role",
        LocalCredentialKind::AtticToken => "Attic token",
        LocalCredentialKind::NixToken => "Nix token",
    };
    format!("{} ({})", cred.name, suffix)
}

fn credential_matches_cache_type(cred: &LocalCredential, cache_type: &str) -> bool {
    match cache_type {
        "s3" => matches!(
            cred.kind,
            LocalCredentialKind::AwsKey | LocalCredentialKind::AwsRole
        ),
        "attic" => matches!(cred.kind, LocalCredentialKind::AtticToken),
        _ => matches!(cred.kind, LocalCredentialKind::NixToken),
    }
}

fn api_cache_type(cache_type: &str) -> String {
    match cache_type {
        "s3" => "S3".to_string(),
        "attic" => "Attic".to_string(),
        "nix" => "Nix".to_string(),
        other => other.to_string(),
    }
}

fn credential_fields_for_request(
    selected_credential: Option<&LocalCredential>,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    // COMPATIBILITY: The legacy IAM-role draft maps to a builder profile, not
    // an STS role-assumption operation. Preserve that request representation.
    let s3_profile = selected_credential.and_then(|cred| match cred.kind {
        LocalCredentialKind::AwsRole => cred.role_arn.clone(),
        _ => None,
    });
    let s3_access_key_id = selected_credential.and_then(|cred| cred.access_key_id.clone());
    let s3_secret_access_key = selected_credential.and_then(|cred| cred.secret_access_key.clone());
    let attic_token = selected_credential.and_then(|cred| match cred.kind {
        LocalCredentialKind::AtticToken | LocalCredentialKind::NixToken => cred.token.clone(),
        _ => None,
    });

    (
        s3_profile,
        s3_access_key_id,
        s3_secret_access_key,
        attic_token,
    )
}

/// Cache management page
#[component]
pub fn CachesView() -> Element {
    let from_setup = use_signal(came_from_setup);
    let mut show_add_modal = use_signal(|| false);

    // Load destinations for stats display
    let mut refresh_nonce = use_signal(|| 0_u32);
    let destinations = use_resource(move || {
        let _nonce = refresh_nonce();
        async move { client::fetch_cache_destinations(false).await }
    });

    rsx! {
        div {
            class: "space-y-6",

            // Page header matching mockup (JSX lines 23-33)
            div {
                class: "page-head",
                "data-coach-target": "cache-page-head",
                div {
                    h1 { class: "page-title", "Caches" }
                    p {
                        class: "page-subtitle",
                        // Show totals: X destinations · Y enabled. "Paths cached" is not
                        // shown here because no backend metric exists yet for it —
                        // see the stat strip below, which renders "—" for that card
                        // rather than a fabricated number.
                        match destinations.read().as_ref() {
                            Some(Ok(dests)) => {
                                let total = dests.len();
                                let enabled = dests.iter().filter(|d| d.enabled).count();
                                rsx! { "{total} destinations · {enabled} enabled" }
                            },
                            _ => rsx! { "Loading…" }
                        }
                    }
                }
                // + Add cache button (mockup lines 30-32)
                button {
                    "data-coach-target": "cache",
                    class: "btn btn-primary focus-ring",
                    onclick: move |_| {
                        show_add_modal.set(true);
                    },
                    svg {
                        width: "14",
                        height: "14",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        style: "display:inline-block; vertical-align:text-bottom; margin-right:4px;",
                        line { x1: "12", y1: "5", x2: "12", y2: "19" }
                        line { x1: "5", y1: "12", x2: "19", y2: "12" }
                    }
                    " Add cache"
                }
            }

            // Stat strip matching mockup (JSX lines 35-48)
            div {
                class: "stat-strip",
                match destinations.read().as_ref() {
                    Some(Ok(dests)) => {
                        let total = dests.len();
                        let enabled_count = dests.iter().filter(|d| d.enabled).count();
                        rsx! {
                            div {
                                class: "stat",
                                span { class: "stat-accent", style: "--stat-color: #a78bfa;" }
                                div { class: "stat-label", "Total caches" }
                                div { class: "stat-value", "{total}" }
                            }
                            div {
                                class: "stat",
                                span { class: "stat-accent", style: "--stat-color: #34d399;" }
                                div { class: "stat-label", "Enabled" }
                                div { class: "stat-value", "{enabled_count}" }
                            }
                            div {
                                class: "stat",
                                span { class: "stat-accent", style: "--stat-color: #fbbf24;" }
                                div { class: "stat-label", "Disabled" }
                                div { class: "stat-value", "{total - enabled_count}" }
                            }
                            div {
                                class: "stat",
                                span { class: "stat-accent", style: "--stat-color: #60a5fa;" }
                                div { class: "stat-label", "Paths cached" }
                                div { class: "stat-value", "—" }
                            }
                        }
                    },
                    _ => rsx! {
                        div {
                            class: "stat",
                            span { class: "stat-accent", style: "--stat-color: #a78bfa;" }
                            div { class: "stat-label", "Total caches" }
                            div { class: "stat-value", "—" }
                        }
                    }
                }
            }

            CacheDestinationsList {
                show_onboarding_hint: from_setup(),
                refresh_nonce: refresh_nonce,
                show_add_modal: show_add_modal,
            }
        }
    }
}

/// List of cache destinations with CRUD operations
#[component]
fn CacheDestinationsList(
    show_onboarding_hint: bool,
    refresh_nonce: Signal<u32>,
    mut show_add_modal: Signal<bool>,
) -> Element {
    let destinations = use_resource(move || {
        let _nonce = refresh_nonce();
        async move { client::fetch_cache_destinations(false).await }
    });

    let mut search_query = use_signal(String::new);
    let mut view_mode = use_signal(|| CacheViewMode::Cards);
    let mut edit_destination = use_signal(|| None::<CacheDestination>);
    let mut view_destination = use_signal(|| None::<CacheDestination>);
    let focus_value = query_param("focus");

    // Fetch available environments for assignment and cache-assignment display.
    let environments = use_resource(|| async move { client::fetch_environments().await });

    {
        let maybe_dests = destinations.read().clone();
        let mut view_destination = view_destination.clone();
        use_effect(move || {
            if view_destination.read().is_some() {
                return;
            }
            let Some(focus) = focus_value.clone() else {
                return;
            };
            let Some(Ok(dests)) = maybe_dests.as_ref() else {
                return;
            };
            let focus_lower = focus.to_ascii_lowercase();
            if let Some(dest) = dests.iter().find(|dest| {
                dest.name.to_ascii_lowercase() == focus_lower
                    || dest
                        .push_to
                        .as_ref()
                        .is_some_and(|url| url.to_ascii_lowercase() == focus_lower)
                    || dest.id.to_string() == focus
            }) {
                view_destination.set(Some(dest.clone()));
                // Clear focus param so closing and re-opening the panel stays closed.
                clear_url_params(&["focus"]);
            }
        });
    }

    rsx! {
        div {
            class: "space-y-4",

            // Filter bar matching mockup (JSX lines 50-56)
            div {
                class: "filterbar",
                div {
                    class: "filter-search",
                    style: "max-width:320px;",
                    // Search icon (simplified inline SVG)
                    svg {
                        width: "16",
                        height: "16",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        circle { cx: "11", cy: "11", r: "8" }
                        path { d: "m21 21-4.3-4.3" }
                    }
                    input {
                        class: "input focus-ring",
                        placeholder: "Search caches…",
                        value: "{search_query}",
                        oninput: move |evt| search_query.set(evt.value())
                    }
                }
                div {
                    class: "seg",
                    button {
                        class: if view_mode() == CacheViewMode::Cards { "active" } else { "" },
                        onclick: move |_| view_mode.set(CacheViewMode::Cards),
                        Icon { name: crate::components::icon::IconName::Grid, size: 12 }
                        " Cards"
                    }
                    button {
                        class: if view_mode() == CacheViewMode::Table { "active" } else { "" },
                        onclick: move |_| view_mode.set(CacheViewMode::Table),
                        Icon { name: crate::components::icon::IconName::Rows, size: 12 }
                        " Table"
                    }
                }
                span {
                    class: "filter-count",
                    // Show filtered count
                    match destinations.read().as_ref() {
                        Some(Ok(dests)) => {
                            let query = search_query().to_lowercase();
                            let filtered = if query.is_empty() {
                                dests.len()
                            } else {
                                dests.iter().filter(|d| {
                                    d.name.to_lowercase().contains(&query) ||
                                    d.push_to.as_ref().map(|u| u.to_lowercase().contains(&query)).unwrap_or(false)
                                }).count()
                            };
                            rsx! { "{filtered} caches" }
                        },
                        _ => rsx! { "— caches" }
                    }
                }
            }

            // List - table format matching mockup (JSX lines 58-76)
            match &*destinations.read_unchecked() {
                Some(Ok(dests)) => {
                    // Filter destinations based on search query
                    let query = search_query().to_lowercase();
                    let filtered: Vec<_> = if query.is_empty() {
                        dests.iter().collect()
                    } else {
                        dests.iter().filter(|d| {
                            d.name.to_lowercase().contains(&query) ||
                            d.push_to.as_ref().map(|u| u.to_lowercase().contains(&query)).unwrap_or(false)
                        }).collect()
                    };

                    rsx! {
                        if filtered.is_empty() && !query.is_empty() {
                            div {
                                class: "{theme::presets::CARD} text-center py-12",
                                p { class: "{theme::text::SECONDARY}", "No caches match \"{query}\"" }
                            }
                        } else if dests.is_empty() {
                            div {
                                class: "{theme::presets::CARD} text-center py-12",
                                p { class: "{theme::text::SECONDARY}", "No cache destinations configured." }
                                p { class: "{theme::text::MUTED} text-sm mt-2", "Add your first cache destination to start pushing build artifacts." }
                            }
                        } else {
                            if view_mode() == CacheViewMode::Cards {
                                div {
                                    class: "cards-grid",
                                    for dest in filtered {
                                        CacheDestinationCardNew {
                                            destination: dest.clone(),
                                            environments: environments,
                                            on_view: move |d: CacheDestination| view_destination.set(Some(d)),
                                            on_edit: move |d: CacheDestination| edit_destination.set(Some(d)),
                                        }
                                    }
                                }
                            } else {
                                div {
                                    class: "card",
                                    style: "overflow:hidden;",
                                    table {
                                        class: "sys-table",
                                        thead {
                                            tr {
                                                th { "Cache" }
                                                th { "Type" }
                                                th { "Status" }
                                                th { "Storage" }
                                                th { "Paths" }
                                                th { "Last push" }
                                                th { "Environments" }
                                                th { style: "text-align:right;", " " }
                                            }
                                        }
                                        tbody {
                                            for dest in filtered {
                                                CacheDestinationRow {
                                                    destination: dest.clone(),
                                                    environments: environments,
                                                    on_view: move |d: CacheDestination| view_destination.set(Some(d)),
                                                    on_edit: move |d: CacheDestination| edit_destination.set(Some(d)),
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                Some(Err(e)) => rsx! {
                    div {
                        class: "{theme::presets::CARD} border-red-500/30 bg-red-500/5",
                        p { class: "text-red-400", "Error loading destinations: {e}" }
                    }
                },
                None => rsx! {
                    div {
                        class: "{theme::presets::CARD} text-center py-12",
                        p { class: "{theme::text::SECONDARY}", "Loading cache destinations..." }
                    }
                },
            }

            if let Some(destination) = view_destination() {
                CacheDestinationPanel {
                    destination,
                    refresh_nonce,
                    on_close: move |_| view_destination.set(None),
                    on_edit: move |dest: CacheDestination| {
                        edit_destination.set(Some(dest));
                    },
                }
            }

            // The Add dialog stays mounted while its selected type changes.
            if show_add_modal() || edit_destination().is_some() {
                CacheDestinationForm {
                    destination: edit_destination(),
                    add_draft: Niks3FormState::from_destination(None),
                    add_environment_ids: Vec::new(),
                    on_close: move |_| { show_add_modal.set(false); edit_destination.set(None); },
                    on_saved: move |saved: CacheDestination| {
                        show_add_modal.set(false); edit_destination.set(None);
                        if view_destination().is_some_and(|d| d.id == saved.id) { view_destination.set(Some(saved)); }
                        refresh_nonce.set(refresh_nonce() + 1);
                    },
                }
            }

        }
    }
}

/// Renders a named, keyboard-contained dialog for a local credential draft.
/// Closing restores focus to the opener. Escape cancels only this dialog;
/// confirmation returns draft fields without persisting a credential inventory.
#[component]
fn CacheCredModal(
    cache_type: String,
    initial: Option<LocalCredential>,
    on_close: EventHandler<Option<LocalCredential>>,
) -> Element {
    let mut cred_kind = use_signal(|| {
        if initial
            .as_ref()
            .is_some_and(|c| c.kind == LocalCredentialKind::AwsRole)
        {
            "aws-role"
        } else if cache_type == "s3" {
            "aws-key"
        } else if cache_type == "attic" {
            "attic-token"
        } else {
            "nix-token"
        }
    });
    let seed = initial.clone();
    let mut cred_name = use_signal(move || seed.map(|c| c.name).unwrap_or_default());
    let seed = initial.clone();
    let mut cred_access_key =
        use_signal(move || seed.and_then(|c| c.access_key_id).unwrap_or_default());
    let seed = initial.clone();
    let mut cred_secret_key =
        use_signal(move || seed.and_then(|c| c.secret_access_key).unwrap_or_default());
    let seed = initial.clone();
    let mut cred_token = use_signal(move || seed.and_then(|c| c.token).unwrap_or_default());
    let seed = initial.clone();
    let mut cred_role_arn = use_signal(move || seed.and_then(|c| c.role_arn).unwrap_or_default());
    let mut cred_session =
        use_signal(move || initial.and_then(|c| c.session_token).unwrap_or_default());

    rsx! {
        div {
            // The production overlay uses a higher stack level than the JSX
            // example. Use its existing nested-overlay tier above the parent.
            class: "modal-backdrop modal-backdrop-above-drawer",
            style: CACHE_CREDENTIAL_BACKDROP_STYLE,
            onclick: move |event| { event.stop_propagation(); on_close.call(None); },
            div {
                id: "cache-credential-dialog",
                class: "modal",
                style: CACHE_CREDENTIAL_DIALOG_STYLE,
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "cache-credential-title",
                aria_describedby: "cache-credential-description",
                tabindex: "-1",
                onclick: move |e| e.stop_propagation(),
                onkeydown: move |event| {
                    // Keep nested keys out of the destination overlay. Native
                    // Tab traversal is contained by the two focus sentinels.
                    event.stop_propagation();
                    if event.key() == Key::Escape {
                        event.prevent_default();
                        on_close.call(None);
                    }
                },
                DialogFocusRestore {}
                DialogInitialFocus { dialog_id: "cache-credential-dialog".to_string() }
                DialogFocusSentinel { dialog_id: "cache-credential-dialog".to_string(), boundary: DialogFocusBoundary::Last }

                div {
                    class: "modal-head",
                    style: "display:block;",
                    h2 {
                        id: "cache-credential-title",
                        svg {
                            width: "14",
                            height: "14",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            style: CACHE_CREDENTIAL_ICON_STYLE,
                            circle { cx: "7.5", cy: "12", r: "3.5" }
                            path { d: "M11 12h10" }
                            path { d: "M18 12v3" }
                            path { d: "M15.5 12v2" }
                        }
                        "Add credential"
                    }
                    p { id: "cache-credential-description", "Create a dialog-local credential draft. Secrets are encrypted when the cache is saved." }
                }

                div {
                    class: "modal-body",

                    div {
                        class: "field",
                        label { r#for: "cache-credential-name", "Name" }
                        input {
                            id: "cache-credential-name",
                            class: "input focus-ring",
                            value: cred_name(),
                            oninput: move |evt| cred_name.set(evt.value()),
                            placeholder: "e.g. aws-prod-role"
                        }
                    }

                    div {
                        class: "field",
                        label { id: "cache-credential-type-label", "Type" }
                        div {
                            class: "seg",
                            role: "group",
                            aria_labelledby: "cache-credential-type-label",
                            if cache_type == "s3" {
                                button {
                                    class: if cred_kind() == "aws-key" { "active focus-ring" } else { "focus-ring" },
                                    r#type: "button",
                                    aria_pressed: cred_kind() == "aws-key",
                                    onclick: move |_| cred_kind.set("aws-key"),
                                    "AWS access key"
                                }
                                button {
                                    class: if cred_kind() == "aws-role" { "active focus-ring" } else { "focus-ring" },
                                    r#type: "button",
                                    aria_pressed: cred_kind() == "aws-role",
                                    onclick: move |_| cred_kind.set("aws-role"),
                                    "IAM role (IRSA)"
                                }
                            } else if cache_type == "attic" {
                                button {
                                    class: "active focus-ring",
                                    r#type: "button",
                                    aria_pressed: true,
                                    "Attic token"
                                }
                            } else {
                                button {
                                    class: "active focus-ring",
                                    r#type: "button",
                                    aria_pressed: true,
                                    "Nix HTTPS token"
                                }
                            }
                        }
                    }

                    if cred_kind() == "aws-key" {
                        div {
                            class: "field",
                            label { r#for: "cache-credential-access", "Access key ID" }
                            input {
                                id: "cache-credential-access",
                                class: "input focus-ring mono",
                                style: "font-size:12px;",
                                value: cred_access_key(),
                                oninput: move |evt| cred_access_key.set(evt.value()),
                                placeholder: "AKIA…"
                            }
                        }
                        div { class: "field", label { r#for: "cache-credential-session", "AWS session token (optional)" }
                            input { id: "cache-credential-session", autocomplete: "off", r#type: "password", class: "input focus-ring mono", value: cred_session(), oninput: move |e| cred_session.set(e.value()) }
                            p { class: "help", "Blank clears a previous session token when replacing access keys. Retaining the current configured credential preserves it." }
                        }
                        div {
                            class: "field",
                            label { r#for: "cache-credential-secret", "Secret access key" }
                            input {
                                id: "cache-credential-secret",
                                autocomplete: "off",
                                r#type: "password",
                                class: "input focus-ring mono",
                                style: "font-size:12px;",
                                value: cred_secret_key(),
                                oninput: move |evt| cred_secret_key.set(evt.value()),
                                placeholder: "•••••••••••••••••"
                            }
                        }
                    }

                    if cred_kind() == "aws-role" {
                        div {
                            class: "field",
                            label { r#for: "cache-credential-role", "Role ARN" }
                            input {
                                id: "cache-credential-role",
                                class: "input focus-ring mono",
                                style: "font-size:12px;",
                                value: cred_role_arn(),
                                oninput: move |evt| cred_role_arn.set(evt.value()),
                                placeholder: "arn:aws:iam::123456789012:role/cache-pusher"
                            }
                            div {
                                class: "help",
                                "Legacy mapping: this value is sent as the S3 profile. Configure that profile on the executing builder; this form does not assume an IAM role. The cache API still requires access keys."
                            }
                        }
                    }

                    if cred_kind() == "attic-token" || cred_kind() == "nix-token" {
                        div {
                            class: "field",
                            label { r#for: "cache-credential-token", "Token" }
                            input {
                                id: "cache-credential-token",
                                autocomplete: "off",
                                r#type: "password",
                                class: "input focus-ring mono",
                                style: "font-size:12px;",
                                value: cred_token(),
                                oninput: move |evt| cred_token.set(evt.value()),
                                placeholder: "•••••••••••••••••"
                            }
                            div {
                                class: "help",
                                if cred_kind() == "attic-token" {
                                    "Attic / cache-server bearer token with push permission."
                                } else {
                                    "HTTPS cache bearer token or equivalent secret."
                                }
                            }
                        }
                    }
                }

                div {
                    class: "modal-foot",
                    button {
                        class: "btn btn-ghost focus-ring",
                        r#type: "button",
                        onclick: move |_| on_close.call(None),
                        "Cancel"
                    }
                    button {
                        class: "btn btn-primary focus-ring",
                        r#type: "button",
                        disabled: cred_name().trim().is_empty(),
                        onclick: move |_| {
                            let name = cred_name();
                            let cred_id = format!("cred-{cache_type}-{}", name.to_lowercase().replace(|c: char| !c.is_ascii_alphanumeric(), "-"));
                            let credential = LocalCredential {
                                id: cred_id,
                                name,
                                kind: match cred_kind() {
                                    "aws-key" => LocalCredentialKind::AwsKey,
                                    "aws-role" => LocalCredentialKind::AwsRole,
                                    "attic-token" => LocalCredentialKind::AtticToken,
                                    _ => LocalCredentialKind::NixToken,
                                },
                                access_key_id: if cred_kind() == "aws-key" { Some(cred_access_key()) } else { None },
                                secret_access_key: if cred_kind() == "aws-key" { Some(cred_secret_key()) } else { None },
                                role_arn: if cred_kind() == "aws-role" { Some(cred_role_arn()) } else { None },
                                token: if cred_kind() == "attic-token" || cred_kind() == "nix-token" { Some(cred_token()) } else { None },
                                session_token: if cred_kind() == "aws-key" { Some(cred_session()) } else { None },
                            };
                            on_close.call(Some(credential));
                        },
                        svg {
                            width: "13",
                            height: "13",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            style: "display:inline-block; vertical-align:text-bottom;",
                            polyline { points: "20 6 9 17 4 12" }
                        }
                        " Save credential"
                    }
                }
                DialogFocusSentinel { dialog_id: "cache-credential-dialog".to_string(), boundary: DialogFocusBoundary::First }
            }
        }
    }
}

#[component]
fn CacheDestinationCardNew(
    destination: CacheDestination,
    environments: Resource<Result<Vec<EnvironmentSummary>, ApiClientError>>,
    on_view: EventHandler<CacheDestination>,
    on_edit: EventHandler<CacheDestination>,
) -> Element {
    let cache_id = destination.id;
    let env_ids =
        use_resource(move || async move { client::get_cache_environments(cache_id).await });
    let dest_for_view = destination.clone();
    let dest_for_edit = destination.clone();
    let (status_cls, status_color, status_label) = if destination.enabled {
        ("chip-healthy", "#34d399", "enabled")
    } else {
        ("chip-critical", "#f87171", "disabled")
    };

    rsx! {
        div {
            class: "env-card",
            style: "cursor:pointer;",
            onclick: move |_| on_view.call(dest_for_view.clone()),
            div { class: "env-card-rail", style: "background:{status_color};" }
            div { class: "env-card-head",
                div {
                    div { class: "env-card-title",
                        Icon { name: cache_type_icon(&destination.cache_type), size: 13 }
                        span { "{destination.name}" }
                    }
                    if let Some(url) = destination.push_to.clone() {
                        div { class: "env-card-desc mono", "{url}" }
                    }
                }
                div { style: "display:flex; gap:4px;",
                    button {
                        class: "btn-icon focus-ring",
                        title: "Edit",
                        onclick: move |e| {
                            e.stop_propagation();
                            on_edit.call(dest_for_edit.clone());
                        },
                        Icon { name: IconName::Gear, size: 14 }
                    }
                }
            }
            div { style: "display:flex; gap:8px; flex-wrap:wrap; padding:0 16px;",
                span { class: "chip {status_cls}",
                    span { class: "chip-dot", style: "background:{status_color};" }
                    "{status_label}"
                }
                span { class: "chip chip-unknown mono", "{destination.cache_type}" }
            }
            div { style: "padding:12px 16px 0;",
                div { style: "font-size:11px; color:var(--cf-text-secondary); margin-bottom:4px;",
                    if let Some(last_used) = destination.last_used_at {
                        "Last push "
                        {last_used.format("%Y-%m-%d %H:%M").to_string()}
                    } else {
                        "Never pushed"
                    }
                }
            }
            div { class: "env-card-foot",
                span { style: "font-size:11px; color:var(--cf-text-muted);", "Updated {destination.updated_at.format(\"%Y-%m-%d\")}" }
                div { style: "display:flex; gap:4px; flex-wrap:wrap; justify-content:flex-end;",
                    {render_cache_assignment_state(env_ids, environments, "no environments")}
                }
            }
        }
    }
}

/// Cache destination table row matching mockup (JSX lines 89-153)
#[component]
fn CacheDestinationRow(
    destination: CacheDestination,
    environments: Resource<Result<Vec<EnvironmentSummary>, ApiClientError>>,
    on_view: EventHandler<CacheDestination>,
    on_edit: EventHandler<CacheDestination>,
) -> Element {
    let mut show_delete_confirm = use_signal(|| false);

    // Status mapping
    let (status_cls, status_color, status_label) = if destination.enabled {
        ("chip-healthy", "#34d399", "enabled")
    } else {
        ("chip-critical", "#f87171", "disabled")
    };

    // Type icon glyph family
    let is_link_icon = matches!(destination.cache_type.as_str(), "Nix" | "Http");

    // Fetch environment assignments
    let cache_id = destination.id;
    let env_ids =
        use_resource(move || async move { client::get_cache_environments(cache_id).await });

    let dest_for_click = destination.clone();
    let dest_for_edit_btn = destination.clone();

    rsx! {
        tr {
            style: "cursor:pointer;",
            onclick: move |_| on_view.call(dest_for_click.clone()),

            // Cache column
            td {
                div {
                    style: "font-weight:600; font-size:13px; display:flex; align-items:center; gap:6px;",
                    // Icon (inline SVG)
                    if is_link_icon {
                        svg {
                            width: "12",
                            height: "12",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            style: "opacity:0.6;",
                            path { d: "M10 13a5 5 0 0 0 7.07 0l2.83-2.83a5 5 0 0 0-7.07-7.07L10 5" }
                            path { d: "M14 11a5 5 0 0 0-7.07 0L4.1 13.83a5 5 0 0 0 7.07 7.07L14 19" }
                        }
                    } else {
                        svg {
                            width: "12",
                            height: "12",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            style: "opacity:0.6;",
                            path { d: "M12 3v12" }
                            path { d: "m7 10 5 5 5-5" }
                            path { d: "M5 21h14" }
                        }
                    }
                    "{destination.name}"
                }
                if let Some(ref url) = destination.push_to {
                    div {
                        class: "mono",
                        style: "font-size:11px; color:var(--cf-text-muted);",
                        "{url}"
                    }
                }
            }

            // Type column
            td {
                span {
                    class: "chip chip-unknown mono",
                    style: "font-size:10px;",
                    "{destination.cache_type}"
                }
            }

            // Status column
            td {
                span {
                    class: "chip {status_cls}",
                    title: "{status_label}",
                    span {
                        class: "chip-dot",
                        style: "background: {status_color};",
                    }
                    "{status_label}"
                }
            }

            // Storage column — no backend metric yet; show placeholder
            td {
                span {
                    style: "font-size:11px; color:var(--cf-text-muted);",
                    "—"
                }
            }

            // Paths column
            td {
                class: "mono",
                style: "font-size:12px;",
                "—"
            }

            // Last push column
            td {
                style: "font-size:12px; color:var(--cf-text-secondary);",
                if let Some(ref last_used) = destination.last_used_at {
                    {format!("{}", last_used.format("%Y-%m-%d %H:%M"))}
                } else {
                    "—"
                }
            }

            // Environments column
            td {
                div {
                    style: "display:flex; gap:4px; flex-wrap:wrap;",
                    {render_cache_assignment_state(env_ids, environments, "none")}
                }
            }

            // Actions column
            td {
                div {
                    class: "row-actions",
                    button {
                        class: "btn-icon focus-ring",
                        title: "Edit",
                        onclick: move |e| {
                            e.stop_propagation();
                            on_edit.call(dest_for_edit_btn.clone());
                        },
                        // Gear icon (simple cog/settings icon)
                        svg {
                            width: "14",
                            height: "14",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            circle { cx: "12", cy: "12", r: "3" }
                            path { d: "M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3h.1a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8v.1a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" }
                        }
                    }
                }
            }
        }


    }
}

fn render_cache_assignment_state(
    env_ids: Resource<Result<Vec<Uuid>, ApiClientError>>,
    environments: Resource<Result<Vec<EnvironmentSummary>, ApiClientError>>,
    empty_label: &'static str,
) -> Element {
    let env_ids_state = env_ids.read();
    let environments_state = environments.read();

    match (&*env_ids_state, &*environments_state) {
        (None, _) | (_, None) => rsx! {
            span { style: "font-size:11px; color:var(--cf-text-muted);", "loading…" }
        },
        (Some(Err(err)), _) => rsx! {
            span {
                style: "font-size:11px; color:var(--cf-text-muted);",
                title: "{err}",
                "failed to load"
            }
        },
        (_, Some(Err(err))) => rsx! {
            span {
                style: "font-size:11px; color:var(--cf-text-muted);",
                title: "{err}",
                "environment list unavailable"
            }
        },
        (Some(Ok(ids)), Some(Ok(all_envs))) => {
            if ids.is_empty() {
                rsx! { span { style: "font-size:11px; color:var(--cf-text-muted);", "{empty_label}" } }
            } else {
                let matching: Vec<(String, String)> = all_envs
                    .iter()
                    .filter(|env| ids.contains(&env.id))
                    .map(|env| (env.name.clone(), env.color_hex.clone()))
                    .collect();
                let count = ids.len();
                if matching.is_empty() {
                    rsx! {
                        span {
                            style: "font-size:11px; color:var(--cf-text-muted);",
                            title: "Assigned environments were returned, but their names are unavailable in the shared environment list.",
                            "{count} assigned"
                        }
                    }
                } else {
                    rsx! {
                        for (name, color_hex) in matching.into_iter().take(3) {
                            EnvBadge { env_name: name, color_hex: color_hex }
                        }
                        if count > 3 {
                            span { class: "chip chip-unknown", style: "font-size:10px;", "+{count - 3}" }
                        }
                    }
                }
            }
        }
    }
}

/// Environment badge component
#[component]
fn EnvBadge(env_name: String, color_hex: String) -> Element {
    let color = normalize_env_color(&color_hex);

    rsx! {
        span {
            class: "chip chip-env",
            style: "font-size:10px; padding:3px 7px; border:1px solid {color}; background:color-mix(in oklab, {color} 14%, var(--cf-card-bg)); color:{color};",
            span {
                style: "width:5px; height:5px; border-radius:50%; background:{color}; display:inline-block; margin-right:4px;",
            }
            "{env_name}"
        }
    }
}

fn cache_type_icon(cache_type: &str) -> IconName {
    match cache_type.to_ascii_lowercase().as_str() {
        "nix" | "http" => IconName::Link,
        _ => IconName::Download,
    }
}

#[derive(Props, Clone, PartialEq)]
struct CacheDestinationPanelProps {
    destination: CacheDestination,
    refresh_nonce: Signal<u32>,
    on_close: EventHandler<()>,
    on_edit: EventHandler<CacheDestination>,
}

#[component]
fn CacheDestinationPanel(props: CacheDestinationPanelProps) -> Element {
    let destination = props.destination.clone();
    let dest_for_edit = destination.clone();
    let nav = use_navigator();
    let cache_id = destination.id;
    let assignments = use_resource(move || {
        // The peek remains mounted as the Edit opener. Refresh its scope after
        // a shared-editor save without replacing that focus-return element.
        let _ = (props.refresh_nonce)();
        async move {
            let env_ids = client::get_cache_environments(cache_id)
                .await
                .unwrap_or_default();
            let envs = client::fetch_environments().await.unwrap_or_default();
            let env_list: Vec<_> = envs
                .iter()
                .filter(|env| env_ids.contains(&env.id))
                .map(|env| (env.id, env.name.clone(), env.color_hex.clone()))
                .collect();

            let mut systems = Vec::<SystemSummary>::new();
            let mut seen = HashSet::new();
            for (_, env_name, _) in &env_list {
                if let Ok(response) = client::fetch_systems(&SystemsListParams {
                    page: Some(1),
                    per_page: Some(200),
                    search: None,
                    health_status: None,
                    deployment_status: None,
                    environment: Some(env_name.clone()),
                    sort_by: Some("hostname".to_string()),
                    sort_order: Some(SortOrder::Asc),
                })
                .await
                {
                    for system in response.items {
                        if seen.insert(system.id) {
                            systems.push(system);
                        }
                    }
                }
            }
            systems.sort_by(|a, b| a.hostname.to_lowercase().cmp(&b.hostname.to_lowercase()));
            (env_list, systems)
        }
    });
    let (status_cls, status_color, status_label) = if destination.enabled {
        ("chip-healthy", "#34d399", "enabled")
    } else {
        ("chip-critical", "#f87171", "disabled")
    };

    rsx! {
        div { class: "side-panel-backdrop", onclick: move |_| props.on_close.call(()) }
        aside { class: "side-panel", role: "dialog", aria_modal: "true",
            div { class: "panel-head",
                div { class: "panel-title",
                    h2 {
                        Icon { name: cache_type_icon(&destination.cache_type), size: 14 }
                        "{destination.name}"
                    }
                    if let Some(url) = destination.push_to.clone() {
                        span { class: "fqdn mono", "{url}" }
                    }
                }
                button { class: "btn-icon focus-ring", onclick: move |_| props.on_close.call(()), aria_label: "Close",
                    Icon { name: IconName::X, size: 16 }
                }
            }
            div { class: "panel-body",
                section { class: "panel-section",
                    div { style: "display:flex; gap:8px; flex-wrap:wrap;",
                        span { class: "chip {status_cls}",
                            span { class: "chip-dot", style: "background:{status_color};" }
                            "{status_label}"
                        }
                        span { class: "chip chip-unknown mono", "{destination.cache_type}" }
                    }
                }
                section { class: "panel-section",
                    h3 { "Details" }
                    dl { class: "kv-grid",
                        dt { "Last push" }
                        dd {
                            if let Some(last_used) = destination.last_used_at {
                                {last_used.format("%Y-%m-%d %H:%M").to_string()}
                            } else {
                                "—"
                            }
                        }
                        dt { "Created" }
                        dd { "{destination.created_at.format(\"%Y-%m-%d\")}" }
                        dt { "Compression" }
                        dd {
                            {destination
                                .compression
                                .clone()
                                .unwrap_or_else(|| "—".to_string())}
                        }
                    }
                }
                section { class: "panel-section",
                    h3 { "Environments" }
                    match assignments.read().as_ref() {
                        Some((envs, _)) if !envs.is_empty() => rsx! {
                            div { style: "display:flex; gap:6px; flex-wrap:wrap;",
                                for (_, name, color) in envs.iter() {
                                    EnvBadge { env_name: name.clone(), color_hex: color.clone() }
                                }
                            }
                        },
                        _ => rsx! { div { style: "font-size:12px; color:var(--cf-text-muted);", "none assigned" } }
                    }
                }
                section { class: "panel-section",
                    h3 { "Systems using this cache" }
                    match assignments.read().as_ref() {
                        Some((_, systems)) if !systems.is_empty() => rsx! {
                            div { style: "display:flex; flex-direction:column; gap:6px;",
                                for system in systems.iter().take(8) {
                                    button {
                                        class: "sd-commit-sha-link",
                                        style: "justify-content:flex-start; font-size:12.5px; padding:3px 4px; margin:-3px -4px; background:none; border:none; width:100%;",
                                        onclick: {
                                            let nav = nav.clone();
                                            let system_id = system.id.to_string();
                                            move |_| {
                                                nav.push(Route::SystemDetailView { id: system_id.clone(), tab: String::new(), poam: String::new(), config_mode: String::new(), revision: String::new(), generation: String::new(), deploy_generation: String::new(), cve_target: String::new(), cve_mode: String::new() });
                                            }
                                        },
                                        Icon { name: IconName::Server, size: 10 }
                                        span { class: "mono truncate", style: "flex:1; text-align:left;", "{system.hostname}" }
                                        if let Some(environment) = system.environment.clone() {
                                            span { class: "chip chip-unknown", style: "font-size:10px;", "{environment}" }
                                        }
                                    }
                                }
                                if systems.len() > 8 {
                                    div { style: "font-size:11px; color:var(--cf-text-muted);", "+{systems.len() - 8} more" }
                                }
                            }
                        },
                        _ => rsx! { div { style: "font-size:12px; color:var(--cf-text-muted);", "No systems in an assigned environment yet." } }
                    }
                }
            }
            div { class: "panel-actions",
                button { class: "btn btn-primary focus-ring", onclick: move |_| props.on_edit.call(dest_for_edit.clone()),
                    Icon { name: IconName::Gear, size: 12 }
                    " Edit cache"
                }
            }
        }
    }
}

fn CacheDestinationCard(destination: CacheDestination, on_change: EventHandler<()>) -> Element {
    let enabled_badge_class = if destination.enabled {
        format!(
            "{} bg-emerald-500/10 text-emerald-400 border-emerald-500/30",
            theme::presets::BADGE
        )
    } else {
        format!(
            "{} bg-gray-500/10 text-gray-400 border-gray-500/30",
            theme::presets::BADGE
        )
    };

    let type_badge_class = format!(
        "{} bg-blue-500/10 text-blue-400 border-blue-500/30",
        theme::presets::BADGE
    );

    let last_used_str = destination
        .last_used_at
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string());
    let created_str = destination.created_at.format("%Y-%m-%d").to_string();

    let mut show_delete_confirm = use_signal(|| false);
    let mut show_edit_modal = use_signal(|| false);
    let mut edit_name = use_signal(|| destination.name.clone());
    let mut edit_type = use_signal(|| destination.cache_type.clone());
    let mut edit_push_to = use_signal(|| destination.push_to.clone().unwrap_or_default());
    let mut edit_attic_cache_name =
        use_signal(|| destination.attic_cache_name.clone().unwrap_or_default());
    let mut edit_attic_public_key =
        use_signal(|| destination.attic_public_key.clone().unwrap_or_default());
    let mut edit_attic_token = use_signal(String::new);
    let mut edit_signing_key_path =
        use_signal(|| destination.signing_key_path.clone().unwrap_or_default());
    let mut edit_compression = use_signal(|| destination.compression.clone().unwrap_or_default());
    let mut edit_s3_region = use_signal(|| destination.s3_region.clone().unwrap_or_default());
    let mut edit_s3_profile = use_signal(|| destination.s3_profile.clone().unwrap_or_default());
    let mut edit_s3_access_key_id =
        use_signal(|| destination.s3_access_key_id.clone().unwrap_or_default());
    let mut edit_s3_secret_access_key = use_signal(String::new);
    let mut edit_s3_session_token =
        use_signal(|| destination.s3_session_token.clone().unwrap_or_default());
    let mut edit_s3_endpoint_url =
        use_signal(|| destination.s3_endpoint_url.clone().unwrap_or_default());
    let mut edit_error = use_signal(|| None::<String>);
    let mut edit_field_errors = use_signal(|| std::collections::HashMap::<String, String>::new());
    let mut edit_submitting = use_signal(|| false);
    let edit_modal_title_id = format!("edit-cache-destination-modal-title-{}", destination.id);
    let delete_modal_title_id = format!("delete-cache-destination-modal-title-{}", destination.id);

    // Fetch current environment assignments and available environments
    let cache_id = destination.id;
    let edit_environment_ids = use_resource(move || async move {
        client::get_cache_environments(cache_id)
            .await
            .unwrap_or_default()
    });
    let mut edit_selected_environments = use_signal(Vec::<Uuid>::new);
    let edit_environments = use_resource(|| async move { client::fetch_environments().await });

    // Initialize selected environments when loaded
    use_effect(move || {
        if let Some(loaded_env_ids) = edit_environment_ids.read().as_ref() {
            if edit_selected_environments().is_empty() && !loaded_env_ids.is_empty() {
                edit_selected_environments.set(loaded_env_ids.clone());
            }
        }
    });

    rsx! {
        div {
            class: "{theme::presets::CARD}",

            div {
                class: "flex justify-between items-start",

                div {
                    class: "flex-1",
                    div {
                        class: "flex items-center gap-3 mb-2",
                        h3 {
                            class: "{theme::typography::SECTION_TITLE} {theme::text::PRIMARY}",
                            "{destination.name}"
                        }
                        span {
                            class: "{enabled_badge_class} border",
                            if destination.enabled { "Enabled" } else { "Disabled" }
                        }
                        span {
                            class: "{type_badge_class} border",
                            "{destination.cache_type}"
                        }
                    }

                    if let Some(ref url) = destination.push_to {
                        p {
                            class: "text-sm {theme::text::SECONDARY} mb-3",
                            "→ {url}"
                        }
                    }

                    div {
                        class: "flex gap-4 text-xs {theme::text::MUTED}",
                        if let Some(ref last_used) = last_used_str {
                            span { "Last used: {last_used}" }
                        } else {
                            span { "Never used" }
                        }
                        span { "Created: {created_str}" }
                    }
                }

                div {
                    class: "flex gap-2",
                    button {
                        class: "px-3 py-1 text-sm rounded-lg {theme::interactive::GHOST_BTN} {theme::text::SECONDARY}",
                        onclick: move |_| {
                            show_edit_modal.set(true);
                        },
                        "Edit"
                    }
                    button {
                        class: "px-3 py-1 text-sm rounded-lg {theme::interactive::DANGER_BTN}",
                        onclick: move |_| {
                            show_delete_confirm.set(true);
                        },
                        "Delete"
                    }
                }
            }

            if show_edit_modal() {
                div {
                    class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4 cf-modal-overlay",
                    tabindex: "0",
                    onclick: move |_| show_edit_modal.set(false),
                    onkeydown: move |evt| {
                        if evt.key() == Key::Escape {
                            show_edit_modal.set(false);
                        }
                    },
                    div {
                        class: "relative {theme::surface::CARD_BG} border {theme::surface::CARD_BORDER} rounded-xl shadow-2xl p-6 w-full cf-modal-panel-44 flex flex-col",
                        style: "max-height: calc(100dvh - 2rem);",
                        role: "dialog",
                        aria_modal: "true",
                        aria_labelledby: "{edit_modal_title_id}",
                        onclick: move |e| e.stop_propagation(),

                        // Header
                        div {
                            class: "flex justify-between items-center mb-6 shrink-0",
                            h3 {
                                id: "{edit_modal_title_id}",
                                class: "{theme::typography::SECTION_TITLE} {theme::text::PRIMARY}",
                                "Edit Cache Destination"
                            }
                            button {
                                r#type: "button",
                                class: "{theme::text::SECONDARY} hover:{theme::text::PRIMARY} text-lg",
                                title: "Close edit cache destination modal",
                                aria_label: "Close edit cache destination modal",
                                onclick: move |_| show_edit_modal.set(false),
                                "✕"
                            }
                        }

                        // Scrollable body
                        div {
                            class: "flex-1 min-h-0 overflow-y-auto space-y-4 pr-1",
                            div {
                                label { class: "block text-sm {theme::text::SECONDARY} mb-1", "Name *" }
                                input {
                                    class: if edit_field_errors().contains_key("name") {
                                        "w-full rounded-lg border px-3 py-2 text-sm {theme::text::PRIMARY} cf-policy-modal-field-error focus:outline-none"
                                    } else {
                                        "w-full rounded-lg border px-3 py-2 text-sm {theme::interactive::INPUT} {theme::text::PRIMARY} focus:outline-none"
                                    },
                                    value: edit_name(),
                                    oninput: move |evt| {
                                        edit_name.set(evt.value());
                                        let mut errors = edit_field_errors();
                                        errors.remove("name");
                                        edit_field_errors.set(errors);
                                    },
                                }
                                if let Some(err) = edit_field_errors().get("name") {
                                    p { class: "text-[11px] text-red-300 mt-1", "{err}" }
                                }
                            }

                            div {
                                label { class: "block text-sm {theme::text::SECONDARY} mb-1", "Type" }
                                select {
                                    class: "w-full px-3 py-2 rounded-lg text-sm {theme::interactive::INPUT} {theme::text::PRIMARY}",
                                    value: edit_type(),
                                    onchange: move |evt| edit_type.set(evt.value()),
                                    option { class: "text-slate-900 bg-white", value: "Nix", "Nix" }
                                    option { class: "text-slate-900 bg-white", value: "Http", "Http" }
                                    option { class: "text-slate-900 bg-white", value: "S3", "S3" }
                                    option { class: "text-slate-900 bg-white", value: "Attic", "Attic" }
                                }
                            }

                            // Type-specific required fields
                            if edit_type() == "Attic" {
                                div {
                                    div {
                                        class: "flex items-baseline justify-between gap-2",
                                        label { class: "block text-sm {theme::text::SECONDARY} mb-1", "Cache Name (on Attic server) *" }
                                        if !edit_field_errors().contains_key("attic_cache_name") {
                                            span { class: "text-[11px] {theme::text::MUTED}", "Name of cache configured in your Attic server" }
                                        }
                                    }
                                    input {
                                        class: if edit_field_errors().contains_key("attic_cache_name") {
                                            "w-full rounded-lg border px-3 py-2 text-sm {theme::text::PRIMARY} cf-policy-modal-field-error focus:outline-none"
                                        } else {
                                            "w-full rounded-lg border px-3 py-2 text-sm {theme::interactive::INPUT} {theme::text::PRIMARY} focus:outline-none"
                                        },
                                        placeholder: "my-binary-cache",
                                        value: edit_attic_cache_name(),
                                        oninput: move |evt| {
                                            edit_attic_cache_name.set(evt.value());
                                            let mut errors = edit_field_errors();
                                            errors.remove("attic_cache_name");
                                            edit_field_errors.set(errors);
                                        },
                                    }
                                    if let Some(err) = edit_field_errors().get("attic_cache_name") {
                                        p { class: "text-[11px] text-red-300 mt-1", "{err}" }
                                    }
                                }
                                div {
                                    div {
                                        class: "flex items-baseline justify-between gap-2",
                                        label { class: "block text-sm {theme::text::SECONDARY} mb-1", "Attic Server URL *" }
                                        if !edit_field_errors().contains_key("push_to") {
                                            span { class: "text-[11px] {theme::text::MUTED}", "Base URL for your Attic instance" }
                                        }
                                    }
                                    input {
                                        class: if edit_field_errors().contains_key("push_to") {
                                            "w-full rounded-lg border px-3 py-2 text-sm {theme::text::PRIMARY} cf-policy-modal-field-error focus:outline-none"
                                        } else {
                                            "w-full rounded-lg border px-3 py-2 text-sm {theme::interactive::INPUT} {theme::text::PRIMARY} focus:outline-none"
                                        },
                                        placeholder: "https://attic.example.com",
                                        value: edit_push_to(),
                                        oninput: move |evt| {
                                            edit_push_to.set(evt.value());
                                            let mut errors = edit_field_errors();
                                            errors.remove("push_to");
                                            edit_field_errors.set(errors);
                                        },
                                    }
                                    if let Some(err) = edit_field_errors().get("push_to") {
                                        p { class: "text-[11px] text-red-300 mt-1", "{err}" }
                                    }
                                }
                                div {
                                    div {
                                        class: "flex items-baseline justify-between gap-2",
                                        label { class: "block text-sm {theme::text::SECONDARY} mb-1", "Attic Public Key *" }
                                        if !edit_field_errors().contains_key("attic_public_key") {
                                            span { class: "text-[11px] {theme::text::MUTED}", "Used by agents as trusted-public-key" }
                                        }
                                    }
                                    input {
                                        class: if edit_field_errors().contains_key("attic_public_key") {
                                            "w-full rounded-lg border px-3 py-2 text-sm {theme::text::PRIMARY} cf-policy-modal-field-error focus:outline-none"
                                        } else {
                                            "w-full rounded-lg border px-3 py-2 text-sm {theme::interactive::INPUT} {theme::text::PRIMARY} focus:outline-none"
                                        },
                                        placeholder: "cache.example.org-1:AbCdEf...",
                                        value: edit_attic_public_key(),
                                        oninput: move |evt| {
                                            edit_attic_public_key.set(evt.value());
                                            let mut errors = edit_field_errors();
                                            errors.remove("attic_public_key");
                                            edit_field_errors.set(errors);
                                        },
                                    }
                                    if let Some(err) = edit_field_errors().get("attic_public_key") {
                                        p { class: "text-[11px] text-red-300 mt-1", "{err}" }
                                    }
                                }
                                div {
                                    div {
                                        class: "flex items-baseline justify-between gap-2",
                                        label { class: "block text-sm {theme::text::SECONDARY} mb-1", "Attic Token *" }
                                        span { class: "text-[11px] {theme::text::MUTED}", "Leave blank to keep the existing token" }
                                    }
                                    input {
                                        r#type: "password",
                                        class: if edit_field_errors().contains_key("attic_token") {
                                            "w-full rounded-lg border px-3 py-2 text-sm {theme::text::PRIMARY} cf-policy-modal-field-error focus:outline-none"
                                        } else {
                                            "w-full rounded-lg border px-3 py-2 text-sm {theme::interactive::INPUT} {theme::text::PRIMARY} focus:outline-none"
                                        },
                                        placeholder: "••••••••",
                                        value: edit_attic_token(),
                                        oninput: move |evt| {
                                            edit_attic_token.set(evt.value());
                                            let mut errors = edit_field_errors();
                                            errors.remove("attic_token");
                                            edit_field_errors.set(errors);
                                        },
                                    }
                                    if let Some(err) = edit_field_errors().get("attic_token") {
                                        p { class: "text-[11px] text-red-300 mt-1", "{err}" }
                                    }
                                }
                            } else {
                                div {
                                    div {
                                        class: "flex items-baseline justify-between gap-2",
                                        label { class: "block text-sm {theme::text::SECONDARY} mb-1", "Destination URL *" }
                                        if !edit_field_errors().contains_key("push_to") {
                                            span { class: "text-[11px] {theme::text::MUTED}", "Full URL to the cache destination" }
                                        }
                                    }
                                    input {
                                        class: if edit_field_errors().contains_key("push_to") {
                                            "w-full rounded-lg border px-3 py-2 text-sm {theme::text::PRIMARY} cf-policy-modal-field-error focus:outline-none"
                                        } else {
                                            "w-full rounded-lg border px-3 py-2 text-sm {theme::interactive::INPUT} {theme::text::PRIMARY} focus:outline-none"
                                        },
                                        placeholder: "https://cache.example.com or s3://bucket",
                                        value: edit_push_to(),
                                        oninput: move |evt| {
                                            edit_push_to.set(evt.value());
                                            let mut errors = edit_field_errors();
                                            errors.remove("push_to");
                                            edit_field_errors.set(errors);
                                        },
                                    }
                                    if let Some(err) = edit_field_errors().get("push_to") {
                                        p { class: "text-[11px] text-red-300 mt-1", "{err}" }
                                    }
                                }
                            }

                            // S3-specific fields
                            if edit_type() == "S3" {
                                div {
                                    class: "grid grid-cols-2 gap-4",
                                    div {
                                        label { class: "block text-sm {theme::text::SECONDARY} mb-1", "S3 Region *" }
                                        input {
                                            class: if edit_field_errors().contains_key("s3_region") {
                                                "w-full px-3 py-2 rounded-lg text-sm {theme::text::PRIMARY} cf-policy-modal-field-error"
                                            } else {
                                                "w-full px-3 py-2 rounded-lg text-sm {theme::interactive::INPUT} {theme::text::PRIMARY}"
                                            },
                                            placeholder: "us-east-1",
                                            value: edit_s3_region(),
                                            oninput: move |evt| {
                                                edit_s3_region.set(evt.value());
                                                let mut errors = edit_field_errors();
                                                errors.remove("s3_region");
                                                edit_field_errors.set(errors);
                                            },
                                        }
                                        if let Some(err) = edit_field_errors().get("s3_region") {
                                            p { class: "text-[11px] text-red-300 mt-1", "{err}" }
                                        }
                                    }
                                    div {
                                        label { class: "block text-sm {theme::text::SECONDARY} mb-1", "S3 Profile (optional)" }
                                        input {
                                            class: "w-full px-3 py-2 rounded-lg text-sm {theme::interactive::INPUT} {theme::text::PRIMARY}",
                                            placeholder: "default",
                                            value: edit_s3_profile(),
                                            oninput: move |evt| edit_s3_profile.set(evt.value()),
                                        }
                                    }
                                }
                                div {
                                    class: "grid grid-cols-2 gap-4",
                                    div {
                                        label { class: "block text-sm {theme::text::SECONDARY} mb-1", "AWS Access Key ID *" }
                                        input {
                                            class: if edit_field_errors().contains_key("s3_access_key_id") {
                                                "w-full px-3 py-2 rounded-lg text-sm {theme::text::PRIMARY} cf-policy-modal-field-error"
                                            } else {
                                                "w-full px-3 py-2 rounded-lg text-sm {theme::interactive::INPUT} {theme::text::PRIMARY}"
                                            },
                                            placeholder: "AKIA...",
                                            value: edit_s3_access_key_id(),
                                            oninput: move |evt| {
                                                edit_s3_access_key_id.set(evt.value());
                                                let mut errors = edit_field_errors();
                                                errors.remove("s3_access_key_id");
                                                edit_field_errors.set(errors);
                                            },
                                        }
                                        if let Some(err) = edit_field_errors().get("s3_access_key_id") {
                                            p { class: "text-[11px] text-red-300 mt-1", "{err}" }
                                        }
                                    }
                                    div {
                                        div {
                                            class: "flex items-baseline justify-between gap-2",
                                            label { class: "block text-sm {theme::text::SECONDARY} mb-1", "AWS Secret Access Key *" }
                                            span { class: "text-[11px] {theme::text::MUTED}", "Leave blank to keep the existing secret" }
                                        }
                                        input {
                                            r#type: "password",
                                            class: if edit_field_errors().contains_key("s3_secret_access_key") {
                                                "w-full px-3 py-2 rounded-lg text-sm {theme::text::PRIMARY} cf-policy-modal-field-error"
                                            } else {
                                                "w-full px-3 py-2 rounded-lg text-sm {theme::interactive::INPUT} {theme::text::PRIMARY}"
                                            },
                                            placeholder: "••••••••",
                                            value: edit_s3_secret_access_key(),
                                            oninput: move |evt| {
                                                edit_s3_secret_access_key.set(evt.value());
                                                let mut errors = edit_field_errors();
                                                errors.remove("s3_secret_access_key");
                                                edit_field_errors.set(errors);
                                            },
                                        }
                                        if let Some(err) = edit_field_errors().get("s3_secret_access_key") {
                                            p { class: "text-[11px] text-red-300 mt-1", "{err}" }
                                        }
                                    }
                                }
                                div {
                                    class: "grid grid-cols-2 gap-4",
                                    div {
                                        label { class: "block text-sm {theme::text::SECONDARY} mb-1", "AWS Session Token (optional)" }
                                        input {
                                            r#type: "password",
                                            class: "w-full px-3 py-2 rounded-lg text-sm {theme::interactive::INPUT} {theme::text::PRIMARY}",
                                            placeholder: "session token",
                                            value: edit_s3_session_token(),
                                            oninput: move |evt| edit_s3_session_token.set(evt.value()),
                                        }
                                    }
                                    div {
                                        label { class: "block text-sm {theme::text::SECONDARY} mb-1", "S3 Endpoint URL *" }
                                        input {
                                            class: if edit_field_errors().contains_key("s3_endpoint_url") {
                                                "w-full px-3 py-2 rounded-lg text-sm {theme::text::PRIMARY} cf-policy-modal-field-error"
                                            } else {
                                                "w-full px-3 py-2 rounded-lg text-sm {theme::interactive::INPUT} {theme::text::PRIMARY}"
                                            },
                                            placeholder: "https://s3.us-east-1.amazonaws.com",
                                            value: edit_s3_endpoint_url(),
                                            oninput: move |evt| {
                                                edit_s3_endpoint_url.set(evt.value());
                                                let mut errors = edit_field_errors();
                                                errors.remove("s3_endpoint_url");
                                                edit_field_errors.set(errors);
                                            },
                                        }
                                        if let Some(err) = edit_field_errors().get("s3_endpoint_url") {
                                            p { class: "text-[11px] text-red-300 mt-1", "{err}" }
                                        }
                                    }
                                }
                            }

                            // Signing key (only for Nix binary cache types, not Attic)
                            if edit_type() != "Attic" {
                                div {
                                    div {
                                        class: "flex items-baseline justify-between gap-2",
                                        label { class: "block text-sm {theme::text::SECONDARY} mb-1", "Signing Key Path (optional)" }
                                        span { class: "text-[11px] {theme::text::MUTED}", "Path to Nix cache signing key for signature verification" }
                                    }
                                    input {
                                        class: "w-full rounded-lg border px-3 py-2 text-sm {theme::interactive::INPUT} {theme::text::PRIMARY} focus:outline-none",
                                        placeholder: "/path/to/cache-priv-key.pem",
                                        value: edit_signing_key_path(),
                                        oninput: move |evt| edit_signing_key_path.set(evt.value()),
                                    }
                                }
                            }

                            div {
                                label { class: "block text-sm {theme::text::SECONDARY} mb-1", "Compression (optional)" }
                                select {
                                    class: "w-full px-3 py-2 rounded-lg text-sm {theme::interactive::INPUT} {theme::text::PRIMARY}",
                                    value: edit_compression(),
                                    onchange: move |evt| edit_compression.set(evt.value()),
                                    option { class: "text-slate-900 bg-white", value: "", selected: edit_compression().is_empty(), "(default)" }
                                    option { class: "text-slate-900 bg-white", value: "none", selected: edit_compression() == "none", "None" }
                                    option { class: "text-slate-900 bg-white", value: "xz", selected: edit_compression() == "xz", "XZ" }
                                    option { class: "text-slate-900 bg-white", value: "zstd", selected: edit_compression() == "zstd", "Zstandard" }
                                }
                            }

                            // Environment assignment
                            div {
                                div {
                                    class: "flex items-baseline justify-between gap-2",
                                    label { class: "block text-sm {theme::text::SECONDARY} mb-1", "Environments (optional)" }
                                    span { class: "text-[11px] {theme::text::MUTED}", "Leave empty for global cache (all environments)" }
                                }
                                if let Some(Ok(envs)) = edit_environments.read().as_ref() {
                                    div {
                                        class: "flex flex-wrap gap-2 p-2 rounded-lg border {theme::interactive::INPUT}",
                                        if envs.is_empty() {
                                            p { class: "text-xs {theme::text::MUTED}", "No environments available" }
                                        } else {
                                            for env in envs {
                                                {
                                                    let env_id = env.id;
                                                    let env_name = env.name.clone();
                                                    let color = normalize_env_color(&env.color_hex);
                                                    let is_selected = edit_selected_environments().contains(&env_id);
                                                    rsx! {
                                                        button {
                                                            r#type: "button",
                                                            style: if is_selected {
                                                                format!("padding: 3px 7px; border-radius: 999px; font-size: 10px; border: 1px solid {}; background: color-mix(in oklab, {} 14%, var(--cf-card-bg)); color: {}; cursor: pointer; display: inline-flex; align-items: center; gap: 6px; font-family: inherit; font-weight: 400;", color, color, color)
                                                            } else {
                                                                "padding: 3px 7px; border-radius: 999px; font-size: 10px; border: 1px solid var(--cf-card-border); background: transparent; color: var(--cf-text-secondary); cursor: pointer; display: inline-flex; align-items: center; gap: 6px; font-family: inherit; font-weight: 400;".to_string()
                                                            },
                                                            onclick: move |_| {
                                                                let mut selected = edit_selected_environments();
                                                                if is_selected {
                                                                    selected.retain(|&id| id != env_id);
                                                                } else {
                                                                    selected.push(env_id);
                                                                }
                                                                edit_selected_environments.set(selected);
                                                            },
                                                            span { style: "width:6px; height:6px; border-radius:50%; background:{color};" }
                                                            "{env_name}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    p { class: "text-xs {theme::text::MUTED}", "Loading environments..." }
                                }
                            }

                            if let Some(err) = edit_error() {
                                p { class: "text-sm text-red-400", "{err}" }
                            }
                        }

                        // Footer
                        div {
                            class: "mt-6 flex justify-end gap-3 shrink-0 pt-4 border-t {theme::surface::DIVIDER}",
                            button {
                                class: "px-4 py-2 rounded-lg text-sm font-medium {theme::interactive::GHOST_BTN} {theme::text::SECONDARY}",
                                onclick: move |_| {
                                    show_edit_modal.set(false);
                                    edit_error.set(None);
                                    edit_field_errors.set(std::collections::HashMap::new());
                                },
                                "Cancel"
                            }
                            button {
                                class: "px-4 py-2 rounded-lg text-sm font-medium {theme::interactive::PRIMARY_BTN}",
                                disabled: edit_submitting(),
                                onclick: move |_| {
                                    let name = edit_name().trim().to_string();
                                    let cache_type = edit_type();
                                    let push_to = edit_push_to().trim().to_string();
                                    let attic_cache_name = edit_attic_cache_name().trim().to_string();
                                    let attic_public_key = edit_attic_public_key().trim().to_string();

                                    let errors = validate_cache_destination_form(&CacheFormValidationInput {
                                        name: name.clone(),
                                        cache_type: cache_type.clone(),
                                        push_to: push_to.clone(),
                                        attic_cache_name: attic_cache_name.clone(),
                                        attic_public_key: attic_public_key.clone(),
                                        attic_token: edit_attic_token(),
                                        s3_region: edit_s3_region(),
                                        s3_access_key_id: edit_s3_access_key_id(),
                                        s3_secret_access_key: edit_s3_secret_access_key(),
                                        s3_endpoint_url: edit_s3_endpoint_url(),
                                        require_attic_token: cache_type == "Attic"
                                            && destination.attic_token.is_none(),
                                        require_s3_secret_access_key: cache_type == "S3"
                                            && destination.s3_secret_access_key.is_none(),
                                    });

                                    // If there are validation errors, display them and stop
                                    if !errors.is_empty() {
                                        edit_field_errors.set(errors);
                                        edit_error.set(Some("Please fix the errors above".to_string()));
                                        return;
                                    }

                                    // Clear any previous errors
                                    edit_field_errors.set(std::collections::HashMap::new());
                                    edit_submitting.set(true);
                                    edit_error.set(None);
                                    let on_change = on_change.clone();

                                    let attic_token_val = edit_attic_token();
                                    let signing_key_path_val = edit_signing_key_path();
                                    let compression_val = edit_compression();
                                    let s3_region_val = edit_s3_region();
                                    let s3_profile_val = edit_s3_profile();
                                    let s3_access_key_id_val = edit_s3_access_key_id();
                                    let s3_secret_access_key_val = edit_s3_secret_access_key();
                                    let s3_session_token_val = edit_s3_session_token();
                                    let s3_endpoint_url_val = edit_s3_endpoint_url();

                                    spawn(async move {
                                        let req = UpdateCacheDestination {
                                            name: Some(name),
                                            cache_type: Some(cache_type.clone()),
                                            push_to: if push_to.trim().is_empty() {
                                                None
                                            } else {
                                                Some(push_to)
                                            },
                                            enabled: None,
                                            signing_key_path: if signing_key_path_val.trim().is_empty() { None } else { Some(signing_key_path_val.trim().to_string()) },
                                            compression: if compression_val.trim().is_empty() { None } else { Some(compression_val.trim().to_string()) },
                                            s3_region: if s3_region_val.trim().is_empty() { None } else { Some(s3_region_val.trim().to_string()) },
                                            s3_profile: if s3_profile_val.trim().is_empty() { None } else { Some(s3_profile_val.trim().to_string()) },
                                            s3_access_key_id: if s3_access_key_id_val.trim().is_empty() { None } else { Some(s3_access_key_id_val.trim().to_string()) },
                                            s3_secret_access_key: if s3_secret_access_key_val.trim().is_empty() { None } else { Some(s3_secret_access_key_val.trim().to_string()) },
                                            s3_session_token: if s3_session_token_val.trim().is_empty() { None } else { Some(s3_session_token_val.trim().to_string()) },
                                            s3_endpoint_url: if s3_endpoint_url_val.trim().is_empty() { None } else { Some(s3_endpoint_url_val.trim().to_string()) },
                                            attic_token: if attic_token_val.trim().is_empty() { None } else { Some(attic_token_val.trim().to_string()) },
                                            attic_cache_name: if cache_type == "Attic" {
                                                Some(attic_cache_name)
                                            } else {
                                                None
                                            },
                                            attic_public_key: if cache_type == "Attic" {
                                                if attic_public_key.trim().is_empty() { None } else { Some(attic_public_key.trim().to_string()) }
                                            } else {
                                                None
                                            },
                                            attic_ignore_upstream_cache_filter: None,
                                            attic_jobs: None,
                                            parallel_uploads: None,
                                            max_retries: None,
                                            retry_delay_seconds: None,
                                            push_timeout_seconds: None,
                                            force_repush: None,
                                            require_sigs: None,
                                            environment_ids: if edit_selected_environments().is_empty() {
                                                None
                                            } else {
                                                Some(edit_selected_environments())
                                            },
                                            ..Default::default()
                                        };

                                        match client::update_cache_destination(destination.id, &req).await {
                                            Ok(_) => {
                                                show_edit_modal.set(false);
                                                on_change.call(());
                                            }
                                            Err(e) => {
                                                edit_error.set(Some(format!("Failed to update destination: {e}")));
                                            }
                                        }
                                        edit_submitting.set(false);
                                    });
                                },
                                if edit_submitting() { "Saving..." } else { "Save Changes" }
                            }
                        }
                    }
                }
            }

            // Delete confirmation modal
            if show_delete_confirm() {
                div {
                    class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4 cf-modal-overlay",
                    tabindex: "0",
                    onclick: move |_| show_delete_confirm.set(false),
                    onkeydown: move |evt| {
                        if evt.key() == Key::Escape {
                            show_delete_confirm.set(false);
                        }
                    },
                    div {
                        class: "relative {theme::surface::CARD_BG} border {theme::surface::CARD_BORDER} rounded-xl shadow-2xl p-6 cf-modal-panel-30",
                        role: "dialog",
                        aria_modal: "true",
                        aria_labelledby: "{delete_modal_title_id}",
                        onclick: move |e| e.stop_propagation(),

                        h3 {
                            id: "{delete_modal_title_id}",
                            class: "{theme::typography::SECTION_TITLE} {theme::text::PRIMARY} mb-4",
                            "Delete Cache Destination?"
                        }
                        p { class: "{theme::text::SECONDARY} mb-6", "Are you sure you want to delete \"{destination.name}\"? This action cannot be undone." }

                        div {
                            class: "flex gap-3 justify-end",
                            button {
                                class: "px-4 py-2 rounded-lg text-sm font-medium {theme::interactive::GHOST_BTN} {theme::text::SECONDARY}",
                                onclick: move |_| show_delete_confirm.set(false),
                                "Cancel"
                            }
                            button {
                                class: "px-4 py-2 rounded-lg text-sm font-medium {theme::interactive::DANGER_BTN}",
                                onclick: move |_| {
                                    let dest_id = destination.id;
                                    let on_change = on_change.clone();
                                    spawn(async move {
                                        if client::delete_cache_destination(dest_id).await.is_ok() {
                                            on_change.call(());
                                        }
                                    });
                                    show_delete_confirm.set(false);
                                },
                                "Delete"
                            }
                        }
                    }
                }
            }
        }
    }
}

fn normalize_env_color(color_hex: &str) -> &str {
    let trimmed = color_hex.trim();
    if trimmed.is_empty() {
        "#6b7280"
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CacheFormValidationInput, CacheTypeDrafts, Niks3FormState, cache_form_kind,
        validate_cache_destination_form,
    };

    #[test]
    fn legacy_editor_retains_secrets_disabled_status_and_exact_wire_type() {
        for cache_type in ["S3", "Attic", "Nix", "Http"] {
            let destination = serde_json::from_value(serde_json::json!({
                "id": 1, "name": " retained ", "cache_type": cache_type, "enabled": false,
                "created_at": "2026-10-02T00:00:00Z", "updated_at": "2026-10-02T00:00:00Z",
                "push_to": if cache_type == "S3" { "s3://fixture" } else { "https://fixture.example" },
                "s3_region": "us-east-1", "s3_profile": "builder-profile", "s3_access_key_id": "public-access-id",
                "s3_endpoint_url": "https://s3.example", "s3_secret_access_key": "must-not-prefill",
                "s3_session_token": "must-not-prefill", "attic_token": "must-not-prefill",
                "attic_cache_name": "fixture", "attic_public_key": "fixture:AAAA", "compression": "zstd",
                "signing_key_path": "/fixture/key", "attic_token_configured": cache_type == "Attic",
                "s3_credentials_configured": cache_type == "S3"
            })).unwrap();
            let common = Niks3FormState::from_destination(Some(&destination));
            let drafts = CacheTypeDrafts::from_destination(Some(&destination));
            let kind = cache_form_kind(cache_type);
            assert!(
                drafts
                    .validate(kind, &common, true, Some(&destination))
                    .is_ok()
            );
            let update = drafts.update_request(kind, &common, false, Some(&destination));
            assert!(
                update.cache_type.is_none(),
                "unrelated {cache_type} edits retain the wire type"
            );
            assert!(update.enabled.is_none());
            assert!(update.name.is_none());
            assert!(update.s3_access_key_id.is_none());
            assert!(update.s3_secret_access_key.is_none());
            assert!(update.s3_session_token.is_none());
            assert!(update.attic_token.is_none());
            assert!(update.compression.is_none());
            assert!(update.push_to.is_none());
            assert!(update.attic_public_key.is_none());
        }
    }

    #[test]
    fn legacy_conversion_requires_target_credentials_and_never_borrows_source_secrets() {
        let destination = serde_json::from_value(serde_json::json!({
            "id": 1, "name": "conversion", "cache_type": "S3", "enabled": true,
            "created_at": "2026-10-02T00:00:00Z", "updated_at": "2026-10-02T00:00:00Z",
            "push_to": "s3://fixture", "s3_secret_access_key": "must-not-borrow", "attic_token": "must-not-borrow"
        })).unwrap();
        let common = Niks3FormState::from_destination(Some(&destination));
        let mut drafts = CacheTypeDrafts::from_destination(Some(&destination));
        assert!(
            drafts
                .validate("attic", &common, true, Some(&destination))
                .is_err()
        );
        drafts.0.insert("attic.token".into(), "replacement".into());
        let update = drafts.update_request("attic", &common, true, Some(&destination));
        assert_eq!(update.cache_type.as_deref(), Some("Attic"));
        assert_eq!(update.attic_token.as_deref(), Some("replacement"));
        assert!(update.s3_secret_access_key.is_none());
        assert!(!update.clear_niks3_auth_token);
    }

    #[test]
    fn s3_replacement_is_atomic_and_explicitly_clears_an_omitted_session() {
        let destination = serde_json::from_value(serde_json::json!({
            "id": 1, "name": "fixture", "cache_type": "S3", "enabled": false,
            "created_at": "2026-10-02T00:00:00Z", "updated_at": "2026-10-02T00:00:00Z",
            "push_to": "s3://fixture", "s3_credentials_configured": true,
            "s3_session_token_configured": true
        }))
        .unwrap();
        let common = Niks3FormState::from_destination(Some(&destination));
        let mut drafts = CacheTypeDrafts::from_destination(Some(&destination));
        assert!(
            drafts
                .validate("s3", &common, true, Some(&destination))
                .is_ok()
        );
        let retained = drafts.update_request("s3", &common, false, Some(&destination));
        assert!(retained.s3_session_token.is_none());
        drafts
            .0
            .insert("s3.access".into(), "fixture-replacement-access".into());
        assert!(
            drafts
                .validate("s3", &common, true, Some(&destination))
                .is_err(),
            "never borrow a retained secret for a new access ID"
        );
        drafts
            .0
            .insert("s3.secret".into(), "fixture-replacement-secret".into());
        assert!(
            drafts
                .validate("s3", &common, true, Some(&destination))
                .is_ok()
        );
        let replacement = drafts.update_request("s3", &common, false, Some(&destination));
        assert_eq!(replacement.s3_session_token.as_deref(), Some(""));
        drafts
            .0
            .insert("s3.session".into(), "fixture-new-session".into());
        let replacement = drafts.update_request("s3", &common, false, Some(&destination));
        assert_eq!(
            replacement.s3_session_token.as_deref(),
            Some("fixture-new-session")
        );
    }

    #[test]
    fn configured_flags_cannot_cross_types_or_complete_an_empty_replacement_draft() {
        let destination = serde_json::from_value(serde_json::json!({
            "id": 1, "name": "fixture", "cache_type": "Attic", "enabled": true,
            "created_at": "2026-10-02T00:00:00Z", "updated_at": "2026-10-02T00:00:00Z",
            "attic_token_configured": true, "s3_credentials_configured": true
        }))
        .unwrap();
        let common = Niks3FormState::from_destination(Some(&destination));
        let mut drafts = CacheTypeDrafts::from_destination(Some(&destination));
        assert!(
            drafts
                .validate("attic", &common, true, Some(&destination))
                .is_ok()
        );
        assert!(
            drafts
                .validate("s3", &common, true, Some(&destination))
                .is_err()
        );
        drafts
            .0
            .insert("attic.credential".into(), "local-empty-replacement".into());
        assert!(
            drafts
                .validate("attic", &common, true, Some(&destination))
                .is_err()
        );
    }

    #[test]
    fn basic_and_query_unrelated_edits_omit_sanitized_uri_but_new_authority_is_explicit() {
        for flag in [
            "http_basic_auth_configured",
            "legacy_query_credentials_configured",
        ] {
            let mut json = serde_json::json!({
                "id": 1, "name": "fixture", "cache_type": "Http", "enabled": false,
                "created_at": "2026-10-02T00:00:00Z", "updated_at": "2026-10-02T00:00:00Z",
                "push_to": "https://fixture.example/nix-cache-info"
            });
            json[flag] = true.into();
            let destination = serde_json::from_value(json).unwrap();
            let mut common = Niks3FormState::from_destination(Some(&destination));
            let drafts = CacheTypeDrafts::from_destination(Some(&destination));
            common.name = "new fixture name".into();
            let patch = super::cache_update_patch(
                "nix",
                &common,
                &drafts,
                false,
                Some(&destination),
                vec![],
            );
            assert!(patch.push_to.is_none());
            assert!(patch.cache_type.is_none());
            assert!(patch.enabled.is_none());
            common.read_url = "https://new-authority.example/nix-cache-info".into();
            let patch = super::cache_update_patch(
                "nix",
                &common,
                &drafts,
                false,
                Some(&destination),
                vec![],
            );
            assert_eq!(
                patch.push_to.as_deref(),
                Some("https://new-authority.example/nix-cache-info")
            );
            assert!(patch.attic_token.is_none());
            assert!(patch.s3_secret_access_key.is_none());
        }
    }

    #[test]
    fn niks3_edit_never_prefills_credentials_from_destination() {
        let destination = serde_json::from_value(serde_json::json!({
            "id": 1, "name": "private", "cache_type": "Niks3", "enabled": true,
            "created_at": "2026-10-02T00:00:00Z", "updated_at": "2026-10-02T00:00:00Z",
            "niks3_write_auth_mode": "mtls", "niks3_read_auth_mode": "mtls",
            "niks3_auth_token": "must-not-prefill", "niks3_write_client_key": "must-not-prefill",
            "niks3_read_client_key": "must-not-prefill", "niks3_write_mtls_configured": true,
            "niks3_read_mtls_configured": true
        }))
        .unwrap();
        let state = Niks3FormState::from_destination(Some(&destination));
        let request = state.update_request();
        assert!(request.niks3_auth_token.is_none());
        assert!(request.niks3_write_client_key.is_none());
        assert!(request.niks3_read_client_key.is_none());
        assert!(!request.clear_niks3_write_client_key);
        assert!(!request.clear_niks3_read_client_key);
        assert!(!request.clear_niks3_write_ca_cert);
        assert!(!request.clear_niks3_read_ca_cert);
    }

    #[test]
    fn niks3_mode_transition_excludes_inactive_credentials() {
        let mut state = Niks3FormState::from_destination(None);
        state.token = "replacement-token".into();
        state.write_cert = "inactive-cert".into();
        state.write_key = "inactive-key".into();
        state.read_key = "inactive-read-key".into();
        let request = state.update_request();
        assert_eq!(
            request.niks3_auth_token.as_deref(),
            Some("replacement-token")
        );
        assert!(request.niks3_write_client_cert.is_none());
        assert!(request.niks3_write_client_key.is_none());
        assert!(request.niks3_read_client_key.is_none());
        assert!(request.clear_niks3_write_client_key);
        assert!(request.clear_niks3_read_client_key);
        assert!(!request.clear_niks3_auth_token);
        state.write_mode = "mtls".into();
        let request = state.update_request();
        assert!(request.niks3_auth_token.is_none());
        assert!(request.clear_niks3_auth_token);
        assert!(!request.clear_niks3_write_client_key);
    }

    #[test]
    fn niks3_ca_removal_never_sends_a_conflicting_replacement() {
        let mut state = Niks3FormState::from_destination(None);
        state.write_mode = "mtls".into();
        state.read_mode = "mtls".into();
        state.write_ca = "new-write-ca".into();
        state.read_ca = "new-read-ca".into();
        state.clear_write_ca = true;
        state.clear_read_ca = true;
        let request = state.update_request();
        assert!(request.clear_niks3_write_ca_cert);
        assert!(request.clear_niks3_read_ca_cert);
        assert!(request.niks3_write_ca_cert.is_none());
        assert!(request.niks3_read_ca_cert.is_none());
    }

    #[test]
    fn niks3_success_does_not_imply_write_authorization() {
        let result: crate::api::models::CacheCredentialTestResult = serde_json::from_value(
            serde_json::json!({ "success": true, "message": "Read checks passed", "write_auth_valid": null })
        ).unwrap();
        assert!(result.ok);
        assert_eq!(result.write_auth_valid, None);
    }

    fn base_input(cache_type: &str, push_to: &str) -> CacheFormValidationInput {
        CacheFormValidationInput {
            name: "main-cache".to_string(),
            cache_type: cache_type.to_string(),
            push_to: push_to.to_string(),
            attic_cache_name: "binary-cache".to_string(),
            attic_public_key: "cache.example.org-1:AbCdEf0123+/=".to_string(),
            attic_token: "fixture-attic-token".to_string(),
            s3_region: "us-east-1".to_string(),
            s3_access_key_id: "fixture-access-id".to_string(),
            s3_secret_access_key: "fixture-secret-value".to_string(),
            s3_endpoint_url: "https://s3.us-east-1.amazonaws.com".to_string(),
            require_attic_token: cache_type == "Attic",
            require_s3_secret_access_key: cache_type == "S3",
        }
    }

    #[test]
    fn rejects_invalid_http_destination_for_nix_cache() {
        let errors = validate_cache_destination_form(&base_input("Nix", "cache.example.com"));
        assert_eq!(
            errors.get("push_to").map(String::as_str),
            Some("Destination URL must start with http:// or https://")
        );
    }

    #[test]
    fn rejects_invalid_s3_destination() {
        let errors = validate_cache_destination_form(&base_input("S3", "https://bucket"));
        assert_eq!(
            errors.get("push_to").map(String::as_str),
            Some("S3 destination must look like s3://bucket or s3://bucket/prefix")
        );
    }

    #[test]
    fn rejects_invalid_attic_public_key() {
        let mut input = base_input("Attic", "https://attic.example.com");
        input.attic_public_key = "not-a-valid-key".to_string();
        let errors = validate_cache_destination_form(&input);
        assert_eq!(
            errors.get("attic_public_key").map(String::as_str),
            Some("Attic public key must look like cache-name:BASE64KEY")
        );
    }

    #[test]
    fn accepts_valid_attic_input() {
        let errors =
            validate_cache_destination_form(&base_input("Attic", "https://attic.example.com"));
        assert!(errors.is_empty());
    }

    #[test]
    fn accepts_valid_s3_input() {
        let errors =
            validate_cache_destination_form(&base_input("S3", "s3://my-cache-bucket/releases"));
        assert!(errors.is_empty());
    }

    #[test]
    fn allows_blank_attic_token_on_edit_when_existing_secret_is_preserved() {
        let mut input = base_input("Attic", "https://attic.example.com");
        input.attic_token.clear();
        input.require_attic_token = false;
        let errors = validate_cache_destination_form(&input);
        assert!(!errors.contains_key("attic_token"));
    }

    #[test]
    fn allows_blank_s3_secret_on_edit_when_existing_secret_is_preserved() {
        let mut input = base_input("S3", "s3://my-cache-bucket/releases");
        input.s3_secret_access_key.clear();
        input.require_s3_secret_access_key = false;
        let errors = validate_cache_destination_form(&input);
        assert!(!errors.contains_key("s3_secret_access_key"));
    }
}

/// List of cache push jobs with filtering
#[component]
fn CachePushJobsList() -> Element {
    let mut status_filter = use_signal(|| None::<String>);

    let jobs = use_resource(move || {
        let filter = status_filter();
        async move { client::fetch_cache_push_jobs(filter.as_deref(), 100, 0).await }
    });

    rsx! {
        div {
            class: "space-y-4",

            // Header with filter
            div {
                class: "flex justify-between items-center",
                h2 {
                    class: "{theme::typography::SECTION_TITLE} {theme::text::PRIMARY}",
                    "Cache Push Jobs"
                }

                select {
                    class: "px-3 py-2 rounded-lg text-sm {theme::interactive::INPUT} {theme::text::PRIMARY}",
                    onchange: move |evt| {
                        let value = evt.value();
                        status_filter.set(if value.is_empty() { None } else { Some(value) });
                    },
                    option { class: "text-slate-900 bg-white", value: "", selected: status_filter().is_none(), "All Statuses" }
                    option { class: "text-slate-900 bg-white", value: "pending", selected: status_filter() == Some("pending".to_string()), "Pending" }
                    option { class: "text-slate-900 bg-white", value: "in_progress", selected: status_filter() == Some("in_progress".to_string()), "In Progress" }
                    option { class: "text-slate-900 bg-white", value: "failed", selected: status_filter() == Some("failed".to_string()), "Failed" }
                    option { class: "text-slate-900 bg-white", value: "completed", selected: status_filter() == Some("completed".to_string()), "Completed" }
                    option { class: "text-slate-900 bg-white", value: "cancelled", selected: status_filter() == Some("cancelled".to_string()), "Cancelled" }
                    option { class: "text-slate-900 bg-white", value: "permanently_failed", selected: status_filter() == Some("permanently_failed".to_string()), "Permanently Failed" }
                }
            }

            // Job list
            match &*jobs.read_unchecked() {
                Some(Ok(job_list)) => rsx! {
                    if job_list.is_empty() {
                        div {
                            class: "{theme::presets::CARD} text-center py-12",
                            p { class: "{theme::text::SECONDARY}", "No cache push jobs found." }
                        }
                    } else {
                        div {
                            class: "{theme::presets::TABLE_CONTAINER}",
                            table {
                                class: "w-full",
                                thead {
                                    class: "{theme::surface::SUBTLE_BG}",
                                    tr {
                                        th { class: "{theme::spacing::TABLE_CELL} text-left {theme::typography::TABLE_HEADER}", "ID" }
                                        th { class: "{theme::spacing::TABLE_CELL} text-left {theme::typography::TABLE_HEADER}", "Status" }
                                        th { class: "{theme::spacing::TABLE_CELL} text-left {theme::typography::TABLE_HEADER}", "Destination" }
                                        th { class: "{theme::spacing::TABLE_CELL} text-left {theme::typography::TABLE_HEADER}", "Attempts" }
                                        th { class: "{theme::spacing::TABLE_CELL} text-left {theme::typography::TABLE_HEADER}", "Scheduled" }
                                        th { class: "{theme::spacing::TABLE_CELL} text-left {theme::typography::TABLE_HEADER}", "Actions" }
                                    }
                                }
                                tbody {
                                    for job in job_list {
                                        CachePushJobRow { job: job.clone() }
                                    }
                                }
                            }
                        }
                    }
                },
                Some(Err(e)) => rsx! {
                    div {
                        class: "{theme::presets::CARD} border-red-500/30 bg-red-500/5",
                        p { class: "text-red-400", "Error loading push jobs: {e}" }
                    }
                },
                None => rsx! {
                    div {
                        class: "{theme::presets::CARD} text-center py-12",
                        p { class: "{theme::text::SECONDARY}", "Loading jobs..." }
                    }
                },
            }
        }
    }
}

/// Individual job row in the table
#[component]
fn CachePushJobRow(job: CachePushJob) -> Element {
    let (status_text, status_badge_class) = match job.status.as_str() {
        "completed" => (
            "Completed",
            format!(
                "{} bg-emerald-500/10 text-emerald-400 border-emerald-500/30",
                theme::presets::BADGE
            ),
        ),
        "failed" | "permanently_failed" => (
            "Failed",
            format!(
                "{} bg-red-500/10 text-red-400 border-red-500/30",
                theme::presets::BADGE
            ),
        ),
        "in_progress" => (
            "In Progress",
            format!(
                "{} bg-blue-500/10 text-blue-400 border-blue-500/30",
                theme::presets::BADGE
            ),
        ),
        "pending" => (
            "Pending",
            format!(
                "{} bg-yellow-500/10 text-yellow-400 border-yellow-500/30",
                theme::presets::BADGE
            ),
        ),
        "cancelled" => (
            "Cancelled",
            format!(
                "{} bg-gray-500/10 text-gray-400 border-gray-500/30",
                theme::presets::BADGE
            ),
        ),
        _ => (
            &*job.status,
            format!(
                "{} bg-gray-500/10 text-gray-400 border-gray-500/30",
                theme::presets::BADGE
            ),
        ),
    };

    let scheduled_str = job.scheduled_at.format("%Y-%m-%d %H:%M").to_string();

    rsx! {
        tr {
            class: "border-t {theme::surface::DIVIDER} hover:{theme::surface::SUBTLE_BG}",
            td { class: "{theme::spacing::TABLE_CELL} text-sm {theme::text::PRIMARY}", "{job.id}" }
            td {
                class: "{theme::spacing::TABLE_CELL}",
                span { class: "{status_badge_class} border", "{status_text}" }
            }
            td {
                class: "{theme::spacing::TABLE_CELL} text-sm {theme::text::SECONDARY}",
                if let Some(ref dest) = job.cache_destination {
                    "{dest}"
                } else {
                    span { class: "{theme::text::MUTED}", "(default)" }
                }
            }
            td { class: "{theme::spacing::TABLE_CELL} text-sm {theme::text::SECONDARY}", "{job.attempts}" }
            td {
                class: "{theme::spacing::TABLE_CELL} text-sm {theme::text::MUTED}",
                "{scheduled_str}"
            }
            td {
                class: "{theme::spacing::TABLE_CELL}",
                div {
                    class: "flex gap-2",
                    if job.status == "failed" || job.status == "permanently_failed" {
                        button {
                            class: "px-2 py-1 text-xs rounded {theme::interactive::PRIMARY_BTN}",
                            onclick: move |_| {
                                let job_id = job.id;
                                spawn(async move {
                                    let _ = client::retry_cache_push_job(job_id).await;
                                    // TODO: Refresh list
                                });
                            },
                            "Retry"
                        }
                    }
                    if job.status == "pending" || job.status == "in_progress" {
                        button {
                            class: "px-2 py-1 text-xs rounded {theme::interactive::DANGER_BTN}",
                            onclick: move |_| {
                                let job_id = job.id;
                                spawn(async move {
                                    let _ = client::cancel_cache_push_job(job_id).await;
                                    // TODO: Refresh list
                                });
                            },
                            "Cancel"
                        }
                    }
                }
            }
        }
    }
}
