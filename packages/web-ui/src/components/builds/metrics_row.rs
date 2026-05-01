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

    rsx! {
        div {
            class: "flex flex-wrap gap-3",
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
    let _ = tone_class;
    let value_color = match label {
        "Building" => "#60a5fa",
        "Queued" => "#a78bfa",
        "Failed 24h" => "#f87171",
        "Workers" => "#34d399",
        _ => "#22d3ee",
    };

    rsx! {
        div {
            class: "relative rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} px-3 py-3 min-w-[180px] flex-1",
            span {
                class: "absolute left-2 top-3 h-5 w-0.5 rounded-full",
                style: "background-color: {value_color};"
            }
            p { class: "pl-2 text-[10px] uppercase tracking-wide {theme::text::MUTED}", "{label}" }
            p { class: "pl-2 text-base font-semibold", style: "color: {value_color};", "{value}" }
        }
    }
}
