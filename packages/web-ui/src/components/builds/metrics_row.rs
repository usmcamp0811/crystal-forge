//! Metrics row component for the builds control center.

use dioxus::prelude::*;
use crate::theme;

use super::helpers::BuildStatus;
use super::helpers::WorkerItem;

/// Metrics row showing build queue and worker statistics.
#[component]
pub fn MetricsRow(
    workers: Vec<WorkerItem>,
    builds: Vec<super::helpers::BuildItem>,
    history_builds: Vec<super::helpers::BuildItem>,
) -> Element {
    let building = builds
        .iter()
        .filter(|b| matches!(b.status, BuildStatus::Building | BuildStatus::Stopping))
        .count();
    let queued = builds
        .iter()
        .filter(|b| matches!(b.status, BuildStatus::Queued))
        .count();
    let failed_24h = history_builds
        .iter()
        .filter(|b| matches!(b.status, BuildStatus::Failed))
        .count();
    let active_workers = workers
        .iter()
        .filter(|w| w.status == super::helpers::WorkerStatus::Running)
        .count();
    let slot_total = workers.iter().map(|w| w.total_slots).sum::<usize>();
    let slot_used = workers.iter().map(|w| w.active_slots).sum::<usize>();
    let slot_pct = if slot_total == 0 {
        0
    } else {
        ((slot_used as f64 / slot_total as f64) * 100.0).round() as i32
    };

    // JSX: <div className="stat-strip">
    rsx! {
        div {
            class: "stat-strip",
            Stat { label: "Building", value: building.to_string(), color: "#60a5fa" }
            Stat { label: "Queued", value: queued.to_string(), color: "#a78bfa" }
            Stat { label: "Failed 24h", value: failed_24h.to_string(), color: "#f87171" }
            Stat {
                label: "Workers",
                value: format!("{active_workers}/{}", workers.len()),
                color: "#34d399"
            }
            Stat {
                label: "Slot usage",
                value: format!("{slot_pct}%"),
                color: "#22d3ee"
            }
        }
    }
}

/// Individual stat card matching JSX structure
/// JSX: <div className="stat">
#[component]
fn Stat(label: &'static str, value: String, color: &'static str) -> Element {
    rsx! {
        div {
            class: "stat",
            // JSX: <span className="stat-accent" style={{ "--stat-color": s.color }} />
            span {
                class: "stat-accent",
                style: "--stat-color: {color};"
            }
            // JSX: <div className="stat-label">{s.label}</div>
            div { class: "stat-label", "{label}" }
            // JSX: <div className="stat-value" style={{ color:s.color }}>{s.val}</div>
            div {
                class: "stat-value",
                style: "color: {color};",
                "{value}"
            }
        }
    }
}
