//! Worker strip component for the builds control center.

use dioxus::prelude::*;

use crate::theme;

use super::helpers::{WorkerAction, WorkerItem};

/// Worker strip showing all build workers and their status.
#[component]
pub fn WorkerStrip(
    workers: Vec<WorkerItem>,
    on_action: EventHandler<(String, WorkerAction)>,
) -> Element {
    let _ = &on_action;
    rsx! {
        div {
            class: "grid gap-2",
            style: "grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));",
            for worker in workers {
                {
                    let slot_pct = if worker.total_slots == 0 {
                        0
                    } else {
                        ((worker.active_slots as f64 / worker.total_slots as f64) * 100.0).round() as i32
                    };
                    rsx! {
                        div {
                            key: "{worker.id}",
                            class: "rounded-xl border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} px-4 py-3 space-y-3",
                            div {
                                class: "flex items-center justify-between gap-2",
                                div {
                                    p { class: "text-[13px] text-white font-semibold", "{worker.name}" }
                                    if let Some(host) = worker.host.clone() {
                                        p { class: "text-[11px] font-mono {theme::text::MUTED}", "{host}" }
                                    }
                                }
                                span {
                                    class: "inline-flex px-2 py-0.5 rounded text-[10px] uppercase font-medium",
                                    style: "color: {status_color(worker.status)}; background-color: {status_bg(worker.status)};",
                                    "{worker.status_label()}"
                                }
                            }
                            div {
                                class: "text-[11px] {theme::text::SECONDARY} flex items-center gap-3",
                                if let Some(arch) = worker.arch.clone() {
                                    span { class: "font-mono", "{arch}" }
                                }
                                if let (Some(cores), Some(mem)) = (worker.cpu_cores, worker.memory_gb) {
                                    span { "{cores}c · {mem}GB" }
                                }
                            }
                            div {
                                div {
                                    class: "flex items-center justify-between text-[11px] {theme::text::MUTED} mb-1",
                                    span { "Slots" }
                                    span { "{worker.active_slots}/{worker.total_slots}" }
                                }
                                div {
                                    class: "h-1 rounded-full bg-slate-800 overflow-hidden",
                                    div {
                                        class: "h-full rounded-full transition-all",
                                        style: "width: {slot_pct}%; background-color: {status_color(worker.status)};",
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

fn status_color(status: super::helpers::WorkerStatus) -> &'static str {
    match status {
        super::helpers::WorkerStatus::Running => "#34d399",
        super::helpers::WorkerStatus::Paused => "#fbbf24",
        super::helpers::WorkerStatus::Draining => "#60a5fa",
    }
}

fn status_bg(status: super::helpers::WorkerStatus) -> &'static str {
    match status {
        super::helpers::WorkerStatus::Running => "rgba(52, 211, 153, 0.14)",
        super::helpers::WorkerStatus::Paused => "rgba(251, 191, 36, 0.14)",
        super::helpers::WorkerStatus::Draining => "rgba(96, 165, 250, 0.14)",
    }
}
