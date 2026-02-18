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
    // Apply filter - in real app, this would come from API
    let display_status = if let Some(ref _flake_name) = flake_filter {
        DeploymentStatusSummary {
            up_to_date: status.up_to_date / 3,
            behind: status.behind / 3,
            never_deployed: status.never_deployed.min(1),
            unknown: 0,
        }
    } else {
        status.clone()
    };

    let total = display_status.total().max(1) as f64;
    let total_count = display_status.total();

    // Mock system lists
    let up_to_date_systems: Vec<String> = (1..=display_status.up_to_date.min(50))
        .map(|i| format!("prod-{:02}", i))
        .collect();
    let behind_systems: Vec<String> = if flake_filter.is_some() {
        vec![
            "staging-01".into(),
            "staging-02".into(),
            "dev-server".into(),
            "qa-box".into(),
        ]
    } else {
        vec![
            "staging-01".into(),
            "staging-02".into(),
            "dev-server".into(),
            "qa-box".into(),
            "perf-test".into(),
            "sandbox-01".into(),
            "demo-server".into(),
            "training-vm".into(),
            "backup-01".into(),
            "backup-02".into(),
            "dr-site".into(),
            "edge-node".into(),
        ]
    };
    let never_deployed_systems: Vec<String> = vec!["new-server-01".into()];
    let unknown_systems: Vec<String> = vec![];

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
            if let Some(ref flake_name) = flake_filter {
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
                    span { "{flake_name}" }
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
