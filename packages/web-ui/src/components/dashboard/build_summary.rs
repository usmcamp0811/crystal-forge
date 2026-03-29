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
                    center_value: building + queued,
                    center_label: "BUILDS"
                }
            }
        }
    }
}

fn build_summary_label(item: &crate::api::models::BuildQueueItem) -> String {
    let flake = compact_flake_name(&item.flake_name);
    if flake.is_empty() {
        return item.hostname.clone();
    }

    format!("{flake} - {}", item.hostname)
}

fn compact_flake_name(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }

    let without_git = trimmed.trim_end_matches(".git");
    let tail = without_git
        .rsplit(['/', ':'])
        .next()
        .unwrap_or(without_git)
        .trim();

    if tail.is_empty() {
        without_git.to_string()
    } else {
        tail.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{build_summary_label, compact_flake_name};
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
    fn compact_flake_name_reduces_url_like_values() {
        assert_eq!(
            compact_flake_name("https://gitlab.com/crystal-forge/fmf-flake.git"),
            "fmf-flake"
        );
        assert_eq!(compact_flake_name("github:org/repo"), "repo");
    }

    #[test]
    fn build_summary_label_formats_flake_and_hostname() {
        let label = build_summary_label(&sample_item("github:org/repo", "reckless"));
        assert_eq!(label, "repo - reckless");
    }
}
