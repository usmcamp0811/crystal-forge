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
                    let slot_pct = if worker.total_slots == 0 {
                        0
                    } else {
                        ((worker.active_slots as f64 / worker.total_slots as f64) * 100.0).round() as i32
                    };
                    rsx! {
                        div {
                            key: "{worker.id}",
                            class: "rounded-xl border {theme::surface::CARD_BORDER} overflow-hidden shadow-sm",
                            div {
                                class: "px-4 py-3 border-b border-gray-800 flex items-center justify-between cf-worker-header-gradient",
                                div {
                                    p { class: "text-sm text-white font-semibold", "{worker.name}" }
                                    if let Some(host) = worker.host.clone() {
                                        p { class: "text-[11px] font-mono {theme::text::SECONDARY}", "{host}" }
                                    }
                                }
                                span {
                                    class: "inline-flex px-2 py-1 text-[10px] uppercase rounded border {worker_status_class(worker.status)}",
                                    "{worker.status_label()}"
                                }
                            }
                            div {
                                class: "px-4 py-3 bg-gray-900/80 space-y-3",
                                div {
                                    class: "flex items-center gap-3 text-[11px] {theme::text::SECONDARY}",
                                    if let Some(arch) = worker.arch.clone() {
                                        span { class: "font-mono", "{arch}" }
                                    }
                                    if let (Some(cores), Some(mem)) = (worker.cpu_cores, worker.memory_gb) {
                                        span { "{cores}c · {mem}GB" }
                                    }
                                }
                                div {
                                    class: "space-y-1",
                                    div {
                                        class: "flex items-center justify-between text-[11px] {theme::text::MUTED}",
                                        span { "Slots" }
                                        span { "{worker.active_slots}/{worker.total_slots}" }
                                    }
                                    div {
                                        class: "h-1.5 rounded-full bg-slate-800 overflow-hidden",
                                        div {
                                            class: "h-full rounded-full bg-emerald-400 transition-all",
                                            style: "width: {slot_pct}%",
                                        }
                                    }
                                }
                                div {
                                    class: "flex items-center justify-between",
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
