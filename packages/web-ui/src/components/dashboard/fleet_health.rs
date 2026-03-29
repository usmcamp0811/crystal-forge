//! Fleet health breakdown component with donut chart.

use dioxus::prelude::*;

use crate::api::models::FleetHealthSummary;
use crate::components::charts::{DonutChartWithLegend, DonutSegment};
use crate::theme;

/// Fleet health breakdown with colored donut chart.
#[component]
pub fn FleetHealthBreakdown(
    health: FleetHealthSummary,
    #[props(default)] flake_filter: Option<String>,
) -> Element {
    let display_health = health;
    let filter_label = flake_filter;

    let total = display_health.total().max(1) as f64;
    let total_count = display_health.total();

    let healthy_systems = status_summary_entries(display_health.healthy, "healthy");
    let warning_systems = status_summary_entries(display_health.warning, "warning");
    let critical_systems = status_summary_entries(display_health.critical, "critical");
    let offline_systems = status_summary_entries(display_health.offline, "offline");

    let segments = vec![
        DonutSegment {
            percent: display_health.healthy as f64 / total * 100.0,
            color: "#10b981",
            label: "Healthy",
            count: display_health.healthy,
            systems: healthy_systems,
        },
        DonutSegment {
            percent: display_health.warning as f64 / total * 100.0,
            color: "#f59e0b",
            label: "Warning",
            count: display_health.warning,
            systems: warning_systems,
        },
        DonutSegment {
            percent: display_health.critical as f64 / total * 100.0,
            color: "#ef4444",
            label: "Critical",
            count: display_health.critical,
            systems: critical_systems,
        },
        DonutSegment {
            percent: display_health.offline as f64 / total * 100.0,
            color: "#6b7280",
            label: "Offline",
            count: display_health.offline,
            systems: offline_systems,
        },
    ];

    rsx! {
        div {
            class: "h-full flex flex-col",
            "data-testid": "fleet-health-breakdown",

            // Show filter indicator if filtered
            if let Some(flake_name) = filter_label {
                div {
                    class: "text-xs text-blue-400 mb-1 flex items-center gap-1",
                    svg {
                        class: "w-3 h-3",
                        fill: "none",
                        stroke: "currentColor",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            stroke_width: "2",
                            d: "M3 4a1 1 0 011-1h16a1 1 0 011 1v2.586a1 1 0 01-.293.707l-6.414 6.414a1 1 0 00-.293.707V17l-4 4v-6.586a1 1 0 00-.293-.707L3.293 7.293A1 1 0 013 6.586V4z"
                        }
                    }
                    span { "{flake_name} (global fleet health)" }
                }
            }

            div {
                class: "flex-1",
                DonutChartWithLegend {
                    segments: segments,
                    center_value: total_count,
                    center_label: "SYSTEMS"
                }
            }
        }
    }
}

fn status_summary_entries(count: i64, status: &str) -> Vec<String> {
    if count > 0 {
        vec![format!("{count} systems currently {status}")]
    } else {
        vec![format!("No systems currently {status}")]
    }
}

#[cfg(test)]
mod tests {
    use super::status_summary_entries;

    #[test]
    fn status_summary_entries_reports_count_when_present() {
        let entries = status_summary_entries(3, "warning");
        assert_eq!(entries, vec!["3 systems currently warning"]);
    }

    #[test]
    fn status_summary_entries_reports_empty_state() {
        let entries = status_summary_entries(0, "offline");
        assert_eq!(entries, vec!["No systems currently offline"]);
    }
}
