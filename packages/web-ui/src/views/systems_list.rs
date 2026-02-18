//! Systems list view with table/card toggle.

use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
use std::rc::Rc;
use uuid::Uuid;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen::JsCast;
use web_sys::{window, Node};

use crate::api::models::{
    CveSummary, DeploymentStatus, FlakeSummary, HealthStatus, PipelineStage, SystemSummary,
};
use crate::components::filters::{
    DeploymentFilterDropdown, EnvironmentFilterDropdown, HealthFilterDropdown, ViewMode, ViewToggle,
};
use crate::components::forms::{validate_new_system, AddSystemForm, NewSystemDraft};
use crate::components::layout::Card;
use crate::components::modals::{
    generate_key_pair, GeneratedKeyPair, KeyPairModal, RemoveSystemDialog,
};
use crate::components::system::SystemCard;
use crate::components::tables::SystemsTable;
use crate::routes::Route;
use crate::theme;
use chrono::{Duration, Utc};

const VIEW_PREF_KEY: &str = "crystal_forge.systems.view";

/// Systems list with toggles and filters.
#[component]
pub fn SystemsListView() -> Element {
    let stored_view = LocalStorage::get::<String>(VIEW_PREF_KEY).ok();
    let mut view_mode = use_signal(|| ViewMode::from_storage(stored_view));
    let query_view = prefers_view_from_query();
    let mut open_dropdown = use_signal(|| None::<String>);
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

    // Data state
    let mut systems = use_signal(mock_systems);
    let mut show_add_form = use_signal(|| false);
    let mut add_error = use_signal(|| None::<String>);
    let mut draft = use_signal(NewSystemDraft::new);
    let mut pending_remove = use_signal(|| None::<SystemSummary>);
    let mut show_key_modal = use_signal(|| false);
    let mut generated_keys = use_signal(|| None::<GeneratedKeyPair>);

    let current_systems = systems.read().clone();
    let environments = unique_environments(&current_systems);
    let registered_flakes = unique_registered_flakes();

    let filtered_systems: Vec<SystemSummary> = current_systems
        .into_iter()
        .filter(|system| matches_environment(system, &environment_filter.read()))
        .filter(|system| matches_health(system, &health_filter.read()))
        .filter(|system| matches_deployment(system, &deployment_filter.read()))
        .filter(|system| matches_search(system, &search.read()))
        .collect();

    let registered_flakes_for_submit = registered_flakes.clone();

    rsx! {
        div {
            class: "space-y-6",
            id: "{container_id}",
            header {
                class: "flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between",
                div {
                    h1 { class: "{theme::typography::PAGE_TITLE}", "Systems" }
                    p { class: "text-sm {theme::text::SECONDARY}", "Manage fleet systems and deployment status." }
                }
                div {
                    class: "flex items-center gap-3",
                    button {
                        class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                        onclick: move |_| {
                            let next = !*show_add_form.read();
                            show_add_form.set(next);
                            add_error.set(None);
                        },
                        if *show_add_form.read() { "Close" } else { "Add System" }
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
                        if let Err(message) = validate_new_system(&next, &systems.read(), &registered_flakes_for_submit) {
                            add_error.set(Some(message));
                            return;
                        }

                        let new_item = SystemSummary {
                            id: Uuid::new_v4(),
                            hostname: next.hostname.trim().to_string(),
                            environment: normalize_optional(&next.environment),
                            primary_ip: None,
                            health_status: HealthStatus::Healthy,
                            deployment_status: DeploymentStatus::NeverDeployed,
                            pipeline_stage: Some(PipelineStage::ReadyForBuild),
                            cve_counts: CveSummary { critical: 0, high: 0, medium: 0, low: 0 },
                            nixos_version: None,
                            last_seen: Some(Utc::now()),
                            deployment_policy: normalize_policy(&next.deployment_policy),
                        };

                        let mut values = systems.read().clone();
                        values.push(new_item);
                        values.sort_by(|a, b| a.hostname.to_lowercase().cmp(&b.hostname.to_lowercase()));
                        systems.set(values);
                        draft.set(NewSystemDraft::new());
                        add_error.set(None);
                        show_add_form.set(false);
                    },
                    on_generate_keys: move |_| {
                        generated_keys.set(Some(generate_key_pair()));
                        show_key_modal.set(true);
                    },
                    environments: environments.clone(),
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
                class: "relative z-[2000] grid grid-cols-1 lg:grid-cols-4 gap-4",
                style: "position: sticky; top: 0;",
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
                        div {
                            class: "space-y-2",
                            SystemCard { system: system.clone() }
                            div {
                                class: "px-1 flex justify-end",
                                button {
                                    class: "text-xs text-red-400 hover:text-red-300 px-2 py-1 rounded hover:bg-red-500/10 transition-colors",
                                    onclick: move |_| remove_system_by_id(systems, pending_remove, system.id),
                                    "Remove"
                                }
                            }
                        }
                    }
                }
            } else {
                SystemsTable {
                    systems: filtered_systems.clone(),
                    on_remove: move |id| remove_system_by_id(systems, pending_remove, id),
                }
            }

            // Remove Confirmation Dialog
            if let Some(system) = pending_remove.read().clone() {
                RemoveSystemDialog {
                    hostname: system.hostname.clone(),
                    on_cancel: move |_| pending_remove.set(None),
                    on_confirm: move |_| {
                        let mut values = systems.read().clone();
                        values.retain(|item| item.id != system.id);
                        systems.set(values);
                        pending_remove.set(None);
                    }
                }
            }
        }
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

fn remove_system_by_id(
    systems: Signal<Vec<SystemSummary>>,
    mut pending_remove: Signal<Option<SystemSummary>>,
    system_id: Uuid,
) {
    let target = systems
        .read()
        .iter()
        .find(|item| item.id == system_id)
        .cloned();
    if let Some(system) = target {
        pending_remove.set(Some(system));
    }
}

fn normalize_optional(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_policy(value: &str) -> String {
    let normalized = value.trim().to_lowercase();
    if normalized.is_empty() {
        "manual".to_string()
    } else {
        normalized
    }
}

fn matches_environment(system: &SystemSummary, filters: &[String]) -> bool {
    if filters.is_empty() {
        return true;
    }
    system
        .environment
        .as_deref()
        .is_some_and(|env| filters.iter().any(|f| env.eq_ignore_ascii_case(f)))
}

fn matches_health(system: &SystemSummary, filters: &[HealthStatus]) -> bool {
    filters.is_empty() || filters.contains(&system.health_status)
}

fn matches_deployment(system: &SystemSummary, filters: &[DeploymentStatus]) -> bool {
    filters.is_empty() || filters.contains(&system.deployment_status)
}

fn matches_search(system: &SystemSummary, search: &str) -> bool {
    if search.is_empty() {
        return true;
    }
    system
        .hostname
        .to_lowercase()
        .contains(&search.to_lowercase())
}

fn unique_environments(systems: &[SystemSummary]) -> Vec<String> {
    let mut envs: Vec<String> = systems
        .iter()
        .filter_map(|s| s.environment.clone())
        .collect();
    envs.sort();
    envs.dedup();
    envs
}

fn unique_registered_flakes() -> Vec<String> {
    // TODO: Fetch from API
    vec![
        "infrastructure".to_string(),
        "workstations".to_string(),
        "edge-nodes".to_string(),
    ]
}

fn prefers_view_from_query() -> Option<ViewMode> {
    // TODO: Parse URL query params
    None
}

// =============================================================================
// Mock Data
// =============================================================================

fn mock_systems() -> Vec<SystemSummary> {
    let now = Utc::now();
    vec![
        SystemSummary {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            hostname: "atlas-01".to_string(),
            environment: Some("production".to_string()),
            primary_ip: Some("10.0.1.10".to_string()),
            health_status: HealthStatus::Healthy,
            deployment_status: DeploymentStatus::UpToDate,
            pipeline_stage: Some(PipelineStage::BuildComplete),
            cve_counts: CveSummary {
                critical: 0,
                high: 2,
                medium: 5,
                low: 12,
            },
            nixos_version: Some("24.11".to_string()),
            last_seen: Some(now - Duration::minutes(5)),
            deployment_policy: "auto_latest".to_string(),
        },
        SystemSummary {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap(),
            hostname: "atlas-02".to_string(),
            environment: Some("production".to_string()),
            primary_ip: Some("10.0.1.11".to_string()),
            health_status: HealthStatus::Healthy,
            deployment_status: DeploymentStatus::Behind,
            pipeline_stage: Some(PipelineStage::ReadyForDeploy),
            cve_counts: CveSummary {
                critical: 1,
                high: 3,
                medium: 8,
                low: 15,
            },
            nixos_version: Some("24.11".to_string()),
            last_seen: Some(now - Duration::minutes(10)),
            deployment_policy: "manual".to_string(),
        },
        SystemSummary {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000003").unwrap(),
            hostname: "staging-01".to_string(),
            environment: Some("staging".to_string()),
            primary_ip: Some("10.0.2.10".to_string()),
            health_status: HealthStatus::Warning,
            deployment_status: DeploymentStatus::Behind,
            pipeline_stage: Some(PipelineStage::ReadyForDeploy),
            cve_counts: CveSummary {
                critical: 0,
                high: 0,
                medium: 2,
                low: 5,
            },
            nixos_version: Some("24.11".to_string()),
            last_seen: Some(now - Duration::hours(1)),
            deployment_policy: "manual".to_string(),
        },
        SystemSummary {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000004").unwrap(),
            hostname: "dev-box".to_string(),
            environment: Some("development".to_string()),
            primary_ip: Some("10.0.3.20".to_string()),
            health_status: HealthStatus::Offline,
            deployment_status: DeploymentStatus::NeverDeployed,
            pipeline_stage: Some(PipelineStage::ReadyForBuild),
            cve_counts: CveSummary {
                critical: 0,
                high: 0,
                medium: 0,
                low: 0,
            },
            nixos_version: None,
            last_seen: Some(now - Duration::days(3)),
            deployment_policy: "manual".to_string(),
        },
    ]
}
