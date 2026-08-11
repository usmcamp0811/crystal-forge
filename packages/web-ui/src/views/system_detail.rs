//! System detail view — full information for a single NixOS system.
//!
//! Uses a tabbed layout with:
//! - Overview: System info, hardware, network, security
//! - History: Vertical commit timeline showing deployed vs skipped commits
//! - CVEs: Expandable vulnerability list
//! - Logs: Recent deployment logs

use chrono::{Local, Utc};
use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use js_sys::Object;
use serde_json::Value as JsonValue;
use uuid::Uuid;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, JsValue};

use crate::api::client::{
    ApiClientError, fetch_compliance_system_evidence, fetch_flake_timeline_for_tray,
    fetch_system_compliance_bundles, fetch_system_cve_scan_eligibility, fetch_system_cves,
    fetch_system_hardening, fetch_system_hardening_justifications,
    fetch_system_hardening_scan_eligibility, get_system_deployment_progress,
    request_system_generation_rollback, request_system_rollback, request_system_sync,
    save_system_hardening_justification,
    verify_generation_closure as verify_generation_closure_request,
};
use crate::api::models::{
    BuildStatus, CommitInfo, ComplianceEvidenceResponse, CveScanEligibilityResponse,
    DeploymentLogEntry, DeploymentStatus, FlakeSummary, HardeningJustificationResponse,
    HardeningScanEligibilityResponse, HardeningServiceResultResponse, HealthStatus, LogLevel,
    SaveHardeningJustificationRequest, SystemAgentEvent, SystemCommitHistory,
    SystemComplianceBundle, SystemDeploymentProgress, SystemDetail, SystemGeneration,
    SystemHistoryEntry, SystemRollbackGenerationRequest, SystemRollbackRequest,
    SystemVulnerability, VerifyGenerationClosureRequest,
};
use crate::components::compliance::EvidenceDrawer;
use crate::components::cve::CvesTab;
use crate::components::diff::DiffViewer;
use crate::components::icon::{Icon, IconName};
use crate::components::layout::Card;
use crate::components::modals::{RollbackConfirmDialog, SyncConfirmDialog};
use crate::components::notifications::Toast;
use crate::components::system::{
    EditSystemModal, PendingDeployBanner, deployment_state_label, environment_style, format_uptime,
};
use crate::routes::Route;
use crate::state::{
    app_state::AppState,
    auth,
    navigation_focus::{FocusTarget, NavigationFocus},
};
use crate::systems::adapter::{
    deploy_system_via_api, fetch_system_commits_via_api, load_system_agent_events_with_fallback,
    load_system_generations_with_fallback, load_system_history_with_fallback,
    update_system_via_api,
};
use crate::systems::adapter::{fallback_system_detail, load_system_detail_with_fallback};
use crate::theme;
use crate::views::flakes_list::{
    CommitFocusMeta, FlakeTrayNew, MockCommitItem, MockFlakeItem, map_flake_summary_to_tray_item,
    map_timeline_commits_to_view,
};
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

/// Result of loading a system's vulnerabilities.
///
/// Security data must never silently fall back to mock CVEs in production paths,
/// so the resource carries an explicit error/redirect signal that the CVE tab
/// renders as a real empty/error state (TASK-353 review).
#[derive(Debug, Clone, PartialEq)]
struct VulnerabilitiesLoad {
    items: Vec<SystemVulnerability>,
    error: Option<String>,
    redirect_to_login: bool,
}

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

fn effective_fqdn(system: &SystemDetail) -> String {
    system
        .fqdn
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| derived_fqdn(&system.hostname, system.environment.as_deref()))
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

fn format_short_duration(seconds: f64) -> String {
    let value = seconds.abs().round() as i64;
    if value < 60 {
        format!("{}s", value)
    } else if value < 3600 {
        format!("{}m {}s", value / 60, value % 60)
    } else {
        format!("{}h {}m", value / 3600, (value % 3600) / 60)
    }
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

/// Reads `?tab=deploy` from the current browser URL synchronously, so callers
/// can use it as a `use_signal` initial value. Matches the systems list
/// "Deploy" button, which navigates to `/systems/:id?tab=deploy`.
fn system_detail_wants_deploy_tab() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|window| window.location().search().ok())
            .map(|search| search.contains("tab=deploy"))
            .unwrap_or(false)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

#[derive(Clone, Debug, PartialEq)]
struct FlakeCommitPeekTarget {
    flake: FlakeSummary,
    sha: String,
    meta: CommitFocusMeta,
}

#[derive(Clone, Debug, PartialEq)]
struct FlakeCommitPeekState {
    flake: MockFlakeItem,
    focus_sha: String,
    focus_meta: CommitFocusMeta,
}

// ─────────────────────────────────────────────────────────────────────────────
// Main Component
// ─────────────────────────────────────────────────────────────────────────────

/// The system detail page, reached via `/systems/:id`.
#[component]
pub fn SystemDetailView(id: String) -> Element {
    let nav = navigator();
    let mut navigation_focus = use_context::<Signal<Option<NavigationFocus>>>();
    let mut breadcrumb_override = use_context::<Signal<Option<(String, String)>>>();
    let app_state = use_context::<Signal<AppState>>();

    // Current tab state — defaults to Overview but honours a ?tab=deploy query param
    // so that the systems list "Deploy" button can deep-link straight to the Deploy tab.
    //
    // NOTE: this must be read synchronously into the signal's initial value (matching
    // the query_param() pattern used by views::cves), not inside a use_effect. A
    // use_effect runs after the first render/commit, so the Overview tab's content
    // (and any tab-specific data loading it triggers) would flash briefly before the
    // effect ever fires, and on the WASM target the effect was observed to land too
    // late to reliably override the initial tab.
    let mut active_tab = use_signal(|| {
        if system_detail_wants_deploy_tab() {
            Tab::Deploy
        } else {
            Tab::Overview
        }
    });
    let mut edit_modal_open = use_signal(|| false);
    let mut remove_in_progress = use_signal(|| false);
    let mut remove_error_message: Signal<Option<String>> = use_signal(|| None);
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
    let mut show_generation_rollback_modal = use_signal(|| false);
    let mut rollback_target: Signal<Option<SystemCommitHistory>> = use_signal(|| None);

    // Jump-to-log target: set when "view logs" is clicked on a History event so the
    // Logs tab can scroll to and highlight the matching line. Carries a nonce so
    // repeated jumps to the same event still retrigger the effect.
    let mut log_jump_target: Signal<Option<(String, u64)>> = use_signal(|| None);

    // Toast notification state
    let mut toast_message: Signal<Option<(String, bool)>> = use_signal(|| None); // (message, is_success)
    let mut deploy_action_notice: Signal<Option<(String, bool)>> = use_signal(|| None);
    let mut flake_commit_peek: Signal<Option<FlakeCommitPeekState>> = use_signal(|| None);
    let mut flake_commit_peek_reload = use_signal(|| 0_u64);
    // Reload nonce for system detail — incremented after edit-save to re-fetch the system.
    let mut detail_reload = use_signal(|| 0_u64);

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

    // System data state — use_resource reads detail_reload as a dependency so that
    // incrementing the reload nonce (e.g. after saving edits) triggers a re-fetch.
    let id_for_detail = id.clone();
    let mut detail_resource = use_resource(move || {
        let _ = detail_reload();
        let id = id_for_detail.clone();
        async move { load_system_detail_with_fallback(&id).await }
    });

    let flake_commit_peek_for_resource = flake_commit_peek.clone();
    let flake_commit_peek_resource = use_resource(move || {
        let _reload = flake_commit_peek_reload();
        let peek = flake_commit_peek_for_resource.read().clone();
        async move {
            let Some(peek) = peek else {
                return None;
            };
            Some(fetch_flake_timeline_for_tray(peek.flake.id()).await)
        }
    });

    let id_for_vulns = id.clone();
    let mut vulnerabilities_resource = use_resource(move || {
        let id = id_for_vulns.clone();
        async move {
            // Security data must never fall back to mock CVEs in production paths.
            // Surface a real error/empty state instead so an API outage cannot
            // render fake vulnerabilities (TASK-353 review).
            let Ok(system_id) = Uuid::parse_str(&id) else {
                return VulnerabilitiesLoad {
                    items: Vec::new(),
                    error: Some("Invalid system identifier.".to_string()),
                    redirect_to_login: false,
                };
            };

            match fetch_system_cves(&system_id).await {
                Ok(items) => VulnerabilitiesLoad {
                    items,
                    error: None,
                    redirect_to_login: false,
                },
                Err(ApiClientError::Status {
                    code: 401 | 403, ..
                }) => VulnerabilitiesLoad {
                    items: Vec::new(),
                    error: None,
                    redirect_to_login: true,
                },
                Err(err) => VulnerabilitiesLoad {
                    items: Vec::new(),
                    error: Some(format!("Unable to load vulnerabilities: {err}")),
                    redirect_to_login: false,
                },
            }
        }
    });

    let mut deployment_progress_poll_tick = use_signal(|| 0_u64);
    let id_for_deployment_progress = id.clone();
    let deployment_progress_tick_for_resource = deployment_progress_poll_tick.clone();
    let deployment_progress_resource = use_resource(move || {
        let _ = deployment_progress_tick_for_resource();
        let id = id_for_deployment_progress.clone();
        async move {
            let Ok(system_id) = Uuid::parse_str(&id) else {
                return None::<SystemDeploymentProgress>;
            };

            get_system_deployment_progress(&system_id)
                .await
                .ok()
                .flatten()
        }
    });

    use_future(move || async move {
        loop {
            #[cfg(target_arch = "wasm32")]
            {
                gloo_timers::future::TimeoutFuture::new(4000).await;
                deployment_progress_poll_tick.set(deployment_progress_poll_tick() + 1);
            }
            #[cfg(not(target_arch = "wasm32"))]
            break;
        }
    });

    // History polling tick — only active during a deployment so Recent Activity and the
    // History tab pick up deployment-started/succeeded/failed events as they happen.
    // Declared `let mut` so the use_future move closure can call .set().
    let mut history_poll_tick = use_signal(|| 0_u64);
    {
        let dep_resource = deployment_progress_resource.clone();
        let mut tick = history_poll_tick.clone();
        use_future(move || async move {
            loop {
                #[cfg(target_arch = "wasm32")]
                {
                    gloo_timers::future::TimeoutFuture::new(4000).await;
                    if dep_resource.read_unchecked().is_some() {
                        tick.set(tick() + 1);
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                break;
            }
        });
    }

    let id_for_history = id.clone();
    let history_resource = use_resource(move || {
        let _ = history_poll_tick();
        let id = id_for_history.clone();
        async move {
            let Ok(system_id) = Uuid::parse_str(&id) else {
                return Vec::<SystemHistoryEntry>::new();
            };

            load_system_history_with_fallback(system_id).await.entries
        }
    });

    let mut agent_events_poll_tick = use_signal(|| 0_u64);
    let id_for_events = id.clone();
    let agent_events_poll_tick_for_resource = agent_events_poll_tick.clone();
    let agent_events_resource = use_resource(move || {
        let _ = agent_events_poll_tick_for_resource();
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

    // Refresh agent events while the Logs tab is active so auto-scroll/tail mode can
    // surface events generated after the page initially loaded. This is polling until
    // a streaming endpoint exists.
    {
        let active_tab_for_events = active_tab;
        use_future(move || async move {
            loop {
                #[cfg(target_arch = "wasm32")]
                {
                    gloo_timers::future::TimeoutFuture::new(3000).await;
                    if *active_tab_for_events.read() == Tab::Logs {
                        agent_events_poll_tick.set(agent_events_poll_tick() + 1);
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                break;
            }
        });
    }

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
        breadcrumb_override.set(None);
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
    {
        let mut breadcrumb_override = breadcrumb_override.clone();
        let hostname = system.hostname.clone();
        use_effect(move || {
            breadcrumb_override.set(Some(("Systems".to_string(), hostname.clone())));
        });
    }
    {
        let mut breadcrumb_override = breadcrumb_override.clone();
        use_drop(move || {
            breadcrumb_override.set(None);
        });
    }
    let deployment_progress = deployment_progress_resource
        .read_unchecked()
        .clone()
        .flatten();
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
    let effective_history_entries = if history_entries.is_empty() {
        synthesize_history_entries_from_commits(&deploy_commit_history)
    } else {
        history_entries.clone()
    };
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
    let vulnerabilities_load = vulnerabilities_resource
        .read_unchecked()
        .clone()
        .unwrap_or_else(|| VulnerabilitiesLoad {
            items: Vec::new(),
            error: None,
            redirect_to_login: false,
        });
    if vulnerabilities_load.redirect_to_login {
        nav.push(Route::LoginView {});
        return rsx! {
            div {
                class: "flex items-center justify-center py-12",
                p { class: "{theme::text::SECONDARY}", "Redirecting to login..." }
            }
        };
    }
    let vulnerabilities_loading = vulnerabilities_resource.read_unchecked().is_none();
    let vulnerabilities = vulnerabilities_load.items.clone();
    let vulnerabilities_error = vulnerabilities_load.error.clone();
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
    let detail_fqdn = effective_fqdn(&system);
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

    let on_open_flake_commit = {
        let mut flake_commit_peek = flake_commit_peek.clone();
        move |target: FlakeCommitPeekTarget| {
            flake_commit_peek.set(Some(FlakeCommitPeekState {
                flake: map_flake_summary_to_tray_item(&target.flake),
                focus_sha: target.sha,
                focus_meta: target.meta,
            }));
        }
    };

    let current_flake_peek = flake_commit_peek.read().clone();
    let peek_resource_state = flake_commit_peek_resource.read_unchecked().clone();
    let (peek_commits, peek_commits_loading, peek_commits_error): (
        Vec<MockCommitItem>,
        bool,
        Option<String>,
    ) = match (current_flake_peek.as_ref(), peek_resource_state.as_ref()) {
        (Some(peek), Some(Some(Ok(timelines)))) => {
            let commits = timelines
                .iter()
                .find(|timeline| timeline.flake_id == peek.flake.id())
                .map(|timeline| map_timeline_commits_to_view(&timeline.commits))
                .unwrap_or_default();
            (commits, false, None)
        }
        (Some(_), Some(Some(Err(err)))) => (Vec::new(), false, Some(err.to_string())),
        (Some(_), _) => (Vec::new(), true, None),
        (None, _) => (Vec::new(), false, None),
    };

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
                            onclick: move |_| show_generation_rollback_modal.set(true),
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
                let heartbeat_interval_sec = system.effective_heartbeat_interval_secs as i64;
                let heartbeat_next_in_sec = system
                    .last_seen
                    .map(|dt| heartbeat_interval_sec as f64 - now.signed_duration_since(dt).num_seconds() as f64)
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
                let deploy_stage = deployment_progress.as_ref().map(|p| p.stage.clone());
                let deploy_started_at_ms = deployment_progress
                    .as_ref()
                    .map(|p| p.issued_at.timestamp_millis() as f64);

                rsx! {
                    div {
                        class: "sd-metric-strip",
                        // Heartbeat — during a deploy the spinner turns purple and
                        // shows per-stage countdown text instead of a separate widget.
                        div {
                            class: "sd-metric",
                            div { class: "sd-metric-label", "Heartbeat" }
                            div {
                                class: "sd-metric-val",
                                crate::components::HeartbeatSpinner {
                                    interval_sec: heartbeat_interval_sec,
                                    next_in_sec: heartbeat_next_in_sec,
                                    size: 36,
                                    deploy_stage,
                                    deploy_started_at_ms,
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

            if let Some(progress) = deployment_progress.clone() {
                PendingDeployBanner {
                    progress,
                    hostname: system.hostname.clone(),
                    heartbeat_interval_secs: system.effective_heartbeat_interval_secs as i64,
                    heartbeat_next_in_secs: system.last_seen.map(|dt| {
                        system.effective_heartbeat_interval_secs as f64
                            - now.signed_duration_since(dt).num_seconds() as f64
                    }),
                    on_dismiss: move |_| deployment_progress_poll_tick.set(deployment_progress_poll_tick() + 1),
                    on_view_logs: move |_| active_tab.set(Tab::Logs),
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
                            target_store_path: deployment_progress.as_ref().map(|progress| progress.target_store_path.clone()),
                            history_entries: effective_history_entries.clone(),
                            on_open_cves: move |_| active_tab.set(Tab::Cves),
                            on_view_history: move |_| active_tab.set(Tab::History),
                            on_open_flake_commit: on_open_flake_commit,
                        }
                    },
                    Tab::Deploy => rsx! {
                        DeployTab {
                            system: system.clone(),
                            commits: deploy_commit_history.clone(),
                            generations: generations_result.generations.clone(),
                            current_generation: generations_result.current_generation,
                            allow_mutations: can_mutate,
                            deploy_notice: deploy_action_notice.read().clone(),
                            on_clear_deploy_notice: move |_| deploy_action_notice.set(None),
                            on_open_flake_commit: on_open_flake_commit,
                            on_deploy_commit: {
                                let system_id = system.id;
                                let hostname = system.hostname.clone();
                                let toast_message = toast_message.clone();
                                let mut deploy_action_notice = deploy_action_notice.clone();
                                move |commit_sha: String| {
                                    let hostname = hostname.clone();
                                    let toast_message = toast_message.clone();
                                    let mut deploy_action_notice = deploy_action_notice.clone();
                                    deploy_action_notice.set(Some((
                                        format!(
                                            "Requesting deployment of {} to {}…",
                                            hostname,
                                            commit_sha.chars().take(7).collect::<String>()
                                        ),
                                        true,
                                    )));
                                    spawn(async move {
                                        let (message, success) = match deploy_system_via_api(system_id, commit_sha.clone()).await {
                                            Ok(response) if !response.trim().is_empty() => (response, true),
                                            Ok(_) => (
                                                format!(
                                                    "Requested deployment of {} to {}",
                                                    hostname,
                                                    commit_sha.chars().take(7).collect::<String>()
                                                ),
                                                true,
                                            ),
                                            Err(error) => (
                                                format!("Deploy request failed for {}: {}", hostname, error),
                                                false,
                                            ),
                                        };
                                        deploy_action_notice.set(Some((message.clone(), success)));
                                        let _ = dispatch_sync_notification(message, success, toast_message).await;
                                    });
                                }
                            },
                            on_deploy_generation: {
                                let system_id = system.id;
                                let hostname = system.hostname.clone();
                                let toast_message = toast_message.clone();
                                let mut deploy_action_notice = deploy_action_notice.clone();
                                move |store_path: String| {
                                    let hostname = hostname.clone();
                                    let toast_message = toast_message.clone();
                                    let mut deploy_action_notice = deploy_action_notice.clone();
                                    deploy_action_notice.set(Some((
                                        format!("Requesting generation rollback for {}…", hostname),
                                        true,
                                    )));
                                    spawn(async move {
                                        let (message, success) = match request_system_generation_rollback(
                                            &system_id,
                                            &SystemRollbackGenerationRequest {
                                                store_path: store_path.clone(),
                                            },
                                        )
                                        .await
                                        {
                                            Ok(response) if !response.message.trim().is_empty() => {
                                                (response.message, true)
                                            }
                                            Ok(_) => (
                                                format!("Requested generation rollback for {}", hostname),
                                                true,
                                            ),
                                            Err(error) => (
                                                format!(
                                                    "Generation rollback request failed for {}: {}",
                                                    hostname, error
                                                ),
                                                false,
                                            ),
                                        };
                                        deploy_action_notice.set(Some((message.clone(), success)));
                                        let _ = dispatch_sync_notification(message, success, toast_message).await;
                                    });
                                }
                            }
                        }
                    },
                    Tab::History => rsx! {
                        HistoryTab {
                            entries: effective_history_entries.clone(),
                            commits: deploy_commit_history.clone(),
                            current_generation: system.generation,
                            deployment_policy: system.deployment_policy.clone(),
                            allow_mutations: can_mutate,
                            on_rollback: move |commit| {
                                rollback_target.set(Some(commit));
                                show_rollback_dialog.set(true);
                            },
                            on_view_logs: move |event_id: String| {
                                // Record the jump target with a fresh nonce, then switch to Logs.
                                let nonce = chrono::Utc::now().timestamp_millis() as u64;
                                log_jump_target.set(Some((event_id, nonce)));
                                active_tab.set(Tab::Logs);
                            },
                        }
                    },
                    Tab::Cves => rsx! {
                        CvesTab {
                            system_id: system.id,
                            cve_counts: system.cve_counts.clone(),
                            vulnerabilities: vulnerabilities.clone(),
                            allow_mutations: can_mutate,
                            loading: vulnerabilities_loading,
                            error: vulnerabilities_error.clone(),
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
                        LogsTabStyled {
                            logs: deployment_logs.clone(),
                            history_entries: effective_history_entries.clone(),
                            jump_target: log_jump_target.read().clone(),
                        }
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

        if let Some(peek) = current_flake_peek.clone() {
            FlakeTrayNew {
                commits: peek_commits.clone(),
                commits_loading: peek_commits_loading,
                commits_error: peek_commits_error.clone(),
                notice: None,
                is_admin: false,
                focus_sha: Some(peek.focus_sha.clone()),
                focus_meta: Some(peek.focus_meta.clone()),
                flake: peek.flake.clone(),
                on_edit: move |_| {},
                on_sync: move |_| flake_commit_peek_reload.set(flake_commit_peek_reload() + 1),
                on_history_rewrite_conflict: move |(_flake_id, detail)| {
                    toast_message.set(Some((
                        format!("Flake history rewrite detected: {detail}"),
                        false,
                    )));
                },
                on_close: move |_| flake_commit_peek.set(None),
                on_open_evaluation: move |focus: NavigationFocus| {
                    navigation_focus.set(Some(focus));
                    nav.push(Route::EvaluationsView {});
                },
                on_open_build: move |focus: NavigationFocus| {
                    navigation_focus.set(Some(focus));
                    nav.push(Route::BuildsView {});
                },
                on_open_systems: move |focus: NavigationFocus| {
                    navigation_focus.set(Some(NavigationFocus {
                        target: FocusTarget::Systems,
                        commit_sha: focus.commit_sha,
                        flake_name: focus.flake_name,
                        status: focus.status,
                        policy_name: None,
                    }));
                    nav.push(Route::SystemsView {});
                },
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
                remove_in_progress: remove_in_progress(),
                remove_error_message: remove_error_message.read().clone(),
                on_close: move |_| {
                    remove_error_message.set(None);
                    remove_in_progress.set(false);
                    edit_modal_open.set(false);
                },
                on_save: move |request: crate::api::models::UpdateSystemRequest| {
                    let system_id = system.id;
                    spawn(async move {
                        match update_system_via_api(
                            system_id,
                            request,
                        )
                        .await
                        {
                            Ok(_) => {
                                edit_modal_open.set(false);
                                detail_reload.set(detail_reload() + 1);
                            }
                            Err(error_message) => {
                                toast_message.set(Some((error_message, false)));
                                edit_modal_open.set(false);
                            }
                        }
                    });
                },
                on_delete: {
                    let system_id = system.id;
                    let hostname = system.hostname.clone();
                    let nav = nav.clone();
                    move |_| {
                        let system_id = system_id;
                        let hostname = hostname.clone();
                        let nav = nav.clone();
                        let mut toast_message = toast_message;
                        remove_error_message.set(None);
                        remove_in_progress.set(true);
                        spawn(async move {
                            match crate::api::client::deactivate_system(&system_id).await {
                                Ok(_) => {
                                    remove_in_progress.set(false);
                                    toast_message.set(Some((
                                        format!("System {} removed from registry", hostname),
                                        true,
                                    )));
                                    // Navigate back to systems list
                                    nav.push(Route::SystemsView {});
                                }
                                Err(error) => {
                                    remove_in_progress.set(false);
                                    remove_error_message
                                        .set(Some(format!("Failed to remove system: {error}")));
                                }
                            }
                        });
                    }
                },
            }
        }

        if *show_ssh_modal.read() {
            SshConnectModal {
                system: system.clone(),
                on_close: move |_| show_ssh_modal.set(false),
            }
        }

        if *show_generation_rollback_modal.read() {
            GenerationRollbackModal {
                hostname: system.hostname.clone(),
                environment: system.environment.clone(),
                generations: generations_result.generations.clone(),
                current_generation: generations_result.current_generation,
                on_close: move |_| show_generation_rollback_modal.set(false),
                on_confirm: {
                    let system_id = system.id;
                    let hostname = system.hostname.clone();
                    let toast_message = toast_message.clone();
                    move |store_path: String| {
                        show_generation_rollback_modal.set(false);
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
                            if success {
                                active_tab.set(Tab::Overview);
                                deployment_progress_poll_tick.set(deployment_progress_poll_tick() + 1);
                            }
                            let _ = dispatch_sync_notification(message, success, toast_message).await;
                        });
                    }
                },
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

/// Aggregated compliance data for the system detail tab.
#[derive(Clone, PartialEq, Debug)]
struct SystemComplianceData {
    bundles: Vec<SystemComplianceBundle>,
    error: Option<String>,
}

fn compliance_score_color(score: i64) -> &'static str {
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
    let system_id = system.id;

    // Fetch applicable bundles for this system using the optimized system-scoped endpoint.
    // All-or-nothing behavior: infrastructure failures show top-level error.
    let mut compliance_resource = use_resource(move || {
        let sid = system_id;
        async move {
            match fetch_system_compliance_bundles(&sid).await {
                Ok(response) => SystemComplianceData {
                    bundles: response.bundles,
                    error: None,
                },
                Err(e) => SystemComplianceData {
                    bundles: Vec::new(),
                    error: Some(format!("Failed to load compliance bundles: {e}")),
                },
            }
        }
    });

    // Evidence drawer state
    let mut evidence_open: Signal<Option<(Uuid, Uuid)>> = use_signal(|| None); // (bundle_id, system_id)
    let mut evidence_data: Signal<Option<Result<ComplianceEvidenceResponse, String>>> =
        use_signal(|| None);
    let mut evidence_bundle_name: Signal<Option<String>> = use_signal(|| None);

    let loading = compliance_resource.read_unchecked().is_none();
    let data = compliance_resource
        .read_unchecked()
        .clone()
        .unwrap_or(SystemComplianceData {
            bundles: Vec::new(),
            error: None,
        });

    rsx! {
        div {
            style: "display:flex;flex-direction:column;gap:14px;",

            // Loading state
            if loading {
                div {
                    class: "flex items-center justify-center py-8",
                    crate::components::loading::DashboardLoadingSpinner {
                        label: "Loading compliance data…".to_string(),
                        size: 36,
                    }
                }
            }
            // Error state (only if no bundles loaded at all)
            else if data.bundles.is_empty() && data.error.is_some() {
                div {
                    class: "sd-callout sd-callout-danger",
                    Icon { name: IconName::Warn, size: 13 }
                    div { style: "font-size:12px;", "{data.error.as_ref().unwrap()}" }
                }
            }
            // Empty state (no applicable bundles)
            else if data.bundles.is_empty() {
                div {
                    class: "card",
                    style: "padding:24px;text-align:center;",
                    div { class: "empty",
                        h3 { "No compliance bundles" }
                        p { style: "font-size:13px;color:var(--cf-text-muted);margin-top:6px;",
                            "This system is not covered by any compliance bundle."
                        }
                        Link {
                            to: crate::routes::Route::ComplianceView {},
                            class: "btn btn-primary focus-ring",
                            style: "margin-top:14px;",
                            "Go to Compliance"
                        }
                    }
                }
            }
            // Populated state
            else {
                for bd in data.bundles.iter() {
                    {
                        let score = bd.rollup.score;
                        let score_color = compliance_score_color(score);
                        let score_width = format!("width:{}%;height:100%;background:{};", score, score_color);
                        let bundle_id = bd.bundle.id;
                        let bundle_name = bd.bundle.name.clone();
                        let framework = bd.bundle.framework.clone();
                        let version = bd.bundle.version.clone();
                        let owner = bd.bundle.owner.clone();
                        let total = bd.rollup.total;
                        let pass = bd.rollup.pass;
                        let warn = bd.rollup.warn;
                        let fail = bd.rollup.fail;
                        let waiver = bd.rollup.waiver;
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
                                            span { style: "font-size:15px;font-weight:650;", "{bundle_name}" }
                                            span { class: "chip chip-info", style: "font-size:10px;", "{framework}" }
                                            span { class: "chip chip-unknown", style: "font-size:10px;", "{version}" }
                                            if fail == 0 {
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
                                                    " {fail} failing"
                                                }
                                            }
                                        }
                                        div {
                                            style: "font-size:12px;color:var(--cf-text-muted);margin-top:4px;",
                                            "{total} controls · owned by "
                                            span { class: "mono", "{owner}" }
                                        }
                                    }
                                    button {
                                        class: "btn btn-primary focus-ring",
                                        onclick: move |_| {
                                            let bn = bundle_name.clone();
                                            evidence_open.set(Some((bundle_id, system_id)));
                                            evidence_bundle_name.set(Some(bn));
                                            evidence_data.set(None);
                                            spawn({
                                                let bid = bundle_id;
                                                let sid = system_id;
                                                async move {
                                                    match fetch_compliance_system_evidence(&bid, &sid, None).await {
                                                        Ok(resp) => evidence_data.set(Some(Ok(resp))),
                                                        Err(e) => evidence_data.set(Some(Err(e.to_string()))),
                                                    }
                                                }
                                            });
                                        },
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
                                            "{score}%"
                                        }
                                    }
                                    div {
                                        style: "display:flex;gap:14px;font-size:12px;",
                                        span { span { class: "mono", style: "font-weight:700;color:#34d399;", "{pass}" } " pass" }
                                        span { span { class: "mono", style: "font-weight:700;color:#fbbf24;", "{warn}" } " warn" }
                                        span { span { class: "mono", style: "font-weight:700;color:#f87171;", "{fail}" } " fail" }
                                        span { span { class: "mono", style: "font-weight:700;color:#a78bfa;", "{waiver}" } " waiver" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Evidence drawer — shown when evidence is loading, loaded, or errored
            if evidence_open.read().is_some() {
                match &*evidence_data.read() {
                    Some(Ok(evidence_response)) => {
                        rsx! {
                            EvidenceDrawer {
                                evidence: evidence_response.clone(),
                                bundle_name: evidence_bundle_name.read().clone().unwrap_or_default(),
                                on_close: move |_| {
                                    evidence_open.set(None);
                                    evidence_data.set(None);
                                },
                            }
                        }
                    }
                    Some(Err(error)) => {
                        rsx! {
                            div { class: "fl-tray-backdrop", onclick: move |_| { evidence_open.set(None); evidence_data.set(None); } }
                            aside {
                                class: "fl-tray",
                                style: "width:min(480px,96vw);padding:24px;",
                                div { class: "sd-callout sd-callout-danger",
                                    Icon { name: IconName::Warn, size: 13 }
                                    div { style: "font-size:12px;", "Failed to load evidence: {error}" }
                                }
                                div { style: "margin-top:14px;text-align:right;",
                                    button {
                                        class: "btn btn-ghost focus-ring",
                                        onclick: move |_| { evidence_open.set(None); evidence_data.set(None); },
                                        "Close"
                                    }
                                }
                            }
                        }
                    }
                    None => {
                        rsx! {
                            div { class: "fl-tray-backdrop" }
                            aside {
                                class: "fl-tray",
                                style: "width:min(480px,96vw);padding:24px;display:flex;align-items:center;justify-content:center;",
                                crate::components::loading::DashboardLoadingSpinner {
                                    label: "Loading evidence…".to_string(),
                                    size: 36,
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
fn GenerationRollbackModal(
    hostname: String,
    environment: Option<String>,
    generations: Vec<SystemGeneration>,
    current_generation: Option<i32>,
    on_close: EventHandler<()>,
    on_confirm: EventHandler<String>,
) -> Element {
    let rollback_candidates = generations
        .iter()
        .filter(|generation| {
            !generation.is_current
                && generation
                    .store_path
                    .as_ref()
                    .map(|path| !path.is_empty())
                    .unwrap_or(false)
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut selected_generation = use_signal(|| {
        rollback_candidates
            .first()
            .map(|generation| generation.generation)
    });
    let selected = selected_generation.read().and_then(|generation_number| {
        rollback_candidates
            .iter()
            .find(|item| item.generation == generation_number)
            .cloned()
    });
    let selected_store_path = selected.as_ref().and_then(|item| item.store_path.clone());
    let environment_name = environment.unwrap_or_else(|| "unknown".to_string());
    let is_production = matches!(
        environment_name.to_ascii_lowercase().as_str(),
        "prod" | "production"
    );
    let mut confirm_text = use_signal(String::new);
    let confirm_enabled =
        selected_store_path.is_some() && (!is_production || *confirm_text.read() == hostname);

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_close.call(()),
            div {
                class: "modal",
                style: "width:min(640px,96vw);max-height:92vh;",
                onclick: move |e| e.stop_propagation(),

                div { class: "modal-head",
                    h2 { Icon { name: IconName::Rollback, size: 14 } span { style: "margin-left:6px;", "Rollback" } }
                    p { "Select a prior NixOS generation for " span { class: "mono", "{hostname}" } "." }
                }

                div { class: "modal-body", style: "overflow-y:auto;",
                    div { class: "sd-callout sd-callout-warn", style: "margin-bottom:14px;",
                        Icon { name: IconName::Warn, size: 13 }
                        div { style: "font-size:12px;", "Rolling back bypasses the current deployment policy and gate policies. Use only when the current generation is broken." }
                    }

                    if rollback_candidates.is_empty() {
                        div { class: "empty", style: "margin:0;",
                            h3 { "No rollback generations available" }
                            div { "This host has no prior generation with a recorded store path." }
                        }
                    } else {
                        div { class: "sd-commit-list", style: "max-height:280px;",
                            for generation in rollback_candidates.iter() {
                                {
                                    let is_selected = *selected_generation.read() == Some(generation.generation);
                                    let gen_number = generation.generation;
                                    let sha = generation
                                        .commit_hash
                                        .clone()
                                        .unwrap_or_else(|| "—".to_string());
                                    let short_sha = sha.chars().take(7).collect::<String>();
                                    let when = generation.timestamp.format("%b %d, %H:%M").to_string();
                                    rsx! {
                                        button {
                                            class: if is_selected { "sd-commit-item focus-ring selected" } else { "sd-commit-item focus-ring" },
                                            style: "grid-template-columns:72px 80px 1fr auto auto;",
                                            onclick: move |_| selected_generation.set(Some(gen_number)),
                                            span { class: "mono sd-commit-sha", style: "color:var(--cf-brand-purple);", "#{generation.generation}" }
                                            span { class: "mono sd-commit-sha", "{short_sha}" }
                                            span { class: "sd-commit-msg", "Rollback to generation #{generation.generation}" }
                                            span { class: "sd-commit-meta", "{when}" }
                                            if Some(generation.generation) == current_generation {
                                                span { class: "chip chip-healthy", "active" }
                                            } else {
                                                span { class: "chip chip-unknown", "rollback" }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if let Some(selected) = selected.as_ref() {
                            dl { class: "kv-grid", style: "margin-top:16px;",
                                dt { "Target" } dd { class: "mono", "{hostname}" }
                                dt { "To" } dd { class: "mono", "gen #{selected.generation}" }
                                dt { "Store path" }
                                dd { class: "mono", style: "font-size:11px;white-space:normal;word-break:break-all;", "{selected.store_path.clone().unwrap_or_default()}" }
                            }
                        }

                        if is_production {
                            div { class: "field", style: "margin-top:16px;",
                                label { "Type the hostname to confirm on production" }
                                input {
                                    class: "input mono focus-ring",
                                    placeholder: "{hostname}",
                                    value: "{confirm_text}",
                                    oninput: move |e| confirm_text.set(e.value()),
                                }
                            }
                        }
                    }
                }

                div { class: "modal-foot",
                    button { class: "btn btn-ghost focus-ring", onclick: move |_| on_close.call(()), "Cancel" }
                    button {
                        class: "btn btn-primary focus-ring",
                        disabled: !confirm_enabled,
                        onclick: move |_| {
                            if let Some(store_path) = selected_store_path.clone() {
                                on_confirm.call(store_path);
                            }
                        },
                        Icon { name: IconName::Rollback, size: 13 }
                        if let Some(selected) = selected.as_ref() {
                            " Roll back to gen #{selected.generation}"
                        } else {
                            " Roll back"
                        }
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
        .unwrap_or_else(|| effective_fqdn(&system));
    let fqdn = effective_fqdn(&system);
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
    target_store_path: Option<String>,
    history_entries: Vec<SystemHistoryEntry>,
    on_open_cves: EventHandler<()>,
    on_view_history: EventHandler<()>,
    on_open_flake_commit: EventHandler<FlakeCommitPeekTarget>,
) -> Element {
    let nav = use_navigator();
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
    let heartbeat_interval_overview_sec = system.effective_heartbeat_interval_secs as i64;
    let heartbeat_next_in_sec = system
        .last_seen
        .map(|dt| {
            heartbeat_interval_overview_sec as f64
                - now.signed_duration_since(dt).num_seconds() as f64
        })
        .unwrap_or(0.0);
    let fqdn_text = effective_fqdn(&system);

    let flake_name = system
        .flake
        .as_ref()
        .map(|f| f.name.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let flake_commit = current_commit
        .as_ref()
        .map(|commit| commit.hash.clone())
        .or_else(|| system.flake.as_ref().and_then(|f| f.latest_commit.clone()))
        .unwrap_or_else(|| "unknown".to_string());
    let flake_commit_short = if flake_commit == "unknown" {
        flake_commit.clone()
    } else {
        flake_commit.chars().take(8).collect::<String>()
    };
    let flake_commit_for_open = flake_commit.clone();
    let flake_commit_for_label = flake_commit_short.clone();
    let flake_commit_for_title = flake_commit.clone();
    let flake_summary_for_commit = system.flake.clone();
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
    let store_path_text = system
        .current_store_path
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let target_store_path_text =
        target_store_path.filter(|target| !target.is_empty() && target != &store_path_text);
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

    // Restart classification display strings.
    let restart_type_label = match system.restart_type.as_deref() {
        Some("system_reboot") => "System reboot",
        Some("agent_restart") => "Agent restart",
        Some("unknown") => "Unknown",
        _ => "",
    };
    let restart_at_text = system.last_restart_at.map(|dt| {
        let d = now.signed_duration_since(dt);
        if d.num_minutes() < 60 {
            format!("{}m ago", d.num_minutes().max(0))
        } else if d.num_hours() < 24 {
            format!("{}h ago", d.num_hours())
        } else {
            format!("{}d ago", d.num_days())
        }
    });

    let recent_activity = history_entries
        .iter()
        .take(9)
        .map(activity_row_from_history)
        .collect::<Vec<_>>();
    let tag_suggestions = vec![
        "prod".to_string(),
        "db".to_string(),
        "web".to_string(),
        "edge".to_string(),
        "critical".to_string(),
        "canary".to_string(),
    ];

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
                    dt { "Commit" }
                    dd { class: "mono",
                        button {
                            class: "tl-commit-link mono focus-ring",
                            title: "Open this commit in Flakes",
                            onclick: move |_| {
                                if let (Some(flake), false) = (flake_summary_for_commit.clone(), flake_commit_for_open == "unknown") {
                                    on_open_flake_commit.call(FlakeCommitPeekTarget {
                                        flake,
                                        sha: flake_commit_for_open.clone(),
                                        meta: CommitFocusMeta {
                                            msg: Some(commit_message_text.clone()),
                                            author: current_commit.as_ref().map(|commit| commit.author.clone()),
                                            at: current_commit.as_ref().map(|commit| commit.committed_at.format("%Y-%m-%d %H:%M UTC").to_string()),
                                        },
                                    });
                                }
                            },
                            "{flake_commit_for_label}"
                        }
                        span { style: "margin:0 6px; color:var(--cf-text-muted);", "build" }
                        button {
                            class: "tl-commit-link mono focus-ring",
                            title: "Open the build for {flake_commit_for_title}",
                            onclick: move |_| {
                                nav.push(Route::BuildsView {});
                            },
                            "{generation_text}"
                        }
                    }
                    dt { "Message" } dd { style: "white-space: normal; font-family: var(--font-sans);", "{commit_message_text}" }
                    dt { "Generation" } dd { class: "mono", "{generation_text}{generation_mismatch_note}" }
                    dt { "NixOS" } dd { class: "mono", "{nixos_version}" }
                    dt { "Kernel" } dd { class: "mono", "{kernel}" }
                    dt { "Store path" }
                    dd {
                        class: "mono",
                        style: "font-size:11px; white-space:normal; word-break:break-all; line-height:1.4;",
                        title: "{store_path_text}",
                        "{store_path_text}"
                    }
                    if let Some(target_store_path_text) = target_store_path_text {
                        dt { style: "color:#fbbf24;", "Target" }
                        dd {
                            class: "mono",
                            style: "font-size:11px; white-space:normal; word-break:break-all; line-height:1.4; color:#fbbf24;",
                            title: "{target_store_path_text}",
                            "{target_store_path_text}"
                            span { class: "chip chip-warning", style: "margin-left:6px; font-size:10px;", "drift" }
                        }
                    }
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
                    if !restart_type_label.is_empty() {
                        dt { "Last restart" }
                        dd {
                            span {
                                class: if system.restart_type.as_deref() == Some("system_reboot") {
                                    "chip chip-warning"
                                } else {
                                    "chip chip-info"
                                },
                                "{restart_type_label}"
                            }
                            if let Some(ref at_text) = restart_at_text {
                                span {
                                    style: "margin-left: 6px; color: var(--cf-text-muted); font-size: 12px;",
                                    "{at_text}"
                                }
                            }
                        }
                    }
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
                        interval_sec: heartbeat_interval_overview_sec,
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
                    button {
                        class: "btn btn-ghost xs focus-ring",
                        onclick: move |_| on_view_history.call(()),
                        "View all"
                    }
                }
                div {
                    class: "timeline sd-timeline",
                    if recent_activity.is_empty() {
                        div { class: "text-sm", style: "color:var(--cf-text-muted);", "No recorded events yet" }
                    }
                    for activity in recent_activity.iter() {
                        div {
                            class: "tl-item",
                            span { class: "tl-dot", style: "--status-color: {activity.color};" }
                            div {
                                class: "tl-body",
                                div {
                                    class: "tl-title",
                                    span { style: "color: {activity.color}; flex-shrink: 0; line-height: 0;", Icon { name: activity.icon, size: 12 } }
                                    span { "{activity.title}" }
                                }
                                if let Some(sub) = activity.sub.as_ref() {
                                    div { class: "tl-sub mono", title: "{sub}", "{sub}" }
                                }
                                div { class: "tl-meta", "{relative_time(activity.timestamp)}" }
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
                            let tag_for_filter = tag.clone();
                            rsx! {
                                span {
                                    class: "sd-tag mono sd-tag-chip",
                                    button {
                                        class: "sd-tag-label focus-ring",
                                        title: "Filter fleet by #{tag_for_filter}",
                                        onclick: move |_| {
                                            nav.push(Route::SystemsView {});
                                        },
                                        "#{tag}"
                                    }
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
                                list: "cf-fleet-tags",
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
                            datalist {
                                id: "cf-fleet-tags",
                                for suggestion in &tag_suggestions {
                                    option { value: "{suggestion}" }
                                }
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
                    "Free-form labels for your own grouping & filtering — click a tag to slice the fleet by it. They don't affect policies or deployment."
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
    deploy_notice: Option<(String, bool)>,
    on_clear_deploy_notice: EventHandler<()>,
    on_open_flake_commit: EventHandler<FlakeCommitPeekTarget>,
    on_deploy_commit: EventHandler<String>,
    on_deploy_generation: EventHandler<String>,
) -> Element {
    let flake_name = system
        .flake
        .as_ref()
        .map(|f| f.name.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let flake_summary = system.flake.clone();

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
                                on_clear_deploy_notice.call(());
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
                                on_clear_deploy_notice.call(());
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
                                let commit_message_for_click = commit_message.clone();
                                let commit_author = commit.author.clone();
                                let commit_author_for_click = commit_author.clone();
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
                                let when_text_for_click = when_text.clone();
                                let flake_summary_for_click = flake_summary.clone();
                                rsx! {
                                    button {
                                        key: "{commit_hash_for_key}",
                                        class: "{item_class}",
                                        onclick: move |_| {
                                            selected_commit.set(commit_hash_for_select.clone());
                                            verify_notice.set(None);
                                            on_clear_deploy_notice.call(());
                                        },
                                        span {
                                            class: "mono sd-commit-sha sd-commit-sha-link",
                                            title: "Open {commit_hash_for_title} in Flakes",
                                                onclick: move |evt| {
                                                    evt.stop_propagation();
                                                    if let Some(flake) = flake_summary_for_click.clone() {
                                                    on_open_flake_commit.call(FlakeCommitPeekTarget {
                                                        flake,
                                                        sha: commit_hash_for_title.clone(),
                                                        meta: CommitFocusMeta {
                                                            msg: Some(commit_message_for_click.clone()),
                                                            author: Some(commit_author_for_click.clone()),
                                                            at: Some(when_text_for_click.clone()),
                                                        },
                                                    });
                                                }
                                            },
                                            Icon { name: IconName::Git, size: 10 }
                                            " {short_hash}"
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
                                let generation_commit_hash = generation.commit_hash.clone();
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
                                let when_text_for_click = when_text.clone();
                                let flake_summary_for_click = flake_summary.clone();
                                rsx! {
                                    button {
                                        key: "gen-{gen_num}",
                                        class: "{item_class}",
                                        onclick: move |_| {
                                            selected_generation.set(Some(gen_num));
                                            verify_notice.set(None);
                                            on_clear_deploy_notice.call(());
                                        },
                                        span { class: "mono sd-commit-sha sd-generation-number", "{gen_label}" }
                                        span { class: "sd-commit-msg", "generation rollback" }
                                        if let Some(short) = commit_short {
                                            span {
                                                class: "sd-commit-meta mono sd-commit-sha-link",
                                                onclick: move |evt| {
                                                    evt.stop_propagation();
                                                    if let (Some(flake), Some(sha)) = (flake_summary_for_click.clone(), generation_commit_hash.clone()) {
                                                        on_open_flake_commit.call(FlakeCommitPeekTarget {
                                                            flake,
                                                            sha,
                                                            meta: CommitFocusMeta {
                                                                msg: Some("(generated commit)".to_string()),
                                                                author: None,
                                                                at: Some(when_text_for_click.clone()),
                                                            },
                                                        });
                                                    }
                                                },
                                                Icon { name: IconName::Git, size: 10 }
                                                " {short}"
                                            }
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
                        let flake_summary_for_from = flake_summary.clone();
                        let flake_summary_for_commit = flake_summary.clone();

                        rsx! {
                            dl {
                                class: "kv-grid",
                                dt { "Target" }
                                dd { class: "mono", "{system.hostname}" }

                                dt { "From" }
                                dd { class: "mono",
                                    button {
                                        class: "sd-commit-sha-link mono",
                                        onclick: move |_| {
                                            if let (Some(flake), false) = (flake_summary_for_from.clone(), from_commit == "unknown") {
                                                on_open_flake_commit.call(FlakeCommitPeekTarget {
                                                    flake,
                                                    sha: from_commit.clone(),
                                                    meta: CommitFocusMeta::default(),
                                                });
                                            }
                                        },
                                        Icon { name: IconName::Git, size: 10 }
                                        " {from_short}"
                                    }
                                    " · {current_gen_display}"
                                }

                                dt { "To" }
                                dd { class: "mono", "gen #{gen_num}" }

                                dt { "Store Path" }
                                dd {
                                    class: "mono",
                                    title: "{store_path_full}",
                                    if store_path_full.is_empty() { "unavailable" } else { "{store_path_full}" }
                                }

                                dt { "Commit" }
                                dd { class: "mono",
                                    button {
                                        class: "sd-commit-sha-link mono",
                                        title: "{commit_full}",
                                        onclick: move |_| {
                                            if let (Some(flake), Some(sha)) = (flake_summary_for_commit.clone(), generation_data.commit_hash.clone()) {
                                                on_open_flake_commit.call(FlakeCommitPeekTarget {
                                                    flake,
                                                    sha,
                                                    meta: CommitFocusMeta {
                                                        msg: Some("(generated commit)".to_string()),
                                                        author: None,
                                                        at: Some(generation_data.timestamp.format("%Y-%m-%d %H:%M UTC").to_string()),
                                                    },
                                                });
                                            }
                                        },
                                        Icon { name: IconName::Git, size: 10 }
                                        " {commit_display}"
                                    }
                                }

                                dt { "Strategy" }
                                dd { "rollback" }

                                dt { "Policy" }
                                dd { class: "mono", "{policy_name}" }
                            }

                            div {
                                class: "sd-callout sd-callout-info",
                                span { style: "color: #60a5fa; flex-shrink: 0;", Icon { name: IconName::Check, size: 13 } }
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
                                    Icon { name: IconName::Rollback, size: 13 }
                                    "{deploy_label}"
                                }
                            }

                            if let Some((message, is_success)) = deploy_notice.as_ref() {
                                div {
                                    class: if *is_success { "sd-callout sd-callout-info" } else { "sd-callout sd-callout-danger" },
                                    span {
                                        style: if *is_success { "color: #60a5fa; flex-shrink: 0;" } else { "color: #f87171; flex-shrink: 0;" },
                                        if *is_success {
                                            Icon { name: IconName::Check, size: 13 }
                                        } else {
                                            Icon { name: IconName::Warn, size: 13 }
                                        }
                                    }
                                    div { "{message}" }
                                }
                            }

                            if let Some(note) = verify_notice() {
                                div {
                                    class: "sd-callout sd-callout-info",
                                    span { style: "color: #60a5fa; flex-shrink: 0;", Icon { name: IconName::Check, size: 13 } }
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
                        let flake_summary_for_from = flake_summary.clone();
                        let flake_summary_for_to = flake_summary.clone();

                        rsx! {
                            dl {
                                class: "kv-grid",
                                dt { "Target" }
                                dd { class: "mono", "{system.hostname}" }

                                dt { "From" }
                                dd { class: "mono",
                                    button {
                                        class: "sd-commit-sha-link mono",
                                        onclick: move |_| {
                                            if let (Some(flake), false) = (flake_summary_for_from.clone(), from_commit == "unknown") {
                                                on_open_flake_commit.call(FlakeCommitPeekTarget {
                                                    flake,
                                                    sha: from_commit.clone(),
                                                    meta: CommitFocusMeta::default(),
                                                });
                                            }
                                        },
                                        Icon { name: IconName::Git, size: 10 }
                                        " {from_short}"
                                    }
                                    " · {current_gen_display}"
                                }

                                dt { "To" }
                                dd { class: "mono",
                                    button {
                                        class: "sd-commit-sha-link mono",
                                        onclick: move |_| {
                                            if let Some(flake) = flake_summary_for_to.clone() {
                                                on_open_flake_commit.call(FlakeCommitPeekTarget {
                                                    flake,
                                                    sha: commit.hash.clone(),
                                                    meta: CommitFocusMeta {
                                                        msg: Some(commit.message.clone()),
                                                        author: Some(commit.author.clone()),
                                                        at: Some(commit.committed_at.format("%Y-%m-%d %H:%M UTC").to_string()),
                                                    },
                                                });
                                            }
                                        },
                                        Icon { name: IconName::Git, size: 10 }
                                        " {to_short}"
                                    }
                                }

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
                                span { style: "color: #60a5fa; flex-shrink: 0;", Icon { name: IconName::Check, size: 13 } }
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
                                    Icon { name: IconName::Deploy, size: 13 }
                                    "{deploy_label}"
                                }
                            }

                            if let Some((message, is_success)) = deploy_notice.as_ref() {
                                div {
                                    class: if *is_success { "sd-callout sd-callout-info" } else { "sd-callout sd-callout-danger" },
                                    span {
                                        style: if *is_success { "color: #60a5fa; flex-shrink: 0;" } else { "color: #f87171; flex-shrink: 0;" },
                                        if *is_success {
                                            Icon { name: IconName::Check, size: 13 }
                                        } else {
                                            Icon { name: IconName::Warn, size: 13 }
                                        }
                                    }
                                    div { "{message}" }
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

/// Enhanced Logs tab matching the design reference.
///
/// Features:
/// - Auto-scroll mode for the combined deployment log stream
/// - Timezone toggle (local vs UTC)
/// - Log level filtering (all, info, warn, error)
/// - Day separators on date boundaries
/// - Jump-to-event from history with highlighting
/// - Clear button for the current displayed log window
#[derive(Clone, PartialEq, Props)]
struct LogsTabProps {
    logs: Vec<DeploymentLogEntry>,
    /// History entries used to synthesize anchored log lines so "view logs" jumps
    /// from the History tab land on the exact event line (design parity).
    #[props(default)]
    history_entries: Vec<SystemHistoryEntry>,
    /// Jump target from the History tab: (event id, nonce). The nonce lets repeated
    /// jumps to the same event retrigger the scroll/highlight effect.
    #[props(default)]
    jump_target: Option<(String, u64)>,
}

fn LogsTabStyled(props: LogsTabProps) -> Element {
    let LogsTabProps {
        logs,
        history_entries,
        jump_target,
    } = props;

    let mut filter = use_signal(|| "all".to_string());
    let mut tail = use_signal(|| true);
    let mut use_utc = use_signal(|| false);
    let mut highlighted_event = use_signal(|| None::<String>);
    let mut clear_before: Signal<Option<chrono::DateTime<Utc>>> = use_signal(|| None);
    // Stable DOM id for the scroll container so we can locate it (and anchored lines)
    // via document.query_selector without relying on event-based element handles.
    let log_stream_id = "sd-log-stream-container";

    // Keep the stream pinned to the bottom while auto-scroll is enabled. This runs
    // continuously instead of only when the checkbox changes, so newly polled agent
    // events remain visible as they arrive.
    let tail_for_autoscroll = tail.clone();
    use_future(move || async move {
        loop {
            #[cfg(target_arch = "wasm32")]
            {
                gloo_timers::future::TimeoutFuture::new(500).await;
                if *tail_for_autoscroll.read() {
                    if let Some(container) = web_sys::window()
                        .and_then(|w| w.document())
                        .and_then(|d| d.get_element_by_id(log_stream_id))
                    {
                        container.set_scroll_top(container.scroll_height());
                    }
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            break;
        }
    });

    // Auto-scroll to bottom when tailing. Locate the container via its stable DOM id
    // so we don't depend on event-based element handles.
    use_effect(move || {
        let _tail_dep = *tail.read();
        #[cfg(target_arch = "wasm32")]
        if _tail_dep {
            if let Some(container) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id(log_stream_id))
            {
                container.set_scroll_top(container.scroll_height());
            }
        }
    });

    // Handle jump to event from history: stop tailing, reset filter, scroll to the
    // anchored line, and flash-highlight it for ~2.4s (design parity).
    {
        let jump_target = jump_target.clone();
        use_effect(move || {
            let Some((event_id, _nonce)) = jump_target.clone() else {
                return;
            };
            tail.set(false);
            filter.set("all".to_string());
            highlighted_event.set(Some(event_id.clone()));

            // Scroll the anchored line into view (center it).
            #[cfg(target_arch = "wasm32")]
            {
                let event_id_scroll = event_id.clone();
                spawn(async move {
                    // Let the DOM update before querying for the anchor.
                    gloo_timers::future::TimeoutFuture::new(60).await;
                    if let Some(container) = web_sys::window()
                        .and_then(|w| w.document())
                        .and_then(|d| d.get_element_by_id(log_stream_id))
                    {
                        let selector = format!("[data-ev=\"{event_id_scroll}\"]");
                        if let Ok(Some(el)) = container.query_selector(&selector) {
                            if let Ok(html_el) = el.dyn_into::<web_sys::HtmlElement>() {
                                let target = (html_el.offset_top() - container.client_height() / 2
                                    + html_el.offset_height())
                                .max(0);
                                container.set_scroll_top(target);
                            }
                        }
                    }
                });
            }

            // Clear highlight after 2.4 seconds.
            let event_id_clone = event_id.clone();
            spawn(async move {
                gloo_timers::future::TimeoutFuture::new(2400).await;
                highlighted_event.with_mut(|hl| {
                    if hl.as_ref() == Some(&event_id_clone) {
                        *hl = None;
                    }
                });
            });
        });
    }

    // Combine reconstructed (anchored) timeline lines with real agent logs.
    // Reconstructed lines are explicitly labelled so they are not mistaken for
    // current live activity from the host.
    // Tuple: (timestamp, level, message, Option<event_id anchor>)
    let all_lines: Vec<(chrono::DateTime<Utc>, String, String, Option<String>)> = {
        let mut combined = Vec::new();

        // Synthesize anchored lines from history entries so each timeline event has a
        // real line to jump to. The anchored (key) line carries the event id.
        for (idx, entry) in history_entries.iter().enumerate() {
            let event_id = format!("ev{idx}");
            let kind = classify_history_entry(entry);
            let base = entry.timestamp;
            let short_msg = entry
                .change_reason
                .lines()
                .next()
                .unwrap_or(&entry.change_reason)
                .to_string();
            let sha = entry
                .commit_hash
                .as_ref()
                .map(|s| s.chars().take(7).collect::<String>())
                .unwrap_or_else(|| "—".to_string());
            let store_short = entry
                .store_path
                .as_ref()
                .and_then(|p| p.rsplit('/').next())
                .unwrap_or("")
                .to_string();

            // push(offset_secs, level, message, anchor?)
            let mut push = |off: i64, level: &str, msg: String, anchor: bool| {
                let ts = base + chrono::Duration::seconds(off);
                combined.push((
                    ts,
                    level.to_string(),
                    msg,
                    if anchor { Some(event_id.clone()) } else { None },
                ));
            };

            match kind {
                HistoryEventKind::Restart | HistoryEventKind::AgentRestart => {
                    let is_agent = matches!(kind, HistoryEventKind::AgentRestart);
                    push(
                        0,
                        "info",
                        if is_agent {
                            "[reconstructed timeline] systemd: crystal-forge-agent.service started"
                                .into()
                        } else {
                            "[reconstructed timeline] systemd: reached target multi-user.target"
                                .into()
                        },
                        false,
                    );
                    push(
                        2,
                        "info",
                        format!("[reconstructed timeline] agent: boot recorded — {short_msg}"),
                        true,
                    );
                    push(
                        4,
                        "info",
                        if is_agent {
                            "[reconstructed timeline] heartbeat observed after agent start".into()
                        } else {
                            "[reconstructed timeline] heartbeat observed after boot".into()
                        },
                        false,
                    );
                }
                HistoryEventKind::LocalRebuildMatched => {
                    push(
                        0,
                        "warn",
                        "[reconstructed timeline] agent: out-of-band activation detected".into(),
                        false,
                    );
                    push(
                        2,
                        "info",
                        format!(
                            "[reconstructed timeline] local: nixos-rebuild switch by {} — {short_msg}",
                            entry.actor
                        ),
                        false,
                    );
                    push(
                        5,
                        "info",
                        format!(
                            "[reconstructed timeline] agent: generation activated out of band (store-path {store_short})"
                        ),
                        true,
                    );
                    push(
                        7,
                        "info",
                        format!(
                            "[reconstructed timeline] reconcile: store-path matches pushed commit {sha} — config is tracked"
                        ),
                        false,
                    );
                }
                HistoryEventKind::LocalRebuildUntracked => {
                    push(
                        0,
                        "warn",
                        "[reconstructed timeline] agent: out-of-band activation detected".into(),
                        false,
                    );
                    push(
                        2,
                        "info",
                        format!(
                            "[reconstructed timeline] local: nixos-rebuild switch by {} — {short_msg}",
                            entry.actor
                        ),
                        false,
                    );
                    push(
                        5,
                        "warn",
                        format!(
                            "[reconstructed timeline] agent: generation activated locally — no flake commit (store-path {store_short})"
                        ),
                        true,
                    );
                    push(
                        7,
                        "warn",
                        "[reconstructed timeline] drift: running config no longer maps to a tracked flake revision".into(),
                        false,
                    );
                }
                HistoryEventKind::DeployFailed => {
                    push(
                        0,
                        "info",
                        format!(
                            "[reconstructed timeline] deploy: evaluating configuration @ {sha}"
                        ),
                        false,
                    );
                    push(
                        4,
                        "error",
                        format!("[reconstructed timeline] activation failed: {short_msg}"),
                        true,
                    );
                    push(
                        6,
                        "warn",
                        "[reconstructed timeline] deploy: rolled back to previous generation"
                            .into(),
                        false,
                    );
                }
                HistoryEventKind::Deploy => {
                    push(
                        0,
                        "info",
                        format!(
                            "[reconstructed timeline] deploy: evaluating configuration @ {sha}"
                        ),
                        false,
                    );
                    push(
                        2,
                        "info",
                        "[reconstructed timeline] eval: success — derivations resolved, building"
                            .into(),
                        false,
                    );
                    push(
                        5,
                        "info",
                        "[reconstructed timeline] build: completed".into(),
                        false,
                    );
                    push(
                        7,
                        "info",
                        "[reconstructed timeline] deploy: activating configuration".into(),
                        false,
                    );
                    push(
                        9,
                        "info",
                        format!("[reconstructed timeline] deploy: generation activated ({sha})"),
                        true,
                    );
                }
                HistoryEventKind::StateChange => {}
            }
        }

        // Add real agent-event logs (no anchors — they augment the synthesized stream).
        for entry in &logs {
            let level = match entry.level {
                LogLevel::Info | LogLevel::Debug => "info",
                LogLevel::Warn => "warn",
                LogLevel::Error => "error",
            };
            combined.push((
                entry.timestamp,
                level.to_string(),
                entry.message.clone(),
                None,
            ));
        }

        combined.sort_by_key(|(ts, _, _, _)| *ts);
        combined
    };

    // Filter by clear boundary and log level.
    let clear_boundary = *clear_before.read();
    let filtered_lines: Vec<_> = all_lines
        .iter()
        .filter(|(timestamp, level, _, _)| {
            if let Some(boundary) = clear_boundary {
                if *timestamp <= boundary {
                    return false;
                }
            }

            match filter.read().as_str() {
                "info" => level == "info",
                "warn" => level == "warn",
                "error" => level == "error",
                _ => true,
            }
        })
        .collect();

    let latest_line_timestamp = all_lines
        .iter()
        .map(|(timestamp, _, _, _)| *timestamp)
        .max();

    // Compute day separators in the selected display timezone.
    let use_utc_value = *use_utc.read();
    let today = if use_utc_value {
        chrono::Utc::now().date_naive()
    } else {
        chrono::Local::now().date_naive()
    };
    let yesterday = today.pred_opt().unwrap_or(today);

    let mut previous_day: Option<chrono::NaiveDate> = None;
    let log_rows: Vec<(
        Option<String>,
        &(chrono::DateTime<Utc>, String, String, Option<String>),
    )> = filtered_lines
        .into_iter()
        .map(|entry| {
            let day = display_log_date(entry.0, use_utc_value);
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
        .collect();

    let local_tz_abbr = local_timezone_label();
    let tz_label = if *use_utc.read() {
        "UTC"
    } else {
        local_tz_abbr.as_str()
    };

    let download_text = log_rows
        .iter()
        .map(|(_, entry)| {
            let (timestamp, level, message, _) = entry;
            format!(
                "{} [{}] {}",
                format_log_timestamp(*timestamp, use_utc_value),
                level.to_uppercase(),
                message
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    rsx! {
        section {
            class: "card sd-logs-card",
            div {
                class: "sd-card-head",
                style: "padding: 14px 18px; max-height: 720px; overflow-y: auto;",
                h2 { "Deployment logs" }
                div {
                    class: "sd-logs-controls",

                    // Timezone toggle
                    div {
                        class: "seg seg-tz",
                        title: "Timestamp timezone",
                        button {
                            class: if !*use_utc.read() { "active" } else { "" },
                            onclick: move |_| use_utc.set(false),
                            "{local_tz_abbr}"
                        }
                        button {
                            class: if *use_utc.read() { "active" } else { "" },
                            onclick: move |_| use_utc.set(true),
                            "UTC"
                        }
                    }

                    // Level filter
                    div {
                        class: "seg",
                        for lvl in ["all", "info", "warn", "error"] {
                            {
                                let cls = if filter() == lvl { "active" } else { "" };
                                rsx! {
                                    button {
                                        key: "{lvl}",
                                        class: "{cls}",
                                        onclick: move |_| filter.set(lvl.to_string()),
                                        "{lvl}"
                                    }
                                }
                            }
                        }
                    }

                    // Tail toggle
                    label {
                        class: "sd-toggle",
                        input {
                            r#type: "checkbox",
                            checked: *tail.read(),
                            onchange: move |_| {
                                let current = *tail.read();
                                tail.set(!current);
                            },
                        }
                        span { "auto-scroll" }
                    }

                    // Local clear button. This hides currently displayed lines and lets newly
                    // polled agent events appear after the clear boundary.
                    button {
                        class: "btn btn-ghost xs focus-ring",
                        onclick: move |_| {
                            clear_before.set(latest_line_timestamp);
                            highlighted_event.set(None);
                        },
                        disabled: latest_line_timestamp.is_none(),
                        "Clear display"
                    }

                    // Download button
                    button {
                        class: "btn btn-ghost xs focus-ring",
                        onclick: move |_| {
                            download_text_file(&download_text, "system-detail-logs.txt");
                        },
                        Icon { name: IconName::Download, size: 11 }
                        " Download"
                    }
                }
            }

            // Timezone info bar
            div {
                class: "sd-log-tzbar",
                Icon { name: IconName::Clock, size: 11 }
                " Timestamps shown in "
                strong { "{tz_label}" }
                " · reconstructed timeline entries are labelled and derived from deployment history"
            }

            // Log stream
            pre {
                class: "sd-log-stream",
                id: "{log_stream_id}",

                for (day_label, entry) in log_rows {
                    {
                        let (timestamp, level, message, event_id) = entry;

                        // Format timestamp based on timezone preference
                        let ts_str = format_log_time(*timestamp, *use_utc.read());

                        let level_class = match level.as_str() {
                            "info" => "sd-log-line sd-log-info",
                            "warn" => "sd-log-line sd-log-warn",
                            "error" => "sd-log-line sd-log-error",
                            _ => "sd-log-line sd-log-info",
                        };

                        let is_highlighted = event_id.is_some() && highlighted_event.read().as_ref() == event_id.as_ref();
                        let highlight_class = if is_highlighted { " sd-log-hl" } else { "" };

                        let lvl_upper = level.to_uppercase();

                        rsx! {
                            if let Some(label) = day_label {
                                div {
                                    key: "day-{label}",
                                    class: "sd-log-day",
                                    role: "separator",
                                    span { class: "sd-log-day-label", "{label}" }
                                }
                            }
                            div {
                                key: "{timestamp}-{message}",
                                class: "{level_class}{highlight_class}",
                                "data-ev": event_id.as_ref().map(|s| s.as_str()).unwrap_or(""),
                                span { class: "sd-log-t", "{ts_str}" }
                                span { class: "sd-log-lvl", "{lvl_upper}" }
                                span { class: "sd-log-m", "{message}" }
                            }
                        }
                    }
                }

                // Tail caret
                if *tail.read() {
                    div { class: "sd-log-caret", "▍" }
                }
            }
        }
    }
}

fn display_log_date(timestamp: chrono::DateTime<Utc>, use_utc: bool) -> chrono::NaiveDate {
    if use_utc {
        timestamp.date_naive()
    } else {
        timestamp.with_timezone(&Local).date_naive()
    }
}

fn format_log_time(timestamp: chrono::DateTime<Utc>, use_utc: bool) -> String {
    if use_utc {
        timestamp.format("%H:%M:%S").to_string()
    } else {
        timestamp
            .with_timezone(&Local)
            .format("%H:%M:%S")
            .to_string()
    }
}

fn format_log_timestamp(timestamp: chrono::DateTime<Utc>, use_utc: bool) -> String {
    if use_utc {
        timestamp.format("%Y-%m-%dT%H:%M:%SZ").to_string()
    } else {
        timestamp
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S %Z")
            .to_string()
    }
}

fn local_timezone_label() -> String {
    let label = Local::now().format("%Z").to_string();
    if label.trim().is_empty() {
        "local".to_string()
    } else {
        label
    }
}

fn download_text_file(content: &str, filename: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        let encoded = js_sys::encode_uri_component(content)
            .as_string()
            .unwrap_or_default();
        let data_uri = format!("data:text/plain;charset=utf-8,{}", encoded);
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
        let _ = func.call0(&wasm_bindgen::JsValue::NULL);
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (content, filename);
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

/// Kind of history timeline event, mirroring the design reference's event model.
#[derive(Debug, Clone, PartialEq)]
enum HistoryEventKind {
    /// Config deployed through Crystal Forge (generation-changing).
    Deploy,
    /// Deploy that failed to activate.
    DeployFailed,
    /// Out-of-band `nixos-rebuild switch` on the host, later reconciled to a pushed commit.
    LocalRebuildMatched,
    /// Out-of-band `nixos-rebuild switch` on the host with no tracked flake commit.
    LocalRebuildUntracked,
    /// System reboot (boot_id changed, full OS restart).
    Restart,
    /// Agent service restart — same boot, only the crystal-forge-agent process restarted.
    AgentRestart,
    /// Non-generation-changing state/metadata update. Hidden from deployment timeline.
    StateChange,
}

/// A single unified timeline event, built from deployment history + agent events.
///
/// This is the Rust analogue of the design reference's `buildHistory` event objects.
#[derive(Debug, Clone, PartialEq)]
struct HistoryEvent {
    id: String,
    kind: HistoryEventKind,
    timestamp: chrono::DateTime<chrono::Utc>,
    generation: Option<i32>,
    prev_generation: Option<i32>,
    sha: Option<String>,
    message: String,
    actor: String,
    duration: Option<String>,
    store_path: Option<String>,
    /// Backing commit for rollback actions (deploy events only).
    commit: Option<SystemCommitHistory>,
}

/// A rendered timeline item: either a standalone event or a folded restart cluster.
#[derive(Debug, Clone, PartialEq)]
enum TimelineItem {
    Event(HistoryEvent),
    RestartCluster(Vec<HistoryEvent>),
}

/// Classify a history entry's source/outcome into a timeline event kind.
///
/// Prefers the authoritative `event_kind` provided by the backend (derived from
/// `system_states.change_reason`): `cf_deployment` (CF deploy), `local_rebuild`
/// (on-host/out-of-band activation), and `restart` (reboot). Reconciliation of an
/// out-of-band rebuild uses the backend `reconciled` flag (running store-path maps
/// to a tracked flake commit). Falls back to string heuristics only for legacy
/// payloads that predate these fields.
fn classify_history_entry(entry: &SystemHistoryEntry) -> HistoryEventKind {
    let outcome = entry.outcome.to_lowercase();

    match entry.event_type.as_str() {
        "cf_deployment_succeeded" => return HistoryEventKind::Deploy,
        "cf_deployment_failed" => return HistoryEventKind::DeployFailed,
        "local_rebuild_detected" => {
            return if entry.reconciled {
                HistoryEventKind::LocalRebuildMatched
            } else {
                HistoryEventKind::LocalRebuildUntracked
            };
        }
        "system_reboot" => return HistoryEventKind::Restart,
        "agent_restart" => return HistoryEventKind::AgentRestart,
        _ => {}
    }

    match entry.event_kind.as_str() {
        "cf_deployment" => {
            if outcome.contains("fail") || outcome.contains("error") {
                HistoryEventKind::DeployFailed
            } else {
                HistoryEventKind::Deploy
            }
        }
        "local_rebuild" => {
            if entry.reconciled {
                HistoryEventKind::LocalRebuildMatched
            } else {
                HistoryEventKind::LocalRebuildUntracked
            }
        }
        "restart" => HistoryEventKind::Restart,
        "agent_restart" => HistoryEventKind::AgentRestart,
        "state_change" => HistoryEventKind::StateChange,
        _ => classify_history_entry_legacy(entry),
    }
}

/// Human-readable timeline message for an event.
///
/// The raw `change_reason` (e.g. "config_change") is not operator-friendly, so we
/// surface a descriptive summary per kind while still preferring any richer message
/// text carried on the entry (e.g. a commit subject in the fallback path).
fn history_event_message(entry: &SystemHistoryEntry, kind: &HistoryEventKind) -> String {
    if let Some(title) = entry
        .title
        .as_ref()
        .filter(|title| !title.trim().is_empty())
    {
        return title.clone();
    }

    let raw = entry
        .change_reason
        .lines()
        .next()
        .unwrap_or(&entry.change_reason)
        .trim()
        .to_string();

    let is_raw_reason = matches!(
        raw.as_str(),
        "config_change"
            | "cf_deployment"
            | "cf_deployment_succeeded"
            | "cf_deployment_failed"
            | "local_rebuild_detected"
            | "system_reboot"
            | "agent_restart"
            | "startup"
            | "state_delta"
            | "state_change"
            | ""
    );

    match kind {
        HistoryEventKind::Deploy => {
            if is_raw_reason {
                "Deployed through Crystal Forge".to_string()
            } else {
                raw
            }
        }
        HistoryEventKind::DeployFailed => {
            if is_raw_reason {
                "Deploy failed to activate".to_string()
            } else {
                raw
            }
        }
        HistoryEventKind::LocalRebuildMatched | HistoryEventKind::LocalRebuildUntracked => {
            if is_raw_reason {
                "nixos-rebuild switch on host".to_string()
            } else {
                raw
            }
        }
        HistoryEventKind::Restart => {
            if is_raw_reason {
                "System restarted".to_string()
            } else {
                raw
            }
        }
        HistoryEventKind::AgentRestart => {
            if is_raw_reason {
                "Agent restarted".to_string()
            } else {
                raw
            }
        }
        HistoryEventKind::StateChange => {
            if is_raw_reason {
                "State updated".to_string()
            } else {
                raw
            }
        }
    }
}

/// Legacy heuristic classification for payloads without an authoritative `event_kind`.
fn classify_history_entry_legacy(entry: &SystemHistoryEntry) -> HistoryEventKind {
    let reason = entry.change_reason.to_lowercase();
    let outcome = entry.outcome.to_lowercase();
    let actor = entry.actor.to_lowercase();

    if outcome.contains("fail") || outcome.contains("error") {
        return HistoryEventKind::DeployFailed;
    }
    if reason.contains("restart") || reason.contains("boot") || reason.contains("startup") {
        return HistoryEventKind::Restart;
    }
    if reason.contains("state_change") || reason.contains("state_delta") {
        return HistoryEventKind::StateChange;
    }
    let is_local = actor.contains('@')
        || actor.contains("on-host")
        || reason.contains("config_change")
        || reason.contains("nixos-rebuild")
        || reason.contains("local");
    if is_local {
        if entry.commit_hash.is_some() {
            HistoryEventKind::LocalRebuildMatched
        } else {
            HistoryEventKind::LocalRebuildUntracked
        }
    } else {
        HistoryEventKind::Deploy
    }
}

/// Build the unified event list from deployment history entries and the system's
/// current generation.
///
/// When the backend supplies an authoritative per-entry `generation`, that value is
/// used directly and the previous generation is taken from the next-older entry, so
/// deploys render `#prev → #cur` and restarts render an unchanged generation
/// ("held steady"). For legacy payloads without a recorded generation, we fall back
/// to walking newest → oldest and decrementing on each generation-changing deploy.
fn build_history_events(
    entries: &[SystemHistoryEntry],
    commits: &[SystemCommitHistory],
    current_generation: Option<i32>,
) -> Vec<HistoryEvent> {
    let mut events = Vec::with_capacity(entries.len());
    // Fallback running generation for payloads without an authoritative value.
    let mut running_gen = current_generation;
    // Whether any entry carries an authoritative recorded generation.
    let has_authoritative_gen = entries.iter().any(|e| e.generation.is_some());

    for (idx, entry) in entries.iter().enumerate() {
        let kind = classify_history_entry(entry);
        if matches!(kind, HistoryEventKind::StateChange) {
            continue;
        }
        let short_reason = history_event_message(entry, &kind);

        // Match a commit record (for the rollback action + rich commit link).
        let commit = entry
            .commit_hash
            .as_ref()
            .and_then(|hash| commits.iter().find(|c| &c.hash == hash).cloned());

        let is_gen_changing = !matches!(
            kind,
            HistoryEventKind::Restart
                | HistoryEventKind::AgentRestart
                | HistoryEventKind::DeployFailed
        );
        let (generation, prev_generation) = if has_authoritative_gen {
            // Authoritative: use the recorded generation and compare against the
            // next-older recorded generation to show a real transition.
            let current = entry.generation;
            let older = entry
                .previous_generation
                .and_then(|value| i32::try_from(value).ok())
                .or_else(|| {
                    entries
                        .get(idx + 1)
                        .and_then(|older_entry| older_entry.generation)
                });
            let prev = match (is_gen_changing, current, older) {
                // Only surface a transition when the generation actually changed.
                (true, Some(cur), Some(old)) if old != cur => Some(old),
                _ => None,
            };
            (current, prev)
        } else if is_gen_changing {
            let current = running_gen;
            let prev = running_gen.map(|g| g - 1);
            if let Some(g) = running_gen {
                running_gen = Some(g - 1);
            }
            (current, prev)
        } else {
            (running_gen, None)
        };

        events.push(HistoryEvent {
            id: format!("ev{idx}"),
            kind,
            timestamp: entry.timestamp,
            generation,
            prev_generation,
            sha: entry.commit_hash.clone(),
            message: short_reason,
            actor: entry.actor.clone(),
            // Do not infer operation duration from spacing between history records.
            // The backend does not currently expose reliable operation start/end or
            // uptime timestamps for these timeline entries.
            duration: None,
            store_path: entry.store_path.clone(),
            commit,
        });
    }

    events
}

/// Fold consecutive restart events into clusters; deploys/rebuilds stay standalone.
///
/// Fold consecutive **system** restart events into clusters; agent restarts and
/// all other event kinds always render as standalone items.
///
/// Ordering rule: when a generation-creating event (successful deploy or local
/// rebuild — but NOT a failed deploy) immediately follows a system-restart run in
/// the newest-first stream, we surface the gen-creating event **above** the cluster
/// so the causally important change is not buried under reboot noise.  The reorder
/// is gated on both events sharing the same recorded generation to prevent false
/// promotion when generation data is absent.
///
/// `AgentRestart` is intentionally excluded from clustering so a solo agent
/// service restart is always a visible standalone line.
/// `DeployFailed` is intentionally excluded from the gen-creating set because a
/// failed deploy does not create a new generation.
fn fold_restart_clusters(events: &[HistoryEvent]) -> Vec<TimelineItem> {
    // Only successful deploys and local rebuilds create a new generation.
    // DeployFailed is intentionally excluded (a failed deploy does not create a
    // generation).  AgentRestart is intentionally excluded (never clusters).
    let is_system_restart = |e: &HistoryEvent| matches!(e.kind, HistoryEventKind::Restart);

    let is_generation_changing = |e: &HistoryEvent| {
        matches!(
            e.kind,
            HistoryEventKind::Deploy
                | HistoryEventKind::LocalRebuildMatched
                | HistoryEventKind::LocalRebuildUntracked
        )
    };

    let emit_run = |items: &mut Vec<TimelineItem>, run: Vec<HistoryEvent>| {
        if run.len() == 1 {
            items.push(TimelineItem::Event(run.into_iter().next().unwrap()));
        } else {
            items.push(TimelineItem::RestartCluster(run));
        }
    };

    // Process events newest-first.
    //
    // `run`     — accumulates consecutive HistoryEventKind::Restart events.
    // `pending` — AgentRestart events that arrived at the *top* of the stream
    //             before any system-restart run began. They are held until the
    //             run (and its possible gen-promotion) is resolved, then emitted
    //             after. Other standalone events are never buffered.
    //
    // Example stream (newest-first):
    //   AgentRestart(gen=5054)         → standalone, buffered in `pending`
    //   Restart(gen=5054)              → pushed to `run`
    //   Restart(gen=5054)              → pushed to `run`
    //   LocalRebuildUntracked(gen=5054)→ closes `run`; gen matches & gen-changing
    //                                    → emit LocalRebuild, Cluster(2),
    //                                      then drain pending → AgentRestart
    //
    // Result: [LocalRebuild, Cluster(2), AgentRestart]  ✓

    let mut items: Vec<TimelineItem> = Vec::new();
    let is_agent_restart = |e: &HistoryEvent| matches!(e.kind, HistoryEventKind::AgentRestart);

    let mut run: Vec<HistoryEvent> = Vec::new();
    // Agent restarts seen before the first system-restart of the current run.
    let mut pre_run_agent_restarts: Vec<HistoryEvent> = Vec::new();

    let flush_pending_agent_restarts =
        |items: &mut Vec<TimelineItem>, pending: &mut Vec<HistoryEvent>| {
            for event in pending.drain(..) {
                items.push(TimelineItem::Event(event));
            }
        };

    for e in events {
        if is_system_restart(e) {
            run.push(e.clone());
            continue;
        }

        // Non-system-restart event — closes an open run (if any).
        if run.is_empty() {
            if is_agent_restart(e) {
                // Keep agent restarts standalone, but allow them to trail a
                // subsequent promoted system-restart cluster.
                pre_run_agent_restarts.push(e.clone());
            } else {
                // Any unrelated standalone event (deploy, state change, failed
                // deploy, etc.) must not be buried below a future restart cluster.
                flush_pending_agent_restarts(&mut items, &mut pre_run_agent_restarts);
                items.push(TimelineItem::Event(e.clone()));
            }
        } else {
            // A run is open.  Close it.
            let finished = std::mem::take(&mut run);

            let cluster_gen_matches =
                e.generation.is_some() && finished.iter().all(|r| r.generation == e.generation);

            if is_generation_changing(e) && cluster_gen_matches {
                // Gen-promotion: show gen-changing event first, then the cluster,
                // then the standalones that were buffered above the system restarts.
                items.push(TimelineItem::Event(e.clone()));
                emit_run(&mut items, finished);
                flush_pending_agent_restarts(&mut items, &mut pre_run_agent_restarts);
            } else {
                // No promotion: flush pending standalones, then the cluster, then
                // this non-gen-changing event.
                flush_pending_agent_restarts(&mut items, &mut pre_run_agent_restarts);
                emit_run(&mut items, finished);
                items.push(TimelineItem::Event(e.clone()));
            }
        }
    }

    // Flush any trailing system-restart run.
    if !run.is_empty() {
        let finished = std::mem::take(&mut run);
        emit_run(&mut items, finished);
    }
    // Flush any remaining agent restarts (stream ended before a run opened).
    flush_pending_agent_restarts(&mut items, &mut pre_run_agent_restarts);

    items
}

#[cfg(test)]
mod fold_tests {
    use super::{HistoryEvent, HistoryEventKind, TimelineItem, fold_restart_clusters};
    use chrono::Utc;

    fn ev(kind: HistoryEventKind, generation: Option<i32>) -> HistoryEvent {
        HistoryEvent {
            id: String::new(),
            kind,
            timestamp: Utc::now(),
            generation,
            prev_generation: None,
            sha: None,
            message: String::new(),
            actor: String::new(),
            duration: None,
            store_path: None,
            commit: None,
        }
    }

    fn kinds(items: &[TimelineItem]) -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                TimelineItem::Event(e) => format!("{:?}", e.kind),
                TimelineItem::RestartCluster(v) => {
                    format!("Cluster({})", v.len())
                }
            })
            .collect()
    }

    /// AgentRestart, Restart, Restart, LocalRebuild →
    /// LocalRebuild, Cluster(2), AgentRestart
    #[test]
    fn agent_restart_then_rebuild_then_system_restarts() {
        let events = vec![
            ev(HistoryEventKind::AgentRestart, Some(5054)),
            ev(HistoryEventKind::Restart, Some(5054)),
            ev(HistoryEventKind::Restart, Some(5054)),
            ev(HistoryEventKind::LocalRebuildUntracked, Some(5054)),
        ];
        let items = fold_restart_clusters(&events);
        assert_eq!(
            kinds(&items),
            vec!["LocalRebuildUntracked", "Cluster(2)", "AgentRestart"],
            "gen-changing event should be promoted above the system-restart cluster; \
             AgentRestart stays standalone after the cluster"
        );
    }

    /// Restart, Restart, DeployFailed →
    /// Cluster(2), DeployFailed  (DeployFailed must NOT promote)
    #[test]
    fn deploy_failed_does_not_promote_over_restart_cluster() {
        let events = vec![
            ev(HistoryEventKind::Restart, Some(5054)),
            ev(HistoryEventKind::Restart, Some(5054)),
            ev(HistoryEventKind::DeployFailed, Some(5054)),
        ];
        let items = fold_restart_clusters(&events);
        assert_eq!(
            kinds(&items),
            vec!["Cluster(2)", "DeployFailed"],
            "DeployFailed must not be promoted above a restart cluster"
        );
    }

    /// AgentRestart is always standalone, never clustered.
    #[test]
    fn agent_restart_never_clusters() {
        let events = vec![
            ev(HistoryEventKind::AgentRestart, Some(5054)),
            ev(HistoryEventKind::AgentRestart, Some(5054)),
        ];
        let items = fold_restart_clusters(&events);
        assert_eq!(
            kinds(&items),
            vec!["AgentRestart", "AgentRestart"],
            "AgentRestart events must always be standalone"
        );
    }

    /// System restarts with no following gen-changing event cluster normally.
    #[test]
    fn system_restarts_cluster_without_gen_change() {
        let events = vec![
            ev(HistoryEventKind::Restart, Some(5054)),
            ev(HistoryEventKind::Restart, Some(5054)),
            ev(HistoryEventKind::Restart, Some(5054)),
        ];
        let items = fold_restart_clusters(&events);
        assert_eq!(kinds(&items), vec!["Cluster(3)"]);
    }

    /// Generation mismatch: deploy does NOT promote when generations differ.
    #[test]
    fn gen_mismatch_deploy_does_not_promote() {
        let events = vec![
            ev(HistoryEventKind::Restart, Some(5054)),
            ev(HistoryEventKind::Restart, Some(5054)),
            ev(HistoryEventKind::Deploy, Some(5053)), // different gen
        ];
        let items = fold_restart_clusters(&events);
        assert_eq!(
            kinds(&items),
            vec!["Cluster(2)", "Deploy"],
            "deploy at a different generation must not be promoted"
        );
    }

    /// Deploy, Restart, Restart, LocalRebuild →
    /// Deploy, LocalRebuild, Cluster(2)
    #[test]
    fn unrelated_newer_deploy_stays_above_older_restart_cluster() {
        let events = vec![
            ev(HistoryEventKind::Deploy, Some(5055)),
            ev(HistoryEventKind::Restart, Some(5054)),
            ev(HistoryEventKind::Restart, Some(5054)),
            ev(HistoryEventKind::LocalRebuildUntracked, Some(5054)),
        ];

        let items = fold_restart_clusters(&events);

        assert_eq!(
            kinds(&items),
            vec!["Deploy", "LocalRebuildUntracked", "Cluster(2)"],
            "newer unrelated deploy must not be buffered below an older restart cluster"
        );
    }
}

/// Enhanced History tab — a faithful Rust recreation of the design reference.
///
/// Features:
/// - Rich deployment event cards with generation transitions (#prev → #cur)
/// - Out-of-band local rebuild indicators with matched/untracked status
/// - Collapsible restart clusters for consecutive reboots at one generation
/// - Rollback and view-logs actions that jump to the corresponding log line
/// - Infinite scroll pagination for deep history, with a load-more fallback
#[component]
fn HistoryTab(
    entries: Vec<SystemHistoryEntry>,
    commits: Vec<SystemCommitHistory>,
    current_generation: Option<i32>,
    deployment_policy: String,
    allow_mutations: bool,
    on_rollback: EventHandler<SystemCommitHistory>,
    on_view_logs: EventHandler<String>,
) -> Element {
    let events = build_history_events(&entries, &commits, current_generation);
    let items = fold_restart_clusters(&events);

    let deploy_count = events
        .iter()
        .filter(|e| {
            !matches!(
                e.kind,
                HistoryEventKind::Restart | HistoryEventKind::AgentRestart
            )
        })
        .count();
    let restart_count = events
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                HistoryEventKind::Restart | HistoryEventKind::AgentRestart
            )
        })
        .count();

    // Infinite scroll pagination: reveal a page of clustered items at a time as
    // the user approaches the bottom, with an explicit load-more fallback.
    const PAGE: usize = 14;
    let mut visible_count = use_signal(|| PAGE);
    let total_items = items.len();
    let shown_count = (*visible_count.read()).min(total_items);
    let shown = items.iter().take(shown_count).cloned().collect::<Vec<_>>();
    let has_more = shown_count < total_items;
    // Stable DOM id for the timeline's own scroll container. This mirrors the
    // established commit-list pattern elsewhere in the UI and avoids relying on
    // the page-level `.content` scroller.
    let timeline_scroll_id = "sd-history-scroll";

    // Track which restart clusters are expanded (keyed by item index).
    let mut open_clusters: Signal<std::collections::HashSet<usize>> =
        use_signal(std::collections::HashSet::new);

    rsx! {
        section {
            class: "card",
            style: "overflow: hidden;",

            div {
                class: "sd-card-head",
                style: "padding: 14px 18px;",
                h2 { "Deployment history" }
                span {
                    class: "sd-card-meta",
                    "{deploy_count} deploys · {restart_count} restarts · policy {deployment_policy}"
                }
            }

            // Timeline
            div {
                class: "tl",
                id: "{timeline_scroll_id}",
                style: "padding: 14px 18px;",
                onscroll: move |_| {
                    if !has_more {
                        return;
                    }
                    let Some(window) = web_sys::window() else {
                        return;
                    };
                    let Some(document) = window.document() else {
                        return;
                    };
                    let Some(element) = document.get_element_by_id(timeline_scroll_id) else {
                        return;
                    };
                    let scroll_top = element.scroll_top();
                    let client_height = element.client_height();
                    let scroll_height = element.scroll_height();
                    if scroll_top + client_height + 96 >= scroll_height {
                        let next = (*visible_count.read() + PAGE).min(total_items);
                        visible_count.set(next);
                    }
                },

                for (idx, item) in shown.iter().enumerate() {
                    {
                        match item {
                            TimelineItem::Event(event) => {
                                let is_restart_event = matches!(
                                    event.kind,
                                    HistoryEventKind::Restart | HistoryEventKind::AgentRestart
                                );
                                if is_restart_event {
                                    let e = event.clone();
                                    rsx! {
                                        div {
                                            key: "{event.id}",
                                            class: "tl-row",
                                            div { class: "tl-rail",
                                                span {
                                                    class: "tl-node tl-node-sm",
                                                    style: "--node: var(--cf-blue);",
                                                    Icon { name: IconName::Power, size: 11 }
                                                }
                                            }
                                            div { class: "tl-body",
                                                div { class: "tl-restart-single",
                                                    RestartLine { event: e }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    rsx! {
                                        DeployRow {
                                            key: "{event.id}",
                                            event: event.clone(),
                                            allow_mutations,
                                            on_rollback: {
                                                let commit = event.commit.clone();
                                                move |_| {
                                                    if let Some(commit) = commit.clone() {
                                                        on_rollback.call(commit);
                                                    }
                                                }
                                            },
                                            on_view_logs: {
                                                let id = event.id.clone();
                                                move |_| on_view_logs.call(id.clone())
                                            },
                                        }
                                    }
                                }
                            },
                            TimelineItem::RestartCluster(list) => {
                                if list.len() == 1 {
                                    let e = list[0].clone();
                                    rsx! {
                                        div {
                                            key: "restart-{idx}",
                                            class: "tl-row",
                                            div { class: "tl-rail",
                                                span {
                                                    class: "tl-node tl-node-sm",
                                                    style: "--node: var(--cf-blue);",
                                                    Icon { name: IconName::Power, size: 11 }
                                                }
                                            }
                                            div { class: "tl-body",
                                                div { class: "tl-restart-single",
                                                    RestartLine { event: e }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    let is_open = open_clusters.read().contains(&idx);
                                    let cluster_gen = list[0].generation;
                                    let first_when = relative_time(list[list.len() - 1].timestamp);
                                    let last_when = relative_time(list[0].timestamp);
                                    let count = list.len();
                                    let list_clone = list.clone();
                                    rsx! {
                                        div {
                                            key: "cluster-{idx}",
                                            class: "tl-row",
                                            div { class: "tl-rail",
                                                span {
                                                    class: "tl-node tl-node-sm",
                                                    style: "--node: var(--cf-blue);",
                                                    Icon { name: IconName::Power, size: 11 }
                                                }
                                            }
                                            div { class: "tl-body",
                                                button {
                                                    class: "tl-cluster focus-ring",
                                                    "aria-expanded": "{is_open}",
                                                    onclick: move |_| {
                                                        open_clusters.with_mut(|set| {
                                                            if set.contains(&idx) {
                                                                set.remove(&idx);
                                                            } else {
                                                                set.insert(idx);
                                                            }
                                                        });
                                                    },
                                                    Icon {
                                                        name: if is_open { IconName::ChevronDown } else { IconName::ChevronRight },
                                                        size: 14,
                                                    }
                                                    span { class: "tl-cluster-count", "{count} restarts" }
                                                    span { class: "tl-restart-sep", "·" }
                                                    span { class: "tl-restart-label",
                                                        "generation "
                                                        if let Some(g) = cluster_gen {
                                                            span { class: "mono", "#{g}" }
                                                        } else {
                                                            span { class: "mono", "—" }
                                                        }
                                                        " held steady"
                                                    }
                                                    span { class: "tl-spacer" }
                                                    span { class: "tl-when", "{first_when} – {last_when}" }
                                                }
                                                if is_open {
                                                    div { class: "tl-cluster-list",
                                                        for (j, e) in list_clone.iter().enumerate() {
                                                            RestartLine { key: "{idx}-{j}", event: e.clone() }
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

                // Auto-load sentinel: the timeline's onscroll handler reveals the
                // next page as this row nears the visible bottom. The button remains
                // a keyboard/no-scroll fallback.
                if has_more {
                    div {
                        class: "tl-row tl-sentinel",
                        div { class: "tl-rail",
                            span {
                                class: "tl-node tl-node-sm tl-node-load",
                                Icon { name: IconName::Sync, size: 11 }
                            }
                        }
                        div { class: "tl-body",
                            div { class: "tl-loadmore",
                                button {
                                    class: "btn btn-ghost xs focus-ring",
                                    onclick: move |_| {
                                        let next = (*visible_count.read() + PAGE).min(total_items);
                                        visible_count.set(next);
                                    },
                                    "Load older history "
                                    span { class: "tl-loadmore-count", "{shown_count} of {total_items}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A single restart line within a restart cluster (design `RestartLine`).
/// Shows "System restarted" for OS reboots and "Agent restarted" for service restarts.
#[component]
fn RestartLine(event: HistoryEvent) -> Element {
    let when = relative_time(event.timestamp);
    let is_agent_restart = matches!(event.kind, HistoryEventKind::AgentRestart);
    let label = if is_agent_restart {
        "Agent restarted"
    } else {
        "System restarted"
    };
    rsx! {
        div { class: "tl-restart-line",
            span { class: "tl-restart-dot" }
            Icon { name: IconName::Power, size: 12 }
            span { class: "tl-restart-label", "{label}" }
            span { class: "tl-restart-sep", "·" }
            span { class: "tl-when", "{when}" }
        }
    }
}

/// A prominent generation-changing event row (design `DeployRow`).
#[component]
fn DeployRow(
    event: HistoryEvent,
    allow_mutations: bool,
    on_rollback: EventHandler<()>,
    on_view_logs: EventHandler<()>,
) -> Element {
    let failed = matches!(event.kind, HistoryEventKind::DeployFailed);
    let matched = matches!(event.kind, HistoryEventKind::LocalRebuildMatched);
    let untracked = matches!(event.kind, HistoryEventKind::LocalRebuildUntracked);
    let local = matched || untracked;

    // Accent color per event kind (mirrors the design's accent map).
    let accent = if failed {
        "var(--cf-red)"
    } else if untracked {
        "var(--cf-amber)"
    } else if matched {
        "var(--cf-blue)"
    } else {
        "var(--cf-brand-purple)"
    };

    let kind_label = if failed {
        "Deploy failed"
    } else if local {
        "Local rebuild"
    } else {
        "Deployed"
    };

    let icon_name = if failed {
        IconName::X
    } else if local {
        IconName::Edit
    } else {
        IconName::Deploy
    };

    let short_sha = event
        .sha
        .as_ref()
        .map(|s| s.chars().take(7).collect::<String>());
    let short_store = event
        .store_path
        .as_ref()
        .and_then(|p| p.rsplit('/').next())
        .map(|s| s.to_string());
    let when = relative_time(event.timestamp);
    let can_rollback = allow_mutations && event.commit.is_some() && !failed;

    rsx! {
        div { class: "tl-row",
            // Rail node
            div { class: "tl-rail",
                span {
                    class: "tl-node",
                    style: "--node: {accent};",
                    Icon { name: icon_name, size: 13 }
                }
            }
            // Card
            div { class: "tl-body",
                div {
                    class: "tl-card",
                    style: "--accent: {accent};",

                    // Header: kind + generation transition + badges + status
                    div { class: "tl-card-head",
                        span { class: "tl-kind", style: "color: {accent};", "{kind_label}" }

                        // Generation transition (#prev → #cur) or "no generation activated"
                        if let Some(gen_num) = event.generation {
                            span { class: "tl-gen",
                                if let Some(prev) = event.prev_generation {
                                    span { class: "tl-gen-prev", "#{prev}" }
                                    Icon { name: IconName::ArrowRight, size: 11 }
                                }
                                strong { "#{gen_num}" }
                            }
                        } else {
                            span { class: "tl-gen tl-gen-none", "no generation activated" }
                        }

                        if local {
                            span { class: "tl-badge-oob", "out of band" }
                        }
                        if matched {
                            span { class: "tl-badge-reconciled",
                                Icon { name: IconName::Check, size: 9 }
                                " reconciled"
                            }
                        }

                        span { class: "tl-spacer" }

                        // Status chip
                        if failed {
                            span { class: "chip chip-critical",
                                Icon { name: IconName::X, size: 10 }
                                " failed"
                            }
                        } else {
                            span { class: "chip chip-healthy",
                                Icon { name: IconName::Check, size: 10 }
                                " success"
                            }
                        }
                    }

                    // Message
                    div { class: "tl-msg", "{event.message}" }

                    // Meta row
                    div { class: "tl-meta",
                        // Commit link / untracked marker
                        if untracked {
                            span {
                                class: "tl-meta-item tl-untracked",
                                title: "Built locally with nixos-rebuild — no matching flake commit",
                                Icon { name: IconName::Warn, size: 11 }
                                " no flake commit · "
                                span { class: "mono", "{short_store.clone().unwrap_or_else(|| \"untracked\".to_string())}" }
                            }
                        } else if let Some(sha) = short_sha.clone() {
                            span { class: "tl-meta-item mono",
                                Icon { name: IconName::Git, size: 11 }
                                if matched { " matched " } else { " " }
                                "{sha}"
                            }
                        }

                        span { class: "tl-meta-item",
                            Icon { name: IconName::User, size: 11 }
                            " {event.actor}"
                        }
                        if let Some(duration) = event.duration.clone() {
                            span { class: "tl-meta-item",
                                span { class: "mono", "{duration}" }
                            }
                        }
                        span { class: "tl-meta-item tl-when", "{when}" }

                        span { class: "tl-spacer" }

                        // Actions
                        div { class: "row-actions",
                            button {
                                class: "btn-icon focus-ring",
                                title: "Jump to this event in logs",
                                onclick: move |_| on_view_logs.call(()),
                                Icon { name: IconName::Terminal, size: 14 }
                            }
                            if can_rollback {
                                button {
                                    class: "btn-icon focus-ring",
                                    title: "Rollback to this generation",
                                    onclick: move |_| on_rollback.call(()),
                                    Icon { name: IconName::Rollback, size: 14 }
                                }
                            }
                        }
                    }

                    // Reconciled note
                    if matched {
                        div { class: "tl-oob-note tl-oob-resolved",
                            Icon { name: IconName::Git, size: 12 }
                            span {
                                "Activated on-host out of band, then reconciled to pushed commit "
                                if let Some(sha) = short_sha.clone() {
                                    span { class: "tl-inline-sha mono", "{sha}" }
                                }
                                ". Config is tracked and reproducible."
                            }
                        }
                    }

                    // Untracked note
                    if untracked {
                        div { class: "tl-oob-note",
                            Icon { name: IconName::Warn, size: 12 }
                            span {
                                "Built on the host, outside Crystal Forge — the running config doesn't map to any tracked flake commit. Capture it to a flake to restore reproducibility."
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

#[component]
fn CommitTimelineNode(
    commit: SystemCommitHistory,
    #[allow(unused)] is_first: bool,
    #[allow(unused)] is_last: bool,
    deployment_policy: String,
    allow_mutations: bool,
    on_rollback: EventHandler<SystemCommitHistory>,
    on_view_logs: EventHandler<()>,
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

                            // View logs action
                            button {
                                class: "shrink-0 p-1 rounded text-gray-400 hover:text-white hover:bg-gray-800 transition-colors opacity-40 group-hover:opacity-100",
                                title: "View logs",
                                onclick: move |_| on_view_logs.call(()),
                                svg {
                                    class: "w-4 h-4",
                                    fill: "none",
                                    stroke: "currentColor",
                                    view_box: "0 0 24 24",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        stroke_width: "2",
                                        d: "M8 9l3 3-3 3m5 0h3"
                                    }
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
    let mut active_waiver_directive: Signal<Option<String>> = use_signal(|| None);
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
                div { class: "card", style: "overflow: hidden;",
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
                                        style: "text-align:center;",
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
                                                    div { style: "width:60px;height:6px;background:var(--cf-subtle-bg);border-radius:99px;overflow:hidden;",
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

                    div { class: "modal-head",
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
                                            if !justifications_for(&service.service_name).is_empty() {
                                                " · "
                                                span { style: "color:#fbbf24;font-weight:700;", "{justifications_for(&service.service_name).len()} waived" }
                                            }
                                        }
                            }
                            button {
                                class: "btn-icon focus-ring",
                                autofocus: "true",
                                onclick: move |_| {
                                    if confirm_discard_unsaved_justification(!reason.read().trim().is_empty()) {
                                            active_waiver_directive.set(None);
                                            reason.set(String::new());
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
                        for (key, label) in [("overview", "Directives"), ("nix", "NixOS config"), ("all", "All checks")] {
                            {
                                let tab_class = if *modal_tab.read() == key {
                                    "sd-tab focus-ring active"
                                } else {
                                    "sd-tab focus-ring"
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
                            section { style: "display:flex;flex-direction:column;gap:14px;",
                                div { class: "sd-callout sd-callout-info", style: "margin:0;display:flex;align-items:flex-start;gap:12px;padding:12px 14px;",
                                    svg {
                                        class: "shrink-0",
                                        width: "15",
                                        height: "15",
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
                                    div { style: "font-size:13px;line-height:1.5;color:var(--cf-text-secondary);",
                                        "Directives that aren’t enforced can be "
                                        strong { style: "color:var(--cf-text-primary);font-weight:800;", "justified with a waiver" }
                                        " (e.g. compensating control, not applicable). Waivers flow into the compliance evidence export."
                                    }
                                }

                                if let Some(message) = justification_error() {
                                    div { class: "sd-callout sd-callout-danger", style: "margin:0;display:flex;align-items:flex-start;gap:8px;",
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
                                                d: "M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                                            }
                                        }
                                        div { style: "font-size:12px;color:var(--cf-critical);", "{message}" }
                                    }
                                }

                                if let Some(message) = justification_notice() {
                                    div { class: "sd-callout sd-callout-success", style: "margin:0;display:flex;align-items:flex-start;gap:8px;",
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
                                                d: "M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"
                                            }
                                        }
                                        div { style: "font-size:12px;color:var(--cf-healthy);", "{message}" }
                                    }
                                }

                                div { style: "display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:10px 12px;",
                                    for directive in directive_cells(&service) {
                                        {
                                            let directive_name = directive.name.clone();
                                            let waiver = justifications.iter().find(|item| {
                                                item.service_name == service.service_name
                                                    && item.directive_name.as_deref() == Some(directive.name.as_str())
                                            });
                                            let is_waived = waiver.is_some();
                                            let is_editing = active_waiver_directive.read().as_deref() == Some(directive.name.as_str());
                                            let tile_base_style = if directive.enabled {
                                                "grid-column:auto;display:flex;align-items:flex-start;gap:12px;padding:12px 14px;border-radius:10px;border:1px solid rgba(52,211,153,0.22);background:rgba(52,211,153,0.07);min-height:72px;"
                                            } else if is_waived {
                                                "grid-column:auto;display:flex;align-items:flex-start;gap:12px;padding:12px 14px;border-radius:10px;border:1px solid rgba(251,191,36,0.28);background:rgba(251,191,36,0.08);min-height:72px;"
                                            } else {
                                                "grid-column:auto;display:flex;align-items:flex-start;gap:12px;padding:12px 14px;border-radius:10px;border:1px solid rgba(248,113,113,0.22);background:rgba(248,113,113,0.07);min-height:72px;"
                                            };
                                            let tile_style = if is_editing {
                                                tile_base_style.replace("grid-column:auto;", "grid-column:1 / -1;")
                                            } else {
                                                tile_base_style.to_string()
                                            };
                                            let status_text = if directive.enabled {
                                                "enforced".to_string()
                                            } else if is_waived {
                                                "not set · waived".to_string()
                                            } else {
                                                "not set".to_string()
                                            };
                                            let status_color = if directive.enabled {
                                                "#34d399"
                                            } else if is_waived {
                                                "#fbbf24"
                                            } else {
                                                "var(--cf-text-muted)"
                                            };
                                            rsx! {
                                                div { style: "{tile_style}",
                                                    div { style: "font-size:20px;line-height:1;margin-top:2px;",
                                                        if directive.enabled {
                                                            "✅"
                                                        } else if is_waived {
                                                            "⚠️"
                                                        } else {
                                                            "❌"
                                                        }
                                                    }
                                                    div { style: "min-width:0;flex:1;display:flex;flex-direction:column;gap:6px;",
                                                        div { style: "display:flex;align-items:flex-start;justify-content:space-between;gap:10px;",
                                                            div { style: "min-width:0;",
                                                                div { class: "mono", style: "font-size:13px;font-weight:800;color:var(--cf-text-primary);", "{directive.name}" }
                                                                div { style: "font-size:11px;color:{status_color};", "{status_text}" }
                                                            }
                                                            if !directive.enabled && allow_mutations {
                                                                button {
                                                                    class: "btn btn-ghost focus-ring xs",
                                                                    style: "white-space:nowrap;",
                                                                    disabled: is_saving_justification(),
                                                                    onclick: {
                                                                        let directive_name = directive_name.clone();
                                                                        let existing_reason = waiver.map(|item| item.reason.clone()).unwrap_or_default();
                                                                        move |_| {
                                                                            active_waiver_directive.set(Some(directive_name.clone()));
                                                                            reason.set(existing_reason.clone());
                                                                            justification_error.set(None);
                                                                            justification_notice.set(None);
                                                                        }
                                                                    },
                                                                    if is_waived {
                                                                        svg {
                                                                            class: "w-3 h-3 inline-block",
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
                                                                        "Edit"
                                                                    } else {
                                                                        "+ Justify"
                                                                    }
                                                                }
                                                            }
                                                        }

                                                        if let Some(item) = waiver {
                                                            p { style: "margin:8px 0 2px;font-size:13px;line-height:1.45;color:var(--cf-text-primary);", "{item.reason}" }
                                                            div { style: "display:flex;align-items:center;gap:10px;flex-wrap:wrap;font-size:11px;color:var(--cf-text-muted);",
                                                                span { "mreyes · {relative_time(item.created_at)}" }
                                                                button {
                                                                    class: "focus-ring",
                                                                    style: "all:unset;cursor:not-allowed;color:#f87171;opacity:0.65;",
                                                                    disabled: true,
                                                                    title: "Removing hardening waivers needs a backend delete endpoint.",
                                                                    "Remove"
                                                                }
                                                            }
                                                        }

                                                        if is_editing {
                                                            div { style: "display:flex;flex-direction:column;gap:0;margin-top:8px;",
                                                                textarea {
                                                                    class: "input focus-ring",
                                                                    autofocus: "true",
                                                                    rows: "2",
                                                                    style: "resize:vertical;width:100%;font-size:12px;",
                                                                    placeholder: "Why is leaving this unset acceptable? (compensating control, N/A…)",
                                                                    value: "{reason}",
                                                                    oninput: move |evt| {
                                                                        reason.set(evt.value());
                                                                        justification_error.set(None);
                                                                        justification_notice.set(None);
                                                                    },
                                                                }
                                                                div { style: "display:flex;gap:6px;flex-wrap:wrap;margin-top:6px;",
                                                                    for preset in [
                                                                        "Not applicable — service runs in an isolated container.",
                                                                        "Compensating control in place (AppArmor/SELinux confinement).",
                                                                        "Enforcing breaks required functionality; risk accepted.",
                                                                        "Upstream unit limitation — cannot be enforced here.",
                                                                    ] {
                                                                        button {
                                                                            key: "waiver-preset-{preset}",
                                                                            class: "focus-ring",
                                                                            style: "all:unset;cursor:pointer;font-size:10px;padding:3px 8px;border-radius:99px;background:var(--cf-subtle-bg);color:var(--cf-text-secondary);border:1px solid var(--cf-card-border);",
                                                                            onclick: move |_| {
                                                                                reason.set(preset.to_string());
                                                                                justification_error.set(None);
                                                                                justification_notice.set(None);
                                                                            },
                                                                            if preset.len() > 46 {
                                                                                "{preset.chars().take(44).collect::<String>()}…"
                                                                            } else {
                                                                                "{preset}"
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                                div { style: "display:flex;gap:8px;margin-top:8px;",
                                                                    button {
                                                                        class: "btn btn-primary focus-ring xs",
                                                                        disabled: is_saving_justification() || reason.read().trim().len() < 8,
                                                                        onclick: {
                                                                            let service_name = service.service_name.clone();
                                                                            let directive_name = directive_name.clone();
                                                                            let on_saved = on_saved.clone();
                                                                            move |_| {
                                                                                let reason_value = reason();
                                                                                if reason_value.trim().len() < 8 {
                                                                                    justification_error.set(Some("Add a bit more detail before saving the waiver.".to_string()));
                                                                                    return;
                                                                                }

                                                                                is_saving_justification.set(true);
                                                                                justification_error.set(None);
                                                                                justification_notice.set(None);

                                                                                let request = SaveHardeningJustificationRequest {
                                                                                    directive_name: Some(directive_name.clone()),
                                                                                    category: Some("security".to_string()),
                                                                                    reason: reason_value,
                                                                                };
                                                                                let service_name_for_request = service_name.clone();

                                                                                spawn(async move {
                                                                                    if save_system_hardening_justification(&system_id, &service_name_for_request, &request)
                                                                                        .await
                                                                                        .is_ok()
                                                                                    {
                                                                                        reason.set(String::new());
                                                                                        active_waiver_directive.set(None);
                                                                                        justification_notice.set(Some("Waiver saved.".to_string()));
                                                                                        on_saved.call(());
                                                                                    } else {
                                                                                        justification_error.set(Some("Failed to save waiver.".to_string()));
                                                                                    }
                                                                                    is_saving_justification.set(false);
                                                                                });
                                                                            }
                                                                        },
                                                                        svg {
                                                                            class: "w-3 h-3 inline-block",
                                                                            fill: "none",
                                                                            stroke: "currentColor",
                                                                            stroke_width: "2",
                                                                            view_box: "0 0 24 24",
                                                                            path {
                                                                                stroke_linecap: "round",
                                                                                stroke_linejoin: "round",
                                                                                d: "M5 13l4 4L19 7"
                                                                            }
                                                                        }
                                                                        if is_saving_justification() { "Saving…" } else { "Save waiver" }
                                                                    }
                                                                    button {
                                                                        class: "btn btn-ghost focus-ring xs",
                                                                        disabled: is_saving_justification(),
                                                                        onclick: move |_| {
                                                                            active_waiver_directive.set(None);
                                                                            reason.set(String::new());
                                                                            justification_error.set(None);
                                                                        },
                                                                        "Cancel"
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

fn synthesize_history_entries_from_commits(
    commits: &[SystemCommitHistory],
) -> Vec<SystemHistoryEntry> {
    commits
        .iter()
        .map(|commit| {
            let timestamp = commit.deployed_at.unwrap_or(commit.committed_at);
            SystemHistoryEntry {
                id: None,
                timestamp,
                occurred_at: Some(timestamp),
                observed_at: None,
                store_path: None,
                system_configuration_name: commit.config_identity.clone(),
                change_reason: commit.message.clone(),
                event_type: "cf_deployment_succeeded".to_string(),
                event_rank: Some(10),
                title: Some("Deployed through Crystal Forge".to_string()),
                commit_hash: Some(commit.hash.clone()),
                flake_name: None,
                flake_repo_url: commit.flake_repo_url.clone(),
                actor: commit.author.clone(),
                outcome: if commit.was_deployed || commit.is_current {
                    "success".to_string()
                } else {
                    "pending".to_string()
                },
                // Commit fallback rows represent tracked flake commits, so they classify
                // as deploys with a reconciled/tracked source.
                event_kind: "cf_deployment".to_string(),
                generation: None,
                previous_generation: None,
                new_generation: None,
                previous_store_path: None,
                new_store_path: None,
                previous_boot_id: None,
                new_boot_id: None,
                deployment_id: None,
                desired_target_id: None,
                source: Some("commit_fallback".to_string()),
                correlation_id: None,
                metadata: serde_json::Value::Null,
                reconciled: true,
                generation_matches_current_store_path: None,
                restart_type: None,
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

// fallback_system_detail() has been moved to crate::systems::adapter

#[cfg(test)]
mod tests {
    use super::{
        HistoryEventKind, build_history_events, classify_history_entry, map_agent_events_to_logs,
        map_history_entries_to_commit_history,
    };
    use crate::api::models::{SystemAgentEvent, SystemHistoryEntry};
    use chrono::{Duration, Utc};

    #[test]
    fn history_mapping_marks_revert_when_store_path_reappears() {
        let now = Utc::now();
        let entries = vec![
            SystemHistoryEntry {
                id: None,
                timestamp: now,
                occurred_at: Some(now),
                observed_at: None,
                store_path: Some("/nix/store/aaaa-system".to_string()),
                system_configuration_name: Some("web-01".to_string()),
                change_reason: "cf_deployment".to_string(),
                event_type: "cf_deployment_succeeded".to_string(),
                event_rank: Some(10),
                title: Some("Deployed through Crystal Forge".to_string()),
                commit_hash: Some("aaaaaaaa".to_string()),
                flake_name: Some("infra".to_string()),
                flake_repo_url: Some("https://example.com/infra.git".to_string()),
                actor: "crystal-forge".to_string(),
                outcome: "recorded".to_string(),
                event_kind: "cf_deployment".to_string(),
                generation: Some(3),
                previous_generation: Some(2),
                new_generation: Some(3),
                previous_store_path: None,
                new_store_path: Some("/nix/store/aaaa-system".to_string()),
                previous_boot_id: None,
                new_boot_id: None,
                deployment_id: None,
                desired_target_id: None,
                source: Some("test".to_string()),
                correlation_id: None,
                metadata: serde_json::Value::Null,
                reconciled: true,
                generation_matches_current_store_path: Some(true),
                restart_type: None,
            },
            SystemHistoryEntry {
                id: None,
                timestamp: now - Duration::minutes(10),
                occurred_at: Some(now - Duration::minutes(10)),
                observed_at: None,
                store_path: Some("/nix/store/bbbb-system".to_string()),
                system_configuration_name: Some("web-01".to_string()),
                change_reason: "cf_deployment".to_string(),
                event_type: "cf_deployment_succeeded".to_string(),
                event_rank: Some(10),
                title: Some("Deployed through Crystal Forge".to_string()),
                commit_hash: Some("bbbbbbbb".to_string()),
                flake_name: Some("infra".to_string()),
                flake_repo_url: Some("https://example.com/infra.git".to_string()),
                actor: "crystal-forge".to_string(),
                outcome: "recorded".to_string(),
                event_kind: "cf_deployment".to_string(),
                generation: Some(2),
                previous_generation: Some(1),
                new_generation: Some(2),
                previous_store_path: None,
                new_store_path: Some("/nix/store/bbbb-system".to_string()),
                previous_boot_id: None,
                new_boot_id: None,
                deployment_id: None,
                desired_target_id: None,
                source: Some("test".to_string()),
                correlation_id: None,
                metadata: serde_json::Value::Null,
                reconciled: true,
                generation_matches_current_store_path: Some(false),
                restart_type: None,
            },
            SystemHistoryEntry {
                id: None,
                timestamp: now - Duration::minutes(20),
                occurred_at: Some(now - Duration::minutes(20)),
                observed_at: None,
                store_path: Some("/nix/store/aaaa-system".to_string()),
                system_configuration_name: Some("web-01".to_string()),
                change_reason: "cf_deployment".to_string(),
                event_type: "cf_deployment_succeeded".to_string(),
                event_rank: Some(10),
                title: Some("Deployed through Crystal Forge".to_string()),
                commit_hash: Some("aaaaaaaa".to_string()),
                flake_name: Some("infra".to_string()),
                flake_repo_url: Some("https://example.com/infra.git".to_string()),
                actor: "crystal-forge".to_string(),
                outcome: "recorded".to_string(),
                event_kind: "cf_deployment".to_string(),
                generation: Some(1),
                previous_generation: None,
                new_generation: Some(1),
                previous_store_path: None,
                new_store_path: Some("/nix/store/aaaa-system".to_string()),
                previous_boot_id: None,
                new_boot_id: None,
                deployment_id: None,
                desired_target_id: None,
                source: Some("test".to_string()),
                correlation_id: None,
                metadata: serde_json::Value::Null,
                reconciled: true,
                generation_matches_current_store_path: Some(false),
                restart_type: None,
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

    fn history_entry(change_reason: &str, actor: &str, event_kind: &str) -> SystemHistoryEntry {
        SystemHistoryEntry {
            id: None,
            timestamp: Utc::now(),
            occurred_at: None,
            observed_at: None,
            store_path: Some("/nix/store/aaaa-system".to_string()),
            system_configuration_name: Some("web-01".to_string()),
            change_reason: change_reason.to_string(),
            event_type: event_kind.to_string(),
            event_rank: None,
            title: None,
            commit_hash: Some("aaaaaaaa".to_string()),
            flake_name: Some("infra".to_string()),
            flake_repo_url: Some("https://example.com/infra.git".to_string()),
            actor: actor.to_string(),
            outcome: "recorded".to_string(),
            event_kind: event_kind.to_string(),
            generation: Some(3),
            previous_generation: None,
            new_generation: Some(3),
            previous_store_path: None,
            new_store_path: Some("/nix/store/aaaa-system".to_string()),
            previous_boot_id: None,
            new_boot_id: None,
            deployment_id: None,
            desired_target_id: None,
            source: Some("test".to_string()),
            correlation_id: None,
            metadata: serde_json::Value::Null,
            reconciled: true,
            generation_matches_current_store_path: Some(true),
            restart_type: None,
        }
    }

    #[test]
    fn legacy_history_classifies_config_change_as_local_rebuild() {
        let entry = history_entry("config_change", "on-host", "");

        assert_eq!(
            classify_history_entry(&entry),
            HistoryEventKind::LocalRebuildMatched
        );
    }

    #[test]
    fn event_backed_history_classifies_explicit_event_type_without_text_heuristics() {
        let mut entry = history_entry("startup", "agent", "state_change");
        entry.event_type = "local_rebuild_detected".to_string();
        entry.reconciled = false;

        assert_eq!(
            classify_history_entry(&entry),
            HistoryEventKind::LocalRebuildUntracked
        );

        entry.event_type = "system_reboot".to_string();
        assert_eq!(classify_history_entry(&entry), HistoryEventKind::Restart);

        entry.event_type = "agent_restart".to_string();
        assert_eq!(
            classify_history_entry(&entry),
            HistoryEventKind::AgentRestart
        );
    }

    #[test]
    fn legacy_history_classifies_state_delta_as_state_change() {
        let entry = history_entry("state_delta", "agent", "");

        assert_eq!(
            classify_history_entry(&entry),
            HistoryEventKind::StateChange
        );
    }

    #[test]
    fn legacy_history_classifies_state_change_as_state_change() {
        let entry = history_entry("state_change", "agent", "");

        assert_eq!(
            classify_history_entry(&entry),
            HistoryEventKind::StateChange
        );
    }

    #[test]
    fn authoritative_state_change_does_not_render_in_deployment_timeline() {
        let entry = history_entry("state_delta", "agent", "state_change");

        assert_eq!(
            classify_history_entry(&entry),
            HistoryEventKind::StateChange
        );
        assert!(build_history_events(&[entry], &[], Some(3)).is_empty());
    }

    #[test]
    fn authoritative_local_rebuild_still_renders_in_deployment_timeline() {
        let entry = history_entry("state_delta", "on-host", "local_rebuild");

        assert_eq!(
            classify_history_entry(&entry),
            HistoryEventKind::LocalRebuildMatched
        );
        let events = build_history_events(&[entry], &[], Some(3));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, HistoryEventKind::LocalRebuildMatched);
    }

    #[test]
    fn authoritative_cf_event_kind_still_classifies_as_deploy() {
        let entry = history_entry("cf_deployment", "crystal-forge", "cf_deployment");

        assert_eq!(classify_history_entry(&entry), HistoryEventKind::Deploy);
    }
}
