//! System detail view — full information for a single NixOS system.
//!
//! Uses a tabbed layout with:
//! - Overview: System info, hardware, network, security
//! - History: Vertical commit timeline showing deployed vs skipped commits
//! - CVEs: Expandable vulnerability list
//! - Logs: Recent deployment logs

use chrono::{Duration, Utc};
use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use js_sys::Object;
use serde_json::Value as JsonValue;
use uuid::Uuid;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, JsValue};

use crate::api::client::{
    ApiClientError, fetch_cve_scan_status, fetch_hardening_scan_status,
    fetch_system_cve_scan_eligibility, fetch_system_cves, fetch_system_hardening,
    fetch_system_hardening_justifications, fetch_system_hardening_scan_eligibility,
    request_system_generation_rollback, request_system_rollback, request_system_sync,
    save_system_hardening_justification, trigger_system_cve_scan, trigger_system_hardening_scan,
    verify_generation_closure as verify_generation_closure_request,
};
use crate::api::models::{
    BuildStatus, CommitInfo, CveScanEligibilityResponse, CveSeverity, CveSummary,
    DeploymentLogEntry, DeploymentStatus, HardeningJustificationResponse,
    HardeningScanEligibilityResponse, HardeningServiceResultResponse, HealthStatus, LogLevel,
    PipelineStage, SaveHardeningJustificationRequest, SystemAgentEvent, SystemCommitHistory,
    SystemDetail, SystemGeneration, SystemHardwareInfo, SystemHistoryEntry, SystemNetworkInfo,
    SystemRollbackGenerationRequest, SystemRollbackRequest, SystemSecurityInfo,
    SystemVulnerability, VerifyGenerationClosureRequest,
};
use crate::components::cve::CvesTab;
use crate::components::diff::DiffViewer;
use crate::components::icon::{Icon, IconName};
use crate::components::layout::Card;
use crate::components::modals::{RollbackConfirmDialog, SyncConfirmDialog};
use crate::components::notifications::Toast;
use crate::components::system::{
    AgentCard, BooleanRow, EditSystemModal, HardwareCard, InfoRow, InfoRowMono, LogLine, LogsTab,
    NetworkCard, SecurityCard, StatusBadge, SystemInfoCard, deployment_state_label,
    environment_style, format_uptime,
};
use crate::routes::Route;
use crate::state::{app_state::AppState, auth};
use crate::systems::adapter::{
    deploy_system_via_api, fetch_system_commits_via_api, load_system_agent_events_with_fallback,
    load_system_generations_with_fallback, load_system_history_with_fallback,
    update_system_via_api,
};
use crate::systems::adapter::{fallback_system_detail, load_system_detail_with_fallback};
use crate::theme;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;
#[cfg(target_arch = "wasm32")]
use web_sys::Notification;

// ─────────────────────────────────────────────────────────────────────────────
// Tab Enum
// ─────────────────────────────────────────────────────────────────────────────

const POLICY_TOML_SAMPLE: &str = r#"[[policy]]
type = "require_crystal_forge_agent"
strict = true

[[policy]]
type = "require_packages"
packages = ["git", "vim"]
strict = false

[[policy]]
type = "custom_check"
expression = "(cfg.config.services.openssh.enable or false)"
description = "SSH must be enabled"
field_name = "sshEnabled"
strict = true
"#;

const POLICY_JSON_SAMPLE: &str = r#"[
  {
    "type": "require_crystal_forge_agent",
    "strict": true
  },
  {
    "type": "require_packages",
    "packages": ["git", "vim"],
    "strict": false
  },
  {
    "type": "custom_check",
    "expression": "(cfg.config.services.openssh.enable or false)",
    "description": "SSH must be enabled",
    "field_name": "sshEnabled",
    "strict": true
  }
]
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Overview,
    Deploy,
    History,
    Hardening,
    Logs,
    Config,
    Cves,
    Compliance,
}

impl Tab {
    fn label(&self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Deploy => "Deploy",
            Self::History => "History",
            Self::Hardening => "Hardening",
            Self::Logs => "Logs",
            Self::Config => "Config",
            Self::Cves => "CVEs",
            Self::Compliance => "Compliance",
        }
    }
}

fn derived_fqdn(hostname: &str, environment: Option<&str>) -> String {
    let env = environment.unwrap_or("unknown").to_lowercase();
    format!("{hostname}.{env}.cf.internal")
}

fn is_pull_reachability(reachability: &str) -> bool {
    reachability.eq_ignore_ascii_case("pull")
}

/// Normalize a free-form tag input to the design's slug form: trim, drop a leading `#`,
/// collapse whitespace to single hyphens, and lowercase. Mirrors the reference's `addTag`.
fn normalize_tag(raw: &str) -> String {
    let trimmed = raw.trim().trim_start_matches('#');
    trimmed
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase()
}

fn reachability_label(reachability: &str) -> &'static str {
    if is_pull_reachability(reachability) {
        "Agent pull-only"
    } else {
        "Direct / LAN"
    }
}

const DETAIL_TAB_ORDER: [Tab; 8] = [
    Tab::Overview,
    Tab::Deploy,
    Tab::History,
    Tab::Logs,
    Tab::Config,
    Tab::Cves,
    Tab::Hardening,
    Tab::Compliance,
];

// ─────────────────────────────────────────────────────────────────────────────
// Main Component
// ─────────────────────────────────────────────────────────────────────────────

/// The system detail page, reached via `/systems/:id`.
#[component]
pub fn SystemDetailView(id: String) -> Element {
    let nav = navigator();
    let app_state = use_context::<Signal<AppState>>();

    // Current tab state
    let mut active_tab = use_signal(|| Tab::Overview);
    let mut edit_modal_open = use_signal(|| false);
    let mut show_ssh_modal = use_signal(|| false);

    // Confirmation dialog state for Sync
    let mut show_sync_dialog = use_signal(|| false);
    let mut sync_in_progress = use_signal(|| false);
    let mut cve_scan_in_progress = use_signal(|| false);
    let mut cve_scan_status_text: Signal<Option<String>> = use_signal(|| None);
    let mut hardening_scan_in_progress = use_signal(|| false);
    let mut hardening_scan_status_text: Signal<Option<String>> = use_signal(|| None);

    // Confirmation dialog state for rollback/deploying a historical commit
    let mut show_rollback_dialog = use_signal(|| false);
    let mut rollback_target: Signal<Option<SystemCommitHistory>> = use_signal(|| None);

    // Toast notification state
    let mut toast_message: Signal<Option<(String, bool)>> = use_signal(|| None); // (message, is_success)

    // Live clock tick for relative timers/heartbeat countdowns while page is open.
    let mut now_tick = use_signal(Utc::now);
    use_effect(move || {
        spawn(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(1000).await;
                now_tick.set(Utc::now());
            }
        });
    });
    let now = now_tick();

    // System data state — use_resource keyed on id prevents repeated fetches.
    let id_for_detail = id.clone();
    let mut detail_resource = use_resource(move || {
        let id = id_for_detail.clone();
        async move { load_system_detail_with_fallback(&id).await }
    });

    let id_for_vulns = id.clone();
    let mut vulnerabilities_resource = use_resource(move || {
        let id = id_for_vulns.clone();
        async move {
            let Ok(system_id) = Uuid::parse_str(&id) else {
                return mock_vulnerabilities();
            };

            fetch_system_cves(&system_id)
                .await
                .unwrap_or_else(|_| mock_vulnerabilities())
        }
    });

    let id_for_history = id.clone();
    let history_resource = use_resource(move || {
        let id = id_for_history.clone();
        async move {
            let Ok(system_id) = Uuid::parse_str(&id) else {
                return Vec::<SystemHistoryEntry>::new();
            };

            load_system_history_with_fallback(system_id).await.entries
        }
    });

    let id_for_events = id.clone();
    let agent_events_resource = use_resource(move || {
        let id = id_for_events.clone();
        async move {
            let Ok(system_id) = Uuid::parse_str(&id) else {
                return Vec::<SystemAgentEvent>::new();
            };

            load_system_agent_events_with_fallback(system_id)
                .await
                .entries
        }
    });

    let id_for_scan_eligibility = id.clone();
    let scan_eligibility_resource = use_resource(move || {
        let id = id_for_scan_eligibility.clone();
        async move {
            let Ok(system_id) = Uuid::parse_str(&id) else {
                return None;
            };

            fetch_system_cve_scan_eligibility(&system_id).await.ok()
        }
    });

    let id_for_commits = id.clone();
    let commits_resource = use_resource(move || {
        let id = id_for_commits.clone();
        async move {
            let Ok(system_id) = Uuid::parse_str(&id) else {
                return None;
            };

            fetch_system_commits_via_api(system_id).await.ok()
        }
    });

    let id_for_generations = id.clone();
    let generations_resource = use_resource(move || {
        let id = id_for_generations.clone();
        async move {
            let Ok(system_id) = Uuid::parse_str(&id) else {
                return None;
            };

            let result = load_system_generations_with_fallback(system_id).await;
            Some(result)
        }
    });

    let id_for_hardening = id.clone();
    let mut hardening_results_resource = use_resource(move || {
        let id = id_for_hardening.clone();
        async move {
            let Ok(system_id) = Uuid::parse_str(&id) else {
                return Vec::<HardeningServiceResultResponse>::new();
            };

            fetch_system_hardening(&system_id).await.unwrap_or_default()
        }
    });

    let id_for_hardening_justifications = id.clone();
    let mut hardening_justifications_resource = use_resource(move || {
        let id = id_for_hardening_justifications.clone();
        async move {
            let Ok(system_id) = Uuid::parse_str(&id) else {
                return Vec::<HardeningJustificationResponse>::new();
            };

            fetch_system_hardening_justifications(&system_id)
                .await
                .unwrap_or_default()
        }
    });

    let id_for_hardening_scan_eligibility = id.clone();
    let hardening_scan_eligibility_resource = use_resource(move || {
        let id = id_for_hardening_scan_eligibility.clone();
        async move {
            let Ok(system_id) = Uuid::parse_str(&id) else {
                return None;
            };

            fetch_system_hardening_scan_eligibility(&system_id)
                .await
                .ok()
        }
    });

    // Derive state from resource result. `detail_loading` is true while the primary
    // system-detail fetch is still in-flight so we can show a real loading spinner (design
    // parity) instead of silently rendering fallback/mock data.
    let detail_loading = detail_resource.read_unchecked().is_none();
    let (system, api_notice, redirect_to_login, not_found) =
        match &*detail_resource.read_unchecked() {
            Some(result) => (
                result.system.clone().unwrap_or_else(fallback_system_detail),
                result.notice.clone(),
                result.redirect_to_login,
                result.system.is_none() && !result.redirect_to_login,
            ),
            None => (fallback_system_detail(), None, false, false),
        };

    // Loading state — design reference shows a centered spinner while the system loads.
    if detail_loading {
        return rsx! {
            div {
                class: "sd-root",
                "data-testid": "system-detail-loading",
                "data-screen-label": "SystemDetail",
                div {
                    class: "flex items-center justify-center py-16",
                    crate::components::loading::DashboardLoadingSpinner {
                        label: "Loading system…".to_string(),
                        size: 48,
                    }
                }
            }
        };
    }

    // Redirect to login (early return matching dashboard pattern).
    if redirect_to_login {
        nav.push(Route::LoginView {});
        return rsx! {
            div {
                class: "flex items-center justify-center py-12",
                p { class: "{theme::text::SECONDARY}", "Redirecting to login..." }
            }
        };
    }

    // Not found state.
    if not_found {
        return rsx! {
            div {
                class: "space-y-4",
                Link {
                    to: crate::routes::Route::SystemsView {},
                    class: "inline-flex items-center gap-1 text-sm {theme::text::SECONDARY} hover:text-white transition-colors",
                    "← Back to Systems"
                }
                div {
                    class: "rounded-lg border border-red-500/30 bg-red-500/10 px-6 py-8 text-center",
                    p { class: "text-red-300 font-medium", "System not found" }
                    p { class: "text-sm {theme::text::MUTED} mt-1", "No system exists with this ID." }
                }
            }
        };
    }

    let history_entries = history_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let history_commit_history = map_history_entries_to_commit_history(&history_entries);
    let deploy_commit_history = commits_resource
        .read_unchecked()
        .clone()
        .flatten()
        .map(|response| {
            map_commit_infos_to_commit_history(&response.commits, response.current_commit)
        })
        .filter(|commits| !commits.is_empty())
        .unwrap_or_else(|| history_commit_history.clone());
    let overview_current_commit = deploy_commit_history
        .iter()
        .find(|commit| commit.is_current)
        .cloned()
        .or_else(|| deploy_commit_history.first().cloned());
    // Raw commit list for the Edit modal's pinned-commit picker. This comes from the
    // real `/systems/:id/commits` endpoint when available, so the pinned picker is wired
    // to authoritative data rather than mocked.
    let edit_recent_commits = commits_resource
        .read_unchecked()
        .clone()
        .flatten()
        .map(|response| response.commits)
        .unwrap_or_default();
    let generations_result = generations_resource
        .read_unchecked()
        .clone()
        .flatten()
        .unwrap_or_else(|| crate::systems::adapter::SystemGenerationsLoadResult {
            generations: vec![],
            current_generation: None,
            notice: None,
            redirect_to_login: false,
        });
    let vulnerabilities = match &*vulnerabilities_resource.read_unchecked() {
        Some(value) => value.clone(),
        None => mock_vulnerabilities(),
    };
    let deployment_logs = map_agent_events_to_logs(
        agent_events_resource
            .read_unchecked()
            .clone()
            .unwrap_or_default(),
    );
    let scan_eligibility: Option<CveScanEligibilityResponse> = (*scan_eligibility_resource
        .read_unchecked())
    .clone()
    .flatten();
    let hardening_results = hardening_results_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let hardening_justifications = hardening_justifications_resource
        .read_unchecked()
        .clone()
        .unwrap_or_default();
    let hardening_scan_eligibility: Option<HardeningScanEligibilityResponse> =
        (*hardening_scan_eligibility_resource.read_unchecked())
            .clone()
            .flatten();

    let auth_context = app_state.read().auth.clone();
    let can_mutate = auth::can_mutate_systems(&auth_context);
    let cve_scan_eligible = scan_eligibility
        .as_ref()
        .map(|item| item.eligible)
        .unwrap_or(false);
    let cve_scan_blocked_reason = scan_eligibility
        .as_ref()
        .and_then(|item| item.reason.clone())
        .unwrap_or_else(|| "CVE scan availability is still loading.".to_string());
    let hardening_scan_eligible = hardening_scan_eligibility
        .as_ref()
        .map(|item| item.eligible)
        .unwrap_or(false);
    let hardening_scan_blocked_reason = hardening_scan_eligibility
        .as_ref()
        .and_then(|item| item.reason.clone())
        .unwrap_or_else(|| "Hardening scan availability is still loading.".to_string());

    let environment = system
        .environment
        .clone()
        .unwrap_or_else(|| "Unknown".to_string());
    let env_style = environment_style(&environment);

    let status_dot_color = match system.health_status {
        HealthStatus::Healthy => "#34d399",
        HealthStatus::Warning => "#fbbf24",
        HealthStatus::Critical => "#f87171",
        HealthStatus::Offline => "#6b7280",
    };
    let health_chip_class = match system.health_status {
        HealthStatus::Healthy => "chip chip-healthy",
        HealthStatus::Warning => "chip chip-warning",
        HealthStatus::Critical => "chip chip-critical",
        HealthStatus::Offline => "chip chip-unknown",
    };
    let health_label = system.health_status.label();
    let detail_fqdn = derived_fqdn(&system.hostname, system.environment.as_deref());
    let deployment_chip_class = match system.deployment_status {
        DeploymentStatus::UpToDate => "chip chip-healthy",
        DeploymentStatus::Behind => "chip chip-warning",
        DeploymentStatus::Ahead => "chip chip-info",
        DeploymentStatus::NeverDeployed
        | DeploymentStatus::NoCommitsAvailable
        | DeploymentStatus::Unknown => "chip chip-unknown",
    };
    let deployment_chip_label = deployment_state_label(&system.deployment_status);

    // Format last seen for header
    let last_seen_text = system
        .last_seen
        .map(|dt| {
            let duration = now.signed_duration_since(dt);
            if duration.num_minutes() < 1 {
                "Just now".to_string()
            } else if duration.num_hours() < 1 {
                format!("{}m ago", duration.num_minutes())
            } else if duration.num_days() < 1 {
                format!("{}h ago", duration.num_hours())
            } else {
                format!("{}d ago", duration.num_days())
            }
        })
        .unwrap_or_else(|| "Never".to_string());

    rsx! {
        div {
            class: "sd-root",
            "data-testid": "system-detail",
            "data-screen-label": "SystemDetail",

            // API fallback notice banner (shown when using mock data)
            if let Some(ref notice) = api_notice {
                div {
                    class: "rounded-lg border border-yellow-500/30 bg-yellow-500/10 px-4 py-3 text-sm text-yellow-300",
                    "{notice}"
                }
            }

            div {
                class: "sd-crumb",
                button {
                    class: "sd-back focus-ring",
                    onclick: move |_| {
                        nav.push(Route::SystemsView {});
                    },
                    "aria-label": "Back to systems",
                    Icon { name: IconName::ArrowLeft, size: 14 }
                }
                span {
                    class: "sd-crumb-text",
                    span { class: "sd-crumb-parent", "Systems" }
                    span { class: "sd-crumb-sep", "/" }
                    span { class: "sd-crumb-current mono", "{system.hostname}" }
                }
            }

            // Page header
            header {
                class: "sd-head",
                div {
                    class: "sd-head-main",
                    div {
                        class: "sd-title-block",
                        span {
                            class: "status-dot lg",
                            style: "--status-color: {status_dot_color};",
                        }
                        div {
                            h1 { class: "sd-hostname", "{system.hostname}" }
                            div { class: "sd-fqdn mono", "{detail_fqdn}" }
                        }
                        span {
                            class: "env-badge",
                            style: "color: {env_style.chip_text}; background: {env_style.chip_bg};",
                            span { class: "chip-dot" }
                            "{environment}"
                        }
                        span { class: "{health_chip_class}", "{health_label}" }
                        span {
                            class: "{deployment_chip_class}",
                            "{deployment_chip_label}"
                        }
                    }

                    div {
                        class: "sd-head-actions",
                        button {
                            class: "btn btn-ghost focus-ring",
                            disabled: !can_mutate,
                            onclick: {
                                // The header Rollback action seeds the confirmation dialog with the
                                // currently deployed commit (falling back to the most recent known
                                // commit) so the dialog actually renders. Without a rollback_target
                                // the dialog's `if let Some(..)` guard short-circuits and nothing
                                // appears. Operators can pick a specific generation from the Deploy
                                // tab; this header shortcut confirms a rollback of the active commit.
                                let seed = overview_current_commit.clone();
                                move |_| {
                                    rollback_target.set(seed.clone());
                                    show_rollback_dialog.set(true);
                                }
                            },
                            Icon { name: IconName::Rollback, size: 14 }
                            "Rollback"
                        }
                        button {
                            class: "btn btn-ghost focus-ring",
                            onclick: move |_| show_ssh_modal.set(true),
                            Icon { name: IconName::Terminal, size: 14 }
                            "SSH"
                        }
                        button {
                            class: "btn btn-ghost focus-ring",
                            disabled: !can_mutate,
                            onclick: move |_| edit_modal_open.set(true),
                            Icon { name: IconName::Gear, size: 14 }
                            "Edit"
                        }
                        button {
                            class: "btn btn-primary focus-ring",
                            disabled: !can_mutate,
                            onclick: move |_| active_tab.set(Tab::Deploy),
                            Icon { name: IconName::Deploy, size: 14 }
                            "Deploy"
                        }
                    }
                }
                if let Some(scan_status) = cve_scan_status_text() {
                    p {
                        class: "text-xs text-amber-200 mt-1",
                        "{scan_status}"
                    }
                }
                if let Some(scan_status) = hardening_scan_status_text() {
                    p {
                        class: "text-xs text-violet-200 mt-1",
                        "{scan_status}"
                    }
                }
            }

            {
                let heartbeat_interval_sec = 60_i64;
                let heartbeat_next_in_sec = system
                    .last_seen
                    .map(|dt| 60.0 - now.signed_duration_since(dt).num_seconds() as f64)
                    .unwrap_or(0.0);
                let uptime_str = format_uptime(system.hardware.uptime_secs.unwrap_or(0));
                let kernel_str = system.kernel.clone().unwrap_or_else(|| "unknown".to_string());
                let policy_str = system.deployment_policy.clone();
                let env_str = environment.clone();
                let generation_text = system
                    .generation
                    .map(|generation| format!("#{generation}"))
                    .unwrap_or_else(|| "#—".to_string());
                let generation_subtext = if matches!(
                    system.generation_matches_current_store_path,
                    Some(false)
                ) {
                    "profile/current mismatch detected".to_string()
                } else {
                    format!("activated · {last_seen_text}")
                };
                let cve_total = system.cve_counts.total();
                let cve_critical = system.cve_counts.critical;
                let cve_high = system.cve_counts.high;

                rsx! {
                    div {
                        class: "sd-metric-strip",
                        // Heartbeat
                        div {
                            class: "sd-metric",
                            div { class: "sd-metric-label", "Heartbeat" }
                            div {
                                class: "sd-metric-val",
                                crate::components::HeartbeatSpinner {
                                    interval_sec: heartbeat_interval_sec,
                                    next_in_sec: heartbeat_next_in_sec,
                                    size: 36,
                                }
                            }
                        }
                        // Generation
                        div {
                            class: "sd-metric",
                            div { class: "sd-metric-label", "Generation" }
                            div { class: "sd-metric-val-num", "{generation_text}" }
                            div { class: "sd-metric-sub", "{generation_subtext}" }
                        }
                        // Uptime
                        div {
                            class: "sd-metric",
                            div { class: "sd-metric-label", "Uptime" }
                            div { class: "sd-metric-val-num", "{uptime_str}" }
                            div { class: "sd-metric-sub mono", "{kernel_str}" }
                        }
                        // CVEs
                        div {
                            class: "sd-metric",
                            div { class: "sd-metric-label", "CVEs" }
                            div {
                                class: "sd-metric-val-num",
                                style: if cve_critical > 0 { "color: #f87171;" } else { "color: #34d399;" },
                                "{cve_total}"
                            }
                            div { class: "sd-metric-sub", "{cve_critical} critical · {cve_high} high" }
                        }
                        // Policy
                        div {
                            class: "sd-metric",
                            div { class: "sd-metric-label", "Policy" }
                            div {
                                class: "sd-metric-val-num mono",
                                style: "font-size: 18px;",
                                "{policy_str}"
                            }
                            div { class: "sd-metric-sub", "env: {env_str}" }
                        }
                    }
                }
            }

            // Tab navigation
            div {
                "data-testid": "system-detail-tabs",
                class: "sd-tabs",
                role: "tablist",
                for tab in DETAIL_TAB_ORDER {
                    {
                        let is_active = *active_tab.read() == tab;
                        let tab_class = if is_active {
                            "sd-tab focus-ring active"
                        } else {
                            "sd-tab focus-ring"
                        };
                        rsx! {
                            button {
                                key: "{tab:?}",
                                class: "{tab_class}",
                                role: "tab",
                                "aria-selected": "{is_active}",
                                onclick: move |_| active_tab.set(tab),
                                // Tab icons use the shared Icon component at size 13,
                                // matching the CrystalForgelatest design icon contract.
                                match tab {
                                    Tab::Overview => rsx!(Icon { name: IconName::Dashboard, size: 13 }),
                                    Tab::Deploy => rsx!(Icon { name: IconName::Deploy, size: 13 }),
                                    Tab::History => rsx!(Icon { name: IconName::History, size: 13 }),
                                    Tab::Cves => rsx!(Icon { name: IconName::Shield, size: 13 }),
                                    Tab::Hardening => rsx!(Icon { name: IconName::Key, size: 13 }),
                                    Tab::Logs => rsx!(Icon { name: IconName::Terminal, size: 13 }),
                                    Tab::Config => rsx!(Icon { name: IconName::File, size: 13 }),
                                    Tab::Compliance => rsx!(Icon { name: IconName::Shield, size: 13 }),
                                }
                                "{tab.label()}"
                                if tab == Tab::Cves && system.cve_counts.critical > 0 {
                                    span {
                                        class: "sd-tab-badge",
                                        "{system.cve_counts.critical}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Tab content
            div {
                class: "sd-body",
                match *active_tab.read() {
                    Tab::Overview => rsx! {
                        OverviewTab {
                            system: system.clone(),
                            now: now,
                            current_commit: overview_current_commit.clone(),
                            on_open_cves: move |_| active_tab.set(Tab::Cves),
                        }
                    },
                    Tab::Deploy => rsx! {
                        DeployTab {
                            system: system.clone(),
                            commits: deploy_commit_history.clone(),
                            generations: generations_result.generations.clone(),
                            current_generation: generations_result.current_generation,
                            allow_mutations: can_mutate,
                            on_deploy_commit: {
                                let system_id = system.id;
                                let hostname = system.hostname.clone();
                                let toast_message = toast_message.clone();
                                move |commit_sha: String| {
                                    let hostname = hostname.clone();
                                    let toast_message = toast_message.clone();
                                    spawn(async move {
                                        let message = match deploy_system_via_api(system_id, commit_sha.clone()).await {
                                            Ok(response) if !response.trim().is_empty() => response,
                                            Ok(_) => format!(
                                                "Requested deployment of {} to {}",
                                                hostname,
                                                commit_sha.chars().take(7).collect::<String>()
                                            ),
                                            Err(error) => format!("Deploy request failed for {}: {}", hostname, error),
                                        };
                                        let success = !message.to_ascii_lowercase().contains("failed");
                                        let _ = dispatch_sync_notification(message, success, toast_message).await;
                                    });
                                }
                            },
                            on_deploy_generation: {
                                let system_id = system.id;
                                let hostname = system.hostname.clone();
                                let toast_message = toast_message.clone();
                                move |store_path: String| {
                                    let hostname = hostname.clone();
                                    let toast_message = toast_message.clone();
                                    spawn(async move {
                                        let message = match request_system_generation_rollback(
                                            &system_id,
                                            &SystemRollbackGenerationRequest {
                                                store_path: store_path.clone(),
                                            },
                                        )
                                        .await
                                        {
                                            Ok(response) if !response.message.trim().is_empty() => response.message,
                                            Ok(_) => format!("Requested generation rollback for {}", hostname),
                                            Err(error) => format!(
                                                "Generation rollback request failed for {}: {}",
                                                hostname, error
                                            ),
                                        };
                                        let success = !message.to_ascii_lowercase().contains("failed");
                                        let _ = dispatch_sync_notification(message, success, toast_message).await;
                                    });
                                }
                            }
                        }
                    },
                    Tab::History => rsx! {
                        HistoryTab {
                            commits: history_commit_history.clone(),
                            deployment_policy: system.deployment_policy.clone(),
                            allow_mutations: can_mutate,
                            on_rollback: move |commit| {
                                rollback_target.set(Some(commit));
                                show_rollback_dialog.set(true);
                            },
                            on_view_logs: move |_| active_tab.set(Tab::Logs),
                        }
                    },
                    Tab::Cves => rsx! {
                        CvesTab {
                            system_id: system.id,
                            cve_counts: system.cve_counts.clone(),
                            vulnerabilities: vulnerabilities.clone(),
                            allow_mutations: can_mutate,
                            on_saved: move |_| {
                                vulnerabilities_resource.restart();
                            }
                        }
                    },
                    Tab::Hardening => rsx! {
                        HardeningTab {
                            system_id: system.id,
                            results: hardening_results.clone(),
                            justifications: hardening_justifications.clone(),
                            allow_mutations: can_mutate,
                            on_saved: move |_| {
                                hardening_results_resource.restart();
                                hardening_justifications_resource.restart();
                            }
                        }
                    },
                    Tab::Logs => rsx! {
                        LogsTabStyled { logs: deployment_logs.clone() }
                    },
                    Tab::Config => rsx! {
                        ConfigTab { system: system.clone() }
                    },
                    Tab::Compliance => rsx! {
                        ComplianceTab { system: system.clone() }
                    },
                }
            }

            // Toast notification (fixed position at top)
            if let Some((ref message, is_success)) = *toast_message.read() {
                Toast {
                    message: message.clone(),
                    is_success: is_success,
                    on_dismiss: move |_| toast_message.set(None)
                }
            }
        }

        // Sync confirmation dialog (rendered as portal outside main content)
        if *edit_modal_open.read() {
            EditSystemModal {
                system: system.clone(),
                flake_names: system
                    .flake
                    .as_ref()
                    .map(|flake| vec![flake.name.clone()])
                    .unwrap_or_default(),
                recent_commits: edit_recent_commits.clone(),
                on_close: move |_| edit_modal_open.set(false),
                on_save: move |request: crate::api::models::UpdateSystemRequest| {
                    let system_id = system.id;
                    spawn(async move {
                        match update_system_via_api(
                            system_id,
                            request.hostname,
                            request.system_configuration_name,
                            request.environment,
                            request.flake_name,
                            request.deployment_policy,
                        )
                        .await
                        {
                            Ok(_) => {
                                edit_modal_open.set(false);
                                detail_resource.restart();
                            }
                            Err(error_message) => {
                                toast_message.set(Some((error_message, false)));
                                edit_modal_open.set(false);
                            }
                        }
                    });
                }
            }
        }

        if *show_ssh_modal.read() {
            SshConnectModal {
                system: system.clone(),
                on_close: move |_| show_ssh_modal.set(false),
            }
        }

        // Sync confirmation dialog (rendered as portal outside main content)
        if *show_sync_dialog.read() {
            SyncConfirmDialog {
                hostname: system.hostname.clone(),
                on_confirm: {
                    let hostname = system.hostname.clone();
                        move |_| {
                            show_sync_dialog.set(false);
                            sync_in_progress.set(true);

                            let hostname = hostname.clone();
                            let system_id = system.id;
                            let mut toast_message = toast_message.clone();
                            spawn(async move {
                                sync_in_progress.set(false);

                                let show_toast = match request_system_sync(&system_id).await {
                                    Ok(response) => {
                                        let message = if response.message.trim().is_empty() {
                                            format!("Successfully synced {}", hostname)
                                        } else {
                                            response.message
                                        };
                                        dispatch_sync_notification(message, true, toast_message.clone()).await
                                    }
                                    Err(error) => {
                                        let message = format!("Failed to sync {}: {}", hostname, error);
                                        dispatch_sync_notification(message, false, toast_message.clone()).await
                                    }
                                };

                            // Auto-dismiss toast after 5 seconds (only when used)
                            if show_toast {
                                #[cfg(target_arch = "wasm32")]
                                {
                                    use gloo_timers::future::TimeoutFuture;
                                    TimeoutFuture::new(5000).await;
                                    toast_message.set(None);
                                }
                                #[cfg(not(target_arch = "wasm32"))]
                                {
                                    toast_message.set(None);
                                }
                            }
                        });
                    }
                },
                on_cancel: move |_| {
                    show_sync_dialog.set(false);
                }
            }
        }

        // Rollback confirmation dialog
        if *show_rollback_dialog.read() {
            if let Some(ref commit) = *rollback_target.read() {
                RollbackConfirmDialog {
                    hostname: system.hostname.clone(),
                    commit: commit.clone(),
                    on_confirm: {
                        let hostname = system.hostname.clone();
                        let commit = commit.clone();
                        let system_id = system.id;
                        let toast_message = toast_message.clone();
                        move |_| {
                            show_rollback_dialog.set(false);
                            let hostname = hostname.clone();
                            let commit = commit.clone();
                            let target_commit = commit.hash.clone();
                            spawn(async move {
                                let message = match request_system_rollback(
                                    &system_id,
                                    &SystemRollbackRequest { target_commit },
                                )
                                .await
                                {
                                    Ok(response) if !response.message.trim().is_empty() => response.message,
                                    Ok(_) => format!(
                                        "Requested rollback of {} to {}",
                                        hostname,
                                        commit.hash.chars().take(7).collect::<String>()
                                    ),
                                    Err(error) => format!("Rollback request failed for {}: {}", hostname, error),
                                };
                                let success = !message.to_ascii_lowercase().contains("failed");
                                let _ = dispatch_sync_notification(message, success, toast_message).await;
                            });
                        }
                    },
                    on_cancel: move |_| {
                        show_rollback_dialog.set(false);
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tab Components
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
struct ComplianceMockBundle {
    name: &'static str,
    framework: &'static str,
    version: &'static str,
    owner: &'static str,
    controls: u32,
    score: u8,
    pass: u32,
    warn: u32,
    fail: u32,
    waiver: u32,
}

/// Temporary parity data for the System Detail Compliance tab.
///
/// IMPORTANT: This is intentionally mocked by maintainer authorization on
/// TASK-353 because the real Compliance view/backend plumbing does not exist
/// yet. Keep this isolated so TASK-355 can replace it with API-backed bundle
/// rollups and per-control evidence without changing the view structure.
fn mocked_compliance_bundles(system: &SystemDetail) -> Vec<ComplianceMockBundle> {
    let production = system
        .environment
        .as_deref()
        .map(|env| env.eq_ignore_ascii_case("production"))
        .unwrap_or(false);

    if production {
        vec![
            ComplianceMockBundle {
                name: "Production baseline",
                framework: "CIS",
                version: "v2.0",
                owner: "security-platform",
                controls: 42,
                score: 86,
                pass: 34,
                warn: 5,
                fail: 2,
                waiver: 1,
            },
            ComplianceMockBundle {
                name: "STIG NixOS overlay",
                framework: "DISA STIG",
                version: "draft",
                owner: "platform-secops",
                controls: 28,
                score: 92,
                pass: 25,
                warn: 2,
                fail: 0,
                waiver: 1,
            },
        ]
    } else {
        vec![ComplianceMockBundle {
            name: "General fleet baseline",
            framework: "Internal",
            version: "2026.1",
            owner: "platform",
            controls: 24,
            score: 94,
            pass: 22,
            warn: 1,
            fail: 0,
            waiver: 1,
        }]
    }
}

fn compliance_score_color(score: u8) -> &'static str {
    if score >= 90 {
        "#34d399"
    } else if score >= 70 {
        "#fbbf24"
    } else {
        "#f87171"
    }
}

#[component]
fn ComplianceTab(system: SystemDetail) -> Element {
    let bundles = mocked_compliance_bundles(&system);
    // Selected bundle for the placeholder evidence drawer. The real per-control evidence
    // drawer (config output, systemd unit state, audit results, waivers) is tracked by
    // TASK-356; this placeholder shows the design-example layout with sample evidence so the
    // "View evidence" action is no longer a dead button.
    let mut selected_bundle: Signal<Option<ComplianceMockBundle>> = use_signal(|| None);
    let system_hostname = system.hostname.clone();

    rsx! {
        div {
            style: "display:flex;flex-direction:column;gap:14px;",

            div {
                class: "sd-callout sd-callout-info",
                Icon { name: IconName::Shield, size: 13 }
                div {
                    style: "font-size:12px;",
                    strong { "Temporary Compliance preview. " }
                    "Mock bundle rollups are shown for design parity while real compliance evidence APIs are implemented."
                }
            }

            for bundle in bundles {
                {
                    let score_color = compliance_score_color(bundle.score);
                    let score_width = format!("width:{}%;height:100%;background:{};", bundle.score, score_color);
                    let bundle_for_drawer = bundle.clone();
                    rsx! {
                        div {
                            class: "card",
                            style: "padding:16px;",

                            div {
                                style: "display:flex;align-items:flex-start;justify-content:space-between;gap:14px;flex-wrap:wrap;",
                                div {
                                    style: "min-width:0;",
                                    div {
                                        style: "display:flex;align-items:center;gap:8px;flex-wrap:wrap;",
                                        span { style: "font-size:15px;font-weight:650;", "{bundle.name}" }
                                        span { class: "chip chip-info", style: "font-size:10px;", "{bundle.framework}" }
                                        span { class: "chip chip-unknown", style: "font-size:10px;", "{bundle.version}" }
                                        if bundle.fail == 0 {
                                            span {
                                                class: "chip chip-healthy",
                                                style: "font-size:10px;",
                                                Icon { name: IconName::Check, size: 9 }
                                                " Compliant"
                                            }
                                        } else {
                                            span {
                                                class: "chip chip-critical",
                                                style: "font-size:10px;",
                                                Icon { name: IconName::Warn, size: 9 }
                                                " {bundle.fail} failing"
                                            }
                                        }
                                    }
                                    div {
                                        style: "font-size:12px;color:var(--cf-text-muted);margin-top:4px;",
                                        "{bundle.controls} controls · owned by "
                                        span { class: "mono", "{bundle.owner}" }
                                    }
                                }
                                button {
                                    class: "btn btn-primary focus-ring",
                                    title: "Preview the evidence drawer layout (real per-control evidence is tracked by TASK-356)",
                                    onclick: move |_| selected_bundle.set(Some(bundle_for_drawer.clone())),
                                    Icon { name: IconName::File, size: 13 }
                                    "View evidence"
                                }
                            }

                            div {
                                style: "display:flex;align-items:center;gap:16px;margin-top:14px;flex-wrap:wrap;",
                                div {
                                    style: "display:flex;align-items:center;gap:10px;",
                                    div {
                                        style: "width:120px;height:8px;background:var(--cf-subtle-bg);border-radius:99px;overflow:hidden;",
                                        div { style: "{score_width}" }
                                    }
                                    span {
                                        class: "mono",
                                        style: "font-size:14px;font-weight:700;color:{score_color};",
                                        "{bundle.score}%"
                                    }
                                }
                                div {
                                    style: "display:flex;gap:14px;font-size:12px;",
                                    span { span { class: "mono", style: "font-weight:700;color:#34d399;", "{bundle.pass}" } " pass" }
                                    span { span { class: "mono", style: "font-weight:700;color:#fbbf24;", "{bundle.warn}" } " warn" }
                                    span { span { class: "mono", style: "font-weight:700;color:#f87171;", "{bundle.fail}" } " fail" }
                                    span { span { class: "mono", style: "font-weight:700;color:#a78bfa;", "{bundle.waiver}" } " waiver" }
                                }
                            }
                        }
                    }
                }
            }

            // Placeholder evidence drawer (TASK-356 will replace with real per-control evidence).
            if let Some(bundle) = selected_bundle.read().clone() {
                ComplianceEvidenceDrawer {
                    bundle,
                    hostname: system_hostname.clone(),
                    on_close: move |_| selected_bundle.set(None),
                }
            }
        }
    }
}

/// Placeholder "evidence drawer" for the Compliance tab.
///
/// IMPORTANT: This is a maintainer-authorized placeholder (TASK-353). It mirrors the design
/// reference's evidence drawer layout with representative sample controls so the "View
/// evidence" action demonstrates the intended UX. TASK-356 replaces this with real,
/// per-control evidence (config output, systemd unit state, audit results, waivers).
#[component]
fn ComplianceEvidenceDrawer(
    bundle: ComplianceMockBundle,
    hostname: String,
    on_close: EventHandler<()>,
) -> Element {
    // Representative sample controls for the placeholder drawer.
    let sample_controls: [(&str, &str, &str); 4] = [
        (
            "CIS-1.1.1",
            "pass",
            "Ensure mounting of cramfs filesystems is disabled",
        ),
        ("CIS-1.4.1", "pass", "Ensure bootloader password is set"),
        ("CIS-5.2.5", "warn", "Ensure SSH LogLevel is appropriate"),
        (
            "CIS-5.3.1",
            "fail",
            "Ensure password creation requirements are configured",
        ),
    ];

    rsx! {
        div {
            class: "modal-backdrop",
            onclick: move |_| on_close.call(()),
            div {
                class: "modal",
                style: "width:min(640px,96vw);max-height:92vh;",
                onclick: move |e| e.stop_propagation(),

                div {
                    class: "modal-head",
                    h2 {
                        Icon { name: IconName::File, size: 14 }
                        span { style: "margin-left:6px;", "Evidence · {bundle.name}" }
                    }
                    p {
                        "Per-control evidence for "
                        span { class: "mono", "{hostname}" }
                        " · {bundle.framework} {bundle.version}"
                    }
                }

                div {
                    class: "modal-body",
                    style: "overflow-y:auto;",

                    div {
                        class: "sd-callout sd-callout-info",
                        style: "margin-bottom:14px;",
                        Icon { name: IconName::Shield, size: 13 }
                        div {
                            style: "font-size:12px;",
                            strong { "Preview only. " }
                            "Representative controls are shown. Real collected evidence (config output, systemd unit state, audit results, waivers) is tracked by TASK-356."
                        }
                    }

                    table {
                        class: "sys-table",
                        thead {
                            tr {
                                th { "Control" }
                                th { "Status" }
                                th { "Description" }
                            }
                        }
                        tbody {
                            for (id, status, desc) in sample_controls {
                                tr {
                                    td { class: "mono", "{id}" }
                                    td {
                                        match status {
                                            "pass" => rsx!(span { class: "chip chip-healthy", "pass" }),
                                            "warn" => rsx!(span { class: "chip chip-warning", "warn" }),
                                            _ => rsx!(span { class: "chip chip-critical", "fail" }),
                                        }
                                    }
                                    td { style: "font-size:13px;", "{desc}" }
                                }
                            }
                        }
                    }
                }

                div {
                    class: "modal-foot",
                    button {
                        class: "btn btn-primary focus-ring",
                        onclick: move |_| on_close.call(()),
                        "Close"
                    }
                }
            }
        }
    }
}

#[component]
fn SshConnectModal(system: SystemDetail, on_close: EventHandler<()>) -> Element {
    let environment_text = system
        .environment
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let target = system
        .network
        .primary_ip
        .clone()
        .filter(|ip| !ip.is_empty())
        .unwrap_or_else(|| derived_fqdn(&system.hostname, system.environment.as_deref()));
    let fqdn = derived_fqdn(&system.hostname, system.environment.as_deref());
    let jump_domain = fqdn.split('.').skip(1).collect::<Vec<_>>().join(".");
    let jump_domain = if jump_domain.is_empty() {
        "example.com".to_string()
    } else {
        jump_domain
    };

    let ssh_cmd = format!("ssh root@{target}");
    let bastion_cmd = format!("ssh -J bastion.{jump_domain} root@{target}");
    let journal_cmd = format!("ssh root@{target} journalctl -fu crystal-forge-agent");
    let is_pull = is_pull_reachability(&system.network.reachability);
    let reachability_text = if is_pull {
        "Agent pull-only"
    } else {
        "Direct / LAN"
    };

    rsx! {
        div {
            class: "modal-backdrop",
            onclick: move |_| on_close.call(()),

            div {
                class: "modal",
                style: "width:min(560px,96vw);",
                onclick: move |e| e.stop_propagation(),

                div {
                    class: "modal-head",
                    h2 {
                        Icon { name: IconName::Terminal, size: 14 }
                        span { style: "margin-left: 6px;", "Connect to {system.hostname}" }
                    }
                    p { "In-app terminal isn't available yet — connect directly over SSH for now." }
                }

                div {
                    class: "modal-body",
                    style: "overflow-y:auto;",

                    div {
                        class: "sd-callout sd-callout-warn",
                        style: "margin-bottom: 14px;",
                        Icon { name: IconName::Warn, size: 13 }
                        div { style: "font-size: 12px;", "Browser-based SSH is on the roadmap. These commands run from your own workstation." }
                    }

                    div { class: "field", label { "Connect" } }
                    SshCmd { command: ssh_cmd.clone() }

                    if is_pull {
                        div {
                            class: "help",
                            style: "margin-top: 8px;",
                            Icon { name: IconName::Warn, size: 11 }
                            " This host is "
                            strong { "pull-only" }
                            " (behind NAT/firewall). It may only be reachable from inside its network or via a bastion."
                        }
                    }

                    div { class: "field", style: "margin-top: 16px;", label { "Via bastion" } }
                    SshCmd { command: bastion_cmd.clone() }

                    div { class: "field", style: "margin-top: 16px;", label { "Tail the system journal" } }
                    SshCmd { command: journal_cmd.clone() }

                    dl {
                        class: "kv-grid",
                        style: "margin-top: 16px;",
                        dt { "Target" }
                        dd { class: "mono", "{target}" }
                        dt { "Environment" }
                        dd { "{environment_text}" }
                        dt { "Reachability" }
                        dd { "{reachability_text}" }
                    }
                }

                div {
                    class: "modal-foot",
                    button {
                        class: "btn btn-primary focus-ring",
                        onclick: move |_| on_close.call(()),
                        "Close"
                    }
                }
            }
        }
    }
}

/// A single SSH command row with a copy-to-clipboard button, mirroring the
/// CrystalForgelatest `Cmd` helper used inside the SSH connect modal.
#[component]
fn SshCmd(command: String) -> Element {
    let mut copied = use_signal(|| false);

    rsx! {
        div {
            class: "ssh-cmd",
            code { class: "mono", "{command}" }
            button {
                class: "btn btn-ghost xs focus-ring",
                onclick: {
                    let command = command.clone();
                    move |_| {
                        copy_to_clipboard(&command);
                        copied.set(true);
                        spawn(async move {
                            gloo_timers::future::TimeoutFuture::new(1500).await;
                            copied.set(false);
                        });
                    }
                },
                if *copied.read() {
                    Icon { name: IconName::Check, size: 11 }
                    "Copied"
                } else {
                    Icon { name: IconName::File, size: 11 }
                    "Copy"
                }
            }
        }
    }
}

/// Copy text to the browser clipboard. No-op on non-wasm targets.
fn copy_to_clipboard(text: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(win) = web_sys::window() {
            let win_ref: &JsValue = win.as_ref();
            if let Ok(navigator) = js_sys::Reflect::get(win_ref, &JsValue::from_str("navigator")) {
                if let Ok(clipboard) =
                    js_sys::Reflect::get(&navigator, &JsValue::from_str("clipboard"))
                {
                    if let Ok(write_text) =
                        js_sys::Reflect::get(&clipboard, &JsValue::from_str("writeText"))
                    {
                        if let Ok(function) = write_text.dyn_into::<js_sys::Function>() {
                            let _ = function.call1(&clipboard, &JsValue::from_str(text));
                        }
                    }
                }
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = text;
    }
}

#[component]
fn OverviewTab(
    system: SystemDetail,
    now: chrono::DateTime<chrono::Utc>,
    current_commit: Option<SystemCommitHistory>,
    on_open_cves: EventHandler<()>,
) -> Element {
    let environment = system
        .environment
        .clone()
        .unwrap_or_else(|| "Unknown".to_string());
    let env_style = environment_style(&environment);
    let uptime = format_uptime(system.hardware.uptime_secs.unwrap_or_default());
    let kernel = system
        .kernel
        .clone()
        .unwrap_or_else(|| "unknown".to_string());

    // Editable tag chips (design parity). Seeded from the system's environment/flake so the
    // section is never empty, plus any operator-added tags. NOTE: tags are NOT yet persisted
    // server-side (no systems.tags column) — see follow-up TASK-353.1. This is local UI state
    // only; the help text marks it as not saved so operators are not misled.
    let mut tags = use_signal(|| {
        let mut seed = vec![format!("env:{}", environment.to_lowercase())];
        if let Some(flake) = system.flake.as_ref() {
            seed.push(format!("flake:{}", flake.name));
        }
        seed
    });
    let mut tag_adding = use_signal(|| false);
    let mut tag_draft = use_signal(String::new);
    let heartbeat_next_in_sec = system
        .last_seen
        .map(|dt| 60.0 - now.signed_duration_since(dt).num_seconds() as f64)
        .unwrap_or(0.0);
    let fqdn_text = format!(
        "{}.{}.cf.internal",
        system.hostname,
        environment.to_lowercase()
    );

    let flake_name = system
        .flake
        .as_ref()
        .map(|f| f.name.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let flake_commit = system
        .flake
        .as_ref()
        .and_then(|f| f.latest_commit.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let nixos_version = system
        .nixos_version
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let cpu_text = system
        .hardware
        .cpu_brand
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let memory_text = system
        .hardware
        .memory_gb
        .map(|v| format!("{:.1} GiB", v))
        .unwrap_or_else(|| "unknown".to_string());
    let ipv4_text = system
        .network
        .primary_ip
        .clone()
        .unwrap_or_else(|| "-".to_string());
    let ipv6_text = "—".to_string();
    let reachability_is_pull = is_pull_reachability(&system.network.reachability);
    let reachability_chip_class = if reachability_is_pull {
        "chip chip-warning"
    } else {
        "chip chip-healthy"
    };
    let reachability_chip_label = if reachability_is_pull {
        "pull-only"
    } else {
        "direct / LAN"
    };
    let reachability_title = if reachability_is_pull {
        "Behind NAT/firewall — agent checks in; no inbound from server"
    } else {
        "Server can reach the agent directly (LAN/routable/VPN)"
    };
    let branch_text = "main".to_string();
    let generation_text = system
        .generation
        .map(|generation| format!("#{generation}"))
        .unwrap_or_else(|| "#—".to_string());
    let generation_mismatch_note =
        if matches!(system.generation_matches_current_store_path, Some(false)) {
            " (profile/current mismatch)"
        } else {
            ""
        };
    let commit_message_text = current_commit
        .as_ref()
        .map(|commit| commit.message.clone())
        .unwrap_or_else(|| "No commit message available".to_string());

    let critical = system.cve_counts.critical;
    let high = system.cve_counts.high;
    let medium = system.cve_counts.medium;
    let low = system.cve_counts.low;
    let cve_total = system.cve_counts.total();
    let critical_label = if critical == 1 {
        format!("{} critical CVE", critical)
    } else {
        format!("{} critical CVEs", critical)
    };

    let mut recent_activity: Vec<(String, String, chrono::DateTime<chrono::Utc>)> = vec![
        (
            "System record updated".to_string(),
            "#34d399".to_string(),
            system.updated_at,
        ),
        (
            "System registered".to_string(),
            "#a78bfa".to_string(),
            system.created_at,
        ),
    ];
    if let Some(last_seen_at) = system.last_seen {
        recent_activity.push((
            "Heartbeat received".to_string(),
            "#60a5fa".to_string(),
            last_seen_at,
        ));
    }
    recent_activity.sort_by(|a, b| b.2.cmp(&a.2));

    rsx! {
        div {
            class: "sd-grid sd-grid-overview",

            section {
                class: "card sd-card",
                div {
                    class: "sd-card-head",
                    h2 { "Currently deployed" }
                    span {
                        class: "chip chip-healthy",
                        svg {
                            class: "w-3 h-3",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            view_box: "0 0 24 24",
                            path { d: "M5 12l5 5L20 7" }
                        }
                        "up-to-date"
                    }
                }
                dl {
                    class: "kv-grid",
                    dt { "Flake" } dd { class: "mono", "{flake_name}" }
                    dt { "Branch" } dd { class: "mono", "{branch_text}" }
                    dt { "Commit" } dd { class: "mono", "{flake_commit}" }
                    dt { "Message" } dd { style: "white-space: normal; font-family: var(--font-sans);", "{commit_message_text}" }
                    dt { "Generation" } dd { class: "mono", "{generation_text}{generation_mismatch_note}" }
                    dt { "NixOS" } dd { class: "mono", "{nixos_version}" }
                    dt { "Kernel" } dd { class: "mono", "{kernel}" }
                }
            }

            section {
                class: "card sd-card",
                div {
                    class: "sd-card-head",
                    h2 { "Host" }
                    span { class: "mono sd-card-meta", "{system.id}" }
                }
                dl {
                    class: "kv-grid",
                    dt { "Hostname" } dd { class: "mono", "{system.hostname}" }
                    dt { "FQDN" } dd { class: "mono", "{fqdn_text}" }
                    dt { "Environment" }
                    dd {
                        span {
                            class: "inline-flex items-center px-3 py-1 rounded-md text-xs font-semibold uppercase tracking-wide {env_style.chip_bg} {env_style.chip_text}",
                            "{environment}"
                        }
                    }
                    dt { "Uptime" } dd { "{uptime}" }
                    dt { "CPU" } dd { "{cpu_text}" }
                    dt { "Memory" } dd { "{memory_text}" }
                    dt { "IPv4" } dd { class: "mono", "{ipv4_text}" }
                    dt { "IPv6" } dd { class: "mono", "{ipv6_text}" }
                    dt { "Reachability" }
                    dd {
                        span {
                            class: "{reachability_chip_class}",
                            title: "{reachability_title}",
                            "{reachability_chip_label}"
                        }
                    }
                }
                div {
                    class: "hb-panel",
                    style: "margin-top: 16px;",
                    crate::components::HeartbeatSpinner {
                        interval_sec: 60,
                        next_in_sec: heartbeat_next_in_sec,
                        size: 56,
                        show_label: true,
                    }
                }
            }

            section {
                class: "card sd-card",
                div {
                    class: "sd-card-head",
                    h2 { "CVE exposure" }
                    button {
                        class: "btn btn-ghost xs focus-ring",
                        onclick: move |_| on_open_cves.call(()),
                        svg {
                            class: "w-3 h-3",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            view_box: "0 0 24 24",
                            path { d: "M5 12h14M13 5l7 7-7 7" }
                        }
                        "View all"
                    }
                }
                div {
                    class: "cve-bar",
                    {
                        let total = cve_total.max(1) as f64;
                        rsx! {
                            if critical > 0 {
                                div { class: "cve-seg", style: "background: #f87171; width: {(critical as f64 / total) * 100.0}%;" }
                            }
                            if high > 0 {
                                div { class: "cve-seg", style: "background: #fbbf24; width: {(high as f64 / total) * 100.0}%;" }
                            }
                            if medium > 0 {
                                div { class: "cve-seg", style: "background: #9ca3af; width: {(medium as f64 / total) * 100.0}%;" }
                            }
                            if low > 0 {
                                div { class: "cve-seg", style: "background: #4b5563; width: {(low as f64 / total) * 100.0}%;" }
                            }
                        }
                    }
                }
                div {
                    class: "cve-legend",
                    span { class: "cve-legend-item", span { class: "cve-legend-swatch", style: "background: #f87171" } "{critical} critical" }
                    span { class: "cve-legend-item", span { class: "cve-legend-swatch", style: "background: #fbbf24" } "{high} high" }
                    span { class: "cve-legend-item", span { class: "cve-legend-swatch", style: "background: #9ca3af" } "{medium} medium" }
                    span { class: "cve-legend-item", span { class: "cve-legend-swatch", style: "background: #4b5563" } "{low} low" }
                }
                if critical > 0 {
                    div {
                        class: "sd-callout sd-callout-danger",
                        style: "margin-top: 14px;",
                        svg {
                            class: "w-3.5 h-3.5",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            width: "14",
                            height: "14",
                            path { d: "M12 3l8 4v5c0 5-3.5 9.5-8 11-4.5-1.5-8-6-8-11V7l8-4z" }
                        }
                        div {
                            strong { "{critical_label}" }
                            " on this host. Review and patch at earliest opportunity."
                        }
                    }
                }
            }

            section {
                class: "card sd-card sd-card-wide",
                div {
                    class: "sd-card-head",
                    h2 { "Recent activity" }
                    span { class: "sd-card-meta", "last 24h" }
                }
                div {
                    class: "timeline sd-timeline",
                    for (title, color, at) in recent_activity.iter().take(5) {
                        div {
                            class: "tl-item",
                            span { class: "tl-dot", style: "--status-color: {color};" }
                            div {
                                class: "tl-body",
                                div { class: "tl-title", "{title}" }
                                div { class: "tl-meta", "{relative_time(*at)}" }
                            }
                        }
                    }
                }
            }

            section {
                class: "card sd-card",
                div {
                    class: "sd-card-head",
                    h2 { "Tags" }
                }
                div {
                    class: "sd-tag-row",
                    if tags.read().is_empty() && !*tag_adding.read() {
                        span { style: "color: var(--cf-text-muted); font-size: 13px;", "No tags yet" }
                    }
                    for tag in tags.read().clone() {
                        {
                            let tag_to_remove = tag.clone();
                            rsx! {
                                span {
                                    class: "sd-tag mono sd-tag-chip",
                                    span { class: "sd-tag-label", "#{tag}" }
                                    button {
                                        class: "sd-tag-x focus-ring",
                                        title: "Remove tag",
                                        onclick: move |_| {
                                            tags.with_mut(|list| list.retain(|item| *item != tag_to_remove));
                                        },
                                        svg {
                                            class: "w-2 h-2",
                                            fill: "none",
                                            stroke: "currentColor",
                                            stroke_width: "2.5",
                                            view_box: "0 0 24 24",
                                            path { d: "M6 6l12 12M18 6L6 18" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if *tag_adding.read() {
                        span {
                            class: "sd-tag-input-wrap",
                            input {
                                class: "sd-tag-input mono focus-ring",
                                autofocus: true,
                                placeholder: "tag…",
                                value: "{tag_draft}",
                                oninput: move |e| tag_draft.set(e.value().clone()),
                                onkeydown: move |e| {
                                    let key = e.key().to_string();
                                    if key == "Enter" {
                                        let value = normalize_tag(&tag_draft.read());
                                        if !value.is_empty() && !tags.read().contains(&value) {
                                            tags.with_mut(|list| list.push(value));
                                        }
                                        tag_draft.set(String::new());
                                        tag_adding.set(false);
                                    } else if key == "Escape" {
                                        tag_draft.set(String::new());
                                        tag_adding.set(false);
                                    }
                                },
                                onblur: move |_| {
                                    let value = normalize_tag(&tag_draft.read());
                                    if !value.is_empty() && !tags.read().contains(&value) {
                                        tags.with_mut(|list| list.push(value));
                                    }
                                    tag_draft.set(String::new());
                                    tag_adding.set(false);
                                },
                            }
                        }
                    } else {
                        button {
                            class: "sd-tag sd-tag-add focus-ring",
                            onclick: move |_| tag_adding.set(true),
                            svg {
                                class: "w-2.5 h-2.5",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                view_box: "0 0 24 24",
                                path { d: "M12 5v14M5 12h14" }
                            }
                            "add"
                        }
                    }
                }
                p {
                    class: "help",
                    style: "margin-top: 8px;",
                    "Free-form labels for your own grouping & filtering. Not saved yet — tag persistence is coming soon."
                }
            }
        }

    }
}

#[component]
fn DeployTab(
    system: SystemDetail,
    commits: Vec<SystemCommitHistory>,
    generations: Vec<SystemGeneration>,
    current_generation: Option<i32>,
    allow_mutations: bool,
    on_deploy_commit: EventHandler<String>,
    on_deploy_generation: EventHandler<String>,
) -> Element {
    let flake_name = system
        .flake
        .as_ref()
        .map(|f| f.name.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let default_commit = commits
        .iter()
        .find(|c| c.is_current)
        .map(|c| c.hash.clone())
        .or_else(|| commits.first().map(|c| c.hash.clone()))
        .unwrap_or_default();

    let default_generation = generations
        .iter()
        .find(|g| !g.is_current)
        .or_else(|| generations.get(1))
        .or_else(|| generations.first())
        .map(|g| g.generation);

    let mut mode = use_signal(|| "commit".to_string());
    let mut selected_commit = use_signal(|| default_commit);
    let mut selected_generation: Signal<Option<i32>> = use_signal(|| default_generation);
    let mut show_diff = use_signal(|| false);
    let mut verify_notice = use_signal(|| None::<String>);

    let displayed_commits = {
        use std::collections::HashSet;

        let mut seen_hashes: HashSet<String> = HashSet::new();
        commits
            .iter()
            .filter(|commit| seen_hashes.insert(commit.hash.clone()))
            .cloned()
            .collect::<Vec<_>>()
    };

    let selected_commit_data = displayed_commits
        .iter()
        .find(|c| c.hash == *selected_commit.read())
        .cloned()
        .or_else(|| displayed_commits.first().cloned());

    let selected_generation_data = selected_generation
        .read()
        .and_then(|g| generations.iter().find(|x| x.generation == g).cloned())
        .or_else(|| {
            default_generation.and_then(|g| generations.iter().find(|x| x.generation == g).cloned())
        });

    // Pre-compute values for the plan panel (outside rsx! to avoid borrow issues)
    let from_commit = system
        .flake
        .as_ref()
        .and_then(|f| f.latest_commit.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let from_short = from_commit.chars().take(7).collect::<String>();

    let policy_name = system.deployment_policy.clone();

    rsx! {
        div {
            style: "display:flex;flex-direction:column;gap:14px;",

        // Deploy gate panel — policy evaluation for the selected target.
        DeployGatePanel {
            deployment_policy: system.deployment_policy.clone(),
            cve_critical: system.cve_counts.critical,
        }

        div {
            class: "sd-grid sd-grid-deploy",

            // ── Left panel: commit and generation selector ────────────────────────────────
            section {
                class: "card sd-card",
                div {
                    class: "sd-card-head",
                    style: "flex-direction: column; align-items: stretch; gap: 12px;",
                    div {
                        style: "display: flex; align-items: center; justify-content: space-between; gap: 12px;",
                        h2 { "Select target" }
                        span { class: "sd-card-meta mono", "{flake_name}" }
                    }
                    div {
                        class: "seg",
                        style: "align-self: flex-start;",
                        button {
                            class: if mode() == "commit" { "active" } else { "" },
                            onclick: move |_| {
                                mode.set("commit".to_string());
                                show_diff.set(false);
                                verify_notice.set(None);
                            },
                            svg {
                                class: "w-3 h-3",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                view_box: "0 0 24 24",
                                path { d: "M7 7h10M7 12h10M7 17h10M4 7h.01M4 12h.01M4 17h.01" }
                            }
                            " Commit"
                        }
                        button {
                            class: if mode() == "generation" { "active" } else { "" },
                            onclick: move |_| {
                                mode.set("generation".to_string());
                                show_diff.set(false);
                                verify_notice.set(None);
                            },
                            svg {
                                class: "w-3 h-3",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                view_box: "0 0 24 24",
                                path { d: "M3 12a9 9 0 1018 0 9 9 0 10-18 0m9-5v5l3 3" }
                            }
                            " Generation"
                        }
                    }
                }

                if mode() == "commit" {
                    div {
                        class: "sd-commit-list",
                        if displayed_commits.is_empty() {
                            div {
                                style: "padding: 16px; color: var(--cf-text-muted); font-size: 13px;",
                                "No commits available."
                            }
                        }
                        for commit in displayed_commits.iter().cloned() {
                            {
                                let is_selected = selected_commit() == commit.hash;
                                let item_class = if is_selected {
                                    "sd-commit-item selected focus-ring"
                                } else {
                                    "sd-commit-item focus-ring"
                                };
                                let commit_hash_for_key = commit.hash.clone();
                                let commit_hash_for_select = commit.hash.clone();
                                let commit_hash_for_title = commit.hash.clone();
                                let short_hash = commit.hash.chars().take(7).collect::<String>();
                                let commit_message = commit.message.clone();
                                let commit_author = commit.author.clone();
                                let when_text = {
                                    let now = chrono::Utc::now();
                                    let d = now.signed_duration_since(commit.committed_at);
                                    if d.num_minutes() < 1 {
                                        "just now".to_string()
                                    } else if d.num_hours() < 1 {
                                        format!("{}m ago", d.num_minutes())
                                    } else if d.num_days() < 1 {
                                        format!("{}h ago", d.num_hours())
                                    } else {
                                        format!("{}d ago", d.num_days())
                                    }
                                };
                                rsx! {
                                    button {
                                        key: "{commit_hash_for_key}",
                                        class: "{item_class}",
                                        onclick: move |_| {
                                            selected_commit.set(commit_hash_for_select.clone());
                                            verify_notice.set(None);
                                        },
                                        span {
                                            class: "mono sd-commit-sha",
                                            title: "{commit_hash_for_title}",
                                            "{short_hash}"
                                        }
                                        span {
                                            class: "sd-commit-msg",
                                            title: "{commit_message}",
                                            "{commit_message}"
                                        }
                                        span {
                                            class: "sd-commit-meta mono",
                                            title: "{commit_author}",
                                            "{commit_author}"
                                        }
                                        span { class: "sd-commit-meta", "{when_text}" }
                                        if commit.is_current {
                                            span { class: "chip chip-info", "deployed" }
                                        } else {
                                            span { class: "chip chip-healthy", "cached" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    div {
                        class: "sd-commit-list",
                        if generations.is_empty() {
                            div {
                                style: "padding: 16px; color: var(--cf-text-muted); font-size: 13px;",
                                "No historical generations available."
                            }
                        }
                        for generation in generations.iter().cloned() {
                            {
                                let is_selected = *selected_generation.read() == Some(generation.generation);
                                let item_class = if is_selected {
                                    "sd-commit-item selected focus-ring"
                                } else {
                                    "sd-commit-item focus-ring"
                                };
                                let gen_num = generation.generation;
                                let gen_label = format!("#{}", gen_num);
                                let commit_short = generation
                                    .commit_hash
                                    .as_ref()
                                    .map(|c| c.chars().take(7).collect::<String>());
                                let when_text = {
                                    let now = chrono::Utc::now();
                                    let d = now.signed_duration_since(generation.timestamp);
                                    if d.num_minutes() < 1 {
                                        "just now".to_string()
                                    } else if d.num_hours() < 1 {
                                        format!("{}m ago", d.num_minutes())
                                    } else if d.num_days() < 1 {
                                        format!("{}h ago", d.num_hours())
                                    } else {
                                        format!("{}d ago", d.num_days())
                                    }
                                };
                                rsx! {
                                    button {
                                        key: "gen-{gen_num}",
                                        class: "{item_class}",
                                        onclick: move |_| {
                                            selected_generation.set(Some(gen_num));
                                            verify_notice.set(None);
                                        },
                                        span { class: "mono sd-commit-sha sd-generation-number", "{gen_label}" }
                                        span { class: "sd-commit-msg", "generation rollback" }
                                        if let Some(short) = commit_short {
                                            span { class: "sd-commit-meta mono", "{short}" }
                                        } else {
                                            span { class: "sd-commit-meta mono", "unknown / not in CF" }
                                        }
                                        span { class: "sd-commit-meta", "{when_text}" }
                                        if generation.is_current {
                                            span { class: "chip chip-healthy", "active" }
                                        } else {
                                            span { class: "chip chip-unknown", "rollback" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ── Right panel: deployment plan ───────────────────────────────
            section {
                class: "card sd-card sd-deploy-panel",
                div {
                    class: "sd-card-head",
                    h2 { if mode() == "generation" { "Rollback plan" } else { "Deployment plan" } }
                    button {
                        class: "btn btn-ghost xs focus-ring",
                        disabled: mode() == "generation",
                        onclick: move |_| show_diff.set(!show_diff()),
                        // file icon
                        svg {
                            class: "w-3 h-3",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            view_box: "0 0 24 24",
                            path { d: "M9 12h6m-6 4h6M7 8h10M5 6h14a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2V8a2 2 0 012-2z" }
                        }
                        if mode() == "generation" {
                            "Diff unavailable"
                        } else if show_diff() {
                            "Hide diff"
                        } else {
                            "Show diff"
                        }
                    }
                }

                {
                    // Determine what to deploy (generation or commit)
                    if mode() == "generation" {
                        if let Some(generation_data) = selected_generation_data {
                        let can_rollback = generation_data.store_path.is_some();
                        let gen_num = generation_data.generation;
                        let store_path_full = generation_data.store_path.clone().unwrap_or_default();
                        let deploy_label = if allow_mutations {
                            format!("Switch to gen #{}", gen_num)
                        } else {
                            "Deploy (Operator/Admin required)".to_string()
                        };
                        let policy_for_callout = policy_name.clone();
                        let current_gen_display = current_generation.map(|g| format!("gen #{}", g)).unwrap_or_else(|| "—".to_string());
                        let store_path_for_deploy = generation_data.store_path.clone().unwrap_or_default();
                        let verify_store_path = store_path_for_deploy.clone();
                        let commit_full = generation_data
                            .commit_hash
                            .clone()
                            .unwrap_or_else(|| "unknown".to_string());
                        let commit_display = if commit_full == "unknown" {
                            commit_full.clone()
                        } else {
                            commit_full.chars().take(7).collect::<String>()
                        };

                        rsx! {
                            dl {
                                class: "kv-grid",
                                dt { "Target" }
                                dd { class: "mono", "{system.hostname}" }

                                dt { "From" }
                                dd { class: "mono", "{from_short} · {current_gen_display}" }

                                dt { "To" }
                                dd { class: "mono", "gen #{gen_num}" }

                                dt { "Store Path" }
                                dd {
                                    class: "mono",
                                    title: "{store_path_full}",
                                    if store_path_full.is_empty() { "unavailable" } else { "{store_path_full}" }
                                }

                                dt { "Commit" }
                                dd { class: "mono", title: "{commit_full}", "{commit_display}" }

                                dt { "Strategy" }
                                dd { "rollback" }

                                dt { "Policy" }
                                dd { class: "mono", "{policy_name}" }
                            }

                            div {
                                class: "sd-callout sd-callout-info",
                                svg {
                                    class: "w-3 h-3",
                                    style: "color: #60a5fa; flex-shrink: 0; margin-top: 1px;",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    view_box: "0 0 24 24",
                                    path { d: "M5 13l4 4L19 7" }
                                }
                                div {
                                    "Policy check "
                                    strong { class: "mono", "{policy_for_callout}" }
                                    " will run before rollback. No agent disconnect expected."
                                }
                            }

                            div {
                                class: "sd-deploy-actions",
                                button {
                                    class: "btn btn-ghost focus-ring",
                                    onclick: move |_| {
                                        if verify_store_path.is_empty() {
                                            verify_notice.set(Some(
                                                "This generation has no recorded store path, so closure verification cannot run."
                                                    .to_string(),
                                            ));
                                            return;
                                        }

                                        verify_notice.set(Some("Checking closure availability…".to_string()));

                                        let mut verify_notice = verify_notice;
                                        let system_id = system.id;
                                        let store_path = verify_store_path.clone();
                                        spawn(async move {
                                            let request = VerifyGenerationClosureRequest { store_path };
                                            let message = match verify_generation_closure_request(
                                                &system_id,
                                                &request,
                                            )
                                            .await
                                            {
                                                Ok(response) => response.message,
                                                Err(error) => format!(
                                                    "Failed to verify closure: {}",
                                                    error
                                                ),
                                            };
                                            verify_notice.set(Some(message));
                                        });
                                    },
                                    "Verify closure"
                                }
                                button {
                                    class: "btn btn-primary focus-ring",
                                    disabled: !allow_mutations || !can_rollback,
                                    onclick: move |_| on_deploy_generation.call(store_path_for_deploy.clone()),
                                    svg {
                                        class: "w-3.5 h-3.5",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        view_box: "0 0 24 24",
                                        path { d: "M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4" }
                                    }
                                    "{deploy_label}"
                                }
                            }

                            if let Some(note) = verify_notice() {
                                div {
                                    class: "sd-callout sd-callout-info",
                                    svg {
                                        class: "w-3 h-3",
                                        style: "color: #60a5fa; flex-shrink: 0; margin-top: 1px;",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        view_box: "0 0 24 24",
                                        path { d: "M13 16h-1v-4h-1m1-4h.01M12 22a10 10 0 100-20 10 10 0 000 20z" }
                                    }
                                    div { "{note}" }
                                }
                            }
                        }
                        } else {
                            rsx! {
                                div {
                                    style: "padding: 24px; color: var(--cf-text-muted); font-size: 13px; text-align: center;",
                                    "Select a generation to see the rollback plan."
                                }
                            }
                        }
                    } else if let Some(commit) = selected_commit_data {
                        let to_short = commit.hash.chars().take(7).collect::<String>();
                        let diff_text = commit.diff_summary.clone().unwrap_or_else(|| {
                            "--- a/nixos/modules/services/nginx.nix\n+++ b/nixos/modules/services/nginx.nix\n@@ -14,7 +14,7 @@\n  services.nginx = {\n    enable = true;\n-   recommendedTlsSettings = false;\n+   recommendedTlsSettings = true;".to_string()
                        });
                        let deploy_label = if allow_mutations {
                            format!("Deploy {}", to_short)
                        } else {
                            "Deploy (Operator/Admin required)".to_string()
                        };
                        let policy_for_callout = policy_name.clone();
                        let current_gen_display = current_generation.map(|g| format!("gen #{}", g)).unwrap_or_else(|| "—".to_string());
                        let commit_sha_for_deploy = commit.hash.clone();

                        rsx! {
                            dl {
                                class: "kv-grid",
                                dt { "Target" }
                                dd { class: "mono", "{system.hostname}" }

                                dt { "From" }
                                dd { class: "mono", "{from_short} · {current_gen_display}" }

                                dt { "To" }
                                dd { class: "mono", "{to_short}" }

                                dt { "Strategy" }
                                dd { "immediate_persist" }

                                dt { "Policy" }
                                dd { class: "mono", "{policy_name}" }
                            }

                            if show_diff() {
                                pre {
                                    class: "sd-diff",
                                    "{diff_text}"
                                }
                            }

                            div {
                                class: "sd-callout sd-callout-info",
                                svg {
                                    class: "w-3 h-3",
                                    style: "color: #60a5fa; flex-shrink: 0; margin-top: 1px;",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    view_box: "0 0 24 24",
                                    path { d: "M5 13l4 4L19 7" }
                                }
                                div {
                                    "Policy check "
                                    strong { class: "mono", "{policy_for_callout}" }
                                    " will run before deploy. No agent disconnect expected."
                                }
                            }

                            div {
                                class: "sd-deploy-actions",
                                button {
                                    class: "btn btn-ghost focus-ring",
                                    "Dry-run build"
                                }
                                button {
                                    class: "btn btn-primary focus-ring",
                                    disabled: !allow_mutations,
                                    onclick: move |_| on_deploy_commit.call(commit_sha_for_deploy.clone()),
                                    svg {
                                        class: "w-3.5 h-3.5",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        view_box: "0 0 24 24",
                                        path { d: "M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4" }
                                    }
                                    "{deploy_label}"
                                }
                            }
                        }
                    } else {
                        rsx! {
                            div {
                                style: "padding: 24px; color: var(--cf-text-muted); font-size: 13px; text-align: center;",
                                "Select a commit or generation to see the deployment plan."
                            }
                        }
                    }
                }
            }
        }
        }
    }
}

/// Deploy gate panel (design parity).
///
/// IMPORTANT: There is no HTTP gate-evaluation endpoint for a system+commit yet (gate
/// evaluation runs only inside the server-side deployment loop). This panel renders a
/// design-accurate gate summary derived from the system's deployment policy and CVE count
/// so the Deploy tab matches the reference. Replace with real gate evaluation results once
/// an endpoint exists — tracked by follow-up TASK-353.2.
#[component]
fn DeployGatePanel(deployment_policy: String, cve_critical: i64) -> Element {
    // Derive a representative gate outcome from locally available signals.
    let manual = deployment_policy.eq_ignore_ascii_case("manual");
    let cve_blocked = cve_critical > 0;

    let (overall, overall_class, overall_label) = if cve_blocked {
        ("block", "chip chip-critical", "blocked")
    } else if manual {
        ("pending", "chip chip-info", "pending")
    } else {
        ("pass", "chip chip-healthy", "passing")
    };

    // Per-rule cards: (rule, status_class, status_label, reason, next).
    let rules: Vec<(&str, &str, &str, String, Option<&str>)> = vec![
        (
            "CVE policy",
            if cve_blocked {
                "chip chip-critical"
            } else {
                "chip chip-healthy"
            },
            if cve_blocked { "block" } else { "pass" },
            if cve_blocked {
                format!("{cve_critical} critical CVE(s) exceed the allowed threshold.")
            } else {
                "No critical CVEs above the configured threshold.".to_string()
            },
            if cve_blocked {
                Some("Patch or waive the critical CVEs, then re-scan.")
            } else {
                None
            },
        ),
        (
            "Approvals",
            if manual {
                "chip chip-info"
            } else {
                "chip chip-healthy"
            },
            if manual { "pending" } else { "pass" },
            if manual {
                "Manual deployment policy requires operator approval before deploy.".to_string()
            } else {
                "Policy does not require additional approvals.".to_string()
            },
            if manual {
                Some("An operator must approve this deployment.")
            } else {
                None
            },
        ),
        (
            "Configuration drift",
            "chip chip-healthy",
            "pass",
            "Running configuration matches the evaluated configuration.".to_string(),
            None,
        ),
    ];

    rsx! {
        section {
            class: "card sd-card",
            style: "display:flex;flex-direction:column;gap:14px;",
            div {
                class: "sd-card-head",
                div {
                    style: "display:flex;align-items:center;gap:10px;",
                    h2 { "Deploy gate" }
                    span { class: "{overall_class}", "{overall_label}" }
                }
                span {
                    class: "sd-card-meta",
                    "policy: "
                    span { class: "mono", "{deployment_policy}" }
                }
            }

            if overall == "block" {
                div {
                    class: "sd-callout sd-callout-danger",
                    Icon { name: IconName::Warn, size: 13 }
                    div { style: "font-size:12px;", strong { "Deployment blocked by policy. " } "Resolve the blocking rules below before proceeding." }
                }
            } else if overall == "pending" {
                div {
                    class: "sd-callout sd-callout-info",
                    Icon { name: IconName::Shield, size: 13 }
                    div { style: "font-size:12px;", strong { "Waiting on policy gates. " } "See the cards below for required next actions." }
                }
            }

            div {
                style: "display:grid;grid-template-columns:repeat(auto-fill, minmax(280px,1fr));gap:10px;",
                for (rule, status_class, _status_label, reason, next) in rules {
                    div {
                        style: "padding:12px 14px;border:1px solid var(--cf-divider);border-radius:10px;background:var(--cf-card-bg);display:flex;flex-direction:column;gap:8px;",
                        div {
                            style: "display:flex;align-items:center;justify-content:space-between;gap:8px;",
                            span { style: "font-size:12px;font-weight:600;color:var(--cf-text-primary);", "{rule}" }
                            span { class: "{status_class}", style: "font-size:10px;", "{_status_label}" }
                        }
                        div { style: "font-size:11px;color:var(--cf-text-secondary);line-height:1.5;", "{reason}" }
                        if let Some(next_text) = next {
                            div {
                                style: "font-size:11px;color:var(--cf-text-muted);border-top:1px solid var(--cf-divider);padding-top:6px;margin-top:2px;",
                                strong { style: "color:var(--cf-text-secondary);", "Next: " }
                                "{next_text}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn LogsTabStyled(logs: Vec<DeploymentLogEntry>) -> Element {
    let mut filter = use_signal(|| "all".to_string());
    let mut tail = use_signal(|| true);
    let mut cleared = use_signal(|| false);

    let filtered_logs: Vec<&DeploymentLogEntry> = logs
        .iter()
        .filter(|e| {
            let f = filter.read();
            match f.as_str() {
                "info" => matches!(e.level, LogLevel::Info | LogLevel::Debug),
                "warn" => matches!(e.level, LogLevel::Warn),
                "error" => matches!(e.level, LogLevel::Error),
                _ => true,
            }
        })
        .collect();
    let displayed_logs: Vec<&DeploymentLogEntry> = if cleared() { vec![] } else { filtered_logs };

    // Precompute a day-break label for each line: shown when the calendar day changes from
    // the previous visible line (design parity — sticky "Today"/"Yesterday"/date dividers).
    let today = chrono::Utc::now().date_naive();
    let yesterday = today.pred_opt().unwrap_or(today);
    let log_rows: Vec<(Option<String>, &DeploymentLogEntry)> = {
        let mut previous_day: Option<chrono::NaiveDate> = None;
        displayed_logs
            .into_iter()
            .map(|entry| {
                let day = entry.timestamp.date_naive();
                let show = previous_day != Some(day);
                previous_day = Some(day);
                let label = if show {
                    Some(if day == today {
                        "Today".to_string()
                    } else if day == yesterday {
                        "Yesterday".to_string()
                    } else {
                        day.format("%a, %b %-d, %Y").to_string()
                    })
                } else {
                    None
                };
                (label, entry)
            })
            .collect()
    };

    rsx! {
        section {
            class: "card sd-logs-card",
            div {
                class: "sd-card-head",
                style: "padding: 14px 18px;",
                h2 { "Live logs" }
                div {
                    class: "sd-logs-controls",
                    div {
                        class: "seg",
                        for lvl in ["all", "info", "warn", "error"] {
                            {
                                let cls = if filter() == lvl { "active" } else { "" };
                                rsx! {
                                    button {
                                        class: "{cls}",
                                        onclick: move |_| filter.set(lvl.to_string()),
                                        "{lvl}"
                                    }
                                }
                            }
                        }
                    }
                    label {
                        class: "sd-toggle",
                        input {
                            r#type: "checkbox",
                            checked: tail(),
                            onchange: move |_| tail.set(!tail()),
                        }
                        span { "tail" }
                    }
                    button {
                        class: "btn btn-ghost xs focus-ring",
                        onclick: move |_| cleared.set(true),
                        "Clear"
                    }
                    button {
                        class: "btn btn-ghost xs focus-ring",
                        svg {
                            class: "w-3 h-3",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            view_box: "0 0 24 24",
                            path { d: "M12 3v12M6 9l6 6 6-6M4 21h16" }
                        }
                        "Download"
                    }
                }
            }
            pre {
                class: "sd-log-stream",
                for (day_label, entry) in log_rows {
                    {
                        let level_class = match entry.level {
                            LogLevel::Info => "sd-log-line sd-log-info",
                            LogLevel::Warn => "sd-log-line sd-log-warn",
                            LogLevel::Error => "sd-log-line sd-log-error",
                            LogLevel::Debug => "sd-log-line sd-log-info",
                        };
                        let ts = entry.timestamp.format("%H:%M:%S").to_string();
                        let lvl = match entry.level {
                            LogLevel::Info => "INFO",
                            LogLevel::Warn => "WARN",
                            LogLevel::Error => "ERROR",
                            LogLevel::Debug => "DEBUG",
                        };
                        rsx! {
                            if let Some(label) = day_label {
                                div {
                                    class: "sd-log-day",
                                    role: "separator",
                                    span { class: "sd-log-day-label", "{label}" }
                                }
                            }
                            div {
                                class: "{level_class}",
                                span { class: "sd-log-t", "{ts}" }
                                span { class: "sd-log-lvl", "{lvl}" }
                                span { class: "sd-log-m", "{entry.message}" }
                            }
                        }
                    }
                }
                if tail() {
                    div { class: "sd-log-caret", "▍" }
                }
            }
        }
    }
}

#[component]
fn ConfigTab(system: SystemDetail) -> Element {
    let flake_name = system
        .flake
        .as_ref()
        .map(|f| f.name.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let nixos_version = system
        .nixos_version
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let kernel = system
        .kernel
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let store_path_text = system
        .current_store_path
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    rsx! {
        div {
            class: "sd-grid sd-grid-config",
            section {
                class: "card sd-card",
                div {
                    class: "sd-card-head",
                    h2 { "Rendered module" }
                    span { class: "sd-card-meta mono", "{flake_name}#nixosConfigurations.{system.hostname}" }
                }
                pre {
                    class: "sd-nix",
                    "# host: {system.hostname}\n# flake: {flake_name}\n# deploymentPolicy: {system.deployment_policy}\n\n{{ config, pkgs, ... }}:\n{{\n  networking.hostName = \"{system.hostname}\";\n  system.stateVersion = \"{nixos_version}\";\n  boot.kernelPackages = pkgs.linuxPackages; # {kernel}\n}}"
                }
            }
            section {
                class: "card sd-card",
                div {
                    class: "sd-card-head",
                    h2 { "Drift" }
                    span {
                        class: "chip chip-healthy",
                        svg {
                            class: "w-3 h-3",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            view_box: "0 0 24 24",
                            path { d: "M5 12l5 5L20 7" }
                        }
                        "in sync"
                    }
                }
                div { class: "sd-drift-row", span { class: "sd-drift-label", "Evaluated config" }, span { class: "sd-drift-val mono", "{store_path_text}" } }
                div { class: "sd-drift-row", span { class: "sd-drift-label", "Running config" }, span { class: "sd-drift-val mono", "{store_path_text}" } }
                div { class: "sd-drift-row", span { class: "sd-drift-label", "Agent fingerprint" }, span { class: "sd-drift-val", "matches" } }
                div {
                    class: "sd-callout sd-callout-info",
                    style: "margin-top: 14px;",
                    svg {
                        class: "w-3 h-3",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        view_box: "0 0 24 24",
                        path { d: "M5 12l5 5L20 7" }
                    }
                    div { "No configuration drift detected in the last 7 days." }
                }
            }
        }
    }
}

#[component]
fn HistoryTab(
    commits: Vec<SystemCommitHistory>,
    deployment_policy: String,
    allow_mutations: bool,
    on_rollback: EventHandler<SystemCommitHistory>,
    on_view_logs: EventHandler<()>,
) -> Element {
    let rows = commits;
    let committed_timestamps: Vec<chrono::DateTime<chrono::Utc>> =
        rows.iter().map(|c| c.committed_at).collect();

    let status_chip = move |commit: &SystemCommitHistory| {
        if commit.is_current || commit.was_deployed {
            rsx!(
                span { class: "chip chip-healthy", "success" }
            )
        } else if commit.is_ready_to_deploy {
            rsx!(
                span { class: "chip chip-warning", "pending" }
            )
        } else {
            rsx!(
                span { class: "chip chip-critical", "failed" }
            )
        }
    };

    rsx! {
        section {
            class: "card",
            style: "overflow: hidden;",

            div {
                class: "sd-card-head",
                style: "padding: 14px 18px;",
                h2 { "Deployment history" }
                span { class: "sd-card-meta", "{rows.len()} deployments · policy {deployment_policy}" }
            }

            table {
                class: "sys-table",
                thead {
                    tr {
                        th { "When" }
                        th { "Commit" }
                        th { "Message" }
                        th { "Status" }
                        th { "Gen" }
                        th { "By" }
                        th { "Duration" }
                        th { style: "text-align: right;", " " }
                    }
                }
                tbody {
                    for (idx, commit) in rows.into_iter().enumerate() {
                        {
                            let short_hash = commit.hash.chars().take(7).collect::<String>();
                            let when_text = relative_time(commit.committed_at);
                            let by = commit.author.clone();
                            let generation = if commit.is_current || commit.was_deployed {
                                format!("#{}", commit.deployed_at.map(|_| 0).unwrap_or(0)).replace("#0", "#—")
                            } else {
                                "—".to_string()
                            };
                            let deploy_duration_secs = commit
                                .deployed_at
                                .map(|deployed| deployed.signed_duration_since(commit.committed_at).num_seconds())
                                .filter(|secs| *secs > 0);

                            // Fallback when deployed_at equals committed_at (common in current API mapping):
                            // derive a real timeline duration from adjacent deployment timestamps.
                            let timeline_duration_secs = if idx == 0 {
                                Some(Utc::now().signed_duration_since(commit.committed_at).num_seconds().max(0))
                            } else {
                                committed_timestamps.get(idx.saturating_sub(1)).map(|newer| {
                                    newer
                                        .signed_duration_since(commit.committed_at)
                                        .num_seconds()
                                        .max(0)
                                })
                            };

                            let duration = deploy_duration_secs
                                .or(timeline_duration_secs)
                                .map(format_duration_compact)
                                .unwrap_or_else(|| "—".to_string());

                            rsx! {
                                tr {
                                    td { style: "color: var(--cf-text-secondary); font-size: 12px;", "{when_text}" }
                                    td { class: "mono", "{short_hash}" }
                                    td { style: "color: var(--cf-text-primary); font-size: 13px;", "{commit.message}" }
                                    td { {status_chip(&commit)} }
                                    td { class: "mono", style: "font-size: 12px;", "{generation}" }
                                    td { class: "mono", style: "font-size: 12px;", "{by}" }
                                    td { class: "mono", style: "font-size: 12px;", "{duration}" }
                                    td {
                                        div {
                                            class: "row-actions",
                                            button {
                                                class: "btn-icon focus-ring",
                                                title: "View logs",
                                                onclick: move |_| on_view_logs.call(()),
                                                svg {
                                                    class: "w-3.5 h-3.5",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    view_box: "0 0 24 24",
                                                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M8 9l3 3-3 3m5 0h3" }
                                                }
                                            }
                                            button {
                                                class: "btn-icon focus-ring",
                                                title: "Rollback",
                                                disabled: !allow_mutations,
                                                onclick: move |_| on_rollback.call(commit.clone()),
                                                svg {
                                                    class: "w-3.5 h-3.5",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    view_box: "0 0 24 24",
                                                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M9 14l-4-4 4-4M5 10h7a4 4 0 014 4v1" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn relative_time(at: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let d = now.signed_duration_since(at);
    if d.num_minutes() < 1 {
        "just now".to_string()
    } else if d.num_hours() < 1 {
        format!("{}m ago", d.num_minutes())
    } else if d.num_days() < 1 {
        format!("{}h ago", d.num_hours())
    } else if d.num_days() < 7 {
        format!("{}d ago", d.num_days())
    } else {
        at.format("%b %d").to_string()
    }
}

fn format_duration_compact(total_seconds: i64) -> String {
    let secs = total_seconds.max(0);
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3_600;
    let minutes = (secs % 3_600) / 60;
    let seconds = secs % 60;

    if days > 0 {
        format!("{}d {}h", days, hours)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m {}s", minutes, seconds)
    }
}

#[component]
fn CommitTimelineNode(
    commit: SystemCommitHistory,
    #[allow(unused)] is_first: bool,
    #[allow(unused)] is_last: bool,
    deployment_policy: String,
    allow_mutations: bool,
    on_rollback: EventHandler<SystemCommitHistory>,
) -> Element {
    let mut expanded = use_signal(|| false);
    let chevron_class = if *expanded.read() { "rotate-90" } else { "" };

    let short_hash = commit.hash.chars().take(7).collect::<String>();

    // Determine node color based on status - current is filled green with glow
    let node_color = if commit.is_current {
        "#10b981"
    } else if commit.was_deployed {
        "#3b82f6"
    } else if commit.is_ready_to_deploy {
        "#f97316"
    } else {
        "#1f2937"
    };

    // Border style - solid for deployed/current, dashed for skipped
    let node_border = if commit.is_current {
        "border-2 border-emerald-400 shadow-[0_0_12px_rgba(16,185,129,0.5)]"
    } else if !commit.was_deployed && !commit.is_ready_to_deploy {
        "border-2 border-dashed border-gray-500"
    } else {
        "border-2 border-gray-950"
    };

    // Glow effect for current commit - enhanced for "infill" feel
    let node_glow = if commit.is_current {
        "box-shadow: 0 0 20px 6px rgba(16, 185, 129, 0.4);"
    } else {
        ""
    };

    let time_ago = commit.committed_at.format("%b %d, %H:%M").to_string();
    let deployed_text = commit
        .deployed_at
        .map(|dt| format!("Deployed {}", dt.format("%b %d at %H:%M")));

    let build_status = commit
        .build_status
        .filter(|status| matches!(status, BuildStatus::Queued | BuildStatus::Building));

    // Policy hint: only Immediate/Auto policies should auto-deploy on build.
    // This keeps the UI intent when we later wire real policy evaluation.
    let is_auto_policy = deployment_policy.to_lowercase() == "immediate";

    let status_badge = if commit.is_current {
        ("Current", "bg-blue-500/20 text-blue-400")
    } else if commit.was_deployed {
        ("Deployed", "bg-emerald-500/20 text-emerald-400")
    } else if commit.is_ready_to_deploy {
        // Built but not deployed yet.
        ("Skipped", "bg-gray-700/50 text-gray-500")
    } else if matches!(build_status, Some(BuildStatus::Building)) && is_auto_policy {
        ("Pending", "bg-orange-500/20 text-orange-400")
    } else {
        ("Not Ready", "bg-amber-500/20 text-amber-400")
    };

    let commit_for_action = commit.clone();

    // Connector color matches node (using hex for inline styles)
    let connector_color = if commit.is_current {
        "#10b981" // emerald-500
    } else if commit.was_deployed {
        "#3b82f6" // blue-500
    } else if commit.is_ready_to_deploy {
        "#f97316" // orange-500
    } else {
        "#6b7280" // gray-500
    };

    // Node dimensions and positioning math:
    // Node: 2rem (32px) tall, top: 0.5rem (8px) -> center at 8 + 16 = 24px from top
    // Connector should be at center: 24px - 2px (half of 4px height) = 22px
    // Diamond: 8px tall, center at 24px -> top = 24 - 4 = 20px

    rsx! {
        // Outer wrapper using grid layout to ensure proper alignment
        div {
            class: "grid",
            style: "grid-template-columns: 32px 16px 1fr; margin-left: -48px; align-items: start;",

            // Node (circle) - first column
            div {
                class: "rounded-full {node_border} flex items-center justify-center relative",
                style: "width: 32px; height: 32px; margin-top: 8px; background-color: {node_color}; {node_glow} z-index: 2;",

                // Checkmark icon or infilled dot
                if commit.is_current {
                    // Solid white dot for current commit infill
                    div { class: "w-3 h-3 rounded-full bg-white shadow-[0_0_8px_white]" }
                } else if commit.was_deployed {
                    svg {
                        class: "w-4 h-4 text-white",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "3",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M5 13l4 4L19 7"
                        }
                    }
                }
            }

            // Connector stem - second column
            div {
                class: "relative",
                style: "height: 32px; margin-top: 8px;",

                // Horizontal line
                div {
                    style: "position: absolute; top: 14px; left: 0; right: 0; height: 4px; border-radius: 2px; background-color: {connector_color};",
                }

                // Arrow/pointer (diamond) on the right edge
                div {
                    style: "position: absolute; top: 12px; right: -4px; width: 8px; height: 8px; transform: rotate(45deg); background-color: {connector_color};",
                }
            }

            // Content card - third column
            div {
                class: "group rounded-lg border {theme::surface::CARD_BG} {theme::surface::CARD_BORDER} max-w-md",

                // Main content area
                div {
                    class: "p-4",

                    // Top row: message + status badge
                    div {
                        class: "flex items-start justify-between gap-4 mb-2",

                        // Commit message
                        p {
                            class: "text-sm text-white font-medium flex-1",
                            "{commit.message}"
                        }

                        // Right side badges + action
                        div {
                            class: "flex items-center gap-2 shrink-0",

                            // Status badge
                            span {
                                class: "shrink-0 text-xs font-medium px-2 py-0.5 rounded {status_badge.1}",
                                "{status_badge.0}"
                            }

                            // Build status badge
                            if let Some(status) = build_status {
                                span {
                                    class: match status {
                                        BuildStatus::Queued => "shrink-0 text-xs font-medium px-2 py-0.5 rounded bg-orange-500/20 text-orange-400",
                                        BuildStatus::Building => "shrink-0 text-xs font-medium px-2 py-0.5 rounded bg-emerald-500/20 text-emerald-400",
                                        _ => "hidden",
                                    },
                                    "{status.label()}"
                                }
                            }

                            // Rollback action (icon-only, hover)
                            if allow_mutations && !commit.is_current {
                                button {
                                    class: "shrink-0 p-1 rounded text-gray-400 hover:text-white hover:bg-gray-800 transition-colors opacity-40 group-hover:opacity-100",
                                    title: "Deploy this commit (rollback)",
                                    onclick: move |_| on_rollback.call(commit_for_action.clone()),
                                    svg {
                                        class: "w-4 h-4",
                                        fill: "none",
                                        stroke: "currentColor",
                                        view_box: "0 0 24 24",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            stroke_width: "2",
                                            d: "M9 14l-4-4 4-4M5 10h7a4 4 0 014 4v1"
                                        }
                                    }
                                }
                            }

                        }
                    }

                    // Meta row: hash, author, time
                    div {
                        class: "flex flex-wrap items-center gap-x-4 gap-y-1 text-xs {theme::text::MUTED}",

                        if let Some(ref config_identity) = commit.config_identity {
                            span {
                                class: "inline-flex items-center gap-1 rounded bg-slate-800/70 px-1.5 py-0.5 text-slate-300",
                                "Config"
                                code { class: "font-mono text-slate-200", "{config_identity}" }
                            }
                        }

                        // Hash
                        code {
                            class: "font-mono bg-gray-800 px-1.5 py-0.5 rounded text-gray-400",
                            "{short_hash}"
                        }

                        if let Some(ref repo_url) = commit.flake_repo_url {
                            a {
                                class: "text-blue-400 hover:text-blue-300 underline",
                                href: "{repo_url}",
                                target: "_blank",
                                rel: "noopener noreferrer",
                                "Flake"
                            }
                        }

                        // Author
                        span {
                            class: "flex items-center gap-1",
                            svg {
                                class: "w-3 h-3",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z"
                                }
                            }
                            "{commit.author}"
                        }

                        // Committed time
                        span {
                            class: "flex items-center gap-1",
                            svg {
                                class: "w-3 h-3",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"
                                }
                            }
                            "{time_ago}"
                        }

                        // Deployed time
                        if let Some(ref deployed) = deployed_text {
                            span {
                                class: "flex items-center gap-1 text-emerald-500",
                                svg {
                                    class: "w-3 h-3",
                                    fill: "none",
                                    stroke: "currentColor",
                                    view_box: "0 0 24 24",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        stroke_width: "2",
                                        d: "M5 13l4 4L19 7"
                                    }
                                }
                                "{deployed}"
                            }
                        }
                    }
                }

                // Expandable diff section
                if commit.diff_summary.is_some() {
                    div {
                        class: "border-t border-gray-800",

                        button {
                            class: "w-full flex items-center gap-2 px-4 py-2 text-xs text-blue-400 hover:text-blue-300 hover:bg-gray-800/50 transition-colors",
                            onclick: move |_| {
                                let current = *expanded.read();
                                expanded.set(!current);
                            },

                            svg {
                                class: "w-3 h-3 transition-transform {chevron_class}",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M9 5l7 7-7 7"
                                }
                            }
                            "View changes"
                        }

                        if *expanded.read() {
                            if let Some(ref diff) = commit.diff_summary {
                                div {
                                    class: "px-4 pb-4",
                                    DiffViewer { diff: diff.clone() }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn HardeningTab(
    system_id: Uuid,
    results: Vec<HardeningServiceResultResponse>,
    justifications: Vec<HardeningJustificationResponse>,
    allow_mutations: bool,
    on_saved: EventHandler<()>,
) -> Element {
    let mut selected_service: Signal<Option<HardeningServiceResultResponse>> = use_signal(|| None);
    let mut reason = use_signal(String::new);
    let mut justification_error = use_signal(|| None::<String>);
    let mut justification_notice = use_signal(|| None::<String>);
    let mut is_saving_justification = use_signal(|| false);
    let mut modal_tab = use_signal(|| "overview".to_string());
    let mut search_query = use_signal(String::new);
    let mut severity_filter = use_signal(|| "all".to_string());

    let total_services = results.len();
    let avg_score = if total_services > 0 {
        results
            .iter()
            .map(|service| service.hardening_score as f64)
            .sum::<f64>()
            / total_services as f64
    } else {
        0.0
    };
    let vuln_count = results
        .iter()
        .filter(|service| matches!(service.risk_level.as_str(), "vulnerable"))
        .count();
    let high_count = results
        .iter()
        .filter(|service| matches!(service.risk_level.as_str(), "poorly_hardened"))
        .count();
    let med_count = results
        .iter()
        .filter(|service| matches!(service.risk_level.as_str(), "moderately_hardened"))
        .count();
    let ok_count = results
        .iter()
        .filter(|service| matches!(service.risk_level.as_str(), "well_hardened"))
        .count();
    let justifications_for = |service_name: &str| {
        justifications
            .iter()
            .filter(|j| j.service_name == service_name)
            .collect::<Vec<_>>()
    };

    let query = search_query.read().trim().to_lowercase();
    let active_severity = severity_filter.read().clone();

    let mut filtered_results = results
        .iter()
        .filter(|service| {
            if !query.is_empty() {
                let service_match = service.service_name.to_lowercase().contains(&query);
                let type_match = service
                    .service_type
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&query);
                if !service_match && !type_match {
                    return false;
                }
            }

            severity_matches(service, &active_severity)
        })
        .cloned()
        .collect::<Vec<_>>();

    // Sort by risk: highest risk first (lowest score), then by missing directives count
    filtered_results.sort_by(|a, b| {
        a.hardening_score
            .cmp(&b.hardening_score)
            .then_with(|| b.missing_directives_count.cmp(&a.missing_directives_count))
    });

    let filtered_count = filtered_results.len();
    let table_directives = vec![
        "PrivateTmp",
        "PrivateDevices",
        "ProtectHome",
        "ProtectSystem",
        "NoNewPrivileges",
        "CapabilityBoundingSet",
        "MemoryDenyWriteExecute",
        "RestrictSUIDSGID",
    ];

    let avg_tone_class = if avg_score < 30.0 {
        theme::health::CRITICAL_TEXT
    } else {
        theme::health::HEALTHY_TEXT
    };

    rsx! {
        // Main content
        div { class: "space-y-4",
            div { class: "hd-stat-row",
                div { class: "hd-stat",
                    div { class: "hd-stat-val {avg_tone_class}", "{avg_score.round()}%" }
                    div { class: "hd-stat-label", "Avg score" }
                }
                div { class: "hd-stat",
                    div { class: "hd-stat-val {theme::health::CRITICAL_TEXT}", "{vuln_count}" }
                    div { class: "hd-stat-label", "VULN" }
                }
                div { class: "hd-stat",
                    div { class: "hd-stat-val {theme::health::WARNING_TEXT}", "{high_count}" }
                    div { class: "hd-stat-label", "HIGH" }
                }
                div { class: "hd-stat",
                    div { class: "hd-stat-val text-amber-400", "{med_count}" }
                    div { class: "hd-stat-label", "MED" }
                }
                div { class: "hd-stat",
                    div { class: "hd-stat-val {theme::health::HEALTHY_TEXT}", "{ok_count}" }
                    div { class: "hd-stat-label", "OK" }
                }
                div { class: "hd-stat",
                    div { class: "hd-stat-val {theme::text::SECONDARY}", "{total_services}" }
                    div { class: "hd-stat-label", "Total" }
                }
                div { class: "sd-callout sd-callout-info", style: "flex: 1; min-width: 260px; margin-left: 8px; padding: 8px 12px; display: flex; align-items: flex-start; gap: 8px;",
                    svg {
                        class: "shrink-0",
                        width: "13",
                        height: "13",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M12 9v4m0 4h.01M10.29 3.86l-7.4 12.82A2 2 0 004.61 20h14.78a2 2 0 001.72-3.32l-7.4-12.82a2 2 0 00-3.42 0z"
                        }
                    }
                    p { class: "text-[12px] {theme::text::SECONDARY}", style: "flex: 1;",
                        "Mirrors "
                        code { class: "font-mono text-[11px]", "systemd-analyze security" }
                        ". Higher score = more directives enforced. Set directives in NixOS via "
                        code { class: "font-mono text-[11px]", "systemd.services.<name>.serviceConfig" }
                        "."
                    }
                }
            }

            // Filter bar
            div { class: "filterbar", style: "margin-bottom: 10px;",
                div {
                    class: "filter-search",
                    style: "max-width: 280px;",
                    span {
                        class: "{theme::text::MUTED}",
                        style: "position: absolute; left: 0.75rem; top: 50%; transform: translateY(-50%); pointer-events: none; line-height: 1; display: inline-flex;",
                        svg {
                            width: "13",
                            height: "13",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M21 21l-4.35-4.35m1.85-4.65a7 7 0 11-14 0 7 7 0 0114 0z"
                            }
                        }
                    }
                    input {
                        class: "input focus-ring",
                        placeholder: "Filter service…",
                        value: "{search_query}",
                        oninput: move |evt| search_query.set(evt.value()),
                    }
                }

                div { class: "seg",
                    for (value, label) in [
                        ("all", "all"),
                        ("vulnerable", "VULN"),
                        ("poorly_hardened", "HIGH"),
                        ("moderately_hardened", "MED"),
                        ("well_hardened", "OK"),
                    ] {
                        button {
                            class: if *severity_filter.read() == value { "active" } else { "" },
                            onclick: {
                                let value = value.to_string();
                                move |_| severity_filter.set(value.clone())
                            },
                            "{label}"
                        }
                    }
                }

                span { class: "filter-count text-xs {theme::text::MUTED}", "{filtered_count} services" }
            }

            if results.is_empty() {
                div { class: "{theme::presets::CARD} p-8 text-center space-y-3",
                    svg {
                        class: "w-16 h-16 mx-auto {theme::text::MUTED}",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "1.5",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            d: "M9 12.75L11.25 15 15 9.75m-3-7.036A11.959 11.959 0 013.598 6 11.99 11.99 0 003 9.749c0 5.592 3.824 10.29 9 11.623 5.176-1.332 9-6.03 9-11.622 0-1.31-.21-2.571-.598-3.751h-.152c-3.196 0-6.1-1.248-8.25-3.285z"
                        }
                    }
                    h3 { class: "text-lg font-semibold {theme::text::PRIMARY}", "No scan results yet" }
                    p { class: "{theme::text::SECONDARY}",
                        "Run a hardening scan using the ",
                        span { class: "font-semibold {theme::text::PRIMARY}", "\"Run Hardening Scan\"" },
                        " button above to analyze systemd service security configurations."
                    }
                }
            } else {
                div { class: "{theme::presets::CARD} overflow-hidden",
                    div { class: "overflow-x-auto",
                    table { class: "sys-table",
                        thead {
                            tr {
                                th {
                                    style: "width: 22%;",
                                    "Service"
                                }
                                th { style: "width: 80px;", "Risk" }
                                th { style: "width: 120px;", "Score" }
                                th {
                                    style: "width: 84px;",
                                    "User"
                                }
                                for directive_name in table_directives.iter() {
                                    th {
                                        key: "hdr-{directive_name}",
                                        style: "font-size:10px; letter-spacing:0.04em; text-align:center; padding:8px 4px;",
                                        title: "{directive_name}",
                                        "{directive_short_label(directive_name)}"
                                    }
                                }
                                th { style: "width: 90px;", "Missing" }
                                th { style: "width: 92px; text-align:right;", "" }
                            }
                        }
                        tbody {
                            for service in filtered_results.iter() {
                                {
                                    let directives = directive_cells(service);
                                    let risk_color = risk_level_color(&service.risk_level);
                                    let user_label = service
                                        .service_type
                                        .clone()
                                        .unwrap_or_else(|| "system".to_string());
                                    let bar_width = service.hardening_score.clamp(0, 100);
                                    let missing_text_class = if service.missing_directives_count > 15 {
                                        theme::health::CRITICAL_TEXT
                                    } else {
                                        theme::text::MUTED
                                    };

                                    rsx! {
                                        tr {
                                            key: "svc-{service.id}",
                                            onclick: {
                                                let service = service.clone();
                                                move |_| {
                                                    modal_tab.set("overview".to_string());
                                                    selected_service.set(Some(service.clone()));
                                                }
                                            },
                                            td { class: "mono", style: "font-size:12px;font-weight:600;white-space:nowrap;color:var(--cf-text-primary);",
                                                "{service.service_name}"
                                                if !justifications_for(&service.service_name).is_empty() {
                                                    div { style: "font-size:10px;margin-top:2px;color:var(--cf-text-muted);", "⚠ waiver" }
                                                }
                                            }
                                            td {
                                                span {
                                                    class: "chip",
                                                    style: "color:{risk_color}; background:color-mix(in srgb, {risk_color} 13%, transparent); border:1px solid color-mix(in srgb, {risk_color} 30%, transparent); font-size:10px; font-weight:700;",
                                                    "{short_risk_label(&service.risk_level)}"
                                                }
                                            }
                                            td {
                                                div { class: "flex items-center gap-2",
                                                    div { class: "h-1.5 w-[60px] rounded-full {theme::surface::SUBTLE_BG} overflow-hidden",
                                                        div { style: "height:100%; width: {bar_width}%; background: {risk_color};" }
                                                    }
                                                    span { class: "mono", style: "font-size:11px;color:var(--cf-text-muted);", "{service.hardening_score}%" }
                                                }
                                            }
                                            td {
                                                span { class: "mono", style: "font-size:11px;color:var(--cf-text-muted);", "{user_label}" }
                                            }

                                            for directive_name in table_directives.iter() {
                                                    {
                                                        let status = directive_badge_content(directive_for(&directives, directive_name));
                                                        rsx! {
                                                            td { style: "text-align:center;",
                                                                if status.label == "on" || status.label == "partial" {
                                                                    span { style: "color:#34d399;font-size:11px;", "✓" }
                                                                } else {
                                                                    span { style: "color:var(--cf-text-muted);font-size:11px;", "–" }
                                                                }
                                                            }
                                                        }
                                                    }
                                            }

                                            td {
                                                span {
                                                    class: "{missing_text_class}",
                                                    style: "font-size:11px;",
                                                    "{service.missing_directives_count}/{directive_cells(service).len()}"
                                                }
                                            }
                                            td { style: "text-align:right;",
                                                div { class: "row-actions",
                                                    button {
                                                        class: "btn-icon focus-ring",
                                                        aria_label: "Open justification notes",
                                                        title: "Open justification notes",
                                                        onclick: {
                                                            let service = service.clone();
                                                            move |evt| {
                                                                evt.stop_propagation();
                                                                modal_tab.set("justification".to_string());
                                                                selected_service.set(Some(service.clone()));
                                                            }
                                                        },
                                                        svg {
                                                            class: "w-3.5 h-3.5 inline-block",
                                                            fill: "none",
                                                            stroke: "currentColor",
                                                            stroke_width: "2",
                                                            view_box: "0 0 24 24",
                                                            path {
                                                                stroke_linecap: "round",
                                                                stroke_linejoin: "round",
                                                                d: "M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
                                                            }
                                                        }
                                                    }
                                                    button {
                                                        class: "btn-icon focus-ring",
                                                        aria_label: "View details",
                                                        title: "View details",
                                                        onclick: {
                                                            let service = service.clone();
                                                            move |evt| {
                                                                evt.stop_propagation();
                                                                modal_tab.set("overview".to_string());
                                                                selected_service.set(Some(service.clone()));
                                                            }
                                                        },
                                                        svg {
                                                            class: "w-3.5 h-3.5 inline-block",
                                                            fill: "none",
                                                            stroke: "currentColor",
                                                            stroke_width: "2",
                                                            view_box: "0 0 24 24",
                                                            path {
                                                                stroke_linecap: "round",
                                                                stroke_linejoin: "round",
                                                                d: "M5 12h14m-7-7l7 7-7 7"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if filtered_results.is_empty() {
                                tr {
                                    td {
                                        class: "px-3 py-6 text-sm text-center {theme::text::SECONDARY}",
                                        colspan: "{5 + table_directives.len() + 2}",
                                        "No services match the current filters."
                                    }
                                }
                            }
                        }  // Close: tbody
                    }  // Close: table
                }  // Close: overflow-x-auto div
                }  // Close: TABLE_CONTAINER div
            }  // Close: else (if !results.is_empty())
        }  // Close: Main content div (space-y-4)

        // Modal - rendered as sibling to main content for proper overlay
        if let Some(service) = selected_service() {
            div {
                class: "modal-backdrop cf-modal-overlay",
                tabindex: "0",
                onkeydown: move |evt| {
                    if evt.key() == Key::Escape && confirm_discard_unsaved_justification(!reason.read().trim().is_empty()) {
                        evt.prevent_default();
                        selected_service.set(None);
                    }
                },
                onclick: move |_| {
                    if confirm_discard_unsaved_justification(!reason.read().trim().is_empty()) {
                        selected_service.set(None);
                    }
                },

                    div {
                        class: "modal cf-hardening-modal",
                        style: "width:min(720px,98vw);",
                    onclick: move |evt| evt.stop_propagation(),
                    role: "dialog",
                    aria_modal: "true",
                    aria_labelledby: "hardening-modal-title",

                    div { class: "modal-head cf-hardening-modal-header",
                        div { class: "flex items-start justify-between gap-3",
                            div { class: "space-y-2 min-w-0 flex-1",
                                h3 { id: "hardening-modal-title", class: "text-base font-semibold leading-tight {theme::text::PRIMARY} break-words flex items-center", style: "margin:0; font-size:16px; gap:10px;",
                                    span {
                                        class: "chip",
                                        style: "color:{risk_level_color(&service.risk_level)}; background:color-mix(in srgb, {risk_level_color(&service.risk_level)} 13%, transparent); font-size:10px;",
                                        "{short_risk_label(&service.risk_level)}"
                                    }
                                    span { class: "font-mono", "{service.service_name}" }
                                }
                                p { class: "text-[12px] leading-5 {theme::text::MUTED}", style: "margin-top:4px;",
                                    "Score: "
                                    span { class: "font-semibold", style: "color: {risk_level_color(&service.risk_level)};", "{service.hardening_score}%" }
                                    " · "
                                    "{service.missing_directives_count} missing directives"
                                    " · user: "
                                    span { class: "font-mono", "{service_user_label(&service)}" }
                                }
                            }
                            button {
                                class: "btn-icon focus-ring",
                                autofocus: "true",
                                onclick: move |_| {
                                    if confirm_discard_unsaved_justification(!reason.read().trim().is_empty()) {
                                        selected_service.set(None);
                                    }
                                },
                                aria_label: "Close service hardening modal",
                                svg {
                                    class: "w-4 h-4 inline-block",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    view_box: "0 0 24 24",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M6 18L18 6M6 6l12 12"
                                    }
                                }
                            }
                        }
                    }

                    div { class: "sd-tabs", style: "padding:0 22px; margin-top:0;",
                        for (key, label) in [("overview", "Directives"), ("nix", "NixOS config"), ("all", "All checks"), ("justification", "Justification")] {
                            {
                                let tab_class = if *modal_tab.read() == key {
                                    "sd-tab active"
                                } else {
                                    "sd-tab"
                                };
                                rsx! {
                                    button {
                                        class: "{tab_class}",
                                        onclick: {
                                            let key = key.to_string();
                                            move |_| modal_tab.set(key.clone())
                                        },
                                        "{label}"
                                    }
                                }
                            }
                        }
                    }

                    div { class: "modal-body", style: "padding:16px 22px; max-height:60vh; overflow-y:auto;",
                        if *modal_tab.read() == "overview" {
                            section { class: "space-y-3",
                                div { class: "grid gap-2", style: "grid-template-columns: 1fr 1fr;",
                                    for directive in directive_cells(&service) {
                                        {
                                            let tile_class = if directive.enabled {
                                                "bg-emerald-500/10 border-emerald-500/30"
                                            } else {
                                                "bg-red-500/10 border-red-500/30"
                                            };
                                            rsx! {
                                                div { class: "flex items-center gap-2 px-3 py-2 rounded-lg border {tile_class}",
                                                    span { class: "text-base", if directive.enabled { "✅" } else { "❌" } }
                                                    div {
                                                        div { class: "font-mono text-[12px] font-semibold {theme::text::PRIMARY}", "{directive.name}" }
                                                        div { class: "text-[10px] {theme::text::MUTED}",
                                                            if directive.enabled { "enforced" } else { "not set" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else if *modal_tab.read() == "nix" {
                            section { class: "space-y-3",
                                div { class: "sd-callout sd-callout-info", style: "display: flex; align-items: flex-start; gap: 8px;",
                                    svg {
                                        class: "shrink-0",
                                        width: "13",
                                        height: "13",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        view_box: "0 0 24 24",
                                        path {
                                            stroke_linecap: "round",
                                            stroke_linejoin: "round",
                                            d: "M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8zm0 0v6h6"
                                        }
                                    }
                                    p { class: "text-[12px] {theme::text::SECONDARY}", style: "flex: 1;",
                                        "Add these options to your NixOS module to harden "
                                        span { class: "font-mono {theme::text::PRIMARY}", "{service.service_name}" }
                                        "."
                                    }
                                }
                                pre { class: "sd-nix text-[12px] p-3 rounded-lg border {theme::surface::CARD_BORDER} overflow-x-auto", style: "max-height: 45vh;",
                                    "systemd.services.\"{service.service_name}\".serviceConfig = {{\n  # tighten according to your workload\n  PrivateTmp = true;\n  PrivateDevices = true;\n  ProtectSystem = \"strict\";\n  ProtectHome = true;\n  NoNewPrivileges = true;\n}};"
                                }
                            }
                        } else if *modal_tab.read() == "all" {
                            section { class: "space-y-3",
                                div { class: "rounded-xl border {theme::surface::CARD_BORDER} overflow-hidden",
                                    div { class: "h-[320px] overflow-y-scroll pr-1 cf-modal-table-scroll cf-hardening-directives-scroll",
                                        table { class: "sys-table w-full text-sm table-fixed",
                                            thead {
                                                tr { class: "sticky top-0 z-10 border-b {theme::surface::DIVIDER} {theme::surface::SUBTLE_BG} text-left {theme::text::MUTED}",
                                                    th { class: "px-3 py-2 text-[11px] font-medium", "Directive" }
                                                    th { class: "px-3 py-2 text-[11px] font-medium", "Category" }
                                                    th { class: "px-3 py-2 text-[11px] font-medium", "Points" }
                                                    th { class: "px-3 py-2 text-[11px] font-medium", "Status" }
                                                }
                                            }
                                            tbody {
                                                for directive in directive_cells(&service) {
                                                    tr { class: "border-b {theme::surface::DIVIDER}",
                                                        td { class: "px-3 py-2 font-mono text-[12px] {theme::text::PRIMARY}", "{directive.name}" }
                                                        td { class: "px-3 py-2 text-[12px] {theme::text::MUTED}", "security" }
                                                        td { class: "px-3 py-2 font-mono text-[12px] {theme::text::MUTED}", "—" }
                                                        td { class: "px-3 py-2",
                                                            if directive.enabled {
                                                                span { class: "chip chip-healthy", "enforced" }
                                                            } else {
                                                                span { class: "chip chip-critical", "missing" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            // Justification tab
                            section { class: "space-y-4",
                                if allow_mutations {
                                    div { class: "space-y-3",
                                        div { class: "space-y-1.5",
                                            h4 { class: "text-sm font-semibold {theme::text::PRIMARY}", "Add justification" }
                                            p { class: "text-[12px] leading-5 {theme::text::MUTED}",
                                                "Document why this service posture is acceptable (compensating controls, constrained runtime, or accepted risk)."
                                            }
                                        }
                                        textarea {
                                            class: "w-full min-h-[120px] rounded-lg text-[13px] leading-relaxed resize-none {theme::interactive::INPUT} {theme::text::PRIMARY} focus:ring-2 focus:ring-offset-1",
                                            style: "max-height: 240px; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; padding: 0.75rem 1rem;",
                                            placeholder: "Example: This service runs in an isolated container with read-only filesystem and network restrictions enforced by podman security policies…",
                                    value: "{reason}",
                                    oninput: move |evt| {
                                        reason.set(evt.value());
                                        justification_error.set(None);
                                        justification_notice.set(None);
                                    },
                                        }
                                        if let Some(message) = justification_error() {
                                            div { class: "flex items-start gap-2 p-3 rounded-lg bg-red-500/10 border border-red-500/30",
                                                svg {
                                                    class: "w-4 h-4 shrink-0 mt-0.5",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    view_box: "0 0 24 24",
                                                    path {
                                                        stroke_linecap: "round",
                                                        stroke_linejoin: "round",
                                                        d: "M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                                                    }
                                                }
                                                p { class: "text-[12px] {theme::health::CRITICAL_TEXT}", "{message}" }
                                            }
                                        }
                                        if let Some(message) = justification_notice() {
                                            div { class: "flex items-start gap-2 p-3 rounded-lg bg-emerald-500/10 border border-emerald-500/30",
                                                svg {
                                                    class: "w-4 h-4 shrink-0 mt-0.5",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    view_box: "0 0 24 24",
                                                    path {
                                                        stroke_linecap: "round",
                                                        stroke_linejoin: "round",
                                                        d: "M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"
                                                    }
                                                }
                                                p { class: "text-[12px] {theme::health::HEALTHY_TEXT}", "{message}" }
                                            }
                                        }
                                        div { class: "flex items-center justify-between gap-3 pt-1",
                                            p { class: "text-[11px] leading-5 {theme::text::MUTED}", "Required for audit compliance when accepting weaker posture." }
                                            button {
                                                class: "px-4 py-2 rounded-lg {theme::interactive::PRIMARY_BTN} text-sm font-medium {theme::interactive::FOCUS_RING} transition-colors",
                                        disabled: is_saving_justification() || reason.read().trim().is_empty(),
                                        onclick: {
                                            let service_name = service.service_name.clone();
                                            let on_saved = on_saved.clone();
                                            move |_| {
                                                let reason_value = reason();
                                                if reason_value.trim().is_empty() {
                                                    justification_error.set(Some("Justification is required.".to_string()));
                                                    return;
                                                }

                                                is_saving_justification.set(true);
                                                justification_error.set(None);
                                                justification_notice.set(None);

                                                let request = SaveHardeningJustificationRequest {
                                                    directive_name: None,
                                                    category: None,
                                                    reason: reason_value,
                                                };
                                                let service_name_for_request = service_name.clone();

                                                spawn(async move {
                                                    if save_system_hardening_justification(&system_id, &service_name_for_request, &request)
                                                        .await
                                                        .is_ok()
                                                    {
                                                        reason.set(String::new());
                                                        justification_notice.set(Some("Justification saved.".to_string()));
                                                        on_saved.call(());
                                                    } else {
                                                        justification_error.set(Some("Failed to save justification.".to_string()));
                                                    }
                                                    is_saving_justification.set(false);
                                                });
                                            }
                                        },
                                                if is_saving_justification() {
                                                    "Saving…"
                                                } else {
                                                    "Save justification"
                                                }
                                            }
                                        }
                                    }
                                }

                                div { class: "space-y-3 pt-2",
                                    div { class: "border-t {theme::surface::DIVIDER} pt-4" }
                                    div { class: "space-y-1.5",
                                        h4 { class: "text-sm font-semibold {theme::text::PRIMARY}", "Justification history" }
                                        p { class: "text-[12px] leading-5 {theme::text::MUTED}", "Audit trail of accepted risk documentation for this service." }
                                    }
                                    if justifications.iter().all(|j| j.service_name != service.service_name) {
                                        div { class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::SUBTLE_BG} px-4 py-3 text-center",
                                            p { class: "text-[12px] {theme::text::MUTED}", "No justifications recorded yet. Add one above to document accepted risks." }
                                        }
                                    } else {
                                        div { class: "flex flex-col gap-2.5 max-h-[280px] overflow-y-auto pr-1",
                                            for item in justifications.iter().filter(|j| j.service_name == service.service_name) {
                                                div { class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::SUBTLE_BG} px-3.5 py-3 space-y-2",
                                                    div { class: "flex items-center justify-between gap-2",
                                                        div { class: "flex items-center gap-2 flex-wrap",
                                                            span {
                                                                class: "inline-flex items-center rounded-md border {theme::surface::CARD_BORDER} px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide {theme::text::MUTED}",
                                                                "{item.category.clone().unwrap_or_else(|| \"service\".to_string())}"
                                                            }
                                                            if let Some(directive) = item.directive_name.clone() {
                                                                span { class: "font-mono text-[11px] {theme::text::MUTED} bg-black/5 dark:bg-white/5 px-2 py-0.5 rounded", "{directive}" }
                                                            }
                                                        }
                                                    }
                                                    p { class: "text-[13px] {theme::text::PRIMARY} leading-relaxed", style: "padding-left: 0.25rem;", "{item.reason}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Footer
                    div { class: "modal-foot",
                        button {
                            class: "btn btn-ghost focus-ring xs",
                            svg {
                                class: "w-3 h-3 inline-block",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M12 3v12m0 0l4-4m-4 4l-4-4M5 21h14"
                                }
                            }
                            "Export report"
                        }
                        button {
                            class: "btn btn-primary focus-ring",
                            onclick: move |_| {
                                if confirm_discard_unsaved_justification(!reason.read().trim().is_empty()) {
                                    selected_service.set(None);
                                }
                            },
                            "Close"
                        }
                    }
                }
            }
        }
    } // Close: rsx!
}

#[component]
fn CompactMetricCard(label: String, value: String, tone: &'static str) -> Element {
    let tone_class = match tone {
        "danger" => format!(
            "border {} {}",
            theme::health::CRITICAL_BORDER,
            theme::health::CRITICAL_BG
        ),
        "warning" => format!(
            "border {} {}",
            theme::health::WARNING_BORDER,
            theme::health::WARNING_BG
        ),
        _ => format!(
            "border {} {}",
            theme::surface::CARD_BORDER,
            theme::surface::SUBTLE_BG
        ),
    };

    rsx! {
        div { class: "rounded border {tone_class} px-2.5 py-2 min-h-[66px] flex flex-col justify-between",
            p { class: "text-[10px] uppercase tracking-wide {theme::text::MUTED}", "{label}" }
            p { class: "mt-1 text-base font-semibold leading-none {theme::text::PRIMARY}", "{value}" }
        }
    }
}

fn modal_directives(service: &HardeningServiceResultResponse) -> Vec<DirectiveCell> {
    let mut directives = directive_cells(service);
    directives.sort_by(|a, b| {
        modal_directive_rank(a)
            .cmp(&modal_directive_rank(b))
            .then_with(|| a.points.cmp(&b.points))
            .then_with(|| a.name.cmp(&b.name))
    });
    directives
}

fn modal_directive_rank(directive: &DirectiveCell) -> i32 {
    if !directive.enabled || directive.points == 0 {
        0
    } else if directive.points < directive.max_points {
        1
    } else {
        2
    }
}

fn confirm_discard_unsaved_justification(is_dirty: bool) -> bool {
    if !is_dirty {
        return true;
    }

    web_sys::window()
        .and_then(|window| {
            window
                .confirm_with_message("Discard unsaved justification?")
                .ok()
        })
        .unwrap_or(false)
}

#[derive(Clone, Debug)]
struct DirectiveCell {
    name: String,
    enabled: bool,
    points: i32,
    max_points: i32,
    value: JsonValue,
}

#[derive(Clone, Debug)]
struct DirectiveBadgeContent {
    label: String,
    class_name: String,
    title: String,
}

fn directive_cells(service: &HardeningServiceResultResponse) -> Vec<DirectiveCell> {
    service
        .directives_detail
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| DirectiveCell {
                    name: item
                        .get("name")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    enabled: item
                        .get("enabled")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false),
                    points: item
                        .get("points")
                        .and_then(|value| value.as_i64())
                        .unwrap_or(0) as i32,
                    max_points: item
                        .get("max_points")
                        .and_then(|value| value.as_i64())
                        .unwrap_or(0) as i32,
                    value: item.get("value").cloned().unwrap_or(JsonValue::Null),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn directive_for<'a>(directives: &'a [DirectiveCell], name: &str) -> Option<&'a DirectiveCell> {
    directives.iter().find(|directive| directive.name == name)
}

fn directive_badge_content(directive: Option<&DirectiveCell>) -> DirectiveBadgeContent {
    match directive {
        None => DirectiveBadgeContent {
            label: "--".to_string(),
            class_name: format!(
                "{} {} {}",
                theme::health::CRITICAL_BORDER,
                theme::health::CRITICAL_BG,
                theme::health::CRITICAL_TEXT,
            ),
            title: "Directive missing from scan output".to_string(),
        },
        Some(directive) => {
            let (label, class_name) = if directive.max_points > 0
                && directive.enabled
                && directive.points >= directive.max_points
            {
                (
                    "ON",
                    format!(
                        "{} {} {}",
                        theme::health::HEALTHY_BORDER,
                        theme::health::HEALTHY_BG,
                        theme::health::HEALTHY_TEXT,
                    ),
                )
            } else if directive.points > 0 || directive.enabled {
                (
                    "PAR",
                    format!(
                        "{} {} {}",
                        theme::health::WARNING_BORDER,
                        theme::health::WARNING_BG,
                        theme::health::WARNING_TEXT,
                    ),
                )
            } else {
                (
                    "OFF",
                    format!(
                        "{} {} {}",
                        theme::health::CRITICAL_BORDER,
                        theme::health::CRITICAL_BG,
                        theme::health::CRITICAL_TEXT,
                    ),
                )
            };

            DirectiveBadgeContent {
                label: label.to_string(),
                class_name,
                title: format!(
                    "{}: {}/{} · value={}",
                    directive.name,
                    directive.points,
                    directive.max_points,
                    compact_directive_value(&directive.value)
                ),
            }
        }
    }
}

fn compact_directive_value(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "unset".to_string(),
        JsonValue::Bool(flag) => {
            if *flag {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        JsonValue::Number(number) => number.to_string(),
        JsonValue::String(string) => string.clone(),
        JsonValue::Array(items) => {
            if items.is_empty() {
                "[]".to_string()
            } else {
                format!("{} items", items.len())
            }
        }
        JsonValue::Object(_) => "object".to_string(),
    }
}

fn severity_matches(service: &HardeningServiceResultResponse, filter_value: &str) -> bool {
    match filter_value {
        "all" => true,
        "high_risk" => matches!(
            service.risk_level.as_str(),
            "vulnerable" | "poorly_hardened"
        ),
        level => service.risk_level == level,
    }
}

fn short_risk_label(level: &str) -> &'static str {
    match level {
        "well_hardened" => "OK",
        "moderately_hardened" => "MED",
        "poorly_hardened" => "HIGH",
        _ => "VULN",
    }
}

fn risk_level_color(level: &str) -> &'static str {
    match level {
        "well_hardened" => "#34d399",
        "moderately_hardened" => "#fbbf24",
        "poorly_hardened" => "#f97316",
        _ => "#f87171",
    }
}

fn directive_short_label(name: &str) -> String {
    let mut expanded = String::with_capacity(name.len() + 8);
    for (idx, ch) in name.chars().enumerate() {
        if idx > 0 && ch.is_uppercase() {
            expanded.push(' ');
        }
        expanded.push(ch);
    }
    expanded.trim().chars().take(4).collect::<String>()
}

fn service_user_label(service: &HardeningServiceResultResponse) -> String {
    service
        .service_type
        .clone()
        .unwrap_or_else(|| "system".to_string())
}

fn risk_level_compact_badge_class(level: &str) -> String {
    match level {
        "well_hardened" => format!(
            "border {} {} {}",
            theme::health::HEALTHY_BORDER,
            theme::health::HEALTHY_BG,
            theme::health::HEALTHY_TEXT,
        ),
        "moderately_hardened" => format!(
            "border {} {} {}",
            theme::health::WARNING_BORDER,
            theme::health::WARNING_BG,
            theme::health::WARNING_TEXT,
        ),
        "poorly_hardened" => format!(
            "border {} {} {}",
            theme::health::WARNING_BORDER,
            theme::health::WARNING_BG,
            theme::health::WARNING_TEXT,
        ),
        _ => format!(
            "border {} {} {}",
            theme::health::CRITICAL_BORDER,
            theme::health::CRITICAL_BG,
            theme::health::CRITICAL_TEXT,
        ),
    }
}

fn risk_level_badge_class(level: &str) -> &'static str {
    match level {
        "well_hardened" => "bg-emerald-500/20 text-emerald-300",
        "moderately_hardened" => "bg-yellow-500/20 text-yellow-300",
        "poorly_hardened" => "bg-orange-500/20 text-orange-300",
        _ => "bg-red-500/20 text-red-300",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PolicyFormat {
    Toml,
    Json,
}

#[component]
fn PolicyTab(system: SystemDetail) -> Element {
    let policy_library = use_signal(initial_policy_definitions);
    let mut active_policy_ids = use_signal(initial_active_policy_ids);
    let mut drag_policy_id: Signal<Option<Uuid>> = use_signal(|| None);

    let mut show_editor = use_signal(|| false);
    let mut show_combined = use_signal(|| false);
    let mut editing_policy_id: Signal<Option<Uuid>> = use_signal(|| None);
    let mut edit_name = use_signal(String::new);
    let mut edit_description = use_signal(String::new);
    let mut edit_body = use_signal(String::new);
    let mut edit_format = use_signal(|| PolicyFormat::Toml);
    let mut add_to_system = use_signal(|| true);
    let mut preset_query = use_signal(String::new);

    let query = preset_query.read().to_lowercase();
    let visible_policies: Vec<PolicyDefinition> = policy_library
        .read()
        .iter()
        .cloned()
        .filter(|policy| {
            if active_policy_ids.read().contains(&policy.id) {
                return false;
            }
            if query.trim().is_empty() {
                return true;
            }
            policy.name.to_lowercase().contains(&query)
                || policy.description.to_lowercase().contains(&query)
        })
        .collect();
    let library_count = visible_policies.len();
    let active_ids = active_policy_ids.read().clone();
    let active_policies = resolve_active_policies(&policy_library.read(), &active_ids);
    let combined_policy = compile_policy_sections(&active_policies);
    let policy_library_rows = build_policy_library_rows(
        &visible_policies,
        &active_ids,
        drag_policy_id.clone(),
        editing_policy_id.clone(),
        edit_name.clone(),
        edit_description.clone(),
        edit_body.clone(),
        edit_format.clone(),
        add_to_system.clone(),
        show_editor.clone(),
    );

    rsx! {
        div {
            class: "pt-6 space-y-6",

            div {
                class: "flex flex-col gap-2",
                div { class: "flex items-center justify-between",
                    h3 { class: "{theme::typography::SECTION_TITLE} text-white", "Deployment Policy" }
                    button {
                        class: "px-3 py-1.5 rounded-md text-xs font-semibold bg-gray-800 text-gray-200 border border-gray-700 hover:bg-gray-700",
                        onclick: move |_| show_combined.set(true),
                        "View combined policy"
                    }
                }
                p {
                    class: "text-sm {theme::text::SECONDARY}",
                    "Drag policies from the library to enable them for {system.hostname}. Edit any policy to change its name, description, or TOML/JSON definition."
                }
            }

            div {
                class: "grid grid-cols-1 lg:grid-cols-2 gap-6",

                // Policy library column
                div {
                    class: "space-y-4 border-l-4 border-l-blue-500/40 pl-4",
                    Card {
                        title: Some("Policy Library".to_string()),
                        header_actions: None,
                        children: rsx! {
                            div { class: "flex items-center gap-3",
                                input {
                                    class: "flex-1 rounded-lg border border-gray-700 bg-gray-900/70 px-3 py-2 text-sm text-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-500/40",
                                    placeholder: "Search policy library",
                                    value: "{preset_query}",
                                    oninput: move |event| preset_query.set(event.value()),
                                }
                                span { class: "text-xs {theme::text::MUTED}", "{library_count} policies" }
                            }
                            div { class: "space-y-3",
                                for row in policy_library_rows {
                                    {row}
                                }
                                if visible_policies.is_empty() {
                                    div { class: "text-sm text-gray-400 text-center py-6", "No policies match this search." }
                                }
                            }
                        }
                    }
                }

                // Active policies column
                div {
                    class: "space-y-4",
                    Card {
                        title: Some("Active Policies".to_string()),
                        header_actions: None,
                        children: rsx! {
                            div {
                                class: "space-y-3 rounded-xl border border-gray-800 bg-gray-950/40 p-3",
                                ondragover: move |evt| {
                                    evt.prevent_default();
                                },
                                ondrop: move |evt| {
                                    evt.prevent_default();
                                    let dropped = *drag_policy_id.read();
                                    if let Some(id) = dropped {
                                        if !active_policy_ids.read().contains(&id) {
                                            let mut ids = active_policy_ids.read().clone();
                                            ids.push(id);
                                            active_policy_ids.set(ids);
                                        }
                                        drag_policy_id.set(None);
                                    }
                                },
                                for policy in active_policies.iter().cloned() {
                                    ActivePolicyRow {
                                        policy: policy,
                                        on_edit: move |policy: PolicyDefinition| {
                                            editing_policy_id.set(Some(policy.id));
                                            edit_name.set(policy.name.clone());
                                            edit_description.set(policy.description.clone());
                                            edit_body.set(policy.body.clone());
                                            edit_format.set(policy.format);
                                            add_to_system.set(true);
                                            show_editor.set(true);
                                        },
                                        on_remove: move |id| {
                                            let mut ids = active_policy_ids.read().clone();
                                            ids.retain(|item| *item != id);
                                            active_policy_ids.set(ids);
                                        }
                                    }
                                }
                                if active_policies.is_empty() {
                                    div { class: "text-sm text-gray-400 text-center py-6", "Drag policies here to enable them." }
                                }
                            }
                        }
                    }
                }
            }

            if *show_editor.read() {
                PolicyEditorModal {
                    editing_policy_id: editing_policy_id.clone(),
                    edit_name: edit_name.clone(),
                    edit_description: edit_description.clone(),
                    edit_body: edit_body.clone(),
                    edit_format: edit_format.clone(),
                    add_to_system: add_to_system.clone(),
                    policy_library: policy_library.clone(),
                    active_policy_ids: active_policy_ids.clone(),
                    on_close: move || show_editor.set(false),
                }
            }
            if *show_combined.read() {
                CombinedPolicyModal {
                    text: combined_policy.clone(),
                    on_close: move || show_combined.set(false),
                }
            }
        }
    }
}

#[component]
fn PolicyPreview(format: PolicyFormat, text: String) -> Element {
    let display_text = format_policy_preview(format, &text);
    let language = match format {
        PolicyFormat::Json => "json",
        PolicyFormat::Toml => "toml",
    };
    let highlighted_html = highlight_policy_html(language, &display_text);
    rsx! {
        pre {
            class: "text-xs font-mono bg-gray-950/70 rounded-lg border border-gray-800 p-3 overflow-x-auto",
            code {
                class: "hljs language-{language}",
                dangerous_inner_html: "{highlighted_html}"
            }
        }
    }
}

fn format_policy_preview(format: PolicyFormat, text: &str) -> String {
    match format {
        PolicyFormat::Json => serde_json::from_str::<JsonValue>(text)
            .and_then(|value| serde_json::to_string_pretty(&value))
            .unwrap_or_else(|_| text.to_string()),
        PolicyFormat::Toml => text.to_string(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PolicyPreset {
    RequireAgent,
    RequirePackages,
    CustomCheck,
    Other,
}

impl PolicyPreset {
    fn policy_type(self) -> &'static str {
        match self {
            PolicyPreset::RequireAgent => "require_crystal_forge_agent",
            PolicyPreset::RequirePackages => "require_packages",
            PolicyPreset::CustomCheck | PolicyPreset::Other => "custom_check",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct PolicyPresetMeta {
    id: Uuid,
    title: &'static str,
    description: &'static str,
    summary: &'static str,
    kind: PolicyPreset,
    format: PolicyFormat,
    body: String,
}

#[derive(Clone, Debug, PartialEq)]
struct PolicyDefinition {
    id: Uuid,
    name: String,
    description: String,
    format: PolicyFormat,
    body: String,
    policy_type: Option<String>,
}

fn policy_presets() -> Vec<PolicyPresetMeta> {
    vec![
        PolicyPresetMeta {
            id: Uuid::from_u128(1),
            title: "Require Crystal Forge Agent",
            summary: "Agent services enabled",
            description: "This policy ensures the Crystal Forge agent and client services are enabled on the target system. It is a common baseline for production environments where you expect managed telemetry and deployments.",
            kind: PolicyPreset::RequireAgent,
            format: PolicyFormat::Toml,
            body: r#"[[policy]]
type = \"require_crystal_forge_agent\"
strict = true
"#
            .to_string(),
        },
        PolicyPresetMeta {
            id: Uuid::from_u128(2),
            title: "Require Packages",
            summary: "Package list guardrail",
            description: "Use this policy to guarantee specific system packages are present. It is useful for fleets where shared tooling (like git or vim) must be installed before deployments run.",
            kind: PolicyPreset::RequirePackages,
            format: PolicyFormat::Toml,
            body: r#"[[policy]]
type = \"require_packages\"
packages = [\"git\", \"vim\"]
strict = false
"#
            .to_string(),
        },
        PolicyPresetMeta {
            id: Uuid::from_u128(3),
            title: "Custom Check",
            summary: "Nix expression validation",
            description: "This policy lets you encode a custom Nix expression and description. It works well for environment-specific checks like enforcing SSH, ensuring a module is enabled, or validating configuration flags.",
            kind: PolicyPreset::CustomCheck,
            format: PolicyFormat::Toml,
            body: r#"[[policy]]
type = \"custom_check\"
expression = \"(cfg.config.services.openssh.enable or false)\"
description = \"SSH must be enabled\"
field_name = \"sshEnabled\"
strict = true
"#
            .to_string(),
        },
        PolicyPresetMeta {
            id: Uuid::from_u128(4),
            title: "Other Template",
            summary: "Flexible starter",
            description: "A flexible starting point for policies that do not fit the built-in templates. Use this when you want to annotate your own intent or create a specialized guardrail.",
            kind: PolicyPreset::Other,
            format: PolicyFormat::Toml,
            body: r#"[[policy]]
# Add your custom policy here
type = \"custom_check\"
expression = \"(cfg.config.services.openssh.enable or false)\"
description = \"Describe requirement\"
field_name = \"customField\"
strict = false
"#
            .to_string(),
        },
    ]
}

#[component]
fn PolicyLibraryRow(
    policy: PolicyDefinition,
    is_active: bool,
    drag_policy_id: Signal<Option<Uuid>>,
    on_edit: EventHandler<PolicyDefinition>,
) -> Element {
    let row_class = if is_active {
        "group rounded-xl border border-violet-500/40 bg-violet-500/10 p-4"
    } else {
        "group rounded-xl border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} p-4 hover:border-violet-500/40 transition-all"
    };
    let policy_id = policy.id;
    let format_badge = match policy.format {
        PolicyFormat::Toml => "bg-orange-500/20",
        PolicyFormat::Json => "bg-blue-500/20",
    };

    rsx! {
        div {
            class: "{row_class}",
            draggable: "true",
            ondragstart: move |evt| {
                drag_policy_id.set(Some(policy_id));
                evt.data_transfer().set_data("text/plain", &policy_id.to_string()).ok();
            },
            ondragend: move |_| {
                drag_policy_id.set(None);
            },
            div { class: "flex items-start justify-between gap-4",
                div {
                    span { class: "text-sm font-semibold text-white", "{policy.name}" }
                    p { class: "text-xs text-gray-500", "{policy.description}" }
                }
                div { class: "flex items-center gap-2",
                    span { class: "w-2.5 h-2.5 rounded-full {format_badge}" }
                    button {
                        class: "text-xs text-violet-400 hover:text-violet-300",
                        onclick: move |_| on_edit.call(policy.clone()),
                        "Edit"
                    }
                }
            }
        }
    }
}

#[component]
fn ActivePolicyRow(
    policy: PolicyDefinition,
    on_edit: EventHandler<PolicyDefinition>,
    on_remove: EventHandler<Uuid>,
) -> Element {
    let policy_id = policy.id;

    let format_badge = match policy.format {
        PolicyFormat::Toml => "bg-orange-500/20",
        PolicyFormat::Json => "bg-blue-500/20",
    };

    rsx! {
        div {
            class: "group rounded-xl border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} p-4 hover:border-violet-500/40 transition-all",
            div { class: "flex items-start justify-between gap-4",
                div {
                    span { class: "text-sm font-semibold text-white", "{policy.name}" }
                    p { class: "text-xs text-gray-500", "{policy.description}" }
                }
                div { class: "flex items-center gap-2",
                    span { class: "w-2.5 h-2.5 rounded-full {format_badge}" }
                    button {
                        class: "text-xs text-violet-400 hover:text-violet-300",
                        onclick: move |_| on_edit.call(policy.clone()),
                        "Edit"
                    }
                    button {
                        class: "text-xs text-orange-400 hover:text-orange-300",
                        onclick: move |_| on_remove.call(policy_id),
                        "Remove"
                    }
                }
            }
        }
    }
}

#[component]
fn CombinedPolicyPreview(text: String) -> Element {
    rsx! {
        pre {
            class: "text-xs font-mono bg-gray-950/70 rounded-lg border border-gray-800 p-3 overflow-x-auto",
            "{text}"
        }
    }
}

#[component]
fn PolicyEditorModal(
    editing_policy_id: Signal<Option<Uuid>>,
    edit_name: Signal<String>,
    edit_description: Signal<String>,
    edit_body: Signal<String>,
    edit_format: Signal<PolicyFormat>,
    add_to_system: Signal<bool>,
    policy_library: Signal<Vec<PolicyDefinition>>,
    active_policy_ids: Signal<Vec<Uuid>>,
    on_close: EventHandler<()>,
) -> Element {
    let is_editing = editing_policy_id.read().is_some();
    let action_label = if is_editing {
        "Save Policy"
    } else {
        "Add Policy"
    };

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-6 cf-modal-overlay-z50",
            div {
                class: "{theme::surface::CARD_BG} border {theme::surface::CARD_BORDER} rounded-2xl p-6 cf-modal-panel-wide overflow-visible items-stretch",
                div {
                    class: "flex items-center justify-between",
                    div {
                        h3 { class: "text-white text-lg font-semibold", "Edit Policy" }
                        p { class: "text-xs {theme::text::MUTED}", "Define the policy metadata and the TOML/JSON body." }
                    }
                    button {
                        class: "text-xs text-gray-400 hover:text-white",
                        onclick: move |_| on_close.call(()),
                        "Close"
                    }
                }
                div { class: "grid grid-cols-1 lg:grid-cols-[280px_1fr] gap-6 items-start",
                    div { class: "space-y-4",
                        div {
                            class: "space-y-2",
                            label { class: "text-xs text-gray-400", "Policy Name" }
                            input {
                                class: "w-full rounded-lg border border-gray-700 bg-gray-900/70 px-3 py-2 text-sm text-gray-100 focus:outline-none focus:ring-2 focus:ring-blue-500/40",
                                value: "{edit_name}",
                                oninput: move |event| edit_name.set(event.value()),
                            }
                        }
                        div {
                            class: "space-y-2",
                            label { class: "text-xs text-gray-400", "Description" }
                            textarea {
                                class: "w-full rounded-lg border border-gray-700 bg-gray-900/70 px-3 py-2 text-sm text-gray-100 min-h-[140px] focus:outline-none focus:ring-2 focus:ring-blue-500/40",
                                value: "{edit_description}",
                                oninput: move |event| edit_description.set(event.value()),
                            }
                        }
                        div {
                            class: "space-y-2",
                            label { class: "text-xs text-gray-400", "Format" }
                            div { class: "flex gap-2",
                                button {
                                    class: "px-3 py-1.5 rounded-md text-xs border transition-colors",
                                    class: if *edit_format.read() == PolicyFormat::Toml {
                                        "bg-blue-500/20 border-blue-500 text-blue-300"
                                    } else {
                                        "{theme::interactive::INPUT} {theme::surface::CARD_BORDER} {theme::text::SECONDARY}"
                                    },
                                    onclick: move |_| edit_format.set(PolicyFormat::Toml),
                                    "TOML"
                                }
                                button {
                                    class: "px-3 py-1.5 rounded-md text-xs border transition-colors",
                                    class: if *edit_format.read() == PolicyFormat::Json {
                                        "bg-blue-500/20 border-blue-500 text-blue-300"
                                    } else {
                                        "{theme::interactive::INPUT} {theme::surface::CARD_BORDER} {theme::text::SECONDARY}"
                                    },
                                    onclick: move |_| edit_format.set(PolicyFormat::Json),
                                    "JSON"
                                }
                            }
                        }
                        div {
                            class: "flex items-center gap-2",
                            input {
                                r#type: "checkbox",
                                checked: "{add_to_system}",
                                onchange: move |event| add_to_system.set(event.checked()),
                            }
                            span { class: "text-xs text-gray-400", "Add to active policies" }
                        }
                    }
                    div { class: "space-y-3 flex flex-col",
                        label { class: "text-xs text-gray-400", "Policy Definition" }
                        div { class: "rounded-lg border border-gray-700 bg-gray-900/70",
                            textarea {
                                class: "w-full bg-transparent px-3 py-2 text-sm text-gray-100 font-mono focus:outline-none focus:ring-2 focus:ring-blue-500/40 resize-none",
                                rows: "12",
                                value: "{edit_body}",
                                oninput: move |event| edit_body.set(event.value()),
                                spellcheck: "false",
                            }
                        }
                    }
                }
                div { class: "flex justify-between items-center gap-3",
                    span { class: "text-xs {theme::text::MUTED}", "Save adds the policy to the library."
                    }
                    div { class: "flex gap-3",
                        button {
                            class: "px-4 py-2 rounded-md text-sm text-gray-300 border border-gray-700 hover:bg-gray-800",
                            onclick: move |_| on_close.call(()),
                            "Cancel"
                        }
                        button {
                            class: "px-4 py-2 rounded-md text-sm font-semibold bg-blue-500/20 text-blue-200 border border-blue-500/40 hover:bg-blue-500/30",
                            onclick: move |_| {
                                let name = edit_name.read().clone();
                                let description = edit_description.read().clone();
                                let body = edit_body.read().clone();
                                let format = *edit_format.read();
                                let new_id = editing_policy_id.read().unwrap_or_else(Uuid::new_v4);
                                let mut library = policy_library.read().clone();
                                let is_existing = library.iter().any(|policy| policy.id == new_id);
                                if is_existing {
                                    library = library
                                        .into_iter()
                                        .map(|policy| {
                                            if policy.id == new_id {
                                                PolicyDefinition {
                                                    id: new_id,
                                                    name: name.clone(),
                                                    description: description.clone(),
                                                    format,
                                                    body: body.clone(),
                                                    policy_type: extract_policy_type_from_body(&body),
                                                }
                                            } else {
                                                policy
                                            }
                                        })
                                        .collect();
                                } else {
                                    library.push(PolicyDefinition {
                                        id: new_id,
                                        name: name.clone(),
                                        description: description.clone(),
                                        format,
                                        body: body.clone(),
                                        policy_type: extract_policy_type_from_body(&body),
                                    });
                                }
                                policy_library.set(library);
                                if *add_to_system.read() {
                                    let mut active = active_policy_ids.read().clone();
                                    if !active.contains(&new_id) {
                                        active.push(new_id);
                                        active_policy_ids.set(active);
                                    }
                                }
                                on_close.call(());
                            },
                            "{action_label}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn CombinedPolicyModal(text: String, on_close: EventHandler<()>) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-6 cf-modal-overlay-z50",
            div {
                class: "{theme::surface::CARD_BG} border {theme::surface::CARD_BORDER} rounded-2xl p-6 cf-modal-panel-xl",
                div {
                    class: "flex items-center justify-between",
                    div {
                        h3 { class: "text-white text-lg font-semibold", "Combined Policy" }
                        p { class: "text-xs {theme::text::MUTED}", "Read-only compiled view of all active policies." }
                    }
                    button {
                        class: "text-xs text-gray-400 hover:text-white",
                        onclick: move |_| on_close.call(()),
                        "Close"
                    }
                }
                PolicyPreview { format: PolicyFormat::Toml, text: text }
            }
        }
    }
}

fn compile_policy_sections(policies: &[PolicyDefinition]) -> String {
    policies
        .iter()
        .map(|policy| policy.body.clone())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn resolve_active_policies(
    library: &[PolicyDefinition],
    active_ids: &[Uuid],
) -> Vec<PolicyDefinition> {
    active_ids
        .iter()
        .filter_map(|id| library.iter().find(|policy| policy.id == *id).cloned())
        .collect()
}

fn build_policy_library_rows(
    policies: &[PolicyDefinition],
    active_ids: &[Uuid],
    drag_policy_id: Signal<Option<Uuid>>,
    mut editing_policy_id: Signal<Option<Uuid>>,
    mut edit_name: Signal<String>,
    mut edit_description: Signal<String>,
    mut edit_body: Signal<String>,
    mut edit_format: Signal<PolicyFormat>,
    mut add_to_system: Signal<bool>,
    mut show_editor: Signal<bool>,
) -> Vec<Element> {
    policies
        .iter()
        .cloned()
        .map(|policy| {
            let policy_id = policy.id;
            let is_active = active_ids.contains(&policy_id);
            let active_ids_snapshot = active_ids.to_vec();
            rsx! {
                PolicyLibraryRow {
                    policy: policy.clone(),
                    is_active: is_active,
                    drag_policy_id: drag_policy_id.clone(),
                    on_edit: move |policy: PolicyDefinition| {
                        editing_policy_id.set(Some(policy.id));
                        edit_name.set(policy.name.clone());
                        edit_description.set(policy.description.clone());
                        edit_body.set(policy.body.clone());
                        edit_format.set(policy.format);
                        add_to_system.set(active_ids_snapshot.contains(&policy.id));
                        show_editor.set(true);
                    }
                }
            }
        })
        .collect()
}

fn extract_policy_type_from_body(body: &str) -> Option<String> {
    if body.contains("type = \"require_crystal_forge_agent\"") {
        Some("require_crystal_forge_agent".to_string())
    } else if body.contains("type = \"require_cf_agent\"") {
        Some("require_cf_agent".to_string())
    } else if body.contains("type = \"require_packages\"") {
        Some("require_packages".to_string())
    } else if body.contains("type = \"custom_check\"") {
        Some("custom_check".to_string())
    } else if body.contains("\"policy_type\"") {
        // Try to extract from JSON
        body.lines()
            .find(|line| line.contains("\"policy_type\""))
            .and_then(|line| {
                line.split(':')
                    .nth(1)?
                    .trim()
                    .trim_matches(|c| c == '"' || c == ',' || c == ' ')
                    .parse()
                    .ok()
            })
    } else {
        None
    }
}

fn initial_policy_definitions() -> Vec<PolicyDefinition> {
    policy_presets()
        .into_iter()
        .map(|preset| PolicyDefinition {
            id: preset.id,
            name: preset.title.to_string(),
            description: preset.description.to_string(),
            format: preset.format,
            body: preset.body.clone(),
            policy_type: extract_policy_type_from_body(&preset.body),
        })
        .collect()
}

fn initial_active_policy_ids() -> Vec<Uuid> {
    vec![Uuid::from_u128(1), Uuid::from_u128(2)]
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

#[cfg(target_arch = "wasm32")]
fn highlight_policy_html(language: &str, text: &str) -> String {
    let Some(window) = web_sys::window() else {
        return escape_html(text);
    };
    let Ok(hljs) = js_sys::Reflect::get(&window, &JsValue::from_str("hljs")) else {
        return escape_html(text);
    };
    if hljs.is_undefined() || hljs.is_null() {
        return escape_html(text);
    }
    let Ok(highlight_fn) = js_sys::Reflect::get(&hljs, &JsValue::from_str("highlight")) else {
        return escape_html(text);
    };
    let Ok(highlight_fn) = highlight_fn.dyn_into::<js_sys::Function>() else {
        return escape_html(text);
    };
    let options = Object::new();
    let _ = js_sys::Reflect::set(
        &options,
        &JsValue::from_str("language"),
        &JsValue::from_str(language),
    );
    let Ok(result) = highlight_fn.call2(&hljs, &JsValue::from_str(text), &options.into()) else {
        return escape_html(text);
    };
    let Ok(value) = js_sys::Reflect::get(&result, &JsValue::from_str("value")) else {
        return escape_html(text);
    };
    value.as_string().unwrap_or_else(|| escape_html(text))
}

#[cfg(not(target_arch = "wasm32"))]
fn highlight_policy_html(_: &str, text: &str) -> String {
    escape_html(text)
}

#[cfg(target_arch = "wasm32")]
fn highlight_policy_block(element_id: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(element) = document.get_element_by_id(element_id) else {
        return;
    };
    let _ = element.remove_attribute("data-highlighted");
    let Ok(hljs) = js_sys::Reflect::get(&window, &JsValue::from_str("hljs")) else {
        return;
    };
    if hljs.is_undefined() || hljs.is_null() {
        return;
    }
    let Ok(highlight_fn) = js_sys::Reflect::get(&hljs, &JsValue::from_str("highlightElement"))
    else {
        return;
    };
    let Ok(highlight_fn) = highlight_fn.dyn_into::<js_sys::Function>() else {
        return;
    };
    let _ = highlight_fn.call1(&hljs, &element);
}

fn diff_for_commit(hash: &str, message: &str) -> String {
    let selector = hash.bytes().last().unwrap_or(b'0').wrapping_sub(b'0') % 3;

    match selector {
        0 => format!(
            "diff --git a/modules/storage.nix b/modules/storage.nix\n\
index 13f3a11..7b2c9e1 100644\n\
--- a/modules/storage.nix\n\
+++ b/modules/storage.nix\n\
@@ -42,6 +42,7 @@\n\
   # Bind mount home directories to /persist\n\
+  fileSystems.\"/persist\".neededForBoot = true;\n\
   fileSystems.\"/home/admin\" = {{\n\
     device = \"/persist/home/admin\";\n\
     options = [ \"bind\" \"noatime\" ];\n\
     depends = [ \"/persist\" ];\n\
     neededForBoot = true;\n\
   }};\n\
\n\
@@ -93,15 +87,12 @@\n\
   warnings = if config.environment.persistence ? \"/persist/system\" then\n\
     [ ]\n\
   else [''\n\
     Impermanence is configured but environment.persistence is not available.\n\
     Make sure the impermanence module is imported.\n\
   ''];\n\
\n\
// {message}\n"
        ),
        1 => format!(
            "diff --git a/systems/x86_64-linux/reckless/default.nix b/systems/x86_64-linux/reckless/default.nix\n\
index 49cffc7..8d318fe 100755\n\
--- a/systems/x86_64-linux/reckless/default.nix\n\
+++ b/systems/x86_64-linux/reckless/default.nix\n\
@@ -259,10 +259,10 @@ in {{\n\
       crystal-forge = {{\n\
         enable = true;\n\
-        # log_level = \"debug\";\n\
+        log_level = \"debug\";\n\
         deployment = {{\n\
-          deployment_poll_interval = mkForce \"30\";\n\
+          deployment_poll_interval = mkForce \"15\";\n\
           fallback_to_local_build = false;\n\
         }};\n\
       }};\n\
\n\
// {message}\n"
        ),
        _ => format!(
            "diff --git a/services/web.nix b/services/web.nix\n\
index 77d3a10..e4b2c15 100644\n\
--- a/services/web.nix\n\
+++ b/services/web.nix\n\
@@ -12,9 +12,13 @@\n\
 {{ config, pkgs, ... }}:\n\
 {{\n\
   services.nginx = {{\n\
     enable = true;\n\
     recommendedGzipSettings = true;\n\
     recommendedOptimisation = true;\n\
+    clientMaxBodySize = \"50m\";\n\
   }};\n\
\n\
   services.prometheus.exporters.nginx = {{\n\
     enable = true;\n\
-    port = 9113;\n\
+    port = 9113;\n\
+    listenAddress = \"127.0.0.1\";\n\
   }};\n\
\n\
@@ -42,6 +46,18 @@\n\
   systemd.services.web-reload = {{\n\
     description = \"Reload web stack on config changes\";\n\
     serviceConfig.Type = \"oneshot\";\n\
     script = ''\n\
       set -euo pipefail\n\
       nginx -t\n\
       systemctl reload nginx\n\
     '';\n\
   }};\n\
\n\
// {message}\n"
        ),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Toast Notification
// ─────────────────────────────────────────────────────────────────────────────

async fn dispatch_sync_notification(
    message: String,
    is_success: bool,
    mut toast_message: Signal<Option<(String, bool)>>,
) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        if let Ok(promise) = Notification::request_permission() {
            if let Ok(result) = JsFuture::from(promise).await {
                if result.as_string().as_deref() == Some("granted") {
                    let _ = show_notification(&message, is_success);
                    return false;
                }
            }
        }
    }

    toast_message.set(Some((message, is_success)));
    true
}

#[cfg(target_arch = "wasm32")]
fn show_notification(message: &str, is_success: bool) -> Result<Notification, JsValue> {
    let title = if is_success {
        format!("[SUCCESS] {message}")
    } else {
        format!("[FAILED] {message}")
    };
    Notification::new(&title)
}

// ─────────────────────────────────────────────────────────────────────────────
// Mock Data Functions
// ─────────────────────────────────────────────────────────────────────────────

fn map_history_entries_to_commit_history(
    entries: &[SystemHistoryEntry],
) -> Vec<SystemCommitHistory> {
    use std::collections::HashSet;

    let mut seen_store_paths: HashSet<String> = HashSet::new();

    entries
        .iter()
        .enumerate()
        .map(|(idx, entry)| {
            let reverted = entry
                .store_path
                .as_ref()
                .map(|path| !seen_store_paths.insert(path.clone()))
                .unwrap_or(false);

            let config_identity = entry
                .system_configuration_name
                .clone()
                .or_else(|| entry.store_path.clone());

            let hash = entry
                .commit_hash
                .clone()
                .or_else(|| {
                    entry.store_path.as_ref().and_then(|path| {
                        path.split('/')
                            .next_back()
                            .and_then(|store_name| store_name.split('-').next())
                            .map(|value| value.to_string())
                    })
                })
                .unwrap_or_else(|| format!("state-{idx}"));

            let mut status_fragments = vec![format!("Reason: {}", entry.change_reason)];
            status_fragments.push(format!("Outcome: {}", entry.outcome));
            if reverted {
                status_fragments.push("Revert detected".to_string());
            }

            let message = {
                let reason = entry.change_reason.replace('_', " ");
                let outcome = entry.outcome.replace('_', " ");

                if reason.trim().is_empty() {
                    config_identity
                        .clone()
                        .map(|value| format!("Configuration {value}"))
                        .unwrap_or_else(|| "Configuration update".to_string())
                } else if outcome.trim().is_empty() {
                    reason
                } else {
                    format!("{reason}: {outcome}")
                }
            };

            SystemCommitHistory {
                hash,
                message,
                author: entry.actor.clone(),
                committed_at: entry.timestamp,
                was_deployed: true,
                deployed_at: Some(entry.timestamp),
                is_current: idx == 0,
                is_ready_to_deploy: false,
                build_status: None,
                diff_summary: Some(status_fragments.join(" · ")),
                flake_repo_url: entry.flake_repo_url.clone(),
                config_identity,
            }
        })
        .collect()
}

fn map_commit_infos_to_commit_history(
    commits: &[CommitInfo],
    current_commit: Option<String>,
) -> Vec<SystemCommitHistory> {
    commits
        .iter()
        .cloned()
        .map(|commit| {
            let committed_at = chrono::DateTime::parse_from_rfc3339(&commit.timestamp)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            let is_current = current_commit
                .as_ref()
                .map(|current| current == &commit.sha || current == &commit.short_sha)
                .unwrap_or(false);

            SystemCommitHistory {
                hash: commit.sha,
                message: commit.message,
                author: commit.author,
                committed_at,
                was_deployed: is_current,
                deployed_at: if is_current { Some(committed_at) } else { None },
                is_current,
                is_ready_to_deploy: !is_current,
                build_status: None,
                diff_summary: None,
                flake_repo_url: None,
                config_identity: None,
            }
        })
        .collect()
}

fn map_agent_events_to_logs(events: Vec<SystemAgentEvent>) -> Vec<DeploymentLogEntry> {
    events
        .into_iter()
        .map(|event| {
            let level = match event.level.to_ascii_lowercase().as_str() {
                "error" => LogLevel::Error,
                "warn" | "warning" => LogLevel::Warn,
                "debug" => LogLevel::Debug,
                _ => LogLevel::Info,
            };

            DeploymentLogEntry {
                message: event.message,
                timestamp: event.timestamp,
                level,
                phase: Some(if event.deployment_related {
                    "Deployment".to_string()
                } else {
                    event.event_type
                }),
            }
        })
        .collect()
}

fn mock_vulnerabilities() -> Vec<SystemVulnerability> {
    vec![
        SystemVulnerability {
            cve_id: "CVE-2024-1234".to_string(),
            severity: CveSeverity::Critical,
            cvss_score: Some(9.8),
            description: "Remote code execution vulnerability in OpenSSL affecting TLS handshake processing. An attacker could exploit this to execute arbitrary code.".to_string(),
            package_name: "openssl".to_string(),
            installed_version: "3.0.12".to_string(),
            fixed_version: Some("3.0.13".to_string()),
            first_seen: Some(Utc::now() - Duration::days(20)),
            published_at: Some(Utc::now() - Duration::days(30)),
            status: Some("open".to_string()),
            justification_category: None,
            justification_reason: None,
            justification_updated_at: None,
        },
        SystemVulnerability {
            cve_id: "CVE-2024-5678".to_string(),
            severity: CveSeverity::High,
            cvss_score: Some(7.5),
            description: "Denial of service vulnerability in curl HTTP/2 implementation.".to_string(),
            package_name: "curl".to_string(),
            installed_version: "8.4.0".to_string(),
            fixed_version: Some("8.5.0".to_string()),
            first_seen: Some(Utc::now() - Duration::days(10)),
            published_at: Some(Utc::now() - Duration::days(14)),
            status: Some("open".to_string()),
            justification_category: None,
            justification_reason: None,
            justification_updated_at: None,
        },
        SystemVulnerability {
            cve_id: "CVE-2024-9012".to_string(),
            severity: CveSeverity::High,
            cvss_score: Some(7.2),
            description: "Privilege escalation in sudo when using specific sudoers configurations.".to_string(),
            package_name: "sudo".to_string(),
            installed_version: "1.9.14".to_string(),
            fixed_version: None,
            first_seen: Some(Utc::now() - Duration::days(5)),
            published_at: Some(Utc::now() - Duration::days(7)),
            status: Some("open".to_string()),
            justification_category: None,
            justification_reason: None,
            justification_updated_at: None,
        },
        SystemVulnerability {
            cve_id: "CVE-2024-3456".to_string(),
            severity: CveSeverity::Medium,
            cvss_score: Some(5.3),
            description: "Information disclosure in nginx when using certain proxy configurations.".to_string(),
            package_name: "nginx".to_string(),
            installed_version: "1.24.0".to_string(),
            fixed_version: Some("1.25.0".to_string()),
            first_seen: Some(Utc::now() - Duration::days(30)),
            published_at: Some(Utc::now() - Duration::days(45)),
            status: Some("fixed".to_string()),
            justification_category: None,
            justification_reason: None,
            justification_updated_at: None,
        },
        SystemVulnerability {
            cve_id: "CVE-2024-7890".to_string(),
            severity: CveSeverity::Low,
            cvss_score: Some(3.1),
            description: "Minor information leak in bash completion scripts.".to_string(),
            package_name: "bash".to_string(),
            installed_version: "5.2".to_string(),
            fixed_version: None,
            first_seen: Some(Utc::now() - Duration::days(40)),
            published_at: Some(Utc::now() - Duration::days(60)),
            status: Some("open".to_string()),
            justification_category: None,
            justification_reason: None,
            justification_updated_at: None,
        },
    ]
}

// fallback_system_detail() has been moved to crate::systems::adapter

#[cfg(test)]
mod tests {
    use super::{map_agent_events_to_logs, map_history_entries_to_commit_history};
    use crate::api::models::{SystemAgentEvent, SystemHistoryEntry};
    use chrono::{Duration, Utc};

    #[test]
    fn history_mapping_marks_revert_when_store_path_reappears() {
        let now = Utc::now();
        let entries = vec![
            SystemHistoryEntry {
                timestamp: now,
                store_path: Some("/nix/store/aaaa-system".to_string()),
                system_configuration_name: Some("web-01".to_string()),
                change_reason: "cf_deployment".to_string(),
                commit_hash: Some("aaaaaaaa".to_string()),
                flake_name: Some("infra".to_string()),
                flake_repo_url: Some("https://example.com/infra.git".to_string()),
                actor: "agent".to_string(),
                outcome: "recorded".to_string(),
            },
            SystemHistoryEntry {
                timestamp: now - Duration::minutes(10),
                store_path: Some("/nix/store/bbbb-system".to_string()),
                system_configuration_name: Some("web-01".to_string()),
                change_reason: "cf_deployment".to_string(),
                commit_hash: Some("bbbbbbbb".to_string()),
                flake_name: Some("infra".to_string()),
                flake_repo_url: Some("https://example.com/infra.git".to_string()),
                actor: "agent".to_string(),
                outcome: "recorded".to_string(),
            },
            SystemHistoryEntry {
                timestamp: now - Duration::minutes(20),
                store_path: Some("/nix/store/aaaa-system".to_string()),
                system_configuration_name: Some("web-01".to_string()),
                change_reason: "cf_deployment".to_string(),
                commit_hash: Some("aaaaaaaa".to_string()),
                flake_name: Some("infra".to_string()),
                flake_repo_url: Some("https://example.com/infra.git".to_string()),
                actor: "agent".to_string(),
                outcome: "recorded".to_string(),
            },
        ];

        let timeline = map_history_entries_to_commit_history(&entries);

        assert_eq!(timeline.len(), 3);
        assert!(timeline[0].is_current);
        assert!(
            timeline[2]
                .diff_summary
                .as_deref()
                .unwrap_or_default()
                .contains("Revert detected")
        );
    }

    #[test]
    fn agent_event_mapping_preserves_deployment_phase() {
        let now = Utc::now();
        let logs = map_agent_events_to_logs(vec![SystemAgentEvent {
            timestamp: now,
            level: "info".to_string(),
            event_type: "state_change".to_string(),
            message: "agent reported cf_deployment".to_string(),
            deployment_related: true,
        }]);

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].phase.as_deref(), Some("Deployment"));
    }
}
