//! Build detail pane component for the builds control center.

use dioxus::prelude::*;

use crate::components::layout::Card;
use crate::theme;

use super::helpers::{
    BuildAction, BuildItem, PendingAction, build_status_badge_class, event_level_class,
    mock_artifacts, mock_events, mock_logs,
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

/// Build detail pane showing logs, events, and artifacts for a selected build.
#[component]
pub fn BuildDetailPane(
    selected: Option<BuildItem>,
    tab: Signal<DetailTab>,
    on_tab_change: EventHandler<DetailTab>,
    follow_logs: Signal<bool>,
    pause_logs: Signal<bool>,
    wrap_logs: Signal<bool>,
    log_query: Signal<String>,
) -> Element {
    let Some(build) = selected else {
        return rsx! {
            Card {
                title: Some("Build Detail".to_string()),
                children: rsx! {
                    p { class: "text-sm {theme::text::SECONDARY}", "Select a queue item to inspect logs and build metadata." }
                }
            }
        };
    };

    let events = mock_events(build.id);
    let artifacts = mock_artifacts(build.id);
    let logs = filtered_logs(build.id, &log_query.read());

    rsx! {
        Card {
            title: Some("Build Detail".to_string()),
            children: rsx! {
                div {
                    class: "space-y-4",
                    div {
                        class: "rounded-lg border {theme::surface::CARD_BORDER} bg-gray-900/70 p-4",
                        div {
                            class: "flex flex-col md:flex-row md:items-center md:justify-between gap-3",
                            div {
                                p { class: "text-sm text-white font-semibold", "{build.hostname}" }
                                p { class: "text-xs text-gray-400", "{build.flake} · {build.branch} · {short_commit(&build.commit)}" }
                                p { class: "text-xs text-gray-500 mt-1", "Queued by {build.started_by} · worker {build.worker_id}" }
                            }
                                span {
                                    class: "inline-flex px-2 py-1 text-[10px] uppercase rounded border {build_status_badge_class(build.status)}",
                                    "{build.status_label()}"
                                }
                        }
                    }

                    div {
                        class: "inline-flex rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG}",
                        for item in [DetailTab::Logs, DetailTab::Events, DetailTab::Artifacts] {
                            button {
                                key: "{item.label()}",
                                class: "px-3 py-2 text-sm font-medium transition",
                                class: if *tab.read() == item {
                                    "bg-gray-700 text-white"
                                } else {
                                    "text-gray-400 hover:text-white"
                                },
                                onclick: move |_| on_tab_change.call(item),
                                "{item.label()}"
                            }
                        }
                    }

                    if *tab.read() == DetailTab::Logs {
                        div {
                            class: "space-y-3",
                            div {
                                class: "flex flex-wrap gap-2",
                                TogglePill { label: "Follow", value: follow_logs }
                                TogglePill { label: "Pause", value: pause_logs }
                                TogglePill { label: "Wrap", value: wrap_logs }
                                button {
                                    class: "text-xs text-gray-300 border border-gray-700 rounded px-2 py-1 hover:bg-gray-700",
                                    onclick: move |_| log_query.set(String::new()),
                                    "Clear"
                                }
                            }
                            input {
                                class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                r#type: "search",
                                placeholder: "Search logs...",
                                value: "{log_query.read()}",
                                oninput: move |evt| log_query.set(evt.value()),
                            }
                            pre {
                                class: "rounded-lg border border-gray-700 bg-gray-950 p-3 text-xs font-mono text-gray-200 overflow-auto",
                                style: if *wrap_logs.read() { "white-space: pre-wrap; max-height: 22rem;" } else { "white-space: pre; max-height: 22rem;" },
                                if logs.is_empty() {
                                    "No log lines match your filter."
                                } else {
                                    for line in logs {
                                        "{line}\n"
                                    }
                                }
                            }
                        }
                    }

                    if *tab.read() == DetailTab::Events {
                        div {
                            class: "space-y-2",
                            for event in events {
                                div {
                                    class: "rounded-lg border border-gray-700 bg-gray-900/60 p-3",
                                    div {
                                        class: "flex items-center justify-between gap-2",
                                        p { class: "text-xs text-gray-400", "{event.ts}" }
                                        span {
                                            class: "text-[10px] uppercase px-2 py-1 rounded border {event_level_class(event.level)}",
                                            "{event.level}"
                                        }
                                    }
                                    p { class: "text-sm text-gray-200 mt-1", "{event.message}" }
                                }
                            }
                        }
                    }

                    if *tab.read() == DetailTab::Artifacts {
                        div {
                            class: "space-y-2",
                            if artifacts.is_empty() {
                                p { class: "text-sm {theme::text::SECONDARY}", "No artifacts recorded yet for this build." }
                            } else {
                                for artifact in artifacts {
                                    div {
                                        class: "rounded-lg border border-gray-700 bg-gray-900/60 px-3 py-2 flex items-center justify-between gap-2",
                                        div {
                                            p { class: "text-sm text-white", "{artifact.name}" }
                                            p { class: "text-xs text-gray-500 font-mono", "{artifact.hash}" }
                                        }
                                        p { class: "text-xs text-gray-400", "{artifact.size}" }
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

fn short_commit(commit: &str) -> String {
    commit.chars().take(7).collect()
}

fn filtered_logs(build_id: i32, query: &str) -> Vec<String> {
    let lines = mock_logs(build_id);
    if query.trim().is_empty() {
        return lines;
    }
    let q = query.to_lowercase();
    lines
        .into_iter()
        .filter(|line| line.to_lowercase().contains(&q))
        .collect()
}

/// Toggle pill button component.
#[component]
fn TogglePill(label: &'static str, value: Signal<bool>) -> Element {
    rsx! {
        button {
            class: "text-xs rounded border px-2 py-1 transition",
            class: if *value.read() { "cf-toggle-active" } else { "cf-toggle-inactive" },
            onclick: move |_| {
                let next = !*value.read();
                value.set(next);
            },
            "{label}"
        }
    }
}

/// Queue action button component.
#[component]
pub fn QueueActionButton(label: &'static str, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
            onclick: move |evt| onclick.call(evt),
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
