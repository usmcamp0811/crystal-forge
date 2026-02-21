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
    // When a flake is selected, we would filter the health data
    // For now, we show a filtered label and adjust the mock data slightly
    let (display_health, filter_label) = if let Some(ref flake_name) = flake_filter {
        // Simulate filtered data (in real app, this would come from API)
        let filtered = FleetHealthSummary {
            healthy: health.healthy / 3,
            warning: health.warning / 3,
            critical: health.critical.min(1),
            offline: health.offline.min(1),
        };
        (filtered, Some(flake_name.clone()))
    } else {
        (health.clone(), None)
    };

    let total = display_health.total().max(1) as f64;
    let total_count = display_health.total();

    // Mock system lists for each category
    let healthy_systems: Vec<String> = (1..=display_health.healthy.min(50))
        .map(|i| format!("server-{:02}", i))
        .collect();
    let warning_systems: Vec<String> = if flake_filter.is_some() {
        vec!["db-replica-01".into(), "cache-02".into()]
    } else {
        vec![
            "db-replica-01".into(),
            "cache-02".into(),
            "worker-07".into(),
            "api-staging".into(),
            "monitor-01".into(),
            "backup-srv".into(),
            "dev-box".into(),
        ]
    };
    let critical_systems: Vec<String> = vec!["db-primary".into()];
    let offline_systems: Vec<String> = vec!["legacy-app".into()];

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
                    span { "{flake_name}" }
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
