//! Worker strip component for the builds control center.

use dioxus::prelude::*;

use crate::theme;

use super::helpers::{WorkerAction, WorkerItem, worker_status_class};

/// Worker strip showing all build workers and their status.
#[component]
pub fn WorkerStrip(
    workers: Vec<WorkerItem>,
    on_action: EventHandler<(String, WorkerAction)>,
) -> Element {
    rsx! {
        div {
            class: "grid grid-cols-1 lg:grid-cols-2 gap-3",
            for worker in workers {
                {
                    let worker_id = worker.id.clone();
                    rsx! {
                        div {
                            key: "{worker.id}",
                            class: "rounded-xl border {theme::surface::CARD_BORDER} overflow-hidden shadow-sm",
                            div {
                                class: "px-4 py-3 border-b border-gray-800 flex items-center justify-between cf-worker-header-gradient",
                                div {
                                    p { class: "text-sm text-white font-semibold", "{worker.name}" }
                                    p { class: "text-xs {theme::text::SECONDARY}", "{worker.active_slots}/{worker.total_slots} active slots" }
                                }
                                span {
                                    class: "inline-flex px-2 py-1 text-[10px] uppercase rounded border {worker_status_class(worker.status)}",
                                    "{worker.status_label()}"
                                }
                            }
                            div {
                                class: "px-4 py-3 bg-gray-900/80 flex items-center justify-between",
                                p { class: "text-xs text-gray-400", "Queue depth: {worker.queue_depth}" }
                                div {
                                    class: "inline-flex items-center gap-2",
                                    WorkerTextAction {
                                        label: "Start",
                                        on_click: {
                                            let worker_id = worker_id.clone();
                                            move |_| on_action.call((worker_id.clone(), WorkerAction::Start))
                                        },
                                    }
                                    WorkerTextAction {
                                        label: "Pause",
                                        on_click: {
                                            let worker_id = worker_id.clone();
                                            move |_| on_action.call((worker_id.clone(), WorkerAction::Pause))
                                        },
                                    }
                                    WorkerTextAction {
                                        label: "Drain",
                                        on_click: move |_| on_action.call((worker_id.clone(), WorkerAction::Drain)),
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

/// Worker text action button component.
#[component]
fn WorkerTextAction(label: &'static str, on_click: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            class: "text-xs px-2 py-1 rounded transition-colors cf-action-link",
            onclick: move |evt| on_click.call(evt),
            "{label}"
        }
    }
}
