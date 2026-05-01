//! Build detail pane component for the builds control center.

use dioxus::prelude::*;

use crate::theme;

use super::helpers::{
    BuildAction, BuildItem, BuildStatus, PendingAction, build_status_badge_class, extract_system_name,
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

    let progress = if matches!(build.status, BuildStatus::Building | BuildStatus::Stopping) {
        56
    } else {
        0
    };
    let _ = tab;
    let _ = on_tab_change;
    let _ = follow_logs;
    let _ = pause_logs;
    let _ = wrap_logs;
    let _ = log_query;
    let log_line_count = build
        .logs
        .as_deref()
        .map(|text| text.lines().count())
        .unwrap_or(0);
    let duration_label = build.runtime.clone().unwrap_or_else(|| "-".to_string());

    rsx! {
        aside {
            class: "rounded-xl border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} p-4 shadow-2xl",
            div {
                class: "flex items-start justify-between gap-3",
                div {
                    h2 {
                        class: "text-[15px] font-semibold text-white leading-5",
                        span {
                            class: "inline-flex mr-2 px-1.5 py-0.5 text-[10px] uppercase rounded border {build_status_badge_class(build.status)}",
                            "{build.status_label()}"
                        }
                        "{extract_system_name(&build.hostname)}"
                    }
                    p { class: "text-[11px] font-mono {theme::text::MUTED} truncate", "{build.summary}" }
                    p { class: "text-[11px] {theme::text::MUTED} truncate", "{build.flake} · {build.commit}" }
                }
                button {
                    class: "btn-icon focus-ring",
                    onclick: move |_| on_close.call(()),
                    "✕"
                }
            }

            dl {
                class: "mt-4 grid grid-cols-[92px,1fr] gap-x-3 gap-y-1.5 text-xs",
                dt { class: "{theme::text::MUTED}", "Flake" } dd { class: "{theme::text::SECONDARY}", "{build.flake}" }
                dt { class: "{theme::text::MUTED}", "Commit" } dd { class: "font-mono {theme::text::SECONDARY} truncate", "{build.commit}" }
                dt { class: "{theme::text::MUTED}", "Worker" } dd { class: "font-mono {theme::text::SECONDARY}", "{build.worker_id}" }
                dt { class: "{theme::text::MUTED}", "Arch" } dd { class: "font-mono {theme::text::SECONDARY}", "x86_64-linux" }
                dt { class: "{theme::text::MUTED}", "Queued" } dd { class: "{theme::text::SECONDARY}", "{build.queued_for}" }
                dt { class: "{theme::text::MUTED}", "Duration" } dd { class: "font-mono {theme::text::SECONDARY}", "{duration_label}" }
                dt { class: "{theme::text::MUTED}", "Attempts" } dd { class: "{theme::text::SECONDARY}", "1" }
                dt { class: "{theme::text::MUTED}", "Log lines" } dd { class: "{theme::text::SECONDARY}", "{log_line_count}" }
            }

            if progress > 0 && progress < 100 {
                section {
                    class: "mt-4",
                    h3 { class: "text-xs font-medium {theme::text::SECONDARY} mb-2", "Progress" }
                    div {
                        class: "h-1.5 bg-slate-800 rounded-full overflow-hidden",
                        div { class: "h-full bg-cyan-400", style: "width: {progress}%" }
                    }
                    p { class: "text-[11px] {theme::text::MUTED} mt-1", "{progress}% complete" }
                }
            }

            div {
                class: "mt-4 flex items-center gap-2 pt-1",
                button {
                    class: "inline-flex items-center gap-2 rounded-lg px-3 py-1.5 text-xs border transition-colors {theme::interactive::GHOST_BTN}",
                    onclick: move |_| on_log.call(()),
                    "Logs"
                }
                button {
                    class: "inline-flex items-center gap-2 rounded-lg px-3 py-1.5 text-xs border transition-colors {theme::interactive::GHOST_BTN}",
                    onclick: move |_| on_close.call(()),
                    "Cancel"
                }
            }
        }
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
