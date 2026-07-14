//! Systems list view with table/card toggle.

use std::collections::HashMap;

use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
use uuid::Uuid;

use crate::alerts::{acknowledge, attention_row_class, dismiss_attention_item, should_flash};

use crate::api::client::set_setup_wizard_agent_acknowledged;
use crate::api::models::{
    DeploymentStatus, HealthStatus, SystemDetail, SystemHistoryEntry, SystemSummary,
    SystemsListParams,
};
use crate::components::environments::{normalize_color_hex, with_alpha};
use crate::components::filters::ViewMode;
use crate::components::forms::{AddSystemForm, NewSystemDraft, validate_new_system};
use crate::components::heartbeat_spinner::HeartbeatSpinner;
use crate::components::icon::{Icon, IconName};
use crate::components::modals::{
    GeneratedKeyPair, KeyPairModal, RemoveSystemDialog, UpdatePublicKeyModal, generate_key_pair,
};
use crate::components::notifications::{AlertBanner, AlertSeverity};
use crate::components::system::{
    EditSystemModal, PendingDeployBanner, SystemCardV2, deployment_state_label,
};
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
    create_system_via_api, deactivate_system_via_api, fallback_flake_names,
    load_flake_context_with_fallback, load_flake_names_with_fallback,
    load_system_deployment_progress_with_fallback, load_system_detail_with_fallback,
    load_system_history_with_fallback, load_systems_with_fallback,
    update_system_public_key_via_api, update_system_via_api,
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
    matches_environment, matches_flake, matches_search, matches_status, normalize_optional,
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

#[derive(Debug, Clone, PartialEq)]
struct ActivityRow {
    title: String,
    sub: Option<String>,
    color: &'static str,
    icon: IconName,
    timestamp: chrono::DateTime<chrono::Utc>,
}

fn activity_row_from_history(entry: &SystemHistoryEntry) -> ActivityRow {
    let event_kind = entry.event_kind.as_str();
    let outcome = entry.outcome.as_str();
    let title = match (event_kind, outcome) {
        ("cf_deployment", "started") => "Deployment started".to_string(),
        ("cf_deployment", "failed") => "Deploy failed".to_string(),
        ("cf_deployment", _) => entry
            .generation
            .map(|generation| format!("Deployed #{generation}"))
            .unwrap_or_else(|| "Deployed".to_string()),
        ("local_rebuild", _) => {
            if entry.reconciled {
                "Local rebuild (reconciled)".to_string()
            } else {
                "Local rebuild (out of band)".to_string()
            }
        }
        ("restart", _) => "System restarted".to_string(),
        ("agent_restart", _) => "Agent restarted".to_string(),
        _ => entry
            .title
            .clone()
            .unwrap_or_else(|| entry.change_reason.clone()),
    };
    let sub = entry
        .commit_hash
        .clone()
        .or_else(|| entry.store_path.clone())
        .or_else(|| entry.title.clone());
    let (color, icon) = match (event_kind, outcome, entry.reconciled) {
        ("cf_deployment", "failed", _) => ("#f87171", IconName::X),
        ("cf_deployment", "started", _) => ("#60a5fa", IconName::Deploy),
        ("cf_deployment", _, _) => ("#a78bfa", IconName::Deploy),
        ("local_rebuild", _, true) => ("#60a5fa", IconName::Edit),
        ("local_rebuild", _, false) => ("#fbbf24", IconName::Warn),
        ("restart", _, _) | ("agent_restart", _, _) => ("#60a5fa", IconName::Power),
        _ => ("#9ca3af", IconName::History),
    };

    ActivityRow {
        title,
        sub,
        color,
        icon,
        timestamp: entry.occurred_at.unwrap_or(entry.timestamp),
    }
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
    let container_id = use_memo(|| format!("systems-filters-{}", uuid::Uuid::new_v4()));

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

    // Filter state — single-select values matching the design's filter bar
    // ("all" mirrors the design's "All environments/statuses/flakes" options).
    let mut search = use_signal(String::new);
    let mut environment_filter = use_signal(|| "all".to_string());
    let mut status_filter = use_signal(|| "all".to_string());
    let mut flake_filter = use_signal(|| "all".to_string());

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

    // Sync attention count and acknowledge the "systems" sidebar badge (TASK-385).
    // Placed after `local_systems` so the closure can capture it.
    use_effect(move || {
        let attention_count = local_systems
            .read()
            .iter()
            .filter(|s| {
                matches!(
                    s.health_status,
                    HealthStatus::Critical | HealthStatus::Offline
                )
            })
            .count() as i64;
        acknowledge("systems", attention_count);
    });

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
    let mut removing_in_progress = use_signal(|| false);
    let mut pending_update_key = use_signal(|| None::<SystemSummary>);
    let mut show_key_modal = use_signal(|| false);
    let mut generated_keys = use_signal(|| None::<GeneratedKeyPair>);
    let mut update_key_error = use_signal(|| None::<String>);
    let mut onboarding_agent_reminder = use_signal(|| None::<String>);

    // New modal state for edit
    let mut edit_modal_system = use_signal(|| None::<SystemDetail>);
    let mut edit_modal_error = use_signal(|| None::<String>);
    let mut edit_remove_in_progress = use_signal(|| false);
    let mut preview_system = use_signal(|| None::<SystemDetail>);
    let selected_preview_id = preview_system.read().as_ref().map(|d| d.id);

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
        .filter(|system| {
            let flake_info = system
                .flake_id
                .and_then(|id| flake_context.iter().find(|(flake_id, ..)| *flake_id == id));
            let flake_name = flake_info.map(|(_, name, _, _)| name.as_str());
            let flake_commit = flake_info.and_then(|(_, _, _, commit)| commit.as_deref());

            matches_environment(system, &environment_filter.read())
                && matches_status(system, &status_filter.read())
                && matches_flake(flake_name, &flake_filter.read())
                && matches_search(system, &search.read(), flake_name, flake_commit)
        })
        .cloned()
        .collect();
    let has_active_filters = !search.read().trim().is_empty()
        || *environment_filter.read() != "all"
        || *status_filter.read() != "all"
        || *flake_filter.read() != "all";
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

    // Attention/flash state for alerting systems (TASK-385 follow-up).
    let has_attention_systems = filtered_systems.iter().any(|s| {
        matches!(
            s.health_status,
            HealthStatus::Critical | HealthStatus::Offline
        )
    });
    let flash_global = should_flash("systems", has_attention_systems);
    let mut attention_classes: HashMap<Uuid, String> = HashMap::new();
    for system in &filtered_systems {
        let is_attention = matches!(
            system.health_status,
            HealthStatus::Critical | HealthStatus::Offline
        );
        let system_key = system.id.to_string();
        let ac = attention_row_class(
            "",
            "systems",
            &system_key,
            is_attention,
            is_attention && flash_global,
        );
        if !ac.is_empty() {
            attention_classes.insert(system.id, ac);
        }
    }

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
                        Icon { name: IconName::Download, size: 14 }
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
                            Icon { name: IconName::Plus, size: 14 }
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
                                        fqdn: detail.fqdn,
                                        heartbeat_interval_secs: detail.heartbeat_interval_secs,
                                        effective_heartbeat_interval_secs: detail
                                            .effective_heartbeat_interval_secs,
                                        boot_id: detail.boot_id,
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
                    // Environment select (design: "All environments")
                    select {
                        class: "input filter-select focus-ring",
                        style: "width: auto;",
                        "aria-label": "Filter by environment",
                        value: "{environment_filter.read()}",
                        onchange: move |evt| environment_filter.set(evt.value()),
                        option { value: "all", "All environments" }
                        for env in dropdown_environments.clone() {
                            option {
                                value: "{env}",
                                selected: *environment_filter.read() == env,
                                "{env}"
                            }
                        }
                    }
                    // Status select (design: "All statuses")
                    select {
                        class: "input filter-select focus-ring",
                        style: "width: auto;",
                        "aria-label": "Filter by status",
                        value: "{status_filter.read()}",
                        onchange: move |evt| status_filter.set(evt.value()),
                        option { value: "all", "All statuses" }
                        option { value: "online", "Online" }
                        option { value: "warning", "Warning / drift" }
                        option { value: "critical", "Critical" }
                        option { value: "offline", "Offline" }
                    }
                    // Flake select (design: "All flakes")
                    select {
                        class: "input filter-select focus-ring",
                        style: "width: auto;",
                        "aria-label": "Filter by flake",
                        value: "{flake_filter.read()}",
                        onchange: move |evt| flake_filter.set(evt.value()),
                        option { value: "all", "All flakes" }
                        for flake in registered_flakes.clone() {
                            option {
                                value: "{flake}",
                                selected: *flake_filter.read() == flake,
                                "{flake}"
                            }
                        }
                    }
                    // Tag select placeholder (design: "All tags" — requires backend tag support)
                    select {
                        class: "input filter-select focus-ring",
                        style: "width: auto; opacity: 0.5;",
                        "aria-label": "Filter by tag",
                        title: "Tag filters are not yet available (requires backend tag support)",
                        disabled: "true",
                        option { value: "all", "All tags" }
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
                    crate::components::loading::DashboardLoadingSpinner {
                        label: "Loading systems".to_string(),
                        size: 36,
                    }
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
                            selected: selected_preview_id == Some(system.id),
                            environment_colors: environment_color_pairs.clone(),
                            flake_context: flake_context.clone(),
                            attention_class: attention_classes.get(&system.id).cloned().unwrap_or_default(),
                            flash: flash_global && matches!(system.health_status, HealthStatus::Critical | HealthStatus::Offline),
                            on_open: move |_| {
                                dismiss_attention_item("systems", &system.id.to_string());
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
                                let mut edit_modal_error = edit_modal_error.clone();
                                spawn(async move {
                                    edit_modal_error.set(None);
                                    let detail = load_system_detail_with_fallback(&system.id.to_string()).await;
                                    if let Some(detail) = detail.system {
                                        edit_modal_system.set(Some(detail));
                                    }
                                });
                            },
                            on_deploy: move |_| {
                                // Navigate to system detail with deploy tab pre-selected,
                                // matching the design example behaviour.
                                #[cfg(target_arch = "wasm32")]
                                if let Some(window) = web_sys::window() {
                                    let url = format!("/systems/{}?tab=deploy", system.id);
                                    let _ = window.location().assign(&url);
                                }
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
                    attention_classes: attention_classes.clone(),
                    on_remove: move |id| remove_system_by_id(local_systems, pending_remove, id),
                    on_update_key: move |id| update_key_for_system(local_systems, pending_update_key, id),
                     on_edit: move |id: uuid::Uuid| {
                         let mut edit_modal_system = edit_modal_system.clone();
                         let mut edit_modal_error = edit_modal_error.clone();
                         spawn(async move {
                             edit_modal_error.set(None);
                             let detail = load_system_detail_with_fallback(&id.to_string()).await;
                             if let Some(detail) = detail.system {
                                 edit_modal_system.set(Some(detail));
                            }
                        });
                    },
                    on_deploy: move |id: uuid::Uuid| {
                        #[cfg(target_arch = "wasm32")]
                        if let Some(window) = web_sys::window() {
                            let url = format!("/systems/{id}?tab=deploy");
                            let _ = window.location().assign(&url);
                        }
                    },
                    on_open: move |id: uuid::Uuid| {
                        dismiss_attention_item("systems", &id.to_string());
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
                {
                    let detail_for_edit = detail.clone();
                    let detail_for_open_detail = detail.clone();
                    rsx! {
                        SystemPreviewPanel {
                            detail: detail.clone(),
                            environment_colors: environment_color_pairs.clone(),
                            on_close: move |_| preview_system.set(None),
                            on_edit: move |_| {
                                edit_modal_error.set(None);
                                edit_modal_system.set(Some(detail_for_edit.clone()));
                                preview_system.set(None);
                            },
                            on_open_detail: move |_| {
                                preview_system.set(None);
                                nav.push(Route::SystemDetailView { id: detail_for_open_detail.id.to_string() });
                            },
                            on_deploy: move |_| {
                                #[cfg(target_arch = "wasm32")]
                                if let Some(window) = web_sys::window() {
                                    let url = format!("/systems/{}?tab=deploy", detail.id);
                                    let _ = window.location().assign(&url);
                                }
                            },
                        }
                    }
                }
            }

            // Remove Confirmation Dialog
            if let Some(system) = pending_remove.read().clone() {
                RemoveSystemDialog {
                    hostname: system.hostname.clone(),
                    environment: system.environment.clone(),
                    is_loading: removing_in_progress(),
                    on_cancel: move |_| {
                        pending_remove.set(None);
                        removing_in_progress.set(false);
                    },
                    on_confirm: move |_| {
                        let system_id = system.id;
                        removing_in_progress.set(true);
                        spawn(async move {
                            match deactivate_system_via_api(system_id).await {
                                Ok(_) => {
                                    let mut values = local_systems.read().clone();
                                    values.retain(|item| item.id != system_id);
                                    local_systems.set(values);
                                    pending_remove.set(None);
                                    removing_in_progress.set(false);
                                }
                                Err(error_message) => {
                                    api_notice.set(Some(error_message));
                                    pending_remove.set(None);
                                    removing_in_progress.set(false);
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
                    environments: dropdown_environments.clone(),
                    error_message: edit_modal_error.read().clone(),
                    remove_in_progress: edit_remove_in_progress(),
                    remove_error_message: edit_modal_error.read().clone(),
                    on_close: move |_| {
                        edit_modal_error.set(None);
                        edit_remove_in_progress.set(false);
                        edit_modal_system.set(None)
                    },
                    on_save: move |request: crate::api::models::UpdateSystemRequest| {
                        let system_id = detail.id;
                        spawn(async move {
                            match update_system_via_api(
                                system_id,
                                request,
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
                                     edit_modal_error.set(None);
                                     edit_modal_system.set(None);
                                 }
                                 Err(error_message) => {
                                     edit_modal_error.set(Some(error_message));
                                 }
                             }
                         });
                    },
                    on_delete: move |_| {
                        let system_id = detail.id;
                        edit_modal_error.set(None);
                        edit_remove_in_progress.set(true);
                        spawn(async move {
                            match deactivate_system_via_api(system_id).await {
                                Ok(_) => {
                                    let mut values = local_systems.read().clone();
                                    values.retain(|item| item.id != system_id);
                                    local_systems.set(values);
                                    edit_modal_error.set(None);
                                    edit_remove_in_progress.set(false);
                                    edit_modal_system.set(None);
                                }
                                Err(error_message) => {
                                    edit_remove_in_progress.set(false);
                                    edit_modal_error.set(Some(error_message));
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
    on_edit: EventHandler<()>,
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

    let mut deployment_progress_poll_tick = use_signal(|| 0_u64);
    let deployment_progress_resource = use_resource({
        let system_id = detail.id;
        move || async move {
            let _ = deployment_progress_poll_tick();
            load_system_deployment_progress_with_fallback(system_id).await
        }
    });
    use_future({
        let mut deployment_progress_poll_tick = deployment_progress_poll_tick.clone();
        move || async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(4_000).await;
                deployment_progress_poll_tick.set(deployment_progress_poll_tick() + 1);
            }
        }
    });

    let history_resource = use_resource({
        let system_id = detail.id;
        move || async move { load_system_history_with_fallback(system_id).await }
    });

    let deployment_progress = deployment_progress_resource
        .read_unchecked()
        .as_ref()
        .and_then(|result| result.progress.clone());
    let recent_activity_rows = history_resource
        .read_unchecked()
        .as_ref()
        .map(|result| {
            result
                .entries
                .iter()
                .take(5)
                .map(activity_row_from_history)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let status_color = match detail.health_status {
        HealthStatus::Healthy => "#34d399",
        HealthStatus::Warning => "#fbbf24",
        HealthStatus::Critical => "#f87171",
        HealthStatus::Offline => "#6b7280",
    };

    let heartbeat_interval_sec = detail.effective_heartbeat_interval_secs as i64;
    let heartbeat_next_in_sec = detail
        .last_seen
        .map(|dt| {
            heartbeat_interval_sec as f64 - now.signed_duration_since(dt).num_seconds() as f64
        })
        .unwrap_or(0.0);
    let last_heartbeat = if let Some(progress) = deployment_progress.as_ref() {
        let action = if progress.kind == "rollback" {
            "Rollback"
        } else {
            "Deployment"
        };
        format!("{action} {}", progress.stage.replace('_', " "))
    } else {
        detail
            .last_seen
            .map(|dt| format_relative_time_from(now, dt))
            .unwrap_or_else(|| "Never".to_string())
    };

    let (env_fg, env_bg, env_border) = env_colors_for_badge(
        detail.environment.as_deref().unwrap_or("unknown"),
        &environment_colors,
    );

    let flake_name = detail
        .flake
        .as_ref()
        .map(|f| f.name.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let flake_branch = derived_branch_for_environment(detail.environment.as_deref());
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

    rsx! {
        div {
            class: "side-panel-backdrop",
            onclick: move |_| on_close.call(()),
        }
        aside {
            class: "side-panel",
            role: "dialog",
            "aria-modal": "true",
            "data-testid": "systems-side-panel",

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
                    span {
                        class: "fqdn",
                        "{detail.fqdn.clone().filter(|value| !value.trim().is_empty()).unwrap_or_else(|| derived_fqdn(&detail.hostname, detail.environment.as_deref()))}"
                    }
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
                if let Some(progress) = deployment_progress.clone() {
                    section {
                        class: "panel-section",
                        PendingDeployBanner {
                            progress,
                            hostname: detail.hostname.clone(),
                            heartbeat_interval_secs: detail.effective_heartbeat_interval_secs as i64,
                            heartbeat_next_in_secs: Some(heartbeat_next_in_sec),
                            on_dismiss: move |_| deployment_progress_poll_tick.set(deployment_progress_poll_tick() + 1),
                            on_view_logs: move |_| on_open_detail.call(()),
                        }
                    }
                }
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
                            "{deployment_state_label(&detail.deployment_status)}"
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
                        dt { "Branch" } dd { "{flake_branch}" }
                        dt { "Generation" } dd { "#{detail.generation.unwrap_or_default()}" }
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
                        dt { "IPv6" } dd { "-" }
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
                        if recent_activity_rows.is_empty() {
                            div { class: "tl-empty", "No deployment activity recorded yet." }
                        } else {
                            for row in recent_activity_rows {
                                div {
                                    class: "tl-item",
                                    span { class: "tl-dot", style: "--status-color: {row.color};" }
                                    div {
                                        class: "tl-body",
                                        div { class: "tl-title", span { style: "color: {row.color}; flex-shrink: 0; line-height: 0;", Icon { name: row.icon, size: 12 } } span { "{row.title}" } }
                                        if let Some(sub) = row.sub {
                                            div { class: "tl-sub mono", title: "{sub}", "{sub}" }
                                        }
                                        div { class: "tl-meta", "{format_relative_time_from(now, row.timestamp)}" }
                                    }
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
                    Icon { name: IconName::ArrowRight, size: 12 }
                    "Open full detail"
                }
                button {
                    class: "btn btn-ghost focus-ring",
                    onclick: move |_| on_edit.call(()),
                    Icon { name: IconName::Gear, size: 12 }
                    "Edit"
                }
                button {
                    class: "btn btn-primary focus-ring",
                    onclick: move |_| on_deploy.call(()),
                    Icon { name: IconName::Deploy, size: 12 }
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

fn derived_fqdn(hostname: &str, environment: Option<&str>) -> String {
    let env = environment.unwrap_or("unknown").to_lowercase();
    format!("{hostname}.{env}.cf.internal")
}

fn derived_branch_for_environment(environment: Option<&str>) -> &'static str {
    match environment.unwrap_or("dev").to_lowercase().as_str() {
        "production" | "prod" => "main",
        "staging" | "stage" => "staging",
        _ => "dev",
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
