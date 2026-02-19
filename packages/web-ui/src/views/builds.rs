//! Builds control center view.

use dioxus::prelude::*;

use crate::components::builds::{
    apply_action, mock_builds, mock_workers, selected_build_data, BuildAction, BuildDetailPane,
    BuildItem, BuildQueuePane, BuildStatus, ConfirmActionModal, DetailTab, MetricsRow,
    PendingAction, QueueAction, QueueActionButton, WorkerAction, WorkerItem, WorkerStatus,
    WorkerStrip,
};
use crate::theme;

/// Builds control center page.
#[component]
pub fn BuildsView() -> Element {
    let mut workers = use_signal(mock_workers);
    let mut builds = use_signal(mock_builds);
    let mut selected_build = use_signal(|| Some(1_i32));
    let mut active_tab = use_signal(|| DetailTab::Logs);

    let follow_logs = use_signal(|| true);
    let pause_logs = use_signal(|| false);
    let wrap_logs = use_signal(|| false);
    let log_query = use_signal(String::new);

    let mut pending_action = use_signal(|| None::<PendingAction>);
    let mut last_action_note = use_signal(|| None::<String>);

    let queue_data = builds.read().clone();
    let worker_data = workers.read().clone();
    let selected = selected_build_data(selected_build.read().to_owned(), &queue_data);

    rsx! {
        div {
            class: "space-y-6",

            header {
                class: "flex flex-col gap-4 xl:flex-row xl:items-center xl:justify-between",
                div {
                    h1 { class: "{theme::typography::PAGE_TITLE}", "Builds" }
                    p {
                        class: "text-sm {theme::text::SECONDARY}",
                        "Control active build workers, inspect queue state, and monitor live build logs."
                    }
                }
                div {
                    class: "flex flex-wrap items-center gap-2",
                    span {
                        class: "inline-flex items-center px-2 py-1 text-xs rounded border text-emerald-100",
                        style: "background-color: #1E3A2E; border-color: #2F6B4A;",
                        span { class: "w-2 h-2 rounded-full bg-emerald-300 mr-2 animate-pulse", }
                        "Live"
                    }
                    QueueActionButton {
                        label: "Start All",
                        onclick: move |_| pending_action.set(Some(PendingAction::Queue(QueueAction::StartAll))),
                    }
                    QueueActionButton {
                        label: "Pause All",
                        onclick: move |_| pending_action.set(Some(PendingAction::Queue(QueueAction::PauseAll))),
                    }
                    QueueActionButton {
                        label: "Drain All",
                        onclick: move |_| pending_action.set(Some(PendingAction::Queue(QueueAction::DrainAll))),
                    }
                }
            }

            MetricsRow {
                workers: worker_data.clone(),
                builds: queue_data.clone(),
            }

            WorkerStrip {
                workers: worker_data.clone(),
                on_action: move |(worker_id, action)| {
                    pending_action.set(Some(PendingAction::Worker { worker_id, action }))
                },
            }

            if let Some(note) = last_action_note.read().clone() {
                p {
                    class: "text-xs px-3 py-2 rounded-lg border text-blue-100",
                    style: "background-color: #23354B; border-color: #406084;",
                    "{note}"
                }
            }

            div {
                class: "cf-builds-split",
                div {
                    BuildQueuePane {
                        builds: queue_data.clone(),
                        selected_id: selected_build,
                        on_build_action: move |(build_id, action)| {
                            pending_action.set(Some(PendingAction::Build { build_id, action }))
                        },
                    }
                }

                div {
                    BuildDetailPane {
                        selected: selected,
                        tab: active_tab,
                        on_tab_change: move |tab| active_tab.set(tab),
                        follow_logs: follow_logs,
                        pause_logs: pause_logs,
                        wrap_logs: wrap_logs,
                        log_query: log_query,
                    }
                }
            }

            if let Some(action) = pending_action.read().clone() {
                ConfirmActionModal {
                    action: action.clone(),
                    on_cancel: move |_| pending_action.set(None),
                    on_confirm: move |_| {
                        if let Some(next_action) = pending_action.read().clone() {
                            apply_action(next_action, &mut workers, &mut builds, &mut selected_build, &mut last_action_note);
                        }
                        pending_action.set(None);
                    }
                }
            }
        }
    }
}
