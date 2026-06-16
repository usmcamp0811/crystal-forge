//! Worker strip component for the builds control center.
//!
//! Matches BuildsView.jsx WorkerCard exactly — inline styles, no Tailwind.

use dioxus::prelude::*;

use super::helpers::{WorkerAction, WorkerItem, WorkerStatus};

/// Worker grid showing all build workers and their status.
/// JSX: <div style={{ display:"grid", gridTemplateColumns:"repeat(auto-fill,minmax(240px,1fr))", gap:10 }}>
#[component]
pub fn WorkerStrip(
    workers: Vec<WorkerItem>,
    on_action: EventHandler<(String, WorkerAction)>,
) -> Element {
    let _ = &on_action;
    rsx! {
        div {
            style: "display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); gap: 10px;",
            for worker in workers {
                WorkerCard { worker: worker.clone() }
            }
        }
    }
}

/// Individual worker card.
/// JSX: <div className="card" style={{ padding:"14px 16px", display:"flex", flexDirection:"column", gap:10 }}>
#[component]
fn WorkerCard(worker: WorkerItem) -> Element {
    let slot_pct = if worker.total_slots == 0 {
        0
    } else {
        ((worker.active_slots as f64 / worker.total_slots as f64) * 100.0).round() as usize
    };
    let status_col = status_color(worker.status);
    let chip_style = format!(
        "color: {status_col}; background: {status_col}22; font-size: 10px;",
    );

    rsx! {
        div {
            key: "{worker.id}",
            class: "card",
            style: "padding: 14px 16px; display: flex; flex-direction: column; gap: 10px;",

            // Row 1: name + host / status chip
            div {
                style: "display: flex; align-items: center; justify-content: space-between;",
                div {
                    div {
                        style: "font-size: 13px; font-weight: 600;",
                        "{worker.name}"
                    }
                    if let Some(ref host) = worker.host {
                        div {
                            class: "mono",
                            style: "font-size: 11px; color: var(--cf-text-muted);",
                            "{host}"
                        }
                    }
                }
                span {
                    class: "chip",
                    style: "{chip_style}",
                    "{status_label(worker.status)}"
                }
            }

            // Row 2: arch · cores · mem
            div {
                style: "font-size: 11px; color: var(--cf-text-secondary); display: flex; gap: 12px;",
                if let Some(ref arch) = worker.arch {
                    span { "{arch}" }
                }
                if let (Some(cores), Some(mem)) = (worker.cpu_cores, worker.memory_gb) {
                    span { "{cores}c · {mem}GB" }
                }
            }

            // Row 3: slots label + progress bar
            div {
                div {
                    style: "display: flex; justify-content: space-between; font-size: 11px; color: var(--cf-text-muted); margin-bottom: 4px;",
                    span { "Slots" }
                    span { "{worker.active_slots}/{worker.total_slots}" }
                }
                div {
                    style: "height: 4px; background: var(--cf-subtle-bg); border-radius: 99px; overflow: hidden;",
                    div {
                        style: "width: {slot_pct}%; height: 100%; background: {status_col};",
                    }
                }
            }
        }
    }
}

fn status_color(status: WorkerStatus) -> &'static str {
    match status {
        WorkerStatus::Running  => "#34d399",
        WorkerStatus::Paused   => "#fbbf24",
        WorkerStatus::Draining => "#60a5fa",
    }
}

fn status_label(status: WorkerStatus) -> &'static str {
    match status {
        WorkerStatus::Running  => "running",
        WorkerStatus::Paused   => "paused",
        WorkerStatus::Draining => "draining",
    }
}
