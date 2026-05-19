//! Cache management view - configure cache destinations and monitor push jobs.

use dioxus::prelude::*;
use std::collections::HashMap;
use uuid::Uuid;

use crate::api::client;
use crate::api::models::{
    CacheDestination, CreateCacheDestination, UpdateCacheDestination,
};
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
        "attic" => {
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
        "s3" => {
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
        "nix" => {
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

/// Cache management page - main view matching JSX CachesView.jsx structure
#[component]
pub fn CachesView() -> Element {
    let mut query = use_signal(String::new);
    let mut refresh_nonce = use_signal(|| 0_u32);
    
    let destinations = use_resource(move || {
        let _nonce = refresh_nonce();
        async move { client::fetch_cache_destinations(false).await }
    });

    let mut edit_cache = use_signal(|| None::<CacheDestination>);
    let mut add_open = use_signal(|| false);

    // Compute filtered caches based on search query
    let caches = use_memo(move || {
        let q = query().to_lowercase();
        if let Some(Ok(dests)) = destinations.read().as_ref() {
            if q.is_empty() {
                dests.clone()
            } else {
                dests
                    .iter()
                    .filter(|c| {
                        c.name.to_lowercase().contains(&q)
                            || c.push_to
                                .as_ref()
                                .map(|url| url.to_lowercase().contains(&q))
                                .unwrap_or(false)
                    })
                    .cloned()
                    .collect()
            }
        } else {
            Vec::new()
        }
    });

    // Compute totals for stat strip
    let totals = use_memo(move || {
        if let Some(Ok(dests)) = destinations.read().as_ref() {
            let total = dests.len();
            let healthy = dests.iter().filter(|c| c.enabled).count();
            let issues = dests.iter().filter(|c| !c.enabled).count();
            // Note: paths is not available in our API model, so we'll show placeholder
            let paths = 0;
            (total, healthy, issues, paths)
        } else {
            (0, 0, 0, 0)
        }
    });

    // Extract totals for use in rsx
    let (total, healthy, issues, paths) = totals();

    rsx! {
        // Page container - JSX: div with display:flex, flexDirection:column, gap:16
        div {
            style: "display:flex; flex-direction:column; gap:16px;",

            // Page head - JSX: page-head class
            div {
                class: "page-head",
                div {
                    h1 { class: "page-title", "Caches" }
                    p {
                        class: "page-subtitle",
                        "{total} destinations · {healthy} healthy · {paths} paths cached"
                    }
                }
                button {
                    class: "btn btn-primary focus-ring",
                    onclick: move |_| add_open.set(true),
                    // Plus icon SVG
                    svg {
                        width: "14",
                        height: "14",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        style: "display:inline-block; vertical-align:text-bottom; margin-right:4px;",
                        line { x1: "12", y1: "5", x2: "12", y2: "19" }
                        line { x1: "5", y1: "12", x2: "19", y2: "12" }
                    }
                    " Add cache"
                }
            }

            // Stat strip - JSX: stat-strip class
            div {
                class: "stat-strip",
                StatCard { label: "Total caches", value: total.to_string(), color: "#a78bfa" }
                StatCard { label: "Healthy", value: healthy.to_string(), color: "#34d399" }
                StatCard { label: "Issues", value: issues.to_string(), color: "#fbbf24" }
                StatCard { label: "Paths cached", value: paths.to_string(), color: "#60a5fa" }
            }

            // Filterbar - JSX: filterbar class
            div {
                class: "filterbar",
                div {
                    class: "filter-search",
                    style: "max-width:320px;",
                    // Search icon SVG
                    svg {
                        width: "16",
                        height: "16",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        circle { cx: "11", cy: "11", r: "8" }
                        path { d: "m21 21-4.3-4.3" }
                    }
                    input {
                        class: "input focus-ring",
                        placeholder: "Search caches…",
                        value: query(),
                        oninput: move |evt| query.set(evt.value()),
                    }
                }
                span {
                    class: "filter-count",
                    {format!("{} caches", caches().len())}
                }
            }

            // Table card - JSX: card with overflow:hidden, sys-table
            div {
                class: "card",
                style: "overflow:hidden;",
                
                match &*destinations.read_unchecked() {
                    Some(Ok(_)) => rsx! {
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
                                for cache in caches() {
                                    CacheRow {
                                        key: "{cache.id}",
                                        cache: cache.clone(),
                                        on_edit: move |c: CacheDestination| {
                                            edit_cache.set(Some(c));
                                        }
                                    }
                                }
                            }
                        }
                    },
                    Some(Err(e)) => rsx! {
                        div {
                            style: "padding:48px; text-align:center; color:var(--cf-text-muted);",
                            "Error loading caches: {e}"
                        }
                    },
                    None => rsx! {
                        div {
                            style: "padding:48px; text-align:center; color:var(--cf-text-muted);",
                            "Loading caches..."
                        }
                    }
                }
            }

            // Modals
            if edit_cache().is_some() || add_open() {
                CacheFormModal {
                    mode: if add_open() { "add" } else { "edit" },
                    cache: edit_cache(),
                    on_close: move |_| {
                        edit_cache.set(None);
                        add_open.set(false);
                        refresh_nonce.set(refresh_nonce() + 1);
                    }
                }
            }
        }
    }
}

/// Stat card component for the stat strip
#[component]
fn StatCard(label: &'static str, value: String, color: &'static str) -> Element {
    rsx! {
        div {
            class: "stat",
            span {
                class: "stat-accent",
                style: "--stat-color: {color};",
            }
            div { class: "stat-label", "{label}" }
            div { class: "stat-value", "{value}" }
        }
    }
}

/// Cache row component matching JSX CacheRow structure exactly
#[component]
fn CacheRow(cache: CacheDestination, on_edit: EventHandler<CacheDestination>) -> Element {
    // Fetch environment assignments for this cache
    let cache_id = cache.id;
    let env_ids = use_resource(move || async move {
        client::get_cache_environments(cache_id).await.unwrap_or_default()
    });

    let environments = use_resource(|| async move { 
        client::fetch_environments().await 
    });

    // Status mapping - JSX lines 90-94
    let (status_cls, status_color, status_label) = if cache.enabled {
        ("chip-healthy", "#34d399", "healthy")
    } else {
        ("chip-critical", "#f87171", "error")
    };

    // Type icon mapping - JSX line 96
    let type_icon = match cache.cache_type.to_lowercase().as_str() {
        "s3" => "download",
        "attic" => "download",
        "nix" => "link",
        _ => "download",
    };

    let cache_for_click = cache.clone();
    let cache_for_edit = cache.clone();

    rsx! {
        tr {
            style: "cursor:pointer;",
            onclick: move |_| on_edit.call(cache_for_click.clone()),
            
            // Cache column - JSX lines 100-107
            td {
                div {
                    style: "font-weight:600; font-size:13px; display:flex; align-items:center; gap:6px;",
                    // Type icon
                    {render_icon(type_icon, 12, "opacity:0.6;")}
                    "{cache.name}"
                    if cache.cache_type.to_lowercase() == "nix" && cache.push_to.as_ref().map(|u| u.contains("cache.nixos.org")).unwrap_or(false) {
                        span {
                            class: "chip chip-info",
                            style: "font-size:9px;",
                            "system"
                        }
                    }
                }
                if let Some(ref url) = cache.push_to {
                    div {
                        class: "mono",
                        style: "font-size:11px; color:var(--cf-text-muted);",
                        "{url}"
                    }
                }
            }

            // Type column - JSX line 108
            td {
                span {
                    class: "chip chip-unknown mono",
                    style: "font-size:10px;",
                    "{cache.cache_type}"
                }
            }

            // Status column - JSX lines 109-114
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

            // Storage column - JSX lines 115-132
            td {
                div {
                    style: "min-width:120px; height:30px; display:flex; flex-direction:column; justify-content:center; gap:3px;",
                    // Mock storage data since API doesn't provide it
                    div {
                        style: "font-size:11px; color:var(--cf-text-secondary);",
                        span { class: "mono", "—" }
                    }
                }
            }

            // Paths column - JSX line 133
            td {
                class: "mono",
                style: "font-size:12px;",
                "—"
            }

            // Last push column - JSX line 134
            td {
                style: "font-size:12px; color:var(--cf-text-secondary);",
                if let Some(ref last_used) = cache.last_used_at {
                    "{last_used.format(\"%Y-%m-%d %H:%M\")}"
                } else {
                    "—"
                }
            }

            // Environments column - JSX lines 135-143
            td {
                div {
                    style: "display:flex; gap:4px; flex-wrap:wrap;",
                    match (env_ids.read().as_ref(), environments.read().as_ref()) {
                        (Some(ids), Some(Ok(all_envs))) if !ids.is_empty() => {
                            let matching_envs: Vec<_> = all_envs.iter()
                                .filter(|e| ids.contains(&e.id))
                                .take(3)
                                .collect();
                            
                            rsx! {
                                for env in matching_envs {
                                    EnvBadge { env_name: env.name.clone() }
                                }
                                if ids.len() > 3 {
                                    span {
                                        class: "chip chip-unknown",
                                        style: "font-size:10px;",
                                        "+{ids.len() - 3}"
                                    }
                                }
                            }
                        },
                        _ => rsx! {
                            span {
                                style: "font-size:11px; color:var(--cf-text-muted);",
                                "none"
                            }
                        }
                    }
                }
            }

            // Actions column - JSX lines 144-150
            td {
                div {
                    class: "row-actions",
                    button {
                        class: "btn-icon focus-ring",
                        title: "Edit",
                        onclick: move |e| {
                            e.stop_propagation();
                            on_edit.call(cache_for_edit.clone());
                        },
                        {render_icon("gear", 14, "")}
                    }
                }
            }
        }
    }
}

/// Environment badge component
#[component]
fn EnvBadge(env_name: String) -> Element {
    // Simple environment color mapping
    let color = match env_name.to_lowercase().as_str() {
        "production" => "#f43f5e",
        "staging" => "#f59e0b",
        "dev" | "development" => "#3b82f6",
        "edge" => "#8b5cf6",
        _ => "#6b7280",
    };

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

/// Cache form modal - matches JSX CacheFormModal structure (lines 155-284)
#[component]
fn CacheFormModal(mode: &'static str, cache: Option<CacheDestination>, on_close: EventHandler<()>) -> Element {
    let is_edit = mode == "edit";
    
    // Form state
    let mut form_name = use_signal(|| {
        cache.as_ref().map(|c| c.name.clone()).unwrap_or_default()
    });
    let mut form_type = use_signal(|| {
        cache.as_ref().map(|c| c.cache_type.clone()).unwrap_or_else(|| "s3".to_string())
    });
    let mut form_url = use_signal(|| {
        cache.as_ref().and_then(|c| c.push_to.clone()).unwrap_or_default()
    });
    let mut form_requires_auth = use_signal(|| true);
    let mut form_cred_id = use_signal(String::new);
    let mut form_environments = use_signal(Vec::<Uuid>::new);
    let mut testing = use_signal(|| None::<String>);

    // Load environments
    let environments = use_resource(|| async move {
        client::fetch_environments().await
    });

    // Load current environment assignments if editing
    if let Some(ref c) = cache {
        let cache_id = c.id;
        let env_ids = use_resource(move || async move {
            client::get_cache_environments(cache_id).await.unwrap_or_default()
        });
        
        use_effect(move || {
            if let Some(ids) = env_ids.read().as_ref() {
                if form_environments().is_empty() && !ids.is_empty() {
                    form_environments.set(ids.clone());
                }
            }
        });
    }

    rsx! {
        // Modal backdrop - JSX line 180
        div {
            class: "modal-backdrop",
            onclick: move |_| on_close.call(()),
            
            // Modal container - JSX line 181
            div {
                class: "modal",
                onclick: move |e| e.stop_propagation(),
                style: "width:min(620px,96vw); max-height:92vh;",
                
                // Modal head - JSX lines 182-188
                div {
                    class: "modal-head",
                    h2 {
                        if is_edit {
                            {render_icon("gear", 14, "margin-right:6px; vertical-align:text-bottom;")}
                            if let Some(ref c) = cache {
                                "Edit {c.name}"
                            }
                        } else {
                            {render_icon("plus", 14, "margin-right:6px; vertical-align:text-bottom;")}
                            "Add cache destination"
                        }
                    }
                    p {
                        if is_edit {
                            "Update binary cache destination."
                        } else {
                            "Register a new binary cache destination."
                        }
                    }
                }

                // Modal body - JSX lines 189-264
                div {
                    class: "modal-body",
                    style: "overflow-y:auto;",
                    
                    // Name field - JSX lines 190-193
                    div {
                        class: "field",
                        label { "Name" }
                        input {
                            class: "input focus-ring",
                            value: form_name(),
                            oninput: move |evt| form_name.set(evt.value()),
                            placeholder: "e.g. crystal-forge-prod-cache",
                        }
                    }

                    // Type field - JSX lines 194-205
                    div {
                        class: "field",
                        label { "Type" }
                        div {
                            class: "seg",
                            for (val, label) in [("s3", "S3-compatible"), ("attic", "Attic"), ("nix", "Nix HTTPS")] {
                                button {
                                    class: if form_type() == val { "active" } else { "" },
                                    onclick: move |_| form_type.set(val.to_string()),
                                    "{label}"
                                }
                            }
                        }
                    }

                    // URL field - JSX lines 206-210
                    div {
                        class: "field",
                        label { "URL" }
                        input {
                            class: "input focus-ring mono",
                            style: "font-size:12px;",
                            value: form_url(),
                            oninput: move |evt| form_url.set(evt.value()),
                            placeholder: match form_type().as_str() {
                                "s3" => "s3://bucket?region=us-east-1",
                                "attic" => "attic://host/cache",
                                _ => "https://cache.nixos.org"
                            }
                        }
                    }

                    // Requires auth checkbox - JSX lines 211-214
                    label {
                        style: "display:flex; gap:8px; align-items:center; font-size:13px; cursor:pointer;",
                        input {
                            r#type: "checkbox",
                            checked: form_requires_auth(),
                            onchange: move |evt| form_requires_auth.set(evt.checked()),
                            style: "accent-color:var(--cf-brand-purple);",
                        }
                        span { "Requires authentication" }
                    }

                    // Credential field - JSX lines 215-234 (simplified for now)
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
                                    onchange: move |evt| form_cred_id.set(evt.value()),
                                    option { value: "", "Select a credential…" }
                                    option { value: "aws-prod-role", "aws-prod-role (IAM role)" }
                                    option { value: "aws-staging-role", "aws-staging-role (IAM role)" }
                                    option { value: "attic-token-dev", "attic-token-dev (Attic token)" }
                                }
                                button {
                                    class: "btn btn-ghost focus-ring xs",
                                    disabled: form_cred_id().is_empty(),
                                    onclick: move |_| {
                                        testing.set(Some("running".to_string()));
                                        // Simulate test
                                        spawn(async move {
                                            gloo_timers::future::TimeoutFuture::new(700).await;
                                            testing.set(Some("ok".to_string()));
                                        });
                                    },
                                    match testing().as_deref() {
                                        Some("running") => "Testing…",
                                        Some("ok") => "✓ Connected",
                                        Some("fail") => "✗ Failed",
                                        _ => "Test"
                                    }
                                }
                            }
                        }
                    }

                    // Assigned environments - JSX lines 236-264
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
                                        let is_selected = form_environments().contains(&env_id);
                                        let env_color = match env_name.to_lowercase().as_str() {
                                            "production" => "#f43f5e",
                                            "staging" => "#f59e0b",
                                            "dev" | "development" => "#3b82f6",
                                            "edge" => "#8b5cf6",
                                            _ => "#6b7280",
                                        };
                                        
                                        rsx! {
                                            button {
                                                class: "focus-ring",
                                                onclick: move |_| {
                                                    let mut envs = form_environments();
                                                    if is_selected {
                                                        envs.retain(|&id| id != env_id);
                                                    } else {
                                                        envs.push(env_id);
                                                    }
                                                    form_environments.set(envs);
                                                },
                                                style: "
                                                    padding: 4px 10px;
                                                    border-radius: 99px;
                                                    font-size: 11px;
                                                    border: 1px solid {if is_selected { env_color } else { \"var(--cf-card-border)\" }};
                                                    background: {if is_selected { format!(\"color-mix(in oklab, {} 14%, var(--cf-card-bg))\", env_color) } else { \"transparent\".to_string() }};
                                                    color: {if is_selected { env_color } else { \"var(--cf-text-secondary)\" }};
                                                    cursor: pointer;
                                                    display: inline-flex;
                                                    align-items: center;
                                                    gap: 6px;
                                                    font-family: inherit;
                                                ",
                                                span {
                                                    style: "width:6px; height:6px; border-radius:50%; background:{env_color};",
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

                // Modal foot - JSX lines 266-271
                div {
                    class: "modal-foot",
                    button {
                        class: "btn btn-ghost focus-ring",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "btn btn-primary focus-ring",
                        onclick: move |_| {
                            // TODO: Submit form
                            on_close.call(());
                        },
                        {render_icon("check", 13, "")}
                        " "
                        if is_edit { "Save changes" } else { "Add cache" }
                    }
                }
            }
        }
    }
}

/// Helper function to render SVG icons matching Feather Icons style
fn render_icon(name: &str, size: u32, extra_style: &str) -> Element {
    let size_str = format!("{}", size);
    let base_style = format!("display:inline-block; vertical-align:text-bottom; {}", extra_style);
    
    match name {
        "plus" => rsx! {
            svg {
                width: "{size_str}",
                height: "{size_str}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                style: "{base_style}",
                line { x1: "12", y1: "5", x2: "12", y2: "19" }
                line { x1: "5", y1: "12", x2: "19", y2: "12" }
            }
        },
        "search" => rsx! {
            svg {
                width: "{size_str}",
                height: "{size_str}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                style: "{base_style}",
                circle { cx: "11", cy: "11", r: "8" }
                path { d: "m21 21-4.3-4.3" }
            }
        },
        "download" => rsx! {
            svg {
                width: "{size_str}",
                height: "{size_str}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                style: "{base_style}",
                path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
                polyline { points: "7 10 12 15 17 10" }
                line { x1: "12", y1: "15", x2: "12", y2: "3" }
            }
        },
        "link" => rsx! {
            svg {
                width: "{size_str}",
                height: "{size_str}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                style: "{base_style}",
                path { d: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" }
                path { d: "M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" }
            }
        },
        "gear" => rsx! {
            svg {
                width: "{size_str}",
                height: "{size_str}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                style: "{base_style}",
                circle { cx: "12", cy: "12", r: "3" }
                path { d: "M12 1v6m0 6v6m-9-7h6m6 0h6m-1-5l-4 4m-6 6l-4 4m0-12l4 4m6 6l4 4" }
            }
        },
        "check" => rsx! {
            svg {
                width: "{size_str}",
                height: "{size_str}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                style: "{base_style}",
                polyline { points: "20 6 9 17 4 12" }
            }
        },
        _ => rsx! { span {} }
    }
}
