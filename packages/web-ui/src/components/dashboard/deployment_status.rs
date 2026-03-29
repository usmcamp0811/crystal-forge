//! Deployment status breakdown component.

use dioxus::prelude::*;

use crate::api::models::DeploymentStatusSummary;
use crate::components::charts::{DonutChartWithLegend, DonutSegment};

/// Deployment status breakdown with donut chart.
#[component]
pub fn DeploymentStatusBreakdown(
    status: DeploymentStatusSummary,
    #[props(default)] flake_filter: Option<String>,
) -> Element {
    let display_status = status;
    let filter_label = flake_filter;

    let total = display_status.total().max(1) as f64;
    let total_count = display_status.total();

    let up_to_date_systems = status_summary_entries(display_status.up_to_date, "up to date");
    let behind_systems = status_summary_entries(display_status.behind, "behind");
    let never_deployed_systems =
        status_summary_entries(display_status.never_deployed, "never deployed");
    let unknown_systems = status_summary_entries(display_status.unknown, "unknown");

    let segments = vec![
        DonutSegment {
            percent: display_status.up_to_date as f64 / total * 100.0,
            color: "#10b981",
            label: "Up to Date",
            count: display_status.up_to_date,
            systems: up_to_date_systems,
        },
        DonutSegment {
            percent: display_status.behind as f64 / total * 100.0,
            color: "#f59e0b",
            label: "Behind",
            count: display_status.behind,
            systems: behind_systems,
        },
        DonutSegment {
            percent: display_status.never_deployed as f64 / total * 100.0,
            color: "#4b5563",
            label: "Never Deployed",
            count: display_status.never_deployed,
            systems: never_deployed_systems,
        },
        DonutSegment {
            percent: display_status.unknown as f64 / total * 100.0,
            color: "#6b7280",
            label: "Unknown",
            count: display_status.unknown,
            systems: unknown_systems,
        },
    ];

    rsx! {
        div {
            class: "h-full flex flex-col",
            "data-testid": "deployment-status",

            // Show filter indicator if filtered
            if let Some(ref flake_name) = filter_label {
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
                    span { "{flake_name} (global deployment status)" }
                }
            }

            div {
                class: "flex-1",
                DonutChartWithLegend {
                    segments: segments,
                    center_value: total_count,
                    center_label: "DEPLOYED"
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
        let entries = status_summary_entries(2, "never deployed");
        assert_eq!(entries, vec!["2 systems currently never deployed"]);
    }

    #[test]
    fn status_summary_entries_reports_empty_state() {
        let entries = status_summary_entries(0, "behind");
        assert_eq!(entries, vec!["No systems currently behind"]);
    }
}
