//! Build detail pane component for the builds control center.

use dioxus::prelude::*;

use crate::theme;

use super::helpers::{
    BuildAction, BuildItem, BuildStatus, PendingAction, build_status_badge_class,
    extract_system_name,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailTab {
    Logs,
    Events,
    Artifacts,
}

impl DetailTab {
    pub fn label(self) -> &'static str {
        match self {
            DetailTab::Logs => "Live Logs",
            DetailTab::Events => "Events",
            DetailTab::Artifacts => "Artifacts",
        }
    }
}

/// Build detail side panel matching the mockup structure.
#[component]
pub fn BuildDetailPane(
    selected: Option<BuildItem>,
    on_close: EventHandler<()>,
    on_log: EventHandler<()>,
    tab: Signal<DetailTab>,
    on_tab_change: EventHandler<DetailTab>,
    follow_logs: Signal<bool>,
    pause_logs: Signal<bool>,
    wrap_logs: Signal<bool>,
    log_query: Signal<String>,
) -> Element {
    let Some(build) = selected else {
        return rsx! { div {} };
    };

    let _ = tab;
    let _ = on_tab_change;
    let _ = follow_logs;
    let _ = pause_logs;
    let _ = wrap_logs;
    let _ = log_query;
    let duration_label = build.runtime.clone().unwrap_or_else(|| "—".to_string());
    let has_drv_progress = build.total_derivs > 0
        && build.built_derivs < build.total_derivs
        && matches!(build.status, BuildStatus::Building | BuildStatus::Stopping);
    let derivs_label = if build.total_derivs > 0 {
        format!(
            "{}/{} built · {} cached",
            build.built_derivs, build.total_derivs, build.cached_derivs
        )
    } else {
        "—".to_string()
    };

    rsx! {
        // Note: The aside.side-panel wrapper is in builds.rs
        div {
            // JSX: <div className="panel-head">
            div {
                class: "panel-head",
                // JSX: <div className="panel-title">
                div {
                    class: "panel-title",
                    h2 {
                        style: "font-size: 15px;",
                        span {
                            class: "chip {build_status_badge_class(build.status)}",
                            style: "margin-right: 6px;",
                            "{build.status_label()}"
                        }
                        // JSX title: Build {b.pkg}
                        "Build {build.pkg()}"
                    }
                    // JSX subtitle: {b.drv} (full derivation path, mono, muted)
                    span { class: "fqdn mono", "{build.drv()}" }
                }
                // JSX: <button className="btn-icon focus-ring"><Icon name="x" size={16} /></button>
                button {
                    class: "btn-icon focus-ring",
                    onclick: move |_| on_close.call(()),
                    svg {
                        width: "16",
                        height: "16",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        line { x1: "18", y1: "6", x2: "6", y2: "18" }
                        line { x1: "6", y1: "6", x2: "18", y2: "18" }
                    }
                }
            }

            // JSX: <div className="panel-body">
            div {
                class: "panel-body",
                // JSX: <section className="panel-section">
                section {
                    class: "panel-section",
                    // JSX: <dl className="kv-grid">
                    // JSX: <dl className="kv-grid">
                    // System, Flake, Commit, Worker, Arch, Derivations, Queued, Duration, Attempts
                    dl { class: "kv-grid",
                        dt { "System" }
                        dd { class: "mono", "{extract_system_name(&build.hostname)}" }
                        dt { "Flake" }
                        dd { "{build.flake}" }
                        dt { "Commit" }
                        dd { class: "mono", "{build.commit}" }
                        dt { "Worker" }
                        dd { class: "mono",
                            if build.worker_id == "unassigned" { "unassigned" } else { "{build.worker_id}" }
                        }
                        dt { "Arch" }
                        dd { class: "mono", "{build.arch}" }
                        dt { "Derivations" }
                        dd { "{derivs_label}" }
                        dt { "Queued" }
                        dd { "{build.queued_for}" }
                        dt { "Duration" }
                        dd { class: "mono", "{duration_label}" }
                        dt { "Attempts" }
                        dd { "{build.attempts}" }
                    }
                }

                // JSX: derivation progress section — shown when build is in progress
                if has_drv_progress {
                    section { class: "panel-section",
                        h3 { "Derivation progress" }
                        {
                            let total = build.total_derivs as f64;
                            let cached_pct = (build.cached_derivs as f64 / total * 100.0).min(100.0);
                            let built_pct = ((build.built_derivs.saturating_sub(build.cached_derivs)) as f64 / total * 100.0).min(100.0);
                            let col = status_color(build.status);
                            rsx! {
                                div {
                                    style: "height: 6px; background: var(--cf-subtle-bg); border-radius: 99px; overflow: hidden; display: flex;",
                                    div { style: "width: {cached_pct}%; background: #34d399;" }
                                    div { style: "width: {built_pct}%; background: {col};" }
                                }
                                div {
                                    style: "font-size: 11px; color: var(--cf-text-muted); margin-top: 4px;",
                                    "{build.built_derivs} of {build.total_derivs} derivations"
                                    if let Some(ref pkg) = build.current_pkg {
                                        " · "
                                        span { class: "mono", style: "color: #60a5fa;", "building {pkg}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // JSX: <div className="panel-actions">
            div {
                class: "panel-actions",
                // Logs always available
                button {
                    class: "btn btn-ghost focus-ring xs",
                    onclick: move |_| on_log.call(()),
                    // terminal icon
                    svg {
                        width: "12", height: "12",
                        view_box: "0 0 24 24",
                        fill: "none", stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round", stroke_linejoin: "round",
                        style: "margin-right: 4px;",
                        polyline { points: "4 17 10 11 4 5" }
                        line { x1: "12", y1: "19", x2: "20", y2: "19" }
                    }
                    "Logs"
                }
                // Cancel for building/queued
                if matches!(build.status, BuildStatus::Building | BuildStatus::Queued) {
                    button {
                        class: "btn btn-ghost focus-ring xs",
                        svg {
                            width: "12", height: "12",
                            view_box: "0 0 24 24",
                            fill: "none", stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round", stroke_linejoin: "round",
                            style: "margin-right: 4px;",
                            line { x1: "18", y1: "6", x2: "6", y2: "18" }
                            line { x1: "6", y1: "6", x2: "18", y2: "18" }
                        }
                        "Cancel build"
                    }
                }
                // Force kill for stopping
                if build.status == BuildStatus::Stopping {
                    button {
                        class: "btn btn-ghost focus-ring xs",
                        style: "color: var(--cf-red, #f87171);",
                        svg {
                            width: "12", height: "12",
                            view_box: "0 0 24 24",
                            fill: "none", stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round", stroke_linejoin: "round",
                            style: "margin-right: 4px;",
                            line { x1: "18", y1: "6", x2: "6", y2: "18" }
                            line { x1: "6", y1: "6", x2: "18", y2: "18" }
                        }
                        "Force kill"
                    }
                }
                // Retry for failed
                if build.status == BuildStatus::Failed {
                    button {
                        class: "btn btn-ghost focus-ring xs",
                        svg {
                            width: "12", height: "12",
                            view_box: "0 0 24 24",
                            fill: "none", stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round", stroke_linejoin: "round",
                            style: "margin-right: 4px;",
                            path { d: "M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74" }
                            path { d: "M21 3v9h-9" }
                            path { d: "M21 12A9 9 0 0 0 3.26 9.26" }
                        }
                        "Retry build"
                    }
                }
            }
        }
    }
}

fn status_color(status: BuildStatus) -> &'static str {
    match status {
        BuildStatus::Queued => "#a78bfa",
        BuildStatus::Building => "#60a5fa",
        BuildStatus::Stopping => "#fbbf24",
        BuildStatus::Failed => "#f87171",
        BuildStatus::Complete => "#34d399",
        BuildStatus::Cancelled => "#94a3b8",
    }
}

/// Queue action button component.
#[component]
pub fn QueueActionButton(label: &'static str, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            class: "inline-flex items-center gap-2 px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
            onclick: move |evt| onclick.call(evt),
            span { class: "text-xs", "+" }
            "{label}"
        }
    }
}

/// Confirmation modal for build actions.
#[component]
pub fn ConfirmActionModal(
    action: PendingAction,
    on_cancel: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    let (title, description, confirm_label) = action_prompt(&action);

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4 cf-modal-overlay",
            onclick: move |_| on_cancel.call(()),
            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6 cf-modal-panel-30",
                onclick: |evt| evt.stop_propagation(),
                h3 { class: "text-lg font-semibold text-white mb-2", "{title}" }
                p { class: "text-sm {theme::text::SECONDARY} mb-6", "{description}" }
                div {
                    class: "flex gap-3",
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-gray-700 hover:bg-gray-600 text-white",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm text-white {theme::interactive::PRIMARY_BTN}",
                        onclick: move |_| on_confirm.call(()),
                        "{confirm_label}"
                    }
                }
            }
        }
    }
}

fn action_prompt(action: &PendingAction) -> (&'static str, String, &'static str) {
    match action {
        PendingAction::Queue(super::helpers::QueueAction::StartAll) => (
            "Start all workers?",
            "This resumes queue processing on all build workers.".to_string(),
            "Start All",
        ),
        PendingAction::Queue(super::helpers::QueueAction::PauseAll) => (
            "Pause all workers?",
            "Queued builds will remain queued until workers are resumed.".to_string(),
            "Pause All",
        ),
        PendingAction::Queue(super::helpers::QueueAction::DrainAll) => (
            "Drain all workers?",
            "Workers finish active builds but stop taking new queued work.".to_string(),
            "Drain All",
        ),
        PendingAction::Worker {
            worker_id,
            action: super::helpers::WorkerAction::Start,
        } => (
            "Start worker?",
            format!("Worker {worker_id} will resume processing queued builds."),
            "Start",
        ),
        PendingAction::Worker {
            worker_id,
            action: super::helpers::WorkerAction::Pause,
        } => (
            "Pause worker?",
            format!("Worker {worker_id} will stop taking new work immediately."),
            "Pause",
        ),
        PendingAction::Worker {
            worker_id,
            action: super::helpers::WorkerAction::Drain,
        } => (
            "Drain worker?",
            format!("Worker {worker_id} will finish active builds, then idle."),
            "Drain",
        ),
        PendingAction::Build {
            build_id,
            action: BuildAction::Stop,
        } => (
            "Stop build?",
            format!(
                "Build #{build_id} will send stop to the active systemd unit and mark canceled."
            ),
            "Stop",
        ),
        PendingAction::Build {
            build_id,
            action: BuildAction::ForceCancel,
        } => (
            "Force cancel build?",
            format!(
                "Build #{build_id} will be immediately marked as cancelled without waiting for builder confirmation. Use this for stuck builds."
            ),
            "Force Cancel",
        ),
        PendingAction::Build {
            build_id,
            action: BuildAction::Restart,
        } => (
            "Restart build?",
            format!(
                "Build #{build_id} will cancel current systemd execution and rerun build commands."
            ),
            "Restart",
        ),
        PendingAction::Build {
            build_id,
            action: BuildAction::RunNext,
        } => (
            "Run this build next?",
            format!("Build #{build_id} will be promoted to the front of the queued set."),
            "Prioritize",
        ),
    }
}
