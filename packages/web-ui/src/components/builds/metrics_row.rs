//! Metrics row component for the builds control center.

use dioxus::prelude::*;

use crate::theme;

use super::helpers::BuildStatus;
use super::helpers::WorkerItem;

/// Metrics row showing build queue and worker statistics.
#[component]
pub fn MetricsRow(workers: Vec<WorkerItem>, builds: Vec<super::helpers::BuildItem>) -> Element {
    let building = builds
        .iter()
        .filter(|b| matches!(b.status, BuildStatus::Building | BuildStatus::Restarting))
        .count();
    let queued = builds
        .iter()
        .filter(|b| matches!(b.status, BuildStatus::Queued))
        .count();
    let failed = builds
        .iter()
        .filter(|b| matches!(b.status, BuildStatus::Failed))
        .count();
    let active_workers = workers
        .iter()
        .filter(|w| w.status == super::helpers::WorkerStatus::Running)
        .count();

    rsx! {
        div {
            class: "grid grid-cols-2 md:grid-cols-4 gap-3",
            MetricBadge { label: "Building", value: building.to_string(), bg: "#23363A", border: "#3D6870" }
            MetricBadge { label: "Queued", value: queued.to_string(), bg: "#2E2E3F", border: "#4D4D72" }
            MetricBadge { label: "Failed", value: failed.to_string(), bg: "#44262A", border: "#7A3D48" }
            MetricBadge {
                label: "Workers",
                value: format!("{active_workers}/{}", workers.len()),
                bg: "#2B303B",
                border: "#495264",
            }
        }
    }
}

/// Individual metric badge component.
#[component]
fn MetricBadge(
    label: &'static str,
    value: String,
    bg: &'static str,
    border: &'static str,
) -> Element {
    rsx! {
        div {
            class: "rounded-lg border px-3 py-2",
            style: "background-color: {bg}; border-color: {border};",
            p { class: "text-[10px] uppercase tracking-wide text-gray-400", "{label}" }
            p { class: "text-sm text-white font-semibold", "{value}" }
        }
    }
}
