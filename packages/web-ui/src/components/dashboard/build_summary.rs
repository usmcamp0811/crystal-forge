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
    let flake = item.flake_name.trim();
    let config = extract_nixos_configuration_name(&item.hostname);

    match (flake.is_empty(), config.is_empty()) {
        (false, false) => format!("{flake} - {config}"),
        (false, true) => flake.to_string(),
        (true, false) => config,
        (true, true) => item.hostname.trim().to_string(),
    }
}

fn extract_nixos_configuration_name(raw: &str) -> String {
    let value = raw.trim();
    if value.is_empty() {
        return String::new();
    }

    if let Some(name) = extract_attr_name(value, "nixosConfigurations.") {
        return name;
    }

    if let Some(hash_pos) = value.find('#') {
        let after_hash = &value[hash_pos + 1..];
        if let Some(name) = extract_attr_name(after_hash, "nixosConfigurations.") {
            return name;
        }
    }

    if !value.contains("://")
        && !value.starts_with("git+")
        && !value.starts_with("github:")
        && !value.starts_with("gitlab:")
    {
        return value.to_string();
    }

    String::new()
}

fn extract_attr_name(source: &str, marker: &str) -> Option<String> {
    let start = source.find(marker)? + marker.len();
    let tail = &source[start..];
    let raw_name: String = tail
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect();
    let name = raw_name.split(".config.").next().unwrap_or("").to_string();

    if name.is_empty() { None } else { Some(name) }
}

#[cfg(test)]
mod tests {
    use super::{build_summary_label, extract_nixos_configuration_name};
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
    fn build_summary_label_formats_flake_and_config_name() {
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

    #[test]
    fn extract_nixos_configuration_name_from_attr_path() {
        let host = "git+https://gitlab.com/crystal-forge/fmf-flake?ref=main#nixosConfigurations.reckless.config.system.build.toplevel";
        assert_eq!(extract_nixos_configuration_name(host), "reckless");
    }
}
