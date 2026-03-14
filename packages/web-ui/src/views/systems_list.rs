//! Systems list view with table/card toggle.

use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
use std::rc::Rc;
use uuid::Uuid;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;
use web_sys::{Node, window};

use crate::api::models::{
    CveSummary, DeploymentStatus, HealthStatus, PipelineStage, SystemSummary, SystemsListParams,
};
use crate::components::filters::{
    DeploymentFilterDropdown, EnvironmentFilterDropdown, HealthFilterDropdown, ViewMode, ViewToggle,
};
use crate::components::forms::{AddSystemForm, NewSystemDraft, validate_new_system};
use crate::components::layout::Card;
use crate::components::modals::{
    GeneratedKeyPair, KeyPairModal, RemoveSystemDialog, UpdatePublicKeyModal, generate_key_pair,
};
use crate::components::system::SystemCard;
use crate::components::tables::SystemsTable;
use crate::environments::adapter::load_environment_names_with_fallback;
use crate::routes::Route;
use crate::systems::adapter::{
    create_system_via_api, deactivate_system_via_api, fallback_flake_names, fallback_systems,
    load_flake_names_with_fallback, load_systems_with_fallback, update_system_public_key_via_api,
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
    normalize_policy, prefers_view_from_query, remove_system_by_id, unique_environments,
    update_key_for_system,
};

const VIEW_PREF_KEY: &str = "crystal_forge.systems.view";

/// Systems list with toggles and filters.
#[component]
pub fn SystemsListView() -> Element {
    let nav = navigator();

    let stored_view = LocalStorage::get::<String>(VIEW_PREF_KEY).ok();
    let mut view_mode = use_signal(|| ViewMode::from_storage(stored_view));
    let query_view = prefers_view_from_query();
    let open_dropdown = use_signal(|| None::<String>);
    let container_id = use_memo(|| format!("systems-filters-{}", uuid::Uuid::new_v4()));
    let container_id_value = Rc::new(container_id.read().clone());

    use_effect(move || {
        if let Some(mode) = query_view {
            view_mode.set(mode);
            let _ = LocalStorage::set(VIEW_PREF_KEY, mode.as_storage());
        }
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
    let flake_names_resource =
        use_resource(move || async move { load_flake_names_with_fallback().await });

    // Local mutable state for systems (allows client-side add/remove until backend supports it)
    let mut local_systems = use_signal(fallback_systems);
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
            api_notice.set(result.notice.clone());
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
    let mut show_key_modal = use_signal(|| false);
    let mut generated_keys = use_signal(|| None::<GeneratedKeyPair>);
    let mut update_key_error = use_signal(|| None::<String>);
    let mut onboarding_agent_reminder = use_signal(|| None::<String>);

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

    let filtered_systems: Vec<SystemSummary> = current_systems
        .into_iter()
        .filter(|system| matches_environment(system, &environment_filter.read()))
        .filter(|system| matches_health(system, &health_filter.read()))
        .filter(|system| matches_deployment(system, &deployment_filter.read()))
        .filter(|system| matches_search(system, &search.read()))
        .collect();

    let registered_flakes_for_submit = registered_flakes.clone();

    let from_setup = use_signal(came_from_setup);

    rsx! {
        div {
            class: "space-y-6",
            id: "{container_id}",

            if from_setup() {
                div {
                    "data-testid": "setup-coach-systems-callout",
                    style: "background:rgba(109,40,217,0.2); border:1px solid rgba(139,92,246,0.5); border-radius:8px; padding:12px 16px;",
                    div {
                        style: "display:flex; flex-direction:column; gap:6px;",
                        p { style: "color:#e9d5ff; font-size:12px; font-weight:700; margin:0; letter-spacing:0.03em; text-transform:uppercase;", "Setup Tour - Step 5 of 6" }
                        p { style: "color:#e9d5ff; font-size:14px; font-weight:600; margin:0;", "Register a system and its agent" }
                        p { style: "color:#ddd6fe; font-size:13px; margin:0;", "Use Add System to register a machine in this fleet and connect it to environment + flake." }
                        p { style: "color:#c4b5fd; font-size:12px; margin:0;", "Agents are lightweight clients installed on systems so Crystal Forge can evaluate and apply deployments." }
                    }
                }
            }

            if let Some(ref reminder) = *onboarding_agent_reminder.read() {
                div {
                    "data-testid": "setup-coach-agent-runtime-reminder",
                    style: "background:rgba(30,58,138,0.22); border:1px solid rgba(96,165,250,0.55); border-radius:10px; padding:12px 16px;",
                    p { style: "margin:0; color:#dbeafe; font-weight:700; font-size:13px; letter-spacing:0.02em; text-transform:uppercase;", "Agent activation required" }
                    p { style: "margin:6px 0 0 0; color:#bfdbfe; font-size:13px;", "{reminder}" }
                }
            }

            // API fallback notice banner (shown when using mock data)
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

            // Loading spinner (shown during initial fetch)
            if *loading.read() {
                div {
                    class: "flex items-center justify-center py-12",
                    div {
                        class: "animate-spin rounded-full h-8 w-8 border-b-2 border-blue-400"
                    }
                }
            }

            header {
                class: "flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between",
                div {
                    h1 { class: "{theme::typography::PAGE_TITLE}", "Systems" }
                    p { class: "text-sm {theme::text::SECONDARY}", "Manage fleet systems and deployment status." }
                }
                div {
                    class: "flex items-center gap-3",
                    div {
                        class: "relative z-40",
                        button {
                            class: if from_setup() && !*show_add_form.read() {
                                "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN} animate-pulse ring-2 ring-violet-300/70 ring-offset-2 ring-offset-slate-950"
                            } else {
                                "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}"
                            },
                            onclick: move |_| {
                                let next = !*show_add_form.read();
                                show_add_form.set(next);
                                add_error.set(None);
                            },
                            if *show_add_form.read() { "Close" } else { "Add System" }
                        }
                        if from_setup() && !*show_add_form.read() {
                            div {
                                "data-testid": "setup-coach-systems-target-callout",
                                style: "position:absolute; z-index:70; right:0; top:calc(100% + 10px); background:rgba(30,41,59,0.96); border:1px solid rgba(167,139,250,0.6); border-radius:10px; padding:8px 10px; color:#ddd6fe; font-size:12px; width:220px; box-shadow:0 10px 24px rgba(15,23,42,0.45);",
                                div {
                                    style: "position:absolute; top:-6px; right:18px; width:10px; height:10px; background:rgba(30,41,59,0.96); border-left:1px solid rgba(167,139,250,0.6); border-top:1px solid rgba(167,139,250,0.6); transform:rotate(45deg);"
                                }
                                p { style: "margin:0; color:#e9d5ff; font-weight:600;", "Next action" }
                                p { style: "margin:2px 0 0 0;", "Click Add System to register your first managed machine." }
                            }
                        }
                    }
                    ViewToggle {
                        view_mode: *view_mode.read(),
                        on_change: move |mode| {
                            view_mode.set(mode);
                            let _ = LocalStorage::set(VIEW_PREF_KEY, mode.as_storage());
                        }
                    }
                }
            }

            // Add System Form
            if *show_add_form.read() {
                AddSystemForm {
                    draft: draft,
                    error: add_error,
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

                        // Call backend API to create the system
                        spawn(async move {
                            match create_system_via_api(
                                next.hostname.trim().to_string(),
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
                                        environment: detail.environment,
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
                                    if from_setup_active {
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
                    environments: dropdown_environments,
                    flake_names: registered_flakes.clone(),
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

            // Filters Bar
            div {
                class: "grid grid-cols-1 lg:grid-cols-4 gap-4",
                input {
                    class: "rounded-lg px-4 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                    r#type: "search",
                    placeholder: "Search hostname...",
                    value: "{search.read()}",
                    oninput: move |evt| search.set(evt.value()),
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
            }

            // Systems List (Cards or Table)
            if filtered_systems.is_empty() {
                Card {
                    title: Some("No systems".to_string()),
                    children: rsx! {
                        p { class: "{theme::text::SECONDARY}", "No systems matched your filters." }
                    }
                }
            } else if *view_mode.read() == ViewMode::Cards {
                div {
                    class: "grid grid-cols-1 xl:grid-cols-2 gap-6",
                    "data-testid": "systems-cards",
                    for system in filtered_systems.clone() {
                        SystemCard {
                            system: system.clone(),
                            on_remove: move |_| remove_system_by_id(local_systems, pending_remove, system.id),
                            on_update_key: move |_| update_key_for_system(local_systems, pending_update_key, system.id),
                        }
                    }
                }
            } else {
                SystemsTable {
                    systems: filtered_systems.clone(),
                    on_remove: move |id| remove_system_by_id(local_systems, pending_remove, id),
                    on_update_key: move |id| update_key_for_system(local_systems, pending_update_key, id),
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
        }
    }
}

// Mock data has been moved to `crate::systems::adapter::fallback_systems`.
