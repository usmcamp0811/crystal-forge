//! Cache management view - configure cache destinations and monitor push jobs.

use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::api::client::{self, ApiClientError};
use crate::api::models::{
    CacheDestination, CachePushJob, CreateCacheDestination, EnvironmentSummary, SortOrder,
    SystemSummary, SystemsListParams, UpdateCacheDestination,
};
use crate::components::icon::{Icon, IconName};
use crate::routes::Route;
use crate::theme;

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

            if from_setup() {
                div {
                    "data-testid": "setup-coach-caches-callout",
                    style: "background:rgba(30,58,138,0.22); border:1px solid rgba(96,165,250,0.55); border-radius:8px; padding:12px 16px;",
                    p { style: "color:#dbeafe; font-size:12px; font-weight:700; margin:0; letter-spacing:0.03em; text-transform:uppercase;", "Setup Tour - Step 4 of 6" }
                    p { style: "color:#dbeafe; font-size:14px; font-weight:600; margin:4px 0 0 0;", "Add a binary cache" }
                    p { style: "color:#bfdbfe; font-size:13px; margin:4px 0 0 0;", "Create a cache destination and assign environments so outputs can be distributed." }
                }
            }

            // Page header matching mockup (JSX lines 23-33)
            div {
                class: "page-head",
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

    // Unified form state for both add and edit (simplified to match mockup)
    let mut form_name = use_signal(String::new);
    let mut form_type = use_signal(|| "s3".to_string());
    let mut form_url = use_signal(String::new);
    let mut form_requires_auth = use_signal(|| true);
    let mut form_cred_id = use_signal(String::new);
    let mut form_environment_ids = use_signal(|| Vec::<Uuid>::new());
    let mut form_testing = use_signal(|| None::<String>);
    let mut form_test_error = use_signal(|| None::<String>);
    let mut form_save_error = use_signal(|| None::<String>);
    let mut form_show_cred_modal = use_signal(|| false);
    let mut local_credentials = use_signal(Vec::<LocalCredential>::new);

    // Pre-populate form when switching between add/edit
    use_effect(move || {
        if let Some(dest) = edit_destination() {
            // Edit mode - populate from existing cache
            form_name.set(dest.name.clone());
            form_type.set(dest.cache_type.to_lowercase());
            form_url.set(dest.push_to.clone().unwrap_or_default());
            // Infer requires auth from any credential/config indicator.
            // Secrets are redacted by API, so rely on durable config fields too.
            let has_auth = dest.s3_secret_access_key.is_some()
                || dest.s3_access_key_id.is_some()
                || dest.attic_token.is_some()
                || dest
                    .s3_profile
                    .as_ref()
                    .is_some_and(|v| !v.trim().is_empty())
                || dest
                    .attic_cache_name
                    .as_ref()
                    .is_some_and(|v| !v.trim().is_empty())
                || matches!(dest.cache_type.as_str(), "S3" | "Attic" | "s3" | "attic");
            form_requires_auth.set(has_auth);
            form_cred_id.set(String::new());
            form_testing.set(None);
            form_test_error.set(None);
            form_save_error.set(None);
            form_show_cred_modal.set(false);

            // Load environment assignments
            let cache_id = dest.id;
            form_environment_ids.set(Vec::new());
            spawn(async move {
                if let Ok(env_ids) = client::get_cache_environments(cache_id).await {
                    form_environment_ids.set(env_ids);
                }
            });
        } else if show_add_modal() {
            // Add mode - reset to defaults
            form_name.set(String::new());
            form_type.set("s3".to_string());
            form_url.set(String::new());
            form_requires_auth.set(true);
            form_cred_id.set(String::new());
            form_environment_ids.set(Vec::new());
            form_testing.set(None);
            form_test_error.set(None);
            form_save_error.set(None);
        }
    });
    let mut dismiss_add_target_callout = use_signal(|| false);

    // Fetch available environments for assignment and cache-assignment display.
    let environments = use_resource(|| async move { client::fetch_environments().await });
    let show_add_target_callout = show_onboarding_hint
        && !dismiss_add_target_callout()
        && !show_add_modal()
        && matches!(&*destinations.read_unchecked(), Some(Ok(dests)) if dests.is_empty());

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

            if show_add_target_callout {
                div {
                    "data-testid": "setup-coach-caches-target-callout",
                    style: "position:relative; background:rgba(30,64,175,0.94); border:1px solid rgba(96,165,250,0.75); border-radius:10px; padding:8px 10px; color:#dbeafe; font-size:12px; margin-bottom:16px;",
                    p { style: "margin:0; color:#eff6ff; font-weight:600;", "Next action" }
                    p { style: "margin:2px 0 0 0;", "Click Add cache (in the page header above) to create your first cache endpoint." }
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
                    on_close: move |_| view_destination.set(None),
                    on_edit: move |dest: CacheDestination| {
                        view_destination.set(None);
                        edit_destination.set(Some(dest));
                    },
                }
            }

            // Add modal - matching JSX mockup CacheFormModal (add mode)
            if show_add_modal() {
                div {
                    class: "modal-backdrop",
                    onclick: move |_| {
                        form_show_cred_modal.set(false);
                        show_add_modal.set(false)
                    },
                    div {
                        class: "modal",
                        onclick: move |e| e.stop_propagation(),
                        style: "width:min(620px,96vw); max-height:92vh;",
                        div {
                            class: "modal-head",
                            h2 {
                                svg {
                                    width: "14", height: "14", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2",
                                    style: "margin-right:6px; vertical-align:text-bottom; display:inline-block;",
                                    line { x1: "12", y1: "5", x2: "12", y2: "19" }
                                    line { x1: "5", y1: "12", x2: "19", y2: "12" }
                                }
                                "Add cache destination"
                            }
                            p { "Register a new binary cache destination." }
                        }
                        div {
                            class: "modal-body",
                            style: "overflow-y:auto;",
                            div { class: "field", label { "Name" }
                                input { class: "input focus-ring", value: form_name(), oninput: move |evt| form_name.set(evt.value()), placeholder: "e.g. crystal-forge-prod-cache" }
                            }
                            div { class: "field", label { "Type" }
                                div { class: "seg",
                                    for (val, label) in [("s3", "S3-compatible"), ("attic", "Attic"), ("nix", "Nix HTTPS")] {
                                        button {
                                            class: if form_type() == val { "active" } else { "" },
                                            onclick: move |_| {
                                                form_testing.set(None);
                                                form_type.set(val.to_string())
                                            },
                                            "{label}"
                                        }
                                    }
                                }
                            }
                            div { class: "field", label { "URL" }
                                input {
                                    class: "input focus-ring mono", style: "font-size:12px;", value: form_url(), oninput: move |evt| { form_testing.set(None); form_url.set(evt.value()) },
                                    placeholder: match form_type().as_str() { "s3" => "s3://bucket?region=us-east-1", "attic" => "attic://host/cache", _ => "https://cache.nixos.org" }
                                }
                            }
                            label {
                                style: "display:flex; gap:8px; align-items:center; font-size:13px; cursor:pointer;",
                                input { r#type: "checkbox", checked: form_requires_auth(), onchange: move |evt| { form_testing.set(None); form_requires_auth.set(evt.checked()) }, style: "accent-color:var(--cf-brand-purple);" }
                                span { "Requires authentication" }
                            }
                            if form_requires_auth() {
                                div { class: "field", label { "Credential" }
                                    div { style: "display:flex; gap:8px;",
                                        select {
                                            class: "input focus-ring", style: "flex:1;", value: form_cred_id(),
                                            onchange: move |evt| {
                                                form_testing.set(None);
                                                form_test_error.set(None);
                                                let v = evt.value();
                                                if v == "__new__" { form_show_cred_modal.set(true); } else { form_cred_id.set(v); }
                                            },
                                            option { value: "", "Select a credential…" }
                                            for cred in local_credentials().into_iter().filter(|cred| credential_matches_cache_type(cred, &form_type())) {
                                                option { value: "{cred.id}", "{credential_label(&cred)}" }
                                            }
                                            option { value: "__new__", "+ Add new credential…" }
                                        }
                                        button {
                                            class: "btn btn-ghost focus-ring xs", disabled: form_requires_auth() && form_cred_id().is_empty(),
                                            onclick: move |_| {
                                                form_testing.set(Some("running".to_string()));
                                                form_test_error.set(None);
                                                let cache_type = form_type();
                                                let url_value = form_url();
                                                if cache_type == "s3" && s3_endpoint_url_from_form(&cache_type, &url_value).is_none() {
                                                    form_testing.set(Some("fail".to_string()));
                                                    form_test_error.set(Some("S3 test requires an HTTPS endpoint URL (e.g. https://s3.us-east-1.amazonaws.com).".to_string()));
                                                    return;
                                                }
                                                let selected_credential = local_credentials()
                                                    .into_iter()
                                                    .find(|cred| cred.id == form_cred_id());
                                                let (s3_profile, s3_access_key_id, s3_secret_access_key, attic_token) =
                                                    credential_fields_for_request(selected_credential.as_ref());
                                                let req = CreateCacheDestination {
                                                    name: form_name(),
                                                    cache_type: api_cache_type(&form_type()),
                                                    push_to: if form_url().trim().is_empty() {
                                                        None
                                                    } else {
                                                        Some(form_url())
                                                    },
                                                    s3_endpoint_url: s3_endpoint_url_from_form(&cache_type, &url_value),
                                                    enabled: Some(true),
                                                    s3_profile,
                                                    s3_access_key_id,
                                                    s3_secret_access_key,
                                                    attic_token,
                                                    attic_cache_name: None,
                                                    ..Default::default()
                                                };
                                                spawn(async move {
                                                    match client::test_cache_destination_credentials(&req).await {
                                                        Ok(result) if result.ok => form_testing.set(Some("ok".to_string())),
                                                        Ok(result) => {
                                                            form_testing.set(Some("fail".to_string()));
                                                            form_test_error.set(Some(result.message));
                                                        }
                                                        Err(e) => {
                                                            form_testing.set(Some("fail".to_string()));
                                                            form_test_error.set(Some(e.to_string()));
                                                        }
                                                    }
                                                });
                                            },
                                            match form_testing().as_deref() { Some("running") => "Testing…", Some("ok") => "✓ Connected", Some("fail") => "✗ Failed", _ => "Test" }
                                        }
                                    }
                                    if local_credentials().is_empty() {
                                        div { class: "help", "Saved credentials are not available yet. Disable authentication to test public connectivity, or add a credential in this form." }
                                    }
                                    if let Some(err) = form_test_error() {
                                        div { class: "help", style: "color: var(--cf-danger);", "Test failed: {err}" }
                                    }
                                }
                            }
                            div { class: "field", label { "Assigned environments" }
                                if let Some(Ok(envs)) = environments.read().as_ref() {
                                    div { style: "display:flex; flex-wrap:wrap; gap:6px;",
                                        for env in envs {
                                            {
                                                let env_id = env.id;
                                                let env_name = env.name.clone();
                                                let is_selected = form_environment_ids().contains(&env_id);
                                                let color = normalize_env_color(&env.color_hex);
                                                rsx! {
                                                    button {
                                                        class: "focus-ring",
                                                        onclick: move |_| { let mut ids = form_environment_ids(); if is_selected { ids.retain(|&id| id != env_id); } else { ids.push(env_id); } form_environment_ids.set(ids); },
                                                        style: if is_selected { format!("padding: 3px 7px; border-radius: 999px; font-size: 10px; border: 1px solid {}; background: color-mix(in oklab, {} 14%, var(--cf-card-bg)); color: {}; cursor: pointer; display: inline-flex; align-items: center; gap: 6px; font-family: inherit; font-weight: 400;", color, color, color) } else { "padding: 3px 7px; border-radius: 999px; font-size: 10px; border: 1px solid var(--cf-card-border); background: transparent; color: var(--cf-text-secondary); cursor: pointer; display: inline-flex; align-items: center; gap: 6px; font-family: inherit; font-weight: 400;".to_string() },
                                                        span { style: "width:6px; height:6px; border-radius:50%; background:{color};" }
                                                        "{env_name}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    div { class: "help", "Crystal Forge will push builds for systems in these environments to this cache." }
                                }
                            }
                        }
                        div {
                            class: "modal-foot",
                            button {
                                class: "btn btn-ghost focus-ring",
                                onclick: move |_| {
                                    form_show_cred_modal.set(false);
                                    show_add_modal.set(false)
                                },
                                "Cancel"
                            }
                            button {
                                class: "btn btn-primary focus-ring",
                                onclick: move |_| {
                                    form_save_error.set(None);
                                    let cache_type = form_type();
                                    let url_value = form_url();
                                    let req = CreateCacheDestination {
                                        name: form_name(),
                                        cache_type: api_cache_type(&cache_type),
                                        push_to: if form_url().trim().is_empty() { None } else { Some(form_url()) },
                                        s3_endpoint_url: s3_endpoint_url_from_form(&cache_type, &url_value),
                                        enabled: Some(true),
                                        environment_ids: if form_environment_ids().is_empty() { None } else { Some(form_environment_ids()) },
                                        s3_profile: {
                                            let selected_credential = local_credentials()
                                                .into_iter()
                                                .find(|cred| cred.id == form_cred_id());
                                            let (s3_profile, _, _, _) = credential_fields_for_request(selected_credential.as_ref());
                                            s3_profile
                                        },
                                        s3_access_key_id: {
                                            let selected_credential = local_credentials()
                                                .into_iter()
                                                .find(|cred| cred.id == form_cred_id());
                                            let (_, s3_access_key_id, _, _) = credential_fields_for_request(selected_credential.as_ref());
                                            s3_access_key_id
                                        },
                                        s3_secret_access_key: {
                                            let selected_credential = local_credentials()
                                                .into_iter()
                                                .find(|cred| cred.id == form_cred_id());
                                            let (_, _, s3_secret_access_key, _) = credential_fields_for_request(selected_credential.as_ref());
                                            s3_secret_access_key
                                        },
                                        attic_token: {
                                            let selected_credential = local_credentials()
                                                .into_iter()
                                                .find(|cred| cred.id == form_cred_id());
                                            let (_, _, _, attic_token) = credential_fields_for_request(selected_credential.as_ref());
                                            attic_token
                                        },
                                        attic_cache_name: None,
                                        ..Default::default()
                                    };
                                    spawn(async move {
                                        match client::create_cache_destination(&req).await {
                                            Ok(_) => {
                                                form_show_cred_modal.set(false);
                                                show_add_modal.set(false);
                                                refresh_nonce.set(refresh_nonce() + 1);
                                            }
                                            Err(e) => {
                                                form_save_error.set(Some(format!("Failed to create destination: {e}")));
                                            }
                                        }
                                    });
                                },
                                svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", style: "display:inline-block; vertical-align:text-bottom;", polyline { points: "20 6 9 17 4 12" } }
                                " Add cache"
                            }
                            if let Some(err) = form_save_error() {
                                div { class: "help", style: "color: var(--cf-danger); margin-left:auto;", "{err}" }
                            }
                        }
                    }
                }
            }

            // Edit modal - matching JSX mockup (lines 155-284)
            if let Some(dest) = edit_destination() {
                div {
                    class: "modal-backdrop",
                    onclick: move |_| edit_destination.set(None),
                    div {
                        class: "modal",
                        onclick: move |e| e.stop_propagation(),
                        style: "width:min(620px,96vw); max-height:92vh;",

                        // Modal head
                        div {
                            class: "modal-head",
                            h2 {
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
                                    style: "margin-right:6px; vertical-align:text-bottom;",
                                    circle { cx: "12", cy: "12", r: "3" }
                                    path { d: "M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3h.1a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8v.1a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" }
                                }
                                "Edit {dest.name}"
                            }
                            p { "Update binary cache destination." }
                        }

                        // Modal body
                        div {
                            class: "modal-body",
                            style: "overflow-y:auto;",

                            // Name
                            div {
                                class: "field",
                                label { "Name" }
                                input {
                                    class: "input focus-ring",
                                    value: form_name(),
                                    oninput: move |evt| form_name.set(evt.value()),
                                    placeholder: "e.g. crystal-forge-prod-cache"
                                }
                            }

                            // Type (segmented button)
                            div {
                                class: "field",
                                label { "Type" }
                                div {
                                    class: "seg",
                                    for (val, label) in [("s3", "S3-compatible"), ("attic", "Attic"), ("nix", "Nix HTTPS")] {
                                        button {
                                            class: if form_type() == val { "active" } else { "" },
                                            onclick: move |_| {
                                                form_testing.set(None);
                                                form_type.set(val.to_string())
                                            },
                                            "{label}"
                                        }
                                    }
                                }
                            }

                            // URL
                            div {
                                class: "field",
                                label { "URL" }
                                input {
                                    class: "input focus-ring mono",
                                    style: "font-size:12px;",
                                    value: form_url(),
                                    oninput: move |evt| { form_testing.set(None); form_url.set(evt.value()) },
                                    placeholder: match form_type().as_str() {
                                        "s3" => "s3://bucket?region=us-east-1",
                                        "attic" => "attic://host/cache",
                                        _ => "https://cache.nixos.org"
                                    }
                                }
                            }

                            // Requires authentication checkbox
                            label {
                                style: "display:flex; gap:8px; align-items:center; font-size:13px; cursor:pointer;",
                                input {
                                    r#type: "checkbox",
                                    checked: form_requires_auth(),
                                    onchange: move |evt| { form_testing.set(None); form_requires_auth.set(evt.checked()) },
                                    style: "accent-color:var(--cf-brand-purple);"
                                }
                                span { "Requires authentication" }
                            }

                            // Credential dropdown (if auth required)
                            if form_requires_auth() {
                                div {
                                    class: "field",
                                    label { "Credential" }
                                    div {
                                        style: "display:flex; gap:8px;",
                                        select {
                                            class: "input focus-ring",
                                            style: "flex:1;",
                                            value: form_cred_id(),
                                            onchange: move |evt| {
                                                form_testing.set(None);
                                                form_test_error.set(None);
                                                let val = evt.value();
                                                if val == "__new__" {
                                                    form_show_cred_modal.set(true);
                                                } else {
                                                    form_cred_id.set(val);
                                                }
                                            },
                                            option { value: "", "Select a credential…" }
                                            for cred in local_credentials().into_iter().filter(|cred| credential_matches_cache_type(cred, &form_type())) {
                                                option { value: "{cred.id}", "{credential_label(&cred)}" }
                                            }
                                            option { value: "__new__", "+ Add new credential…" }
                                        }
                                        button {
                                            class: "btn btn-ghost focus-ring xs",
                                            disabled: form_requires_auth() && form_cred_id().is_empty(),
                                            onclick: move |_| {
                                                form_testing.set(Some("running".to_string()));
                                                form_test_error.set(None);
                                                let cache_type = form_type();
                                                let url_value = form_url();
                                                if cache_type == "s3" && s3_endpoint_url_from_form(&cache_type, &url_value).is_none() {
                                                    form_testing.set(Some("fail".to_string()));
                                                    form_test_error.set(Some("S3 test requires an HTTPS endpoint URL (e.g. https://s3.us-east-1.amazonaws.com).".to_string()));
                                                    return;
                                                }
                                                let selected_credential = local_credentials()
                                                    .into_iter()
                                                    .find(|cred| cred.id == form_cred_id());
                                                let (s3_profile, s3_access_key_id, s3_secret_access_key, attic_token) =
                                                    credential_fields_for_request(selected_credential.as_ref());
                                                let req = CreateCacheDestination {
                                                    name: form_name(),
                                                    cache_type: api_cache_type(&form_type()),
                                                    push_to: if form_url().trim().is_empty() {
                                                        None
                                                    } else {
                                                        Some(form_url())
                                                    },
                                                    s3_endpoint_url: s3_endpoint_url_from_form(&cache_type, &url_value),
                                                    enabled: Some(true),
                                                    s3_profile,
                                                    s3_access_key_id,
                                                    s3_secret_access_key,
                                                    attic_token,
                                                    attic_cache_name: None,
                                                    ..Default::default()
                                                };
                                                spawn(async move {
                                                    match client::test_cache_destination_credentials(&req).await {
                                                        Ok(result) if result.ok => form_testing.set(Some("ok".to_string())),
                                                        Ok(result) => {
                                                            form_testing.set(Some("fail".to_string()));
                                                            form_test_error.set(Some(result.message));
                                                        }
                                                        Err(e) => {
                                                            form_testing.set(Some("fail".to_string()));
                                                            form_test_error.set(Some(e.to_string()));
                                                        }
                                                    }
                                                });
                                            },
                                            match form_testing().as_deref() {
                                                Some("running") => "Testing…",
                                                Some("ok") => "✓ Connected",
                                                Some("fail") => "✗ Failed",
                                                _ => "Test"
                                            }
                                        }
                                    }
                                    if local_credentials().is_empty() {
                                        div { class: "help", "Saved credentials are not available yet. Disable authentication to test public connectivity, or add a credential in this form." }
                                    }
                                    if let Some(err) = form_test_error() {
                                        div { class: "help", style: "color: var(--cf-danger);", "Test failed: {err}" }
                                    }
                                }
                            }

                            // Assigned environments
                            div {
                                class: "field",
                                label { "Assigned environments" }
                                if let Some(Ok(envs)) = environments.read().as_ref() {
                                    div {
                                        style: "display:flex; flex-wrap:wrap; gap:6px;",
                                        for env in envs {
                                            {
                                                let env_id = env.id;
                                                let env_name = env.name.clone();
                                                let is_selected = form_environment_ids().contains(&env_id);
                                                let color = normalize_env_color(&env.color_hex);

                                                rsx! {
                                                    button {
                                                        class: "focus-ring",
                                                        onclick: move |_| {
                                                            let mut ids = form_environment_ids();
                                                            if is_selected {
                                                                ids.retain(|&id| id != env_id);
                                                            } else {
                                                                ids.push(env_id);
                                                            }
                                                            form_environment_ids.set(ids);
                                                        },
                                                        style: if is_selected {
                                                            format!("padding: 3px 7px; border-radius: 999px; font-size: 10px; border: 1px solid {}; background: color-mix(in oklab, {} 14%, var(--cf-card-bg)); color: {}; cursor: pointer; display: inline-flex; align-items: center; gap: 6px; font-family: inherit; font-weight: 400;", color, color, color)
                                                        } else {
                                                            format!("padding: 3px 7px; border-radius: 999px; font-size: 10px; border: 1px solid var(--cf-card-border); background: transparent; color: var(--cf-text-secondary); cursor: pointer; display: inline-flex; align-items: center; gap: 6px; font-family: inherit; font-weight: 400;")
                                                        },
                                                        span {
                                                            style: "width:6px; height:6px; border-radius:50%; background:{color};"
                                                        }
                                                        "{env_name}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    div {
                                        class: "help",
                                        "Crystal Forge will push builds for systems in these environments to this cache."
                                    }
                                }
                            }
                        }

                        // Modal foot
                        div {
                            class: "modal-foot",
                            button {
                                class: "btn btn-ghost focus-ring",
                                onclick: move |_| edit_destination.set(None),
                                "Cancel"
                            }
                            button {
                                class: "btn btn-primary focus-ring",
                                onclick: move |_| {
                                    let cache_id = dest.id;
                                    form_save_error.set(None);
                                    spawn(async move {
                                        let cache_type = form_type();
                                        let url_value = form_url();
                                        // Update basic cache fields
                                        let req = UpdateCacheDestination {
                                            name: Some(form_name()),
                                            cache_type: Some(api_cache_type(&cache_type)),
                                            push_to: if form_url().trim().is_empty() { None } else { Some(form_url()) },
                                            s3_endpoint_url: s3_endpoint_url_from_form(&cache_type, &url_value),
                                            s3_profile: {
                                                let selected_credential = local_credentials()
                                                    .into_iter()
                                                    .find(|cred| cred.id == form_cred_id());
                                                let (s3_profile, _, _, _) = credential_fields_for_request(selected_credential.as_ref());
                                                s3_profile
                                            },
                                            s3_access_key_id: {
                                                let selected_credential = local_credentials()
                                                    .into_iter()
                                                    .find(|cred| cred.id == form_cred_id());
                                                let (_, s3_access_key_id, _, _) = credential_fields_for_request(selected_credential.as_ref());
                                                s3_access_key_id
                                            },
                                            s3_secret_access_key: {
                                                let selected_credential = local_credentials()
                                                    .into_iter()
                                                    .find(|cred| cred.id == form_cred_id());
                                                let (_, _, s3_secret_access_key, _) = credential_fields_for_request(selected_credential.as_ref());
                                                s3_secret_access_key
                                            },
                                            attic_token: {
                                                let selected_credential = local_credentials()
                                                    .into_iter()
                                                    .find(|cred| cred.id == form_cred_id());
                                                let (_, _, _, attic_token) = credential_fields_for_request(selected_credential.as_ref());
                                                attic_token
                                            },
                                            attic_cache_name: None,
                                            ..Default::default()
                                        };

                                        match client::update_cache_destination(cache_id, &req).await {
                                            Ok(_) => {
                                                // Update environment assignments
                                                match client::assign_cache_environments(cache_id, form_environment_ids()).await {
                                                    Ok(_) => {
                                                        edit_destination.set(None);
                                                        refresh_nonce.set(refresh_nonce() + 1);
                                                    }
                                                    Err(e) => {
                                                        form_save_error.set(Some(format!("Failed to assign environments: {e}")));
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                form_save_error.set(Some(format!("Failed to update destination: {e}")));
                                            }
                                        }
                                    });
                                },
                                // Check icon
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
                                " Save changes"
                            }
                            if let Some(err) = form_save_error() {
                                div { class: "help", style: "color: var(--cf-danger); margin-left:auto;", "{err}" }
                            }
                        }
                    }

                }
            }

            // Nested credential modal available from both add and edit flows
            if form_show_cred_modal() {
                CacheCredModal {
                    cache_type: form_type(),
                    on_close: move |new_credential: Option<LocalCredential>| {
                        form_show_cred_modal.set(false);
                        if let Some(credential) = new_credential {
                            let cred_id = credential.id.clone();
                            let mut creds = local_credentials();
                            creds.retain(|existing| existing.id != cred_id);
                            creds.push(credential);
                            local_credentials.set(creds);
                            form_cred_id.set(cred_id);
                        }
                    }
                }
            }
        }
    }
}

/// Cache credential modal (mockup lines 286-359)
#[component]
fn CacheCredModal(cache_type: String, on_close: EventHandler<Option<LocalCredential>>) -> Element {
    let mut cred_kind = use_signal(|| {
        if cache_type == "s3" {
            "aws-key"
        } else if cache_type == "attic" {
            "attic-token"
        } else {
            "nix-token"
        }
    });
    let mut cred_name = use_signal(String::new);
    let mut cred_access_key = use_signal(String::new);
    let mut cred_secret_key = use_signal(String::new);
    let mut cred_token = use_signal(String::new);
    let mut cred_role_arn = use_signal(String::new);

    rsx! {
        div {
            class: "modal-backdrop",
            style: "z-index:95;",
            onclick: move |_| on_close.call(None),
            div {
                class: "modal",
                style: "width:min(520px,96vw);",
                onclick: move |e| e.stop_propagation(),

                div {
                    class: "modal-head",
                    h2 {
                        svg {
                            width: "14",
                            height: "14",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            style: "margin-right:6px; vertical-align:text-bottom;",
                            circle { cx: "7.5", cy: "12", r: "3.5" }
                            path { d: "M11 12h10" }
                            path { d: "M18 12v3" }
                            path { d: "M15.5 12v2" }
                        }
                        "Add credential"
                    }
                    p {
                        if cache_type == "s3" {
                            "Saved credentials can be reused across S3 caches. Secrets are encrypted at rest."
                        } else if cache_type == "attic" {
                            "Saved credentials can be reused across Attic caches. Secrets are encrypted at rest."
                        } else {
                            "Saved credentials can be reused across Nix HTTPS caches. Secrets are encrypted at rest."
                        }
                    }
                }

                div {
                    class: "modal-body",

                    div {
                        class: "field",
                        label { "Name" }
                        input {
                            class: "input focus-ring",
                            value: cred_name(),
                            oninput: move |evt| cred_name.set(evt.value()),
                            placeholder: "e.g. aws-prod-role"
                        }
                    }

                    div {
                        class: "field",
                        label { "Type" }
                        div {
                            class: "seg",
                            if cache_type == "s3" {
                                button {
                                    class: if cred_kind() == "aws-key" { "active" } else { "" },
                                    onclick: move |_| cred_kind.set("aws-key"),
                                    "AWS access key"
                                }
                                button {
                                    class: if cred_kind() == "aws-role" { "active" } else { "" },
                                    onclick: move |_| cred_kind.set("aws-role"),
                                    "IAM role (IRSA)"
                                }
                            } else if cache_type == "attic" {
                                button {
                                    class: "active",
                                    "Attic token"
                                }
                            } else {
                                button {
                                    class: "active",
                                    "Nix HTTPS token"
                                }
                            }
                        }
                    }

                    if cred_kind() == "aws-key" {
                        div {
                            class: "field",
                            label { "Access key ID" }
                            input {
                                class: "input focus-ring mono",
                                style: "font-size:12px;",
                                value: cred_access_key(),
                                oninput: move |evt| cred_access_key.set(evt.value()),
                                placeholder: "AKIA…"
                            }
                        }
                        div {
                            class: "field",
                            label { "Secret access key" }
                            input {
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
                            label { "Role ARN" }
                            input {
                                class: "input focus-ring mono",
                                style: "font-size:12px;",
                                value: cred_role_arn(),
                                oninput: move |evt| cred_role_arn.set(evt.value()),
                                placeholder: "arn:aws:iam::123456789012:role/cache-pusher"
                            }
                            div {
                                class: "help",
                                "Crystal Forge must be running with permission to assume this role."
                            }
                        }
                    }

                    if cred_kind() == "attic-token" || cred_kind() == "nix-token" {
                        div {
                            class: "field",
                            label { "Token" }
                            input {
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
                        onclick: move |_| on_close.call(None),
                        "Cancel"
                    }
                    button {
                        class: "btn btn-primary focus-ring",
                        disabled: cred_name().trim().is_empty(),
                        onclick: move |_| {
                            let name = cred_name();
                            let cred_id = format!("cred-{}", name.to_lowercase().replace(|c: char| !c.is_ascii_alphanumeric(), "-"));
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
    on_close: EventHandler<()>,
    on_edit: EventHandler<CacheDestination>,
}

#[component]
fn CacheDestinationPanel(props: CacheDestinationPanelProps) -> Element {
    let destination = props.destination.clone();
    let dest_for_edit = destination.clone();
    let nav = use_navigator();
    let cache_id = destination.id;
    let assignments = use_resource(move || async move {
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
                                                nav.push(Route::SystemDetailView { id: system_id.clone() });
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

fn s3_endpoint_url_from_form(cache_type: &str, url: &str) -> Option<String> {
    let trimmed = url.trim();
    if cache_type == "s3" && is_http_url(trimmed) {
        Some(trimmed.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{CacheFormValidationInput, validate_cache_destination_form};

    fn base_input(cache_type: &str, push_to: &str) -> CacheFormValidationInput {
        CacheFormValidationInput {
            name: "main-cache".to_string(),
            cache_type: cache_type.to_string(),
            push_to: push_to.to_string(),
            attic_cache_name: "binary-cache".to_string(),
            attic_public_key: "cache.example.org-1:AbCdEf0123+/=".to_string(),
            attic_token: "secret-token".to_string(),
            s3_region: "us-east-1".to_string(),
            s3_access_key_id: "AKIA1234567890".to_string(),
            s3_secret_access_key: "secret-access-key".to_string(),
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
