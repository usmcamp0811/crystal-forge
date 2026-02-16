//! System detail view — full information for a single NixOS system.
//!
//! Uses a tabbed layout with:
//! - Overview: System info, hardware, network, security
//! - History: Vertical commit timeline showing deployed vs skipped commits
//! - CVEs: Expandable vulnerability list
//! - Logs: Recent deployment logs

use chrono::{DateTime, Utc};
use dioxus::prelude::*;

use crate::api::models::{
    BuildStatus, CveSeverity, CveSummary, DeploymentLogEntry, DeploymentStatus, LogLevel,
    PipelineStage, SystemCommitHistory, SystemDetail, SystemHardwareInfo, SystemNetworkInfo,
    SystemSecurityInfo, SystemVulnerability,
};
use crate::components::layout::Card;
use crate::theme;
use crate::views::systems_list::{mock_system_detail_by_id, mock_system_details};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
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
    // Current tab state
    let mut active_tab = use_signal(|| Tab::Overview);

    // Confirmation dialog state for Sync
    let mut show_sync_dialog = use_signal(|| false);
    let mut sync_in_progress = use_signal(|| false);

    // Confirmation dialog state for rollback/deploying a historical commit
    let mut show_rollback_dialog = use_signal(|| false);
    let mut rollback_target: Signal<Option<SystemCommitHistory>> = use_signal(|| None);

    // Toast notification state
    let mut toast_message: Signal<Option<(String, bool)>> = use_signal(|| None); // (message, is_success)

    // Counter for alternating success/failure
    let mut sync_attempt_count = use_signal(|| 0u32);

    // TODO: Replace with real API call using use_resource + fetch_system()
    let system = mock_system_detail_by_id(&id).unwrap_or_else(|| fallback_system_detail());
    let commit_history = mock_commit_history_for_system(&system);
    let vulnerabilities = mock_vulnerabilities();
    let deployment_logs = mock_deployment_logs();

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

            // Back link
            div {
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
            }

            // Page header
            header {
                class: "flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between",

                // Left side: hostname, env, status, last seen
                div {
                    class: "space-y-2",
                    div {
                        class: "flex items-center gap-3",
                        div {
                            class: "flex flex-col",
                            h1 { class: "{theme::typography::PAGE_TITLE}", "{system.hostname}" }
                            span {
                                class: "text-xs {theme::text::MUTED}",
                                "{system.id}"
                            }
                        }
                        span {
                            class: "inline-flex items-center px-3 py-1 rounded-md text-xs font-semibold uppercase tracking-wide {env_style.chip_bg} {env_style.chip_text}",
                            "{environment}"
                        }
                    }

                    // Status row
                    div {
                        class: "flex flex-wrap items-center gap-3",
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

                        // Last seen
                        span {
                            class: "text-sm {theme::text::MUTED}",
                            "Last seen: {last_seen_text}"
                        }

                        // Current commit hash
                        if let Some(ref store_path) = system.current_store_path {
                            {
                                // Extract hash from store path
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
                }

                // Right side: Sync Now button
                div {
                    class: "flex items-center gap-2",
                    button {
                        class: "inline-flex items-center gap-2 px-4 py-2 rounded-lg font-medium text-sm transition-all text-white border border-purple-400/40 bg-purple-600/60 hover:bg-purple-600/80 hover:border-purple-300/60 shadow-sm shadow-purple-900/30",
                        disabled: *sync_in_progress.read(),
                        onclick: move |_| show_sync_dialog.set(true),

                        if *sync_in_progress.read() {
                            // Spinner
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
            }

            // Tab navigation
            div {
                class: "border-b {theme::surface::CARD_BORDER}",
                nav {
                    class: "flex gap-1",
                    for tab in [Tab::Overview, Tab::History, Tab::Policy, Tab::Cves, Tab::Logs] {
                        {
                            let is_active = *active_tab.read() == tab;
                            let tab_class = if is_active {
                                "px-4 py-2 text-sm font-medium text-white border-b-2 border-blue-500 -mb-px"
                            } else {
                                "px-4 py-2 text-sm font-medium {theme::text::SECONDARY} hover:text-white transition-colors"
                            };
                            rsx! {
                                button {
                                    key: "{tab:?}",
                                    class: "{tab_class}",
                                    onclick: move |_| active_tab.set(tab),
                                    "{tab.label()}"

                                    // Badge for CVE count
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
        if *show_sync_dialog.read() {
            SyncConfirmDialog {
                hostname: system.hostname.clone(),
                on_confirm: {
                    let hostname = system.hostname.clone();
                    move |_| {
                        show_sync_dialog.set(false);
                        sync_in_progress.set(true);

                        // Increment attempt counter for alternating success/failure
                        let attempt = *sync_attempt_count.read();
                        sync_attempt_count.set(attempt + 1);
                        let will_succeed = attempt % 2 == 0;

                        // Simulate async deployment with spawn
                        let hostname = hostname.clone();
                        let mut toast_message = toast_message.clone();
                        spawn(async move {
                            // Simulate 2-4 second build/deploy time
                            #[cfg(target_arch = "wasm32")]
                            {
                                use gloo_timers::future::TimeoutFuture;
                                TimeoutFuture::new(2500).await;
                            }

                            sync_in_progress.set(false);

                            let show_toast = if will_succeed {
                                let message = format!("Successfully synced {}", hostname);
                                dispatch_sync_notification(message, true, toast_message.clone()).await
                            } else {
                                let message = format!("Failed to sync {}: Build timeout", hostname);
                                dispatch_sync_notification(message, false, toast_message.clone()).await
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
                        let toast_message = toast_message.clone();
                        move |_| {
                            show_rollback_dialog.set(false);
                            // TODO: Implement policy override (temporary disable or switch to manual).
                            let message = format!(
                                "Requested rollback of {} to {}",
                                hostname,
                                commit.hash.chars().take(7).collect::<String>()
                            );
                            spawn(async move {
                                let _ = dispatch_sync_notification(message, true, toast_message).await;
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

        // Store path (full width)
        if let Some(ref store_path) = system.current_store_path {
            div {
                class: "mt-6",
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
    }
}

#[component]
fn HistoryTab(
    commits: Vec<SystemCommitHistory>,
    deployment_policy: String,
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
                    div { class: "w-4 h-4 rounded-full bg-blue-500 ring-2 ring-blue-400" }
                    span { "Current" }
                }
                div {
                    class: "flex items-center gap-2",
                    div { class: "w-4 h-4 rounded-full bg-emerald-500" }
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
                    style: "padding-left: 3rem;",

                    // Continuous vertical line running the full content height
                    div {
                        class: "absolute bg-gray-600",
                        style: "left: 0.875rem; top: 0; bottom: 0; width: 4px; border-radius: 2px; z-index: 1;",
                    }

                    // Commit entries
                    div {
                        class: "space-y-4",
                    for (idx, commit) in commits.iter().enumerate() {
                        CommitTimelineNode {
                            key: "{commit.hash}",
                            commit: commit.clone(),
                            is_first: idx == 0,
                            is_last: idx == commits.len() - 1,
                            deployment_policy: deployment_policy.clone(),
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
    let node_border = if !commit.is_current && !commit.was_deployed && !commit.is_ready_to_deploy {
        "border-2 border-dashed border-gray-500"
    } else {
        "border-2 border-gray-950"
    };

    // Glow effect for current commit
    let node_glow = if commit.is_current {
        "box-shadow: 0 0 16px 4px rgba(16, 185, 129, 0.7);"
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

    // Connector color matches node
    let connector_color = if commit.is_current {
        "bg-emerald-500"
    } else if commit.was_deployed {
        "bg-blue-500"
    } else if commit.is_ready_to_deploy {
        "bg-orange-500"
    } else {
        "bg-gray-500"
    };

    // Node dimensions and positioning math:
    // Node: 2rem (32px) tall, top: 0.5rem (8px) -> center at 8 + 16 = 24px from top
    // Connector should be at center: 24px - 2px (half of 4px height) = 22px
    // Diamond: 8px tall, center at 24px -> top = 24 - 4 = 20px

    rsx! {
        div {
            class: "relative overflow-visible",
            style: "min-height: 3rem;",

            // Node (circle) - positioned on the vertical line, filled solid
            div {
                class: "absolute rounded-full {node_border} flex items-center justify-center",
                style: "left: -3rem; top: 8px; width: 32px; height: 32px; z-index: 10; background-color: {node_color}; {node_glow}",

                // Checkmark icon for deployed commits
                if commit.is_current || commit.was_deployed {
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

            // Horizontal connector from node to card - centered vertically with node
            div {
                class: "{connector_color}",
                style: "position: absolute; left: -1rem; top: 22px; width: 1rem; height: 4px; border-radius: 2px; z-index: 5;",
            }

            // Arrow/pointer (diamond) on the card - centered with connector
            div {
                class: "{connector_color}",
                style: "position: absolute; left: -4px; top: 20px; width: 8px; height: 8px; transform: rotate(45deg); z-index: 6;",
            }

            // Content card
            div {
                class: "group relative rounded-lg border {theme::surface::CARD_BG} {theme::surface::CARD_BORDER} overflow-visible",

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
                            if !commit.is_current {
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

                        // Hash
                        code {
                            class: "font-mono bg-gray-800 px-1.5 py-0.5 rounded text-gray-400",
                            "{short_hash}"
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
fn CvesTab(cve_counts: CveSummary, vulnerabilities: Vec<SystemVulnerability>) -> Element {
    let mut expanded_severity: Signal<Option<CveSeverity>> = use_signal(|| None);

    let total = cve_counts.total();

    rsx! {
        div {
            class: "pt-6 space-y-6",

            // Summary header
            div {
                class: "flex items-baseline gap-3",
                span {
                    class: "text-3xl font-bold text-white",
                    "{total}"
                }
                span {
                    class: "{theme::text::SECONDARY}",
                    "known vulnerabilities"
                }
            }

            // Severity breakdown - clickable to expand
            div {
                class: "space-y-3",
                CveSeverityRow {
                    severity: CveSeverity::Critical,
                    count: cve_counts.critical,
                    vulnerabilities: vulnerabilities.clone(),
                    expanded: *expanded_severity.read() == Some(CveSeverity::Critical),
                    on_toggle: move |_| {
                        let current = *expanded_severity.read();
                        if current == Some(CveSeverity::Critical) {
                            expanded_severity.set(None);
                        } else {
                            expanded_severity.set(Some(CveSeverity::Critical));
                        }
                    }
                }
                CveSeverityRow {
                    severity: CveSeverity::High,
                    count: cve_counts.high,
                    vulnerabilities: vulnerabilities.clone(),
                    expanded: *expanded_severity.read() == Some(CveSeverity::High),
                    on_toggle: move |_| {
                        let current = *expanded_severity.read();
                        if current == Some(CveSeverity::High) {
                            expanded_severity.set(None);
                        } else {
                            expanded_severity.set(Some(CveSeverity::High));
                        }
                    }
                }
                CveSeverityRow {
                    severity: CveSeverity::Medium,
                    count: cve_counts.medium,
                    vulnerabilities: vulnerabilities.clone(),
                    expanded: *expanded_severity.read() == Some(CveSeverity::Medium),
                    on_toggle: move |_| {
                        let current = *expanded_severity.read();
                        if current == Some(CveSeverity::Medium) {
                            expanded_severity.set(None);
                        } else {
                            expanded_severity.set(Some(CveSeverity::Medium));
                        }
                    }
                }
                CveSeverityRow {
                    severity: CveSeverity::Low,
                    count: cve_counts.low,
                    vulnerabilities: vulnerabilities.clone(),
                    expanded: *expanded_severity.read() == Some(CveSeverity::Low),
                    on_toggle: move |_| {
                        let current = *expanded_severity.read();
                        if current == Some(CveSeverity::Low) {
                            expanded_severity.set(None);
                        } else {
                            expanded_severity.set(Some(CveSeverity::Low));
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn CveSeverityRow(
    severity: CveSeverity,
    count: i64,
    vulnerabilities: Vec<SystemVulnerability>,
    expanded: bool,
    on_toggle: EventHandler<()>,
) -> Element {
    let filtered_vulns: Vec<_> = vulnerabilities
        .iter()
        .filter(|v| v.severity == severity)
        .collect();

    let has_vulns = !filtered_vulns.is_empty();

    let severity_dot_color = match severity {
        CveSeverity::Critical => "bg-red-500",
        CveSeverity::High => "bg-orange-500",
        CveSeverity::Medium => "bg-yellow-500",
        CveSeverity::Low => "bg-blue-500",
    };
    let severity_text_color = severity.color_class();
    let chevron_class = if expanded { "rotate-180" } else { "" };
    let button_bg = if expanded { "bg-gray-800/30" } else { "" };

    rsx! {
        div {
            class: "rounded-lg border {theme::surface::CARD_BORDER} overflow-hidden",

            // Header row (clickable)
            button {
                class: "w-full flex items-center justify-between p-4 text-left transition-colors hover:bg-gray-800/50 {button_bg}",
                disabled: !has_vulns,
                onclick: move |_| on_toggle.call(()),

                div {
                    class: "flex items-center gap-3",
                    // Severity indicator
                    span {
                        class: "w-3 h-3 rounded-full {severity_dot_color}",
                    }
                    span {
                        class: "font-medium {severity_text_color}",
                        "{severity.label()}"
                    }
                }

                div {
                    class: "flex items-center gap-3",
                    span {
                        class: "text-xl font-bold {severity_text_color}",
                        "{count}"
                    }
                    if has_vulns {
                        svg {
                            class: "w-4 h-4 text-gray-500 transition-transform {chevron_class}",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M19 9l-7 7-7-7"
                            }
                        }
                    }
                }
            }

            // Expanded content
            if expanded && has_vulns {
                div {
                    class: "border-t {theme::surface::CARD_BORDER} divide-y divide-gray-800",
                    for vuln in filtered_vulns.iter() {
                        VulnerabilityRow { vuln: (*vuln).clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn VulnerabilityRow(vuln: SystemVulnerability) -> Element {
    let severity_color = vuln.severity.color_class();

    rsx! {
        div {
            class: "p-4 hover:bg-gray-800/30 transition-colors",

            div {
                class: "flex items-start justify-between gap-4",
                div {
                    class: "flex-1 min-w-0",

                    // CVE ID and package
                    div {
                        class: "flex items-center gap-2 flex-wrap",
                        span {
                            class: "font-mono text-sm font-medium text-white",
                            "{vuln.cve_id}"
                        }
                        span {
                            class: "text-xs px-2 py-0.5 rounded bg-gray-700 text-gray-300",
                            "{vuln.package_name}"
                        }
                    }

                    // Description
                    p {
                        class: "text-sm {theme::text::SECONDARY} mt-1 line-clamp-2",
                        "{vuln.description}"
                    }

                    // Version info
                    div {
                        class: "flex items-center gap-4 mt-2 text-xs {theme::text::MUTED}",
                        span { "Installed: {vuln.installed_version}" }
                        if let Some(ref fixed) = vuln.fixed_version {
                            span {
                                class: "text-emerald-400",
                                "Fixed in: {fixed}"
                            }
                        }
                    }
                }

                // CVSS score
                if let Some(score) = vuln.cvss_score {
                    div {
                        class: "shrink-0 text-right",
                        div {
                            class: "text-lg font-bold {severity_color}",
                            "{score:.1}"
                        }
                        div {
                            class: "text-xs {theme::text::MUTED}",
                            "CVSS"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn LogsTab(logs: Vec<DeploymentLogEntry>) -> Element {
    if logs.is_empty() {
        return rsx! {
            div {
                class: "pt-6 text-center py-12",
                p {
                    class: "{theme::text::SECONDARY}",
                    "No deployment logs available."
                }
            }
        };
    }

    // Get the deployment phase for grouping
    let first_phase = logs
        .first()
        .and_then(|l| l.phase.clone())
        .unwrap_or_else(|| "Deployment".to_string());

    rsx! {
        div {
            class: "pt-6",

            // Header
            div {
                class: "flex items-center justify-between mb-4",
                h3 {
                    class: "{theme::typography::SECTION_TITLE} text-white",
                    "Recent Deployment"
                }
                // TODO: Add link to full logs view
                button {
                    class: "text-sm text-blue-400 hover:text-blue-300 transition-colors",
                    "View full logs →"
                }
            }

            // Log container
            div {
                class: "rounded-lg border {theme::surface::CARD_BG} {theme::surface::CARD_BORDER} overflow-hidden",

                // Phase header
                div {
                    class: "px-4 py-2 bg-gray-800/50 border-b border-gray-700",
                    span {
                        class: "text-xs font-medium text-gray-400 uppercase tracking-wider",
                        "{first_phase}"
                    }
                }

                // Log entries
                div {
                    class: "font-mono text-sm divide-y divide-gray-800/50 max-h-[400px] overflow-y-auto",
                    for log in logs.iter() {
                        LogLine { log: log.clone() }
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
    let mut format = use_signal(|| PolicyFormat::Toml);
    let mut policy_text = use_signal(|| POLICY_TOML_SAMPLE.to_string());

    rsx! {
        div {
            class: "pt-6 space-y-6",

            div {
                class: "flex flex-col gap-2",
                h3 { class: "{theme::typography::SECTION_TITLE} text-white", "Deployment Policy" }
                p {
                    class: "text-sm {theme::text::SECONDARY}",
                    "Define policy rules for {system.hostname}. Policies are parsed using the server model in deployment_policies.rs."
                }
            }

            div {
                class: "flex items-center gap-2",
                button {
                    class: "px-3 py-1.5 rounded-md text-sm border transition-colors",
                    class: if *format.read() == PolicyFormat::Toml {
                        "bg-blue-500/20 border-blue-500 text-blue-300"
                    } else {
                        "{theme::interactive::INPUT} {theme::surface::CARD_BORDER} {theme::text::SECONDARY}"
                    },
                    onclick: move |_| {
                        format.set(PolicyFormat::Toml);
                        policy_text.set(POLICY_TOML_SAMPLE.to_string());
                    },
                    "TOML"
                }
                button {
                    class: "px-3 py-1.5 rounded-md text-sm border transition-colors",
                    class: if *format.read() == PolicyFormat::Json {
                        "bg-blue-500/20 border-blue-500 text-blue-300"
                    } else {
                        "{theme::interactive::INPUT} {theme::surface::CARD_BORDER} {theme::text::SECONDARY}"
                    },
                    onclick: move |_| {
                        format.set(PolicyFormat::Json);
                        policy_text.set(POLICY_JSON_SAMPLE.to_string());
                    },
                    "JSON"
                }
            }

            Card {
                title: Some("Policy Definition".to_string()),
                children: rsx! {
                    textarea {
                        class: "w-full min-h-[280px] bg-gray-950 text-gray-100 font-mono text-xs rounded-lg border border-gray-800 p-4 focus:outline-none focus:ring-2 focus:ring-blue-500/40",
                        value: "{policy_text}",
                        oninput: move |event| policy_text.set(event.value()),
                    }
                    p {
                        class: "mt-3 text-xs {theme::text::MUTED}",
                        "This is a stub UI; saving will wire to policy evaluation and policy override behavior."
                    }
                }
            }
        }
    }
}

#[component]
fn DiffViewer(diff: String) -> Element {
    rsx! {
        div {
            class: "text-xs font-mono text-gray-300 bg-gray-950 p-3 rounded-lg overflow-x-auto whitespace-pre",
            for line in diff.lines() {
                {
                    let class = if line.starts_with("+++") || line.starts_with("---") {
                        "text-gray-400"
                    } else if line.starts_with("@@") {
                        "text-purple-300"
                    } else if line.starts_with("+") {
                        "text-emerald-300"
                    } else if line.starts_with("-") {
                        "text-red-300"
                    } else if line.starts_with("diff --git") || line.starts_with("index ") {
                        "text-blue-300"
                    } else {
                        "text-gray-300"
                    };
                    rsx! {
                        div { class: "{class}", "{line}" }
                    }
                }
            }
        }
    }
}

fn diff_for_commit(hash: &str, message: &str) -> String {
    let selector = hash
        .bytes()
        .last()
        .unwrap_or(b'0')
        .wrapping_sub(b'0')
        % 3;

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

#[component]
fn LogLine(log: DeploymentLogEntry) -> Element {
    let time = log.timestamp.format("%H:%M:%S").to_string();
    let level_bg = match log.level {
        LogLevel::Error => "bg-red-500",
        LogLevel::Warn => "bg-yellow-500",
        LogLevel::Info => "bg-gray-500",
        LogLevel::Debug => "bg-gray-700",
    };
    let level_text = log.level.color_class();

    rsx! {
        div {
            class: "flex gap-3 px-4 py-2 hover:bg-gray-800/30",

            // Timestamp
            span {
                class: "shrink-0 text-xs {theme::text::MUTED}",
                "{time}"
            }

            // Level indicator
            span {
                class: "shrink-0 w-1 rounded-full {level_bg}",
            }

            // Message
            span {
                class: "flex-1 {level_text}",
                "{log.message}"
            }
        }
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

#[component]
fn Toast(message: String, is_success: bool, on_dismiss: EventHandler<()>) -> Element {
    let (bg_class, icon_class, icon_path) = if is_success {
        (
            "bg-emerald-900/90 border-emerald-700",
            "text-emerald-400",
            "M5 13l4 4L19 7", // checkmark
        )
    } else {
        (
            "bg-red-900/90 border-red-700",
            "text-red-400",
            "M6 18L18 6M6 6l12 12", // X
        )
    };

    rsx! {
        div {
            class: "animate-slide-in",
            style: "position: fixed; top: 1rem; right: 1rem; z-index: 120;",
            div {
                class: "flex items-center gap-3 px-4 py-3 rounded-lg border shadow-lg backdrop-blur-sm {bg_class}",

                // Icon
                div {
                    class: "shrink-0",
                    svg {
                        class: "w-5 h-5 {icon_class}",
                        fill: "none",
                        stroke: "currentColor",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            stroke_width: "2",
                            d: "{icon_path}"
                        }
                    }
                }

                // Message
                span {
                    class: "text-sm text-white font-medium",
                    "{message}"
                }

                // Dismiss button
                button {
                    class: "shrink-0 ml-2 p-1 rounded hover:bg-white/10 transition-colors",
                    onclick: move |_| on_dismiss.call(()),
                    svg {
                        class: "w-4 h-4 text-gray-400",
                        fill: "none",
                        stroke: "currentColor",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            stroke_width: "2",
                            d: "M6 18L18 6M6 6l12 12"
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Sync Confirmation Dialog
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn SyncConfirmDialog(
    hostname: String,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    rsx! {
        // Backdrop
        div {
            class: "bg-black/60 flex items-center justify-center p-4",
            style: "position: fixed; inset: 0; width: 100vw; height: 100vh; z-index: 60; backdrop-filter: blur(6px);",
            onclick: move |_| on_cancel.call(()),

            // Dialog
            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6",
                style: "width: 100%; max-width: 30rem;",
                onclick: |evt| evt.stop_propagation(),

                // Icon
                div {
                    class: "flex justify-center mb-4",
                    div {
                        class: "w-12 h-12 rounded-full bg-blue-500/20 flex items-center justify-center",
                        svg {
                            class: "w-6 h-6 text-blue-400",
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
                    }
                }

                // Title
                h3 {
                    class: "text-lg font-semibold text-white text-center mb-2",
                    "Sync {hostname}?"
                }

                // Description
                p {
                    class: "text-sm {theme::text::SECONDARY} text-center mb-6",
                    "This will build the latest configuration and deploy it to this system immediately. Any in-progress builds will be interrupted."
                }

                // Buttons
                div {
                    class: "flex gap-3",
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-gray-700 hover:bg-gray-600 text-white",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors {theme::interactive::PRIMARY_BTN} text-white",
                        onclick: move |_| on_confirm.call(()),
                        "Sync Now"
                    }
                }
            }
        }
    }
}

#[component]
fn RollbackConfirmDialog(
    hostname: String,
    commit: SystemCommitHistory,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let short_hash = commit.hash.chars().take(7).collect::<String>();

    rsx! {
        // Backdrop
        div {
            class: "bg-black/60 flex items-center justify-center p-4",
            style: "position: fixed; inset: 0; width: 100vw; height: 100vh; z-index: 60; backdrop-filter: blur(6px);",
            onclick: move |_| on_cancel.call(()),

            // Dialog
            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6",
                style: "width: 100%; max-width: 32rem;",
                onclick: |evt| evt.stop_propagation(),

                // Icon
                div {
                    class: "flex justify-center mb-4",
                    div {
                        class: "w-12 h-12 rounded-full bg-amber-500/20 flex items-center justify-center",
                        svg {
                            class: "w-6 h-6 text-amber-400",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M3 12a9 9 0 1018 0 9 9 0 00-18 0zm9-4v4l-3 3"
                            }
                        }
                    }
                }

                // Title
                h3 {
                    class: "text-lg font-semibold text-white text-center mb-2",
                    "Deploy historical commit?"
                }

                // Description
                p {
                    class: "text-sm {theme::text::SECONDARY} text-center mb-4",
                    "This will roll back {hostname} to commit {short_hash}. This may pause automatic deployment policies."
                }

                // Commit summary
                div {
                    class: "rounded-lg border border-gray-700 bg-gray-900/60 p-3 mb-5",
                    div { class: "text-xs text-gray-400", "Commit" }
                    div { class: "text-sm text-white font-medium", "{commit.message}" }
                }

                // Buttons
                div {
                    class: "flex gap-3",
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-gray-700 hover:bg-gray-600 text-white",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-amber-500 hover:bg-amber-400 text-gray-900",
                        onclick: move |_| on_confirm.call(()),
                        "Deploy commit"
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Card Components (from original)
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn SystemInfoCard(system: SystemDetail) -> Element {
    rsx! {
        Card {
            title: Some("System".to_string()),
            children: rsx! {
                dl {
                    class: "space-y-3",
                    InfoRow { label: "Hostname", value: system.hostname.clone() }
                    if let Some(ref nixos_version) = system.nixos_version {
                        InfoRow { label: "NixOS Version", value: nixos_version.clone() }
                    }
                    if let Some(ref kernel) = system.kernel {
                        InfoRow { label: "Kernel", value: kernel.clone() }
                    }
                    InfoRow { label: "Deployment Policy", value: deployment_policy_label(&system.deployment_policy) }
                }
            }
        }
    }
}

#[component]
fn HardwareCard(hardware: SystemHardwareInfo) -> Element {
    rsx! {
        Card {
            title: Some("Hardware".to_string()),
            children: rsx! {
                dl {
                    class: "space-y-3",
                    if let Some(ref cpu) = hardware.cpu_brand {
                        InfoRow { label: "CPU", value: cpu.clone() }
                    }
                    if let Some(cores) = hardware.cpu_cores {
                        InfoRow { label: "CPU Cores", value: cores.to_string() }
                    }
                    if let Some(mem) = hardware.memory_gb {
                        InfoRow { label: "Memory", value: format_memory(mem) }
                    }
                    if let Some(uptime) = hardware.uptime_secs {
                        InfoRow { label: "Uptime", value: format_uptime(uptime) }
                    }
                    if let Some(ref bios) = hardware.bios_version {
                        InfoRow { label: "BIOS Version", value: bios.clone() }
                    }
                    if let Some(ref serial) = hardware.board_serial {
                        InfoRow { label: "Board Serial", value: serial.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn NetworkCard(network: SystemNetworkInfo) -> Element {
    rsx! {
        Card {
            title: Some("Network".to_string()),
            children: rsx! {
                dl {
                    class: "space-y-3",
                    if let Some(ref ip) = network.primary_ip {
                        InfoRowMono { label: "Primary IP", value: ip.clone() }
                    }
                    if let Some(ref mac) = network.primary_mac {
                        InfoRowMono { label: "MAC Address", value: mac.clone() }
                    }
                    if let Some(ref gateway) = network.gateway_ip {
                        InfoRowMono { label: "Gateway", value: gateway.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn SecurityCard(security: SystemSecurityInfo) -> Element {
    rsx! {
        Card {
            title: Some("Security".to_string()),
            children: rsx! {
                dl {
                    class: "space-y-3",
                    if let Some(tpm) = security.tpm_present {
                        BooleanRow { label: "TPM Present", value: tpm }
                    }
                    if let Some(sb) = security.secure_boot_enabled {
                        BooleanRow { label: "Secure Boot", value: sb }
                    }
                    if let Some(fips) = security.fips_mode {
                        BooleanRow { label: "FIPS Mode", value: fips }
                    }
                    if let Some(ref selinux) = security.selinux_status {
                        InfoRow { label: "SELinux", value: selinux.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn AgentCard(system: SystemDetail) -> Element {
    rsx! {
        Card {
            title: Some("Agent".to_string()),
            children: rsx! {
                dl {
                    class: "space-y-3",
                    if let Some(ref version) = system.agent_version {
                        InfoRow { label: "Version", value: version.clone() }
                    }
                    if let Some(ref last_seen) = system.last_seen {
                        InfoRow { label: "Last Seen", value: last_seen.format("%Y-%m-%d %H:%M:%S UTC").to_string() }
                    }
                    BooleanRow { label: "Active", value: system.is_active }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper Components
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn StatusBadge(label: &'static str, color_class: &'static str, bg_class: &'static str) -> Element {
    rsx! {
        span {
            class: "inline-flex items-center px-2.5 py-1 rounded-full text-xs font-medium {color_class} {bg_class}",
            "{label}"
        }
    }
}

#[component]
fn InfoRow(label: &'static str, value: String) -> Element {
    rsx! {
        div {
            class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-1",
            dt { class: "text-xs uppercase tracking-wider text-gray-500", "{label}" }
            dd { class: "text-sm text-gray-200", "{value}" }
        }
    }
}

#[component]
fn InfoRowMono(label: &'static str, value: String) -> Element {
    rsx! {
        div {
            class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-1",
            dt { class: "text-xs uppercase tracking-wider text-gray-500", "{label}" }
            dd { class: "text-sm text-gray-200 font-mono", "{value}" }
        }
    }
}

#[component]
fn BooleanRow(label: &'static str, value: bool) -> Element {
    let (icon, color, text) = if value {
        ("✓", "text-emerald-400", "Enabled")
    } else {
        ("✗", "text-gray-500", "Disabled")
    };
    rsx! {
        div {
            class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-1",
            dt { class: "text-xs uppercase tracking-wider text-gray-500", "{label}" }
            dd { class: "text-sm font-medium {color}", "{icon} {text}" }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper Functions
// ─────────────────────────────────────────────────────────────────────────────

fn format_memory(gb: f64) -> String {
    if gb >= 1000.0 {
        format!("{:.0} GB", gb / 1000.0)
    } else {
        format!("{:.1} GB", gb)
    }
}

fn format_uptime(seconds: i64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;

    if days > 0 {
        format!("{}d {}h {}m", days, hours, minutes)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}

fn deployment_policy_label(policy: &str) -> String {
    match policy {
        "Immediate" => "Auto-deploy: Immediate".to_string(),
        "Boot Only" => "Auto-deploy: On reboot".to_string(),
        _ => policy.to_string(),
    }
}

struct EnvStyle {
    chip_bg: &'static str,
    chip_text: &'static str,
}

fn environment_style(environment: &str) -> EnvStyle {
    match environment.to_lowercase().as_str() {
        "production" => EnvStyle {
            chip_bg: "bg-emerald-500/20",
            chip_text: "text-emerald-300",
        },
        "staging" => EnvStyle {
            chip_bg: "bg-amber-500/20",
            chip_text: "text-amber-300",
        },
        "development" => EnvStyle {
            chip_bg: "bg-blue-500/20",
            chip_text: "text-blue-300",
        },
        _ => EnvStyle {
            chip_bg: "bg-gray-500/20",
            chip_text: "text-gray-300",
        },
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Mock Data
// ─────────────────────────────────────────────────────────────────────────────

fn mock_commit_history_for_system(system: &SystemDetail) -> Vec<SystemCommitHistory> {
    use chrono::Duration;
    let flake_name = system.flake.as_ref().map(|flake| flake.name.as_str());
    let timelines = crate::views::dashboard::mock_flake_timelines();
    let Some(timeline) = flake_name.and_then(|name| timelines.iter().find(|t| t.flake_name == name)) else {
        return Vec::new();
    };

    let mut commits: Vec<SystemCommitHistory> = timeline
        .commits
        .iter()
        .map(|commit| SystemCommitHistory {
            hash: commit.hash.clone(),
            message: commit.message.clone(),
            author: commit.author.clone(),
            committed_at: commit.committed_at,
            was_deployed: false,
            deployed_at: None,
            is_current: false,
            is_ready_to_deploy: false,
            build_status: commit.build_status,
            diff_summary: Some(diff_for_commit(&commit.hash, &commit.message)),
        })
        .collect();

    if commits.is_empty() {
        return commits;
    }

    let mut current_idx: Option<usize> = match system.deployment_status {
        DeploymentStatus::UpToDate => Some(0),
        DeploymentStatus::Behind => Some(2.min(commits.len() - 1)),
        DeploymentStatus::Ahead => Some(0),
        DeploymentStatus::NeverDeployed => None,
        DeploymentStatus::NoCommitsAvailable => return Vec::new(),
        DeploymentStatus::Unknown => Some(1.min(commits.len() - 1)),
    };

    if system.hostname == "ws-001" {
        current_idx = Some(3.min(commits.len().saturating_sub(1)));
    }

    if let Some(idx) = current_idx {
        commits[idx].is_current = true;
        commits[idx].was_deployed = true;
        commits[idx].deployed_at = Some(commits[idx].committed_at + Duration::hours(1));

        for i in (idx + 1)..commits.len() {
            commits[i].was_deployed = true;
            commits[i].deployed_at = Some(commits[i].committed_at + Duration::hours(1));
        }
    }

    // Mark ready-to-deploy commit when pipeline is ready
    if matches!(system.pipeline_stage, Some(PipelineStage::ReadyForDeploy)) {
        let ready_idx = current_idx
            .and_then(|idx| if idx > 0 { Some(idx - 1) } else { None })
            .unwrap_or(0);
        if ready_idx < commits.len() {
            commits[ready_idx].is_ready_to_deploy = true;
            commits[ready_idx].was_deployed = false;
            commits[ready_idx].deployed_at = None;
            commits[ready_idx].is_current = false;
        }
    }

    commits
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
            published_at: Some(Utc::now() - chrono::Duration::days(30)),
        },
        SystemVulnerability {
            cve_id: "CVE-2024-5678".to_string(),
            severity: CveSeverity::High,
            cvss_score: Some(7.5),
            description: "Denial of service vulnerability in curl HTTP/2 implementation.".to_string(),
            package_name: "curl".to_string(),
            installed_version: "8.4.0".to_string(),
            fixed_version: Some("8.5.0".to_string()),
            published_at: Some(Utc::now() - chrono::Duration::days(14)),
        },
        SystemVulnerability {
            cve_id: "CVE-2024-9012".to_string(),
            severity: CveSeverity::High,
            cvss_score: Some(7.2),
            description: "Privilege escalation in sudo when using specific sudoers configurations.".to_string(),
            package_name: "sudo".to_string(),
            installed_version: "1.9.14".to_string(),
            fixed_version: None,
            published_at: Some(Utc::now() - chrono::Duration::days(7)),
        },
        SystemVulnerability {
            cve_id: "CVE-2024-3456".to_string(),
            severity: CveSeverity::Medium,
            cvss_score: Some(5.3),
            description: "Information disclosure in nginx when using certain proxy configurations.".to_string(),
            package_name: "nginx".to_string(),
            installed_version: "1.24.0".to_string(),
            fixed_version: Some("1.25.0".to_string()),
            published_at: Some(Utc::now() - chrono::Duration::days(45)),
        },
        SystemVulnerability {
            cve_id: "CVE-2024-7890".to_string(),
            severity: CveSeverity::Low,
            cvss_score: Some(3.1),
            description: "Minor information leak in bash completion scripts.".to_string(),
            package_name: "bash".to_string(),
            installed_version: "5.2".to_string(),
            fixed_version: None,
            published_at: Some(Utc::now() - chrono::Duration::days(60)),
        },
    ]
}

fn mock_deployment_logs() -> Vec<DeploymentLogEntry> {
    use chrono::Duration;
    let base_time = Utc::now() - Duration::hours(1);

    vec![
        DeploymentLogEntry {
            message: "Starting deployment for commit a1b2c3d".to_string(),
            timestamp: base_time,
            level: LogLevel::Info,
            phase: Some("Deployment".to_string()),
        },
        DeploymentLogEntry {
            message: "Fetching derivation from build cache...".to_string(),
            timestamp: base_time + Duration::seconds(2),
            level: LogLevel::Info,
            phase: Some("Deployment".to_string()),
        },
        DeploymentLogEntry {
            message: "Cache hit: /nix/store/abc123-nixos-system-server-01".to_string(),
            timestamp: base_time + Duration::seconds(5),
            level: LogLevel::Info,
            phase: Some("Deployment".to_string()),
        },
        DeploymentLogEntry {
            message: "Activating new configuration...".to_string(),
            timestamp: base_time + Duration::seconds(8),
            level: LogLevel::Info,
            phase: Some("Deployment".to_string()),
        },
        DeploymentLogEntry {
            message: "Restarting nginx.service".to_string(),
            timestamp: base_time + Duration::seconds(10),
            level: LogLevel::Info,
            phase: Some("Deployment".to_string()),
        },
        DeploymentLogEntry {
            message: "Warning: postgresql.service restart skipped (no changes)".to_string(),
            timestamp: base_time + Duration::seconds(11),
            level: LogLevel::Warn,
            phase: Some("Deployment".to_string()),
        },
        DeploymentLogEntry {
            message: "Setting system profile...".to_string(),
            timestamp: base_time + Duration::seconds(13),
            level: LogLevel::Info,
            phase: Some("Deployment".to_string()),
        },
        DeploymentLogEntry {
            message: "Deployment complete. System now running a1b2c3d".to_string(),
            timestamp: base_time + Duration::seconds(15),
            level: LogLevel::Info,
            phase: Some("Deployment".to_string()),
        },
    ]
}

fn fallback_system_detail() -> SystemDetail {
    crate::views::systems_list::mock_system_details()
        .into_iter()
        .next()
        .unwrap_or_else(|| SystemDetail {
            id: uuid::Uuid::new_v4(),
            hostname: "unknown".to_string(),
            environment: None,
            is_active: false,
            deployment_policy: "Unknown".to_string(),
            health_status: crate::api::models::HealthStatus::Offline,
            deployment_status: crate::api::models::DeploymentStatus::Unknown,
            pipeline_stage: None,
            nixos_version: None,
            kernel: None,
            agent_version: None,
            current_store_path: None,
            hardware: SystemHardwareInfo {
                cpu_brand: None,
                cpu_cores: None,
                memory_gb: None,
                uptime_secs: None,
                board_serial: None,
                bios_version: None,
            },
            network: SystemNetworkInfo {
                primary_ip: None,
                primary_mac: None,
                gateway_ip: None,
            },
            security: SystemSecurityInfo {
                tpm_present: None,
                secure_boot_enabled: None,
                fips_mode: None,
                selinux_status: None,
            },
            cve_counts: CveSummary {
                critical: 0,
                high: 0,
                medium: 0,
                low: 0,
            },
            flake: None,
            last_seen: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
}
