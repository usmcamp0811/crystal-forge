//! Metrics row component for the builds control center.

use dioxus::prelude::*;

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

    rsx! {
        div {
            class: "grid grid-cols-2 xl:grid-cols-5 gap-3",
            MetricBadge { label: "Building", value: building.to_string(), tone_class: "cf-metric-building" }
            MetricBadge { label: "Queued", value: queued.to_string(), tone_class: "cf-metric-queued" }
            MetricBadge { label: "Failed 24h", value: failed_24h.to_string(), tone_class: "cf-metric-failed" }
            MetricBadge {
                label: "Workers",
                value: format!("{active_workers}/{}", workers.len()),
                tone_class: "cf-metric-workers",
            }
            MetricBadge {
                label: "Slot usage",
                value: format!("{slot_pct}%"),
                tone_class: "cf-metric-slots",
            }
        }
    }
}

/// Individual metric badge component.
#[component]
fn MetricBadge(label: &'static str, value: String, tone_class: &'static str) -> Element {
    rsx! {
        div {
            class: "rounded-lg border px-3 py-2 {tone_class}",
            p { class: "text-[10px] uppercase tracking-wide text-gray-400", "{label}" }
            p { class: "text-sm text-white font-semibold", "{value}" }
        }
    }
}
