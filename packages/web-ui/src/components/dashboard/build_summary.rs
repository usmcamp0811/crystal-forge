//! Build summary panel component.

use dioxus::prelude::*;

use crate::api::models::{BuildQueueSummary, BuildStatus};
use crate::components::charts::{DonutChartWithLegend, DonutSegment};

/// Build summary panel showing active builds with donut chart.
#[component]
pub fn BuildSummaryPanel(
    queue: BuildQueueSummary,
    #[props(default)] flake_filter: Option<String>,
) -> Element {
    let building = queue.building_count;
    let queued = queue.queued_count;
    let total = (building + queued).max(1) as f64;

    let building_systems: Vec<String> = queue
        .items
        .iter()
        .filter(|item| item.status == BuildStatus::Building)
        .map(build_summary_label)
        .collect();
    let queued_systems: Vec<String> = queue
        .items
        .iter()
        .filter(|item| item.status == BuildStatus::Queued)
        .map(build_summary_label)
        .collect();
    let _ = flake_filter;

    let segments = vec![
        DonutSegment {
            percent: building as f64 / total * 100.0,
            color: "#42ff65",
            label: "Building",
            count: building,
            systems: if building_systems.is_empty() {
                vec!["No active builds".to_string()]
            } else {
                building_systems
            },
        },
        DonutSegment {
            percent: queued as f64 / total * 100.0,
            color: "#e57c00",
            label: "Queued",
            count: queued,
            systems: if queued_systems.is_empty() {
                vec!["No queued builds".to_string()]
            } else {
                queued_systems
            },
        },
    ];

    rsx! {
        div {
            class: "h-full flex flex-col",
            "data-testid": "build-summary-panel",

            div {
                class: "flex-1",
                DonutChartWithLegend {
                    segments: segments,
                    center_value: building + queued,
                    center_label: "BUILDS"
                }
            }
        }
    }
}

fn build_summary_label(item: &crate::api::models::BuildQueueItem) -> String {
    // flake_name from the API is already the short name (f.name column).
    // Format: "<flake name> - <hostname>"
    let flake = item.flake_name.trim();
    let host = item.hostname.trim();

    match (flake.is_empty(), host.is_empty()) {
        (true, _) => host.to_string(),
        (false, true) => flake.to_string(),
        (false, false) => format!("{flake} - {host}"),
    }
}

#[cfg(test)]
mod tests {
    use super::build_summary_label;
    use crate::api::models::{BuildQueueItem, BuildStatus};
    use chrono::Utc;
    use uuid::Uuid;

    fn sample_item(flake_name: &str, hostname: &str) -> BuildQueueItem {
        BuildQueueItem {
            job_id: Some(Uuid::nil()),
            system_id: Some(Uuid::nil()),
            hostname: hostname.to_string(),
            flake_name: flake_name.to_string(),
            commit_hash: "abcdef123456".to_string(),
            commit_message: Some("msg".to_string()),
            status: BuildStatus::Queued,
            builder_name: Some("builder".to_string()),
            queued_at: Utc::now(),
            started_at: None,
            elapsed_secs: None,
            logs: None,
            environment: Some("prod".to_string()),
        }
    }

    #[test]
    fn build_summary_label_formats_flake_and_hostname() {
        let label = build_summary_label(&sample_item("fmf-flake", "reckless"));
        assert_eq!(label, "fmf-flake - reckless");
    }

    #[test]
    fn build_summary_label_fallback_to_host_when_no_flake() {
        let label = build_summary_label(&sample_item("", "reckless"));
        assert_eq!(label, "reckless");
    }

    #[test]
    fn build_summary_label_fallback_to_flake_when_no_host() {
        let label = build_summary_label(&sample_item("fmf-flake", ""));
        assert_eq!(label, "fmf-flake");
    }
}
