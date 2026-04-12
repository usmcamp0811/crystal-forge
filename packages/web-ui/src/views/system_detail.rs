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
    fetch_cve_scan_status, fetch_system_cve_scan_eligibility, fetch_system_cves,
    request_system_rollback, request_system_sync, trigger_system_cve_scan, ApiClientError,
};
use crate::api::models::{
    BuildStatus, CveScanEligibilityResponse, CveSeverity, CveSummary, DeploymentLogEntry,
    DeploymentStatus, LogLevel, PipelineStage, SystemAgentEvent, SystemCommitHistory,
    SystemDetail, SystemHardwareInfo, SystemHistoryEntry, SystemNetworkInfo,
    SystemRollbackRequest, SystemSecurityInfo,
    SystemVulnerability,
};
use crate::components::cve::CvesTab;
use crate::components::diff::DiffViewer;
use crate::components::layout::Card;
use crate::components::modals::{RollbackConfirmDialog, SyncConfirmDialog};
use crate::components::notifications::Toast;
use crate::components::system::{
    AgentCard, BooleanRow, EditSystemModal, HardwareCard, InfoRow, InfoRowMono, LogLine, LogsTab,
    NetworkCard, SecurityCard, StatusBadge, SystemInfoCard, environment_style,
};
use crate::routes::Route;
use crate::state::{app_state::AppState, auth};
use crate::systems::adapter::{fallback_system_detail, load_system_detail_with_fallback};
use crate::systems::adapter::{
    load_system_agent_events_with_fallback, load_system_history_with_fallback,
    update_system_via_api,
};
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
    History,
    Policy,
    Cves,
    Logs,
}

impl Tab {
    fn label(&self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::History => "History",
            Self::Policy => "Policy",
            Self::Cves => "CVEs",
            Self::Logs => "Logs",
        }
    }
}

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

    // Confirmation dialog state for Sync
    let mut show_sync_dialog = use_signal(|| false);
    let mut sync_in_progress = use_signal(|| false);
    let mut cve_scan_in_progress = use_signal(|| false);
    let mut cve_scan_status_text: Signal<Option<String>> = use_signal(|| None);

    // Confirmation dialog state for rollback/deploying a historical commit
    let mut show_rollback_dialog = use_signal(|| false);
    let mut rollback_target: Signal<Option<SystemCommitHistory>> = use_signal(|| None);

    // Toast notification state
    let mut toast_message: Signal<Option<(String, bool)>> = use_signal(|| None); // (message, is_success)

    // System data state — use_resource keyed on id prevents repeated fetches.
    let id_for_detail = id.clone();
    let mut detail_resource = use_resource(move || {
        let id = id_for_detail.clone();
        async move { load_system_detail_with_fallback(&id).await }
    });

    let id_for_vulns = id.clone();
    let vulnerabilities_resource = use_resource(move || {
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

    // Derive state from resource result
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
    let commit_history = map_history_entries_to_commit_history(&history_entries);
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
    let scan_eligibility: Option<CveScanEligibilityResponse> =
        (*scan_eligibility_resource.read_unchecked()).clone().flatten();

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

    let environment = system
        .environment
        .clone()
        .unwrap_or_else(|| "Unknown".to_string());
    let env_style = environment_style(&environment);

    // Format last seen for header
    let last_seen_text = system
        .last_seen
        .map(|dt| {
            let now = Utc::now();
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
            class: "space-y-6",
            "data-testid": "system-detail",

            // API fallback notice banner (shown when using mock data)
            if let Some(ref notice) = api_notice {
                div {
                    class: "rounded-lg border border-yellow-500/30 bg-yellow-500/10 px-4 py-3 text-sm text-yellow-300",
                    "{notice}"
                }
            }

            // Back link
            Link {
                to: crate::routes::Route::SystemsView {},
                class: "inline-flex items-center gap-1 text-sm {theme::text::SECONDARY} hover:text-white transition-colors",
                svg {
                    class: "w-4 h-4",
                    fill: "none",
                    stroke: "currentColor",
                    view_box: "0 0 24 24",
                    path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M15 19l-7-7 7-7" }
                }
                "Back to Systems"
            }

            // Page header
            header {
                class: "flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between",
                div {
                    class: "flex items-center gap-3 flex-wrap",
                    h1 { class: "{theme::typography::PAGE_TITLE}", "{system.hostname}" }
                    span {
                        class: "inline-flex items-center px-3 py-1 rounded-md text-xs font-semibold uppercase tracking-wide {env_style.chip_bg} {env_style.chip_text}",
                        "{environment}"
                    }
                    StatusBadge {
                        label: system.health_status.label(),
                        color_class: system.health_status.color_class(),
                        bg_class: system.health_status.bg_class()
                    }
                    StatusBadge {
                        label: system.deployment_status.label(),
                        color_class: system.deployment_status.color_class(),
                        bg_class: system.deployment_status.bg_class()
                    }
                    span {
                        class: "text-sm {theme::text::MUTED}",
                        "Last seen: {last_seen_text}"
                    }
                    if let Some(ref store_path) = system.current_store_path {
                        {
                            let hash = store_path.split('-').next().unwrap_or("").chars().skip(11).take(7).collect::<String>();
                            rsx! {
                                if !hash.is_empty() {
                                    span {
                                        class: "font-mono text-xs px-2 py-0.5 rounded bg-gray-800 text-gray-400",
                                        "{hash}"
                                    }
                                }
                            }
                        }
                    }
                }
                div {
                    class: "flex items-center gap-2",
                    button {
                        class: "inline-flex items-center gap-2 px-4 py-2 rounded-lg font-medium text-sm transition-all text-white border border-gray-500/40 bg-gray-700/50 hover:bg-gray-700/80 hover:border-gray-300/60",
                        disabled: !can_mutate,
                        onclick: move |_| edit_modal_open.set(true),
                        if !can_mutate {
                            "Edit (Operator/Admin required)"
                        } else {
                            "Edit"
                        }
                    }
                    button {
                        class: "inline-flex items-center gap-2 px-4 py-2 rounded-lg font-medium text-sm transition-all text-white border border-amber-400/40 bg-amber-600/50 hover:bg-amber-600/70 hover:border-amber-300/60 disabled:opacity-60 disabled:cursor-not-allowed",
                        disabled: *cve_scan_in_progress.read() || !can_mutate || !cve_scan_eligible,
                        title: if cve_scan_eligible {
                            Some("Run CVE scan immediately for this system configuration")
                        } else {
                            Some(cve_scan_blocked_reason.as_str())
                        },
                        onclick: {
                            let system_id = system.id;
                            move |_| {
                                if !can_mutate || !cve_scan_eligible {
                                    return;
                                }

                                cve_scan_in_progress.set(true);
                                cve_scan_status_text.set(Some("CVE scan queued...".to_string()));
                                spawn(async move {
                                    let trigger_result = trigger_system_cve_scan(&system_id).await;
                                    match trigger_result {
                                        Ok(triggered) => {
                                            cve_scan_status_text
                                                .set(Some("CVE scan running...".to_string()));
                                            let mut terminal_status: Option<String> = None;

                                            for _ in 0..25 {
                                                match fetch_cve_scan_status(&triggered.scan_id).await {
                                                    Ok(status) => {
                                                        let normalized = status.status.to_lowercase();
                                                        if normalized == "completed" {
                                                            terminal_status = Some(format!(
                                                                "CVE scan completed: {} vulnerabilities found",
                                                                status.total_vulnerabilities
                                                            ));
                                                            break;
                                                        }
                                                        if normalized == "failed" {
                                                            terminal_status = Some(
                                                                "CVE scan failed. Check server logs for details."
                                                                    .to_string(),
                                                            );
                                                            break;
                                                        }
                                                        cve_scan_status_text
                                                            .set(Some("CVE scan running...".to_string()));
                                                    }
                                                    Err(_) => {
                                                        terminal_status = Some(
                                                            "Unable to poll CVE scan status."
                                                                .to_string(),
                                                        );
                                                        break;
                                                    }
                                                }

                                                use gloo_timers::future::TimeoutFuture;
                                                TimeoutFuture::new(1500).await;
                                            }

                                            if let Some(msg) = terminal_status {
                                                let is_success = msg.contains("completed");
                                                toast_message.set(Some((msg.clone(), is_success)));
                                                cve_scan_status_text.set(Some(msg));
                                            }
                                        }
                                        Err(ApiClientError::Status { code: 409, body }) if body.contains("scan_ineligible") => {
                                            let msg = "CVE scanning is not available on this node (vulnix not installed).".to_string();
                                            toast_message.set(Some((msg.clone(), false)));
                                            cve_scan_status_text.set(Some(msg));
                                        }
                                        Err(err) => {
                                            let msg = format!("Failed to trigger CVE scan: {}", err);
                                            toast_message.set(Some((msg.clone(), false)));
                                            cve_scan_status_text.set(Some(msg));
                                        }
                                    }

                                    cve_scan_in_progress.set(false);
                                });
                            }
                        },

                        if *cve_scan_in_progress.read() {
                            "Scanning..."
                        } else if !can_mutate {
                            "Run CVE Scan (Operator/Admin required)"
                        } else {
                            "Run CVE Scan"
                        }
                    }
                    button {
                        class: "inline-flex items-center gap-2 px-4 py-2 rounded-lg font-medium text-sm transition-all text-white border border-purple-400/40 bg-purple-600/60 hover:bg-purple-600/80 hover:border-purple-300/60 shadow-sm shadow-purple-900/30",
                        disabled: *sync_in_progress.read() || !can_mutate,
                        onclick: move |_| show_sync_dialog.set(true),

                        if *sync_in_progress.read() {
                            svg {
                                class: "w-4 h-4 animate-spin",
                                fill: "none",
                                view_box: "0 0 24 24",
                                circle {
                                    class: "opacity-25",
                                    cx: "12",
                                    cy: "12",
                                    r: "10",
                                    stroke: "currentColor",
                                    stroke_width: "4"
                                }
                                path {
                                    class: "opacity-75",
                                    fill: "currentColor",
                                    d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                                }
                            }
                            "Syncing..."
                        } else if !can_mutate {
                            "Sync (Operator/Admin required)"
                        } else {
                            svg {
                                class: "w-4 h-4",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
                                }
                            }
                            "Sync Now"
                        }
                    }
                }
                if let Some(scan_status) = cve_scan_status_text() {
                    p {
                        class: "text-xs text-amber-200 mt-1",
                        "{scan_status}"
                    }
                }
            }

            // Tab navigation
            div {
                class: "border-b {theme::surface::CARD_BORDER}",
                nav {
                    class: "flex gap-1 -mb-px",
                    for tab in [Tab::Overview, Tab::History, Tab::Policy, Tab::Cves, Tab::Logs] {
                        {
                            let is_active = *active_tab.read() == tab;
                            let tab_class = if is_active {
                                "px-4 py-2 text-sm font-medium text-white border-b-2 border-blue-500"
                            } else {
                                "px-4 py-2 text-sm font-medium {theme::text::SECONDARY} hover:text-white transition-colors border-b-2 border-transparent"
                            };
                            rsx! {
                                button {
                                    key: "{tab:?}",
                                    class: "{tab_class}",
                                    onclick: move |_| active_tab.set(tab),
                                    "{tab.label()}"
                                    if tab == Tab::Cves && system.cve_counts.total() > 0 {
                                        span {
                                            class: "ml-2 px-1.5 py-0.5 text-xs rounded-full bg-red-500/20 text-red-400",
                                            "{system.cve_counts.total()}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Tab content
            div {
                class: "min-h-[400px]",
                match *active_tab.read() {
                    Tab::Overview => rsx! {
                        OverviewTab { system: system.clone() }
                    },
                    Tab::History => rsx! {
                        HistoryTab {
                            commits: commit_history.clone(),
                            deployment_policy: system.deployment_policy.clone(),
                            allow_mutations: can_mutate,
                            on_rollback: move |commit| {
                                rollback_target.set(Some(commit));
                                show_rollback_dialog.set(true);
                            }
                        }
                    },
                    Tab::Policy => rsx! {
                        PolicyTab { system: system.clone() }
                    },
                    Tab::Cves => rsx! {
                        CvesTab {
                            cve_counts: system.cve_counts.clone(),
                            vulnerabilities: vulnerabilities.clone()
                        }
                    },
                    Tab::Logs => rsx! {
                        LogsTab { logs: deployment_logs.clone() }
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

#[component]
fn OverviewTab(system: SystemDetail) -> Element {
    rsx! {
        // Store path (full width)
        if let Some(ref store_path) = system.current_store_path {
            div {
                class: "mb-6",
                Card {
                    title: Some("Current Store Path".to_string()),
                    children: rsx! {
                        code {
                            class: "block text-sm font-mono text-gray-300 bg-gray-800/50 px-4 py-3 rounded-lg overflow-x-auto",
                            "{store_path}"
                        }
                    }
                }
            }
        }

        div {
            class: "grid grid-cols-1 lg:grid-cols-2 xl:grid-cols-3 gap-6 pt-6",

            // System Info card
            SystemInfoCard { system: system.clone() }

            // Hardware card
            HardwareCard { hardware: system.hardware.clone() }

            // Network card
            NetworkCard { network: system.network.clone() }

            // Security card
            SecurityCard { security: system.security.clone() }

            // Agent card
            AgentCard { system: system.clone() }

            // Flake info card (if available)
            if let Some(ref flake) = system.flake {
                Card {
                    title: Some("Flake".to_string()),
                    children: rsx! {
                        dl {
                            class: "space-y-3",
                            InfoRow { label: "Name", value: flake.name.clone() }
                            InfoRowMono { label: "Repository", value: flake.repo_url.clone() }
                            if let Some(ref commit) = flake.latest_commit {
                                InfoRowMono { label: "Latest Commit", value: commit.chars().take(7).collect::<String>() }
                            }
                        }
                    }
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
) -> Element {
    rsx! {
        div {
            class: "pt-6 flex flex-col max-h-[70vh] overflow-hidden",

            // Legend
            div {
                class: "flex items-center gap-6 mb-6 text-sm {theme::text::SECONDARY}",
                div {
                    class: "flex items-center gap-2",
                    div { class: "w-4 h-4 rounded-full bg-emerald-500 ring-2 ring-emerald-400" }
                    span { "Current" }
                }
                div {
                    class: "flex items-center gap-2",
                    div { class: "w-4 h-4 rounded-full bg-blue-500" }
                    span { "Deployed" }
                }
                div {
                    class: "flex items-center gap-2",
                    div { class: "w-4 h-4 rounded-full bg-orange-500" }
                    span { "Pending" }
                }
                div {
                    class: "flex items-center gap-2",
                    div { class: "w-4 h-4 rounded-full bg-amber-500" }
                    span { "Not Ready" }
                }
                div {
                    class: "flex items-center gap-2",
                    div { class: "w-4 h-4 rounded-full border-2 border-dashed border-gray-500 bg-gray-900" }
                    span { "Skipped" }
                }
            }

            // Git graph container - relative positioning for the continuous vertical line
            div {
                class: "flex-1 min-h-0 overflow-y-auto",

                // Inner content wrapper - line sized to full content height
                div {
                    class: "relative",
                    style: "padding-left: 48px;",

                    // Continuous vertical line running the full content height
                    div {
                        class: "absolute bg-gray-600",
                        style: "left: 14px; top: 0; bottom: 0; width: 4px; border-radius: 2px; z-index: 0;",
                    }

                    // Commit entries
                    div {
                        class: "space-y-4 relative",
                        style: "z-index: 1;",
                    for (idx, commit) in commits.iter().enumerate() {
                        CommitTimelineNode {
                            key: "{commit.hash}",
                            commit: commit.clone(),
                            is_first: idx == 0,
                            is_last: idx == commits.len() - 1,
                            deployment_policy: deployment_policy.clone(),
                            allow_mutations,
                            on_rollback: on_rollback.clone()
                        }
                    }
                    }
                }
            }
        }
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

            SystemCommitHistory {
                hash,
                message: config_identity
                    .clone()
                    .map(|value| format!("Configuration {value}"))
                    .unwrap_or_else(|| "Configuration update".to_string()),
                author: format!(
                    "{} · {} · {}",
                    entry.actor, entry.change_reason, entry.outcome
                ),
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
