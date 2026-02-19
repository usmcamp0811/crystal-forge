//! Worker strip component for the builds control center.

use dioxus::prelude::*;

use crate::theme;

use super::helpers::{worker_status_style, WorkerAction, WorkerItem};

/// Worker strip showing all build workers and their status.
#[component]
pub fn WorkerStrip(
    workers: Vec<WorkerItem>,
    on_action: EventHandler<(&'static str, WorkerAction)>,
) -> Element {
    rsx! {
        div {
            class: "grid grid-cols-1 lg:grid-cols-2 gap-3",
            for worker in workers {
                div {
                    key: "{worker.id}",
                    class: "rounded-xl border {theme::surface::CARD_BORDER} overflow-hidden shadow-sm",
                    div {
                        class: "px-4 py-3 border-b border-gray-800 flex items-center justify-between",
                        style: "background: linear-gradient(135deg, rgba(130, 105, 155, 0.34) 0%, rgba(17, 24, 39, 0.92) 100%);",
                        div {
                            p { class: "text-sm text-white font-semibold", "{worker.name}" }
                            p { class: "text-xs {theme::text::SECONDARY}", "{worker.active_slots}/{worker.total_slots} active slots" }
                        }
                        span {
                            class: "inline-flex px-2 py-1 text-[10px] uppercase rounded border",
                            style: "{worker_status_style(worker.status)}",
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
                                on_click: move |_| on_action.call((worker.id, WorkerAction::Start)),
                            }
                            WorkerTextAction {
                                label: "Pause",
                                on_click: move |_| on_action.call((worker.id, WorkerAction::Pause)),
                            }
                            WorkerTextAction {
                                label: "Drain",
                                on_click: move |_| on_action.call((worker.id, WorkerAction::Drain)),
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
            class: "text-xs px-2 py-1 rounded transition-colors",
            style: "color: #D6C3E8;",
            onclick: move |evt| on_click.call(evt),
            "{label}"
        }
    }
}
