//! Systems list view with table/card toggle.

use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;
use web_sys::{Node, window};

use crate::api::client::set_setup_wizard_agent_acknowledged;
use crate::api::models::{
    DeploymentStatus, HealthStatus, SystemDetail, SystemSummary, SystemsListParams,
};
use crate::components::environments::{normalize_color_hex, with_alpha};
use crate::components::filters::{
    DeploymentFilterDropdown, EnvironmentFilterDropdown, HealthFilterDropdown, ViewMode,
};
use crate::components::forms::{AddSystemForm, NewSystemDraft, validate_new_system};
use crate::components::heartbeat_spinner::HeartbeatSpinner;
use crate::components::modals::{
    GeneratedKeyPair, KeyPairModal, RemoveSystemDialog, UpdatePublicKeyModal, generate_key_pair,
};
use crate::components::notifications::{AlertBanner, AlertSeverity};
use crate::components::system::{DeploySystemModal, EditSystemModal, SystemCardV2};
use crate::components::systems_stat_strip::SystemsStatStrip;
use crate::components::tables::SystemsTable;
use crate::components::{Chip, ChipVariant, EnvBadge};
use crate::environments::adapter::{
    load_environment_colors_with_fallback, load_environment_names_with_fallback,
};
use crate::routes::Route;
use crate::state::app_state::AppState;
use crate::state::auth;
use crate::systems::adapter::{
    create_system_via_api, deactivate_system_via_api, deploy_system_via_api, fallback_flake_names,
    fetch_system_commits_via_api, load_flake_context_with_fallback, load_flake_names_with_fallback,
    load_system_detail_with_fallback, load_systems_with_fallback, update_system_public_key_via_api,
    update_system_via_api,
};
use crate::theme;

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

#[path = "systems_list_helpers.rs"]
mod systems_list_helpers;
use systems_list_helpers::{
    matches_deployment, matches_environment, matches_health, matches_search, normalize_optional,
    normalize_policy, prefers_view_from_query, remove_system_by_id, systems_missing_flake_count,
    systems_missing_heartbeat_count, unique_environments, update_key_for_system,
};

const VIEW_PREF_KEY: &str = "crystal_forge.systems.view";
const DENSITY_KEY: &str = "cf.ui.density";

fn load_density() -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
        .and_then(|storage| storage.get_item(DENSITY_KEY).ok())
        .flatten()
        .map(|v| v == "compact")
        .unwrap_or(false)
}

/// Systems list with toggles and filters.
#[component]
pub fn SystemsListView() -> Element {
    let nav = navigator();
    let app_state = use_context::<Signal<AppState>>();
    let is_admin_user = auth::is_admin(&app_state.read().auth);

    let stored_view = LocalStorage::get::<String>(VIEW_PREF_KEY).ok();
    let mut view_mode = use_signal(|| ViewMode::from_storage(stored_view));
    let query_view = prefers_view_from_query();
    let mut is_compact = use_signal(load_density);
    let open_dropdown = use_signal(|| None::<String>);
    let container_id = use_memo(|| format!("systems-filters-{}", uuid::Uuid::new_v4()));
    let container_id_value = Rc::new(container_id.read().clone());

    use_effect(move || {
        if let Some(mode) = query_view {
            view_mode.set(mode);
            let _ = LocalStorage::set(VIEW_PREF_KEY, mode.as_storage());
        }
    });

    // Poll for density changes from topbar tweaks
    use_effect(move || {
        spawn(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(500).await;
                let compact = load_density();
                if compact != is_compact() {
                    is_compact.set(compact);
                }
            }
        });
    });

    // Close dropdown on outside click
    {
        let mut open_dropdown = open_dropdown.clone();
        let container_id_value = container_id_value.clone();
        use_effect(move || {
            let Some(window) = window() else { return };
            let Some(document) = window.document() else {
                return;
            };
            let document_for_listener = document.clone();
            let container_id_value = container_id_value.clone();
            let handler = Closure::<dyn FnMut(_)>::new(move |event: web_sys::Event| {
                if open_dropdown.read().is_none() {
                    return;
                }
                let target = match event.target() {
                    Some(t) => t,
                    None => return,
                };
                let node: Node = match target.dyn_into() {
                    Ok(n) => n,
                    Err(_) => return,
                };
                if let Some(container) =
                    document_for_listener.get_element_by_id(container_id_value.as_str())
                {
                    if !container.contains(Some(&node)) {
                        open_dropdown.set(None);
                    }
                }
            });
            let _ = document
                .add_event_listener_with_callback("mousedown", handler.as_ref().unchecked_ref());
            handler.forget();
        });
    }

    // Filter state
    let mut search = use_signal(String::new);
    let mut environment_filter = use_signal(Vec::<String>::new);
    let mut health_filter = use_signal(Vec::<HealthStatus>::new);
    let mut deployment_filter = use_signal(Vec::<DeploymentStatus>::new);

    // Load real data from the backend using use_resource to prevent repeated fetches.
    // Note: Currently loads all systems; filters applied client-side.
    // Future improvement: pass filters to API via SystemsListParams.
    let systems_resource = use_resource(move || async move {
        load_systems_with_fallback(&SystemsListParams::default()).await
    });

    let environment_names_resource =
        use_resource(move || async move { load_environment_names_with_fallback().await });
    let environment_colors_resource =
        use_resource(move || async move { load_environment_colors_with_fallback().await });
    let flake_names_resource =
        use_resource(move || async move { load_flake_names_with_fallback().await });
    let flake_context_resource =
        use_resource(move || async move { load_flake_context_with_fallback().await });

    // Local mutable state for systems (allows client-side add/remove until backend supports it)
    let mut local_systems = use_signal(Vec::<SystemSummary>::new);
    let mut load_error = use_signal(|| None::<String>);
    let mut api_notice = use_signal(|| None::<String>);
    let mut loading = use_signal(|| true);

    // Sync local_systems with fetched systems when resource loads
    // This effect runs when systems_resource changes
    use_effect(move || {
        if let Some(result) = &*systems_resource.read_unchecked() {
            if result.redirect_to_login {
                // Will be handled by early return below
                return;
            }
            local_systems.set(result.systems.clone());
            load_error.set(result.notice.clone());
            loading.set(false);
        }
    });

    // Check for redirect (early return ensures no flash of fallback data)
    let should_redirect = systems_resource
        .read_unchecked()
        .as_ref()
        .map(|r| r.redirect_to_login)
        .unwrap_or(false)
        || flake_names_resource
            .read_unchecked()
            .as_ref()
            .map(|r| r.redirect_to_login)
            .unwrap_or(false)
        || environment_names_resource
            .read_unchecked()
            .as_ref()
            .map(|r| r.redirect_to_login)
            .unwrap_or(false)
        || environment_colors_resource
            .read_unchecked()
            .as_ref()
            .map(|r| r.redirect_to_login)
            .unwrap_or(false)
        || flake_context_resource
            .read_unchecked()
            .as_ref()
            .map(|r| r.redirect_to_login)
            .unwrap_or(false);

    if should_redirect {
        nav.push(Route::LoginView {});
        return rsx! {
            div {
                class: "flex items-center justify-center py-12",
                p { class: "{theme::text::SECONDARY}", "Redirecting to login..." }
            }
        };
    }

    let mut show_add_form = use_signal(|| false);
    let mut add_error = use_signal(|| None::<String>);
    let mut draft = use_signal(NewSystemDraft::new);
    let mut pending_remove = use_signal(|| None::<SystemSummary>);
    let mut pending_update_key = use_signal(|| None::<SystemSummary>);
    let mut editing_system = use_signal(|| None::<uuid::Uuid>);
    let mut show_key_modal = use_signal(|| false);
    let mut generated_keys = use_signal(|| None::<GeneratedKeyPair>);
    let mut update_key_error = use_signal(|| None::<String>);
    let mut onboarding_agent_reminder = use_signal(|| None::<String>);

    // New modal state for edit and deploy
    let mut edit_modal_system = use_signal(|| None::<SystemDetail>);
    let mut deploy_modal_system = use_signal(|| {
        None::<(
            SystemDetail,
            Vec<crate::api::models::CommitInfo>,
            Option<String>,
        )>
    });
    let mut preview_system = use_signal(|| None::<SystemDetail>);
    let selected_preview_id = preview_system.read().as_ref().map(|d| d.id);
    let mut deploy_error = use_signal(|| None::<String>);

    let current_systems = local_systems.read().clone();
    let environments = unique_environments(&current_systems);
    let dropdown_environments = environment_names_resource
        .read_unchecked()
        .as_ref()
        .map(|r| r.names.clone())
        .unwrap_or_else(|| environments.clone());
    let registered_flakes = flake_names_resource
        .read_unchecked()
        .as_ref()
        .map(|r| r.names.clone())
        .unwrap_or_else(fallback_flake_names);
    let flake_context = flake_context_resource
        .read_unchecked()
        .as_ref()
        .map(|r| r.flakes.clone())
        .unwrap_or_default();
    let environment_color_pairs = environment_colors_resource
        .read_unchecked()
        .as_ref()
        .map(|r| r.colors.clone())
        .unwrap_or_default();

    let filtered_systems: Vec<SystemSummary> = current_systems
        .iter()
        .cloned()
        .filter(|system| matches_environment(system, &environment_filter.read()))
        .filter(|system| matches_health(system, &health_filter.read()))
        .filter(|system| matches_deployment(system, &deployment_filter.read()))
        .filter(|system| matches_search(system, &search.read()))
        .collect();
    let has_active_filters = !search.read().trim().is_empty()
        || !environment_filter.read().is_empty()
        || !health_filter.read().is_empty()
        || !deployment_filter.read().is_empty();
    let systems_subtitle = if *loading.read() {
        "Loading systems…".to_string()
    } else if load_error.read().is_some() {
        "Systems are temporarily unavailable".to_string()
    } else {
        format!(
            "{} systems · {} healthy · {} needing attention",
            current_systems.len(),
            current_systems
                .iter()
                .filter(|s| s.health_status == HealthStatus::Healthy)
                .count(),
            current_systems
                .iter()
                .filter(|s| s.health_status != HealthStatus::Healthy)
                .count()
        )
    };

    let registered_flakes_for_submit = registered_flakes.clone();

    let from_setup = use_signal(came_from_setup);
    let mut dismiss_add_target_callout = use_signal(|| false);

    rsx! {
        div {
            class: "space-y-6",
            id: "{container_id}",

            if from_setup() {
                div {
                    "data-testid": "setup-coach-systems-callout",
                    style: "background:rgba(30,58,138,0.22); border:1px solid rgba(96,165,250,0.55); border-radius:8px; padding:12px 16px;",
                    div {
                        style: "display:flex; flex-direction:column; gap:6px;",
                        p { style: "color:#dbeafe; font-size:12px; font-weight:700; margin:0; letter-spacing:0.03em; text-transform:uppercase;", "Setup Tour - Step 5 of 6" }
                        p { style: "color:#dbeafe; font-size:14px; font-weight:600; margin:0;", "Register a system and its agent" }
                        p { style: "color:#bfdbfe; font-size:13px; margin:0;", "Use Add System to register a machine in this fleet and connect it to environment + flake." }
                        p { style: "color:#93c5fd; font-size:12px; margin:0;", "Agents are lightweight clients installed on systems so Crystal Forge can evaluate and apply deployments." }
                    }
                }
            }

            if let Some(ref reminder) = *onboarding_agent_reminder.read() {
                div {
                    "data-testid": "setup-coach-agent-runtime-reminder-modal",
                    style: "position:fixed; inset:0; z-index:90; background:rgba(2,6,23,0.62); display:flex; align-items:center; justify-content:center; padding:16px;",
                    div {
                        style: "width:min(620px, 100%); border:2px solid rgba(59,130,246,0.75); background:linear-gradient(160deg, rgba(30,41,59,0.98), rgba(30,64,175,0.94)); border-radius:14px; box-shadow:0 18px 46px rgba(15,23,42,0.65); padding:18px 18px 16px 18px;",
                        p { style: "margin:0; color:#bfdbfe; font-weight:800; font-size:12px; letter-spacing:0.05em; text-transform:uppercase;", "Agent activation required" }
                        p { style: "margin:8px 0 0 0; color:#eff6ff; font-size:14px; line-height:1.45;", "{reminder}" }
                        div {
                            style: "margin-top:14px; display:flex; justify-content:flex-end;",
                            button {
                                class: "px-3 py-2 rounded-lg text-sm font-semibold text-white {theme::interactive::PRIMARY_BTN}",
                                onclick: move |_| onboarding_agent_reminder.set(None),
                                "Got it"
                            }
                        }
                    }
                }
            }

            // Action notice banner for create, update, deploy, or remove flows.
            if let Some(ref notice) = *api_notice.read() {
                div {
                    class: "rounded-lg border border-yellow-500/30 bg-yellow-500/10 px-4 py-3 text-sm text-yellow-300",
                    "{notice}"
                }
            }

            if let Some(result) = environment_names_resource.read_unchecked().as_ref() {
                if let Some(ref notice) = result.notice {
                    div {
                        class: "rounded-lg border border-yellow-500/30 bg-yellow-500/10 px-4 py-3 text-sm text-yellow-300",
                        "{notice}"
                    }
                }
            }

            if let Some(result) = flake_names_resource.read_unchecked().as_ref() {
                if let Some(ref notice) = result.notice {
                    div {
                        class: "rounded-lg border border-yellow-500/30 bg-yellow-500/10 px-4 py-3 text-sm text-yellow-300",
                        "{notice}"
                    }
                }
            }

            // Admin-only contextual health warnings (no agent heartbeat).
            if is_admin_user && !*loading.read() {
                {
                    let systems_snap = local_systems.read();
                    let no_flake_count = systems_missing_flake_count(&systems_snap);
                    if no_flake_count > 0 {
                        let suffix_s = if no_flake_count == 1 { "" } else { "s" };
                        let suffix_v = if no_flake_count == 1 { "is" } else { "are" };
                        let affected_hostnames: Vec<String> = systems_snap
                            .iter()
                            .filter(|system| system.flake_id.is_none())
                            .map(|system| system.hostname.clone())
                            .collect();
                        let listed_hostnames = affected_hostnames
                            .iter()
                            .take(3)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ");
                        let remaining = affected_hostnames.len().saturating_sub(3);
                        let affected_summary = if remaining > 0 {
                            format!("{listed_hostnames} (+{remaining} more)")
                        } else {
                            listed_hostnames
                        };
                        let msg = format!(
                            "{no_flake_count} system{suffix_s} {suffix_v} not linked to a flake and won't be included in evaluations. Affected system{suffix_s}: {affected_summary}. To resolve: click Edit on each affected system and set Flake Name."
                        );
                        rsx! {
                            div {
                                "data-testid": "systems-missing-flake-warning",
                                AlertBanner {
                                    severity: AlertSeverity::Warning,
                                    message: msg,
                                    action_label: Some("Review affected systems".to_string()),
                                    action_url: Some("/systems".to_string()),
                                }
                            }
                        }
                    } else {
                        rsx! {}
                    }
                }

                {
                    let systems_snap = local_systems.read();
                    let no_hb_count = systems_missing_heartbeat_count(&systems_snap);
                    if no_hb_count > 0 {
                        let suffix_s = if no_hb_count == 1 { "" } else { "s" };
                        let suffix_v = if no_hb_count == 1 { "has" } else { "have" };
                        let msg = format!(
                            "{no_hb_count} system{suffix_s} {suffix_v} no agent heartbeat on record and cannot receive deployments."
                        );
                        rsx! {
                            AlertBanner {
                                severity: AlertSeverity::Warning,
                                message: msg,
                            }
                        }
                    } else {
                        rsx! {}
                    }
                }
            }

            header {
                class: "page-head",
                div {
                    h1 { class: "page-title", "Systems" }
                    p {
                        class: "page-subtitle",
                        "{systems_subtitle}"
                    }
                }
                div {
                    style: "display: flex; gap: 8px;",
                    // Export — downloads OSCAL SSP system inventory JSON
                    button {
                        class: "btn btn-ghost focus-ring",
                        "data-testid": "export-systems-button",
                        title: "Export system inventory as OSCAL SSP JSON",
                        onclick: {
                            let systems_snapshot = local_systems.read().clone();
                            move |_| {
                                export_systems_oscal(&systems_snapshot);
                            }
                        },
                        svg {
                            class: "w-3.5 h-3.5",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            view_box: "0 0 24 24",
                            path { d: "M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" }
                        }
                        "Export"
                    }
                    // Add system (primary) — keeps existing functionality
                    div {
                        class: "relative z-40",
                        button {
                            "data-testid": "add-system-button",
                            class: if from_setup() && !*show_add_form.read() {
                                "btn btn-primary focus-ring animate-pulse"
                            } else {
                                "btn btn-primary focus-ring"
                            },
                            onclick: move |_| {
                                let next = !*show_add_form.read();
                                show_add_form.set(next);
                                add_error.set(None);
                                if next {
                                    dismiss_add_target_callout.set(true);
                                }
                            },
                            svg {
                                class: "w-3.5 h-3.5",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                view_box: "0 0 24 24",
                                path { d: "M12 4v16m8-8H4" }
                            }
                            if *show_add_form.read() { "Close" } else { "Add system" }
                        }
                        if from_setup() && !*show_add_form.read() && !dismiss_add_target_callout() {
                            div {
                                "data-testid": "setup-coach-systems-target-callout",
                                style: "position:absolute; z-index:70; right:0; top:calc(100% + 10px); background:rgba(30,64,175,0.94); border:1px solid rgba(96,165,250,0.75); border-radius:10px; padding:8px 10px; color:#dbeafe; font-size:12px; width:220px; box-shadow:0 10px 24px rgba(15,23,42,0.45);",
                                div {
                                    style: "position:absolute; top:-6px; right:18px; width:10px; height:10px; background:rgba(30,64,175,0.94); border-left:1px solid rgba(96,165,250,0.75); border-top:1px solid rgba(96,165,250,0.75); transform:rotate(45deg);"
                                }
                                p { style: "margin:0; color:#eff6ff; font-weight:600;", "Next action" }
                                p { style: "margin:2px 0 0 0;", "Click Add system to register your first managed machine." }
                            }
                        }
                    }
                }
            }

            if !*loading.read() && load_error.read().is_none() {
                SystemsStatStrip {
                    systems: filtered_systems.clone(),
                    environment_colors: environment_color_pairs.clone(),
                }
            }

            // System Form Modal
            if *show_add_form.read() {
                AddSystemForm {
                    draft: draft,
                    error: add_error,
                    show_onboarding_callouts: from_setup(),
                    key_modal_open: *show_key_modal.read(),
                    on_cancel: move |_| {
                        draft.set(NewSystemDraft::new());
                        add_error.set(None);
                        show_add_form.set(false);
                    },
                    on_submit: move |_| {
                        let next = draft.read().clone();
                        if let Err(message) = validate_new_system(&next, &local_systems.read(), &registered_flakes_for_submit) {
                            add_error.set(Some(message));
                            return;
                        }
                        let from_setup_active = from_setup();
                        let first_system_in_setup = from_setup_active && local_systems.read().is_empty();

                        // Call backend API to create the system
                        spawn(async move {
                            match create_system_via_api(
                                next.hostname.trim().to_string(),
                                normalize_optional(&next.system_configuration_name),
                                next.public_key.clone(),
                                normalize_optional(&next.environment),
                                normalize_optional(&next.flake_name),
                                normalize_policy(&next.deployment_policy),
                            ).await {
                                Ok(detail) => {
                                    // Convert SystemDetail to SystemSummary for the list
                                    let new_item = SystemSummary {
                                        id: detail.id,
                                        hostname: detail.hostname,
                                        system_configuration_name: detail.system_configuration_name,
                                        environment: detail.environment,
                                        flake_id: detail.flake.as_ref().map(|flake| flake.id),
                                        primary_ip: detail.network.primary_ip,
                                        health_status: detail.health_status,
                                        deployment_status: detail.deployment_status,
                                        pipeline_stage: detail.pipeline_stage,
                                        cve_counts: detail.cve_counts,
                                        nixos_version: detail.nixos_version,
                                        last_seen: detail.last_seen,
                                        deployment_policy: detail.deployment_policy,
                                    };

                                    let mut values = local_systems.read().clone();
                                    values.push(new_item);
                                    values.sort_by(|a, b| a.hostname.to_lowercase().cmp(&b.hostname.to_lowercase()));
                                    local_systems.set(values);
                                    draft.set(NewSystemDraft::new());
                                    add_error.set(None);
                                    show_add_form.set(false);
                                    if first_system_in_setup {
                                        let _ = set_setup_wizard_agent_acknowledged(true).await;
                                        onboarding_agent_reminder.set(Some(
                                            "System record created. Next, ensure this host config enables the Crystal Forge agent module, apply/rebuild that config, and confirm the agent service is running before expecting heartbeats or deployment status.".to_string(),
                                        ));
                                    }
                                }
                                Err(error_message) => {
                                    add_error.set(Some(error_message));
                                }
                            }
                        });
                    },
                    on_generate_keys: move |_| {
                        generated_keys.set(Some(generate_key_pair()));
                        show_key_modal.set(true);
                    },
                    environments: dropdown_environments.clone(),
                    flake_names: registered_flakes.clone(),
                    title: "Register System".to_string(),
                    submit_label: "Save System".to_string(),
                }
            }

            if let Some(system_id) = *editing_system.read() {
                AddSystemForm {
                    draft: draft,
                    error: add_error,
                    show_onboarding_callouts: false,
                    key_modal_open: *show_key_modal.read(),
                    on_cancel: move |_| {
                        draft.set(NewSystemDraft::new());
                        add_error.set(None);
                        editing_system.set(None);
                    },
                    on_submit: move |_| {
                        let next = draft.read().clone();
                        if next.hostname.trim().is_empty() {
                            add_error.set(Some("Hostname is required.".to_string()));
                            return;
                        }
                        spawn(async move {
                            match update_system_via_api(
                                system_id,
                                next.hostname.trim().to_string(),
                                normalize_optional(&next.system_configuration_name),
                                normalize_optional(&next.environment),
                                normalize_optional(&next.flake_name),
                                normalize_policy(&next.deployment_policy),
                            ).await {
                                Ok(detail) => {
                                    let updated = SystemSummary {
                                        id: detail.id,
                                        hostname: detail.hostname,
                                        system_configuration_name: detail.system_configuration_name,
                                        environment: detail.environment,
                                        flake_id: detail.flake.as_ref().map(|flake| flake.id),
                                        primary_ip: detail.network.primary_ip,
                                        health_status: detail.health_status,
                                        deployment_status: detail.deployment_status,
                                        pipeline_stage: detail.pipeline_stage,
                                        cve_counts: detail.cve_counts,
                                        nixos_version: detail.nixos_version,
                                        last_seen: detail.last_seen,
                                        deployment_policy: detail.deployment_policy,
                                    };
                                    let mut values = local_systems.read().clone();
                                    if let Some(item) = values.iter_mut().find(|item| item.id == system_id) {
                                        *item = updated;
                                    }
                                    values.sort_by(|a, b| a.hostname.to_lowercase().cmp(&b.hostname.to_lowercase()));
                                    local_systems.set(values);
                                    draft.set(NewSystemDraft::new());
                                    add_error.set(None);
                                    editing_system.set(None);
                                }
                                Err(error_message) => add_error.set(Some(error_message)),
                            }
                        });
                    },
                    on_generate_keys: move |_| {
                        generated_keys.set(Some(generate_key_pair()));
                        show_key_modal.set(true);
                    },
                    environments: dropdown_environments.clone(),
                    flake_names: registered_flakes.clone(),
                    title: "Edit System".to_string(),
                    submit_label: "Save Changes".to_string(),
                }
            }

            // Key Pair Modal
            if *show_key_modal.read() {
                KeyPairModal {
                    keys: generated_keys.read().clone(),
                    on_close: move |_| show_key_modal.set(false),
                    on_use_public_key: move |_| {
                        if let Some(keys) = generated_keys.read().clone() {
                            let mut next = draft.read().clone();
                            next.public_key = keys.public_key;
                            draft.set(next);
                        }
                        show_key_modal.set(false);
                    }
                }
            }

            if !*loading.read() && load_error.read().is_none() {
                // Filters Bar
                div {
                    class: "filterbar",
                    id: "{container_id}",
                    div {
                        class: "filter-search",
                        svg {
                            class: "w-3.5 h-3.5",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            view_box: "0 0 24 24",
                            circle { cx: "11", cy: "11", r: "8" }
                            path { d: "m21 21-4.35-4.35" }
                        }
                        input {
                            class: "input focus-ring",
                            r#type: "search",
                            placeholder: "Filter by hostname, commit, or flake…",
                            value: "{search.read()}",
                            oninput: move |evt| search.set(evt.value()),
                        }
                    }
                    EnvironmentFilterDropdown {
                        environments: environments.clone(),
                        selected: environment_filter,
                        open_dropdown: open_dropdown,
                    }
                    HealthFilterDropdown {
                        selected: health_filter,
                        open_dropdown: open_dropdown,
                    }
                    DeploymentFilterDropdown {
                        selected: deployment_filter,
                        open_dropdown: open_dropdown,
                    }
                    div {
                        class: "seg",
                        role: "tablist",
                        "aria-label": "View mode",
                        button {
                            class: if *view_mode.read() == ViewMode::Cards { "active" } else { "" },
                            onclick: move |_| {
                                view_mode.set(ViewMode::Cards);
                                let _ = LocalStorage::set(VIEW_PREF_KEY, ViewMode::Cards.as_storage());
                            },
                            svg {
                                class: "w-3 h-3",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                view_box: "0 0 24 24",
                                rect { x: "3", y: "3", width: "7", height: "7" }
                                rect { x: "14", y: "3", width: "7", height: "7" }
                                rect { x: "14", y: "14", width: "7", height: "7" }
                                rect { x: "3", y: "14", width: "7", height: "7" }
                            }
                            " Cards"
                        }
                        button {
                            class: if *view_mode.read() == ViewMode::Table { "active" } else { "" },
                            onclick: move |_| {
                                view_mode.set(ViewMode::Table);
                                let _ = LocalStorage::set(VIEW_PREF_KEY, ViewMode::Table.as_storage());
                            },
                            svg {
                                class: "w-3 h-3",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                view_box: "0 0 24 24",
                                line { x1: "3", y1: "6", x2: "21", y2: "6" }
                                line { x1: "3", y1: "12", x2: "21", y2: "12" }
                                line { x1: "3", y1: "18", x2: "21", y2: "18" }
                            }
                            " Table"
                        }
                    }
                    div {
                        class: "filter-count",
                        "{filtered_systems.len()} shown"
                    }
                }
            }

            // Systems List (Cards or Table)
            if *loading.read() {
                div {
                    class: "empty",
                    style: "margin: 24px;",
                    "data-testid": "systems-loading-state",
                    div {
                        class: "mx-auto mb-3 animate-spin rounded-full h-8 w-8 border-b-2 border-blue-400"
                    }
                    h3 { "Loading systems" }
                    div { "Fetching fleet data from the API." }
                }
            } else if let Some(error_message) = load_error.read().clone() {
                div {
                    class: "empty",
                    style: "margin: 24px;",
                    "data-testid": "systems-error-state",
                    h3 { "Unable to load systems" }
                    div { "{error_message}" }
                }
            } else if filtered_systems.is_empty() {
                div {
                    class: "empty",
                    style: "margin: 24px;",
                    "data-testid": "systems-empty-state",
                    if has_active_filters {
                        h3 { "No systems match" }
                        div { "Try clearing a filter or changing the search." }
                    } else {
                        h3 { "No systems yet" }
                        div { "Use Add system to register your first managed machine." }
                    }
                }
            } else if *view_mode.read() == ViewMode::Cards {
                div {
                    class: "cards-grid",
                    "data-testid": "systems-cards",
                    for system in filtered_systems.clone() {
                        SystemCardV2 {
                            system: system.clone(),
                            compact: *is_compact.read(),
                            environment_colors: environment_color_pairs.clone(),
                            flake_context: flake_context.clone(),
                            on_open: move |_| {
                                let mut preview_system = preview_system.clone();
                                spawn(async move {
                                    let detail = load_system_detail_with_fallback(&system.id.to_string()).await;
                                    if let Some(detail) = detail.system {
                                        preview_system.set(Some(detail));
                                    }
                                });
                            },
                            on_remove: move |_| remove_system_by_id(local_systems, pending_remove, system.id),
                            on_update_key: move |_| update_key_for_system(local_systems, pending_update_key, system.id),
                            on_edit: move |_| {
                                let mut edit_modal_system = edit_modal_system.clone();
                                spawn(async move {
                                    let detail = load_system_detail_with_fallback(&system.id.to_string()).await;
                                    if let Some(detail) = detail.system {
                                        edit_modal_system.set(Some(detail));
                                    }
                                });
                            },
                            on_deploy: move |_| {
                                let mut deploy_modal_system = deploy_modal_system.clone();
                                spawn(async move {
                                    let detail = load_system_detail_with_fallback(&system.id.to_string()).await;
                                    if let Some(detail) = detail.system {
                                        match fetch_system_commits_via_api(system.id).await {
                                            Ok(commits_response) => {
                                                deploy_modal_system.set(Some((detail, commits_response.commits, commits_response.current_commit)));
                                            }
                                            Err(_) => {
                                                // Fall back to showing modal with empty commits
                                                deploy_modal_system.set(Some((detail, vec![], None)));
                                            }
                                        }
                                    }
                                });
                            },
                        }
                    }
                }
            } else {
                SystemsTable {
                    systems: filtered_systems.clone(),
                    compact: *is_compact.read(),
                    environment_colors: environment_color_pairs.clone(),
                    flake_context: flake_context.clone(),
                    on_remove: move |id| remove_system_by_id(local_systems, pending_remove, id),
                    on_update_key: move |id| update_key_for_system(local_systems, pending_update_key, id),
                    on_edit: move |id: uuid::Uuid| {
                        let mut edit_modal_system = edit_modal_system.clone();
                        spawn(async move {
                            let detail = load_system_detail_with_fallback(&id.to_string()).await;
                            if let Some(detail) = detail.system {
                                edit_modal_system.set(Some(detail));
                            }
                        });
                    },
                    on_deploy: move |id: uuid::Uuid| {
                        let mut deploy_modal_system = deploy_modal_system.clone();
                        spawn(async move {
                            let detail = load_system_detail_with_fallback(&id.to_string()).await;
                            if let Some(detail) = detail.system {
                                match fetch_system_commits_via_api(id).await {
                                    Ok(commits_response) => {
                                        deploy_modal_system.set(Some((detail, commits_response.commits, commits_response.current_commit)));
                                    }
                                    Err(_) => {
                                        deploy_modal_system.set(Some((detail, vec![], None)));
                                    }
                                }
                            }
                        });
                    },
                    on_open: move |id: uuid::Uuid| {
                        let mut preview_system = preview_system.clone();
                        spawn(async move {
                            let detail = load_system_detail_with_fallback(&id.to_string()).await;
                            if let Some(detail) = detail.system {
                                preview_system.set(Some(detail));
                            }
                        });
                    },
                    selected_id: selected_preview_id,
                }
            }

            // Side panel preview (design: detail peek drawer)
            if let Some(detail) = preview_system.read().clone() {
                SystemPreviewPanel {
                    detail: detail.clone(),
                    environment_colors: environment_color_pairs.clone(),
                    on_close: move |_| preview_system.set(None),
                    on_open_detail: move |_| {
                        preview_system.set(None);
                        nav.push(Route::SystemDetailView { id: detail.id.to_string() });
                    },
                    on_deploy: move |_| {
                        let detail_for_deploy = detail.clone();
                        let mut deploy_modal_system = deploy_modal_system.clone();
                        let mut preview_system = preview_system.clone();
                        spawn(async move {
                            match fetch_system_commits_via_api(detail_for_deploy.id).await {
                                Ok(commits_response) => {
                                    deploy_modal_system.set(Some((
                                        detail_for_deploy.clone(),
                                        commits_response.commits,
                                        commits_response.current_commit,
                                    )));
                                }
                                Err(_) => {
                                    deploy_modal_system
                                        .set(Some((detail_for_deploy.clone(), vec![], None)));
                                }
                            }
                            preview_system.set(None);
                        });
                    },
                }
            }

            // Remove Confirmation Dialog
            if let Some(system) = pending_remove.read().clone() {
                RemoveSystemDialog {
                    hostname: system.hostname.clone(),
                    on_cancel: move |_| pending_remove.set(None),
                    on_confirm: move |_| {
                        let system_id = system.id;
                        spawn(async move {
                            match deactivate_system_via_api(system_id).await {
                                Ok(_) => {
                                    let mut values = local_systems.read().clone();
                                    values.retain(|item| item.id != system_id);
                                    local_systems.set(values);
                                    pending_remove.set(None);
                                }
                                Err(error_message) => {
                                    api_notice.set(Some(error_message));
                                    pending_remove.set(None);
                                }
                            }
                        });
                    }
                }
            }

            // Update Public Key Modal
            if let Some(system) = pending_update_key.read().clone() {
                UpdatePublicKeyModal {
                    system_id: system.id,
                    hostname: system.hostname.clone(),
                    on_cancel: move |_| {
                        pending_update_key.set(None);
                        update_key_error.set(None);
                    },
                    on_confirm: move |new_public_key| {
                        let system_id = system.id;
                        spawn(async move {
                            match update_system_public_key_via_api(system_id, new_public_key).await {
                                Ok(message) => {
                                    // Success - close modal and maybe show a success toast
                                    pending_update_key.set(None);
                                    update_key_error.set(None);
                                    // TODO: Show success toast with message
                                }
                                Err(error_message) => {
                                    update_key_error.set(Some(error_message));
                                }
                            }
                        });
                    }
                }
            }

            // Edit System Modal
            if let Some(detail) = edit_modal_system.read().clone() {
                EditSystemModal {
                    system: detail.clone(),
                    flake_names: registered_flakes.clone(),
                    on_close: move |_| edit_modal_system.set(None),
                    on_save: move |request: crate::api::models::UpdateSystemRequest| {
                        let system_id = detail.id;
                        spawn(async move {
                            match update_system_via_api(
                                system_id,
                                request.hostname,
                                request.system_configuration_name,
                                request.environment,
                                request.flake_name,
                                request.deployment_policy,
                            ).await {
                                Ok(updated_detail) => {
                                    // Update local systems list
                                    let mut values = local_systems.read().clone();
                                    if let Some(pos) = values.iter().position(|s| s.id == system_id) {
                                        values[pos].hostname = updated_detail.hostname.clone();
                                        values[pos].system_configuration_name = updated_detail.system_configuration_name.clone();
                                        values[pos].environment = updated_detail.environment.clone();
                                        values[pos].deployment_policy = updated_detail.deployment_policy.clone();
                                        values[pos].flake_id = updated_detail.flake.as_ref().map(|flake| flake.id);
                                        local_systems.set(values);
                                    }
                                    edit_modal_system.set(None);
                                }
                                Err(error_message) => {
                                    // TODO: Show error in modal
                                    api_notice.set(Some(error_message));
                                    edit_modal_system.set(None);
                                }
                            }
                        });
                    }
                }
            }

            // Deploy System Modal
            if let Some((detail, commits, current_commit)) = deploy_modal_system.read().clone() {
                DeploySystemModal {
                    system_id: detail.id.to_string(),
                    hostname: detail.hostname.clone(),
                    deployment_policy: detail.deployment_policy.clone(),
                    commits: commits.clone(),
                    current_commit: current_commit.clone(),
                    on_close: move |_| {
                        deploy_modal_system.set(None);
                        deploy_error.set(None);
                    },
                    on_deploy: move |request: crate::api::models::DeploySystemRequest| {
                        let system_id = detail.id;
                        spawn(async move {
                            match deploy_system_via_api(system_id, request.commit_sha).await {
                                Ok(message) => {
                                    // Success - close modal
                                    deploy_modal_system.set(None);
                                    deploy_error.set(None);
                                    api_notice.set(Some(message));
                                }
                                Err(error_message) => {
                                    deploy_error.set(Some(error_message.clone()));
                                    api_notice.set(Some(error_message));
                                    deploy_modal_system.set(None);
                                }
                            }
                        });
                    }
                }
            }
        }
    }
}

#[component]
fn SystemPreviewPanel(
    detail: SystemDetail,
    #[props(default)] environment_colors: Vec<(String, String)>,
    on_close: EventHandler<()>,
    on_open_detail: EventHandler<()>,
    on_deploy: EventHandler<()>,
) -> Element {
    let mut now_tick = use_signal(chrono::Utc::now);
    use_effect(move || {
        spawn(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(1000).await;
                now_tick.set(chrono::Utc::now());
            }
        });
    });

    let now = now_tick();

    let status_color = match detail.health_status {
        HealthStatus::Healthy => "#34d399",
        HealthStatus::Warning => "#fbbf24",
        HealthStatus::Critical => "#f87171",
        HealthStatus::Offline => "#6b7280",
    };

    let heartbeat_interval_sec = 60_i64;
    let heartbeat_next_in_sec = detail
        .last_seen
        .map(|dt| 60.0 - now.signed_duration_since(dt).num_seconds() as f64)
        .unwrap_or(0.0);
    let last_heartbeat = detail
        .last_seen
        .map(|dt| format_relative_time_from(now, dt))
        .unwrap_or_else(|| "Never".to_string());

    let (env_fg, env_bg, env_border) = env_colors_for_badge(
        detail.environment.as_deref().unwrap_or("unknown"),
        &environment_colors,
    );

    let flake_name = detail
        .flake
        .as_ref()
        .map(|f| f.name.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let flake_commit = detail
        .flake
        .as_ref()
        .and_then(|f| f.latest_commit.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let nixos_version = detail
        .nixos_version
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let kernel = detail
        .kernel
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let cpu_brand = detail
        .hardware
        .cpu_brand
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let memory_text = detail
        .hardware
        .memory_gb
        .map(|m| format!("{:.1} GiB", m))
        .unwrap_or_else(|| "unknown".to_string());
    let primary_ip = detail
        .network
        .primary_ip
        .clone()
        .unwrap_or_else(|| "-".to_string());

    let deploy_variant = match detail.deployment_status {
        DeploymentStatus::UpToDate => ChipVariant::Healthy,
        DeploymentStatus::Behind => ChipVariant::Warning,
        DeploymentStatus::Ahead => ChipVariant::Info,
        DeploymentStatus::NeverDeployed
        | DeploymentStatus::NoCommitsAvailable
        | DeploymentStatus::Unknown => ChipVariant::Unknown,
    };

    let mut timeline = vec![
        (
            "System record updated".to_string(),
            "#34d399".to_string(),
            detail.updated_at,
        ),
        (
            "System registered".to_string(),
            "#a78bfa".to_string(),
            detail.created_at,
        ),
    ];
    if let Some(last_seen_at) = detail.last_seen {
        timeline.push((
            "Heartbeat received".to_string(),
            "#60a5fa".to_string(),
            last_seen_at,
        ));
    }
    timeline.sort_by(|a, b| b.2.cmp(&a.2));

    rsx! {
        div {
            class: "side-panel-backdrop",
            onclick: move |_| on_close.call(()),
        }
        aside {
            class: "side-panel",
            role: "dialog",
            "aria-modal": "true",

            div {
                class: "panel-head",
                div {
                    class: "panel-title",
                    h2 {
                        span { class: "status-dot", style: "--status-color: {status_color};" }
                        "{detail.hostname}"
                        Chip {
                            variant: match detail.health_status {
                                HealthStatus::Healthy => ChipVariant::Healthy,
                                HealthStatus::Warning => ChipVariant::Warning,
                                HealthStatus::Critical => ChipVariant::Critical,
                                HealthStatus::Offline => ChipVariant::Unknown,
                            },
                            show_dot: false,
                            "{detail.health_status.label()}"
                        }
                    }
                    span { class: "fqdn", "{detail.hostname}.local" }
                }
                button {
                    class: "btn-icon focus-ring",
                    "aria-label": "Close",
                    onclick: move |_| on_close.call(()),
                    svg {
                        class: "w-4 h-4",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        view_box: "0 0 24 24",
                        path { d: "M18 6L6 18M6 6l12 12" }
                    }
                }
            }

            div {
                class: "panel-body",
                section {
                    class: "panel-section",
                    div {
                        style: "display: flex; gap: 8px; flex-wrap: wrap;",
                        EnvBadge {
                            name: detail.environment.clone().unwrap_or_else(|| "unknown".to_string()),
                            fg: env_fg,
                            bg: env_bg,
                            border: env_border,
                        }
                        Chip {
                            variant: deploy_variant,
                            show_dot: false,
                            "{detail.deployment_status.label()}"
                        }
                        Chip {
                            variant: ChipVariant::Unknown,
                            show_dot: false,
                            "policy: {detail.deployment_policy}"
                        }
                    }
                }

                section {
                    class: "panel-section",
                    h3 { "Currently deployed" }
                    dl {
                        class: "kv-grid",
                        dt { "Flake" } dd { "{flake_name}" }
                        dt { "Commit" } dd { "{flake_commit}" }
                        dt { "NixOS" } dd { "{nixos_version}" }
                        dt { "Kernel" } dd { "{kernel}" }
                    }
                }

                section {
                    class: "panel-section",
                    h3 { "Host" }
                    dl {
                        class: "kv-grid",
                        dt { "Uptime" } dd { "{format_uptime(detail.hardware.uptime_secs)}" }
                        dt { "CPU" } dd { "{cpu_brand}" }
                        dt { "Memory" } dd { "{memory_text}" }
                        dt { "IPv4" } dd { "{primary_ip}" }
                        dt { "Last heartbeat" } dd { "{last_heartbeat}" }
                    }
                    div {
                        class: "hb-panel",
                        HeartbeatSpinner {
                            interval_sec: heartbeat_interval_sec,
                            next_in_sec: heartbeat_next_in_sec,
                            size: 56,
                            show_label: true,
                        }
                    }
                }

                section {
                    class: "panel-section",
                    h3 { "CVE exposure" }
                    div {
                        class: "cve-bar",
                        {
                            let total = (detail.cve_counts.total().max(1)) as f64;
                            rsx! {
                                if detail.cve_counts.critical > 0 {
                                    div { class: "cve-seg", style: "background: #f87171; width: {(detail.cve_counts.critical as f64 / total) * 100.0}%;" }
                                }
                                if detail.cve_counts.high > 0 {
                                    div { class: "cve-seg", style: "background: #fbbf24; width: {(detail.cve_counts.high as f64 / total) * 100.0}%;" }
                                }
                                if detail.cve_counts.medium > 0 {
                                    div { class: "cve-seg", style: "background: #9ca3af; width: {(detail.cve_counts.medium as f64 / total) * 100.0}%;" }
                                }
                                if detail.cve_counts.low > 0 {
                                    div { class: "cve-seg", style: "background: #4b5563; width: {(detail.cve_counts.low as f64 / total) * 100.0}%;" }
                                }
                            }
                        }
                    }
                    div {
                        class: "cve-legend",
                        span { class: "cve-legend-item", span { class: "cve-legend-swatch", style: "background: #f87171" } "{detail.cve_counts.critical} critical" }
                        span { class: "cve-legend-item", span { class: "cve-legend-swatch", style: "background: #fbbf24" } "{detail.cve_counts.high} high" }
                        span { class: "cve-legend-item", span { class: "cve-legend-swatch", style: "background: #9ca3af" } "{detail.cve_counts.medium} medium" }
                        span { class: "cve-legend-item", span { class: "cve-legend-swatch", style: "background: #4b5563" } "{detail.cve_counts.low} low" }
                    }
                }

                section {
                    class: "panel-section",
                    h3 { "Recent activity" }
                    div {
                        class: "timeline",
                        for (title, color, at) in timeline {
                            div {
                                class: "tl-item",
                                span { class: "tl-dot", style: "--status-color: {color};" }
                                div {
                                    class: "tl-body",
                                    div { class: "tl-title", "{title}" }
                                    div { class: "tl-meta", "{format_relative_time_from(now, at)}" }
                                }
                            }
                        }
                    }
                }
            }

            div {
                class: "panel-actions",
                button {
                    class: "btn btn-ghost focus-ring",
                    onclick: move |_| on_open_detail.call(()),
                    "Open full detail"
                }
                button {
                    class: "btn btn-ghost focus-ring",
                    "Evaluate"
                }
                button {
                    class: "btn btn-primary focus-ring",
                    onclick: move |_| on_deploy.call(()),
                    "Deploy"
                }
            }
        }
    }
}

fn format_uptime(seconds: Option<i64>) -> String {
    let Some(total) = seconds else {
        return "unknown".to_string();
    };
    let days = total / 86_400;
    let hours = (total % 86_400) / 3_600;
    if days > 0 {
        format!("{}d {}h", days, hours)
    } else {
        let minutes = (total % 3_600) / 60;
        format!("{}h {}m", hours, minutes)
    }
}

fn format_relative_time_from(
    now: chrono::DateTime<chrono::Utc>,
    at: chrono::DateTime<chrono::Utc>,
) -> String {
    let diff = now.signed_duration_since(at);
    if diff.num_seconds() < 60 {
        format!("{}s ago", diff.num_seconds().max(0))
    } else if diff.num_minutes() < 60 {
        format!("{}m ago", diff.num_minutes().max(0))
    } else if diff.num_hours() < 24 {
        format!("{}h ago", diff.num_hours().max(0))
    } else {
        format!("{}d ago", diff.num_days().max(0))
    }
}

fn env_colors_for_badge(
    env_name: &str,
    environment_colors: &[(String, String)],
) -> (String, String, String) {
    if let Some((_, color_hex)) = environment_colors
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(env_name))
    {
        let fg = normalize_color_hex(color_hex);
        return (fg.clone(), with_alpha(&fg, 0.10), with_alpha(&fg, 0.25));
    }

    match env_name.to_lowercase().as_str() {
        "production" | "prod" => (
            "#f87171".to_string(),
            "rgba(220,38,38,0.10)".to_string(),
            "rgba(248,113,113,0.25)".to_string(),
        ),
        "staging" | "stage" => (
            "#fbbf24".to_string(),
            "rgba(217,119,6,0.10)".to_string(),
            "rgba(251,191,36,0.25)".to_string(),
        ),
        "dev" | "development" => (
            "#60a5fa".to_string(),
            "rgba(37,99,235,0.10)".to_string(),
            "rgba(96,165,250,0.25)".to_string(),
        ),
        "edge" => (
            "#2dd4bf".to_string(),
            "rgba(15,118,110,0.12)".to_string(),
            "rgba(45,212,191,0.25)".to_string(),
        ),
        "lab" => (
            "#a78bfa".to_string(),
            "rgba(124,58,237,0.10)".to_string(),
            "rgba(167,139,250,0.25)".to_string(),
        ),
        _ => (
            "#6b7280".to_string(),
            "rgba(107,114,128,0.16)".to_string(),
            "rgba(107,114,128,0.25)".to_string(),
        ),
    }
}

/// Generate an OSCAL-style System Security Plan (SSP) component inventory JSON
/// and trigger a browser download.
///
/// Format follows NIST OSCAL SSP schema (simplified inventory subset).
/// See: https://pages.nist.gov/OSCAL/resources/concepts/layer/implementation/ssp/
pub fn export_systems_oscal(systems: &[crate::api::models::SystemSummary]) {
    let now = js_sys::Date::new_0();
    let iso_now = now.to_iso_string().as_string().unwrap_or_default();

    // Build OSCAL SSP inventory JSON as a string
    let mut components = Vec::new();
    for sys in systems {
        let env = sys.environment.as_deref().unwrap_or("unknown").to_string();
        let health = sys.health_status.label();
        let deploy = sys.deployment_status.label();
        let last_seen = sys
            .last_seen
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| "never".to_string());
        let nixos = sys
            .nixos_version
            .as_deref()
            .unwrap_or("unknown")
            .to_string();
        let ip = sys.primary_ip.as_deref().unwrap_or("unknown").to_string();
        let cve_crit = sys.cve_counts.critical;
        let cve_high = sys.cve_counts.high;
        let cve_med = sys.cve_counts.medium;
        let cve_low = sys.cve_counts.low;
        let cve_total = sys.cve_counts.total();
        let policy = &sys.deployment_policy;
        let uuid = sys.id;

        components.push(format!(
            r#"        {{
          "uuid": "{uuid}",
          "type": "software",
          "title": "{hostname}",
          "description": "NixOS system managed by Crystal Forge",
          "props": [
            {{ "name": "hostname",             "value": "{hostname}" }},
            {{ "name": "environment",          "value": "{env}" }},
            {{ "name": "ip-address",           "value": "{ip}" }},
            {{ "name": "os-name",              "value": "NixOS" }},
            {{ "name": "os-version",           "value": "{nixos}" }},
            {{ "name": "health-status",        "value": "{health}" }},
            {{ "name": "deployment-status",    "value": "{deploy}" }},
            {{ "name": "deployment-policy",    "value": "{policy}" }},
            {{ "name": "last-heartbeat",       "value": "{last_seen}" }},
            {{ "name": "cve-critical",         "value": "{cve_crit}" }},
            {{ "name": "cve-high",             "value": "{cve_high}" }},
            {{ "name": "cve-medium",           "value": "{cve_med}" }},
            {{ "name": "cve-low",              "value": "{cve_low}" }},
            {{ "name": "cve-total",            "value": "{cve_total}" }}
          ],
          "status": {{ "state": "{health}" }}
        }}"#,
            uuid = uuid,
            hostname = sys.hostname,
            env = env,
            ip = ip,
            nixos = nixos,
            health = health,
            deploy = deploy,
            policy = policy,
            last_seen = last_seen,
            cve_crit = cve_crit,
            cve_high = cve_high,
            cve_med = cve_med,
            cve_low = cve_low,
            cve_total = cve_total,
        ));
    }

    let components_json = components.join(",\n");
    let total = systems.len();
    let critical_hosts = systems.iter().filter(|s| s.cve_counts.critical > 0).count();
    let total_crit_cves: i64 = systems.iter().map(|s| s.cve_counts.critical).sum();

    let json = format!(
        r#"{{
  "oscal-version": "1.1.2",
  "metadata": {{
    "title": "Crystal Forge — System Inventory Report",
    "last-modified": "{iso_now}",
    "version": "1.0",
    "oscal-version": "1.1.2",
    "remarks": "Automated system inventory exported from Crystal Forge fleet management. Contains {total} managed NixOS systems. {critical_hosts} host(s) with critical CVEs ({total_crit_cves} critical CVEs total)."
  }},
  "system-security-plan": {{
    "uuid": "cf-export-{ts}",
    "system-characteristics": {{
      "system-name": "Crystal Forge Managed Fleet",
      "description": "NixOS fleet managed by Crystal Forge",
      "status": {{ "state": "operational" }},
      "system-information": {{
        "information-types": [
          {{
            "title": "Fleet Inventory",
            "description": "System inventory and security posture data"
          }}
        ]
      }}
    }},
    "system-implementation": {{
      "users": [],
      "components": [
{components_json}
      ]
    }}
  }}
}}"#,
        iso_now = iso_now,
        total = total,
        critical_hosts = critical_hosts,
        total_crit_cves = total_crit_cves,
        ts = now.get_time() as u64,
        components_json = components_json,
    );

    // Trigger browser download
    trigger_json_download(
        &json,
        &format!("cf-system-inventory-{ts}.json", ts = now.get_time() as u64),
    );
}

fn trigger_json_download(content: &str, filename: &str) {
    // Build a data: URI and use inline JS to trigger the download.
    // This avoids needing web_sys::Blob/Url/HtmlAnchorElement Cargo features.
    let encoded = js_sys::encode_uri_component(content)
        .as_string()
        .unwrap_or_default();
    let data_uri = format!("data:application/json;charset=utf-8,{}", encoded);

    // Use js_sys::Function to call a tiny JS snippet that creates and clicks
    // a temporary anchor element — the most reliable cross-browser approach.
    let js_code = format!(
        r#"
        (function() {{
            var a = document.createElement('a');
            a.href = '{uri}';
            a.download = '{name}';
            a.style = 'display:none';
            document.body.appendChild(a);
            a.click();
            document.body.removeChild(a);
        }})();
        "#,
        uri = data_uri.replace('\'', "\\'"),
        name = filename.replace('\'', "\\'"),
    );

    let func = js_sys::Function::new_no_args(&js_code);
    func.call0(&wasm_bindgen::JsValue::NULL).ok();
}
