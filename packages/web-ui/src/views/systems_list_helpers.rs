use crate::api::models::{
    CveSummary, DeploymentStatus, HealthStatus, PipelineStage, SystemSummary,
};
use crate::components::filters::ViewMode;
use chrono::Utc;
use dioxus::prelude::*;
use uuid::Uuid;

pub fn remove_system_by_id(
    systems: Signal<Vec<SystemSummary>>,
    mut pending_remove: Signal<Option<SystemSummary>>,
    system_id: Uuid,
) {
    let target = systems
        .read()
        .iter()
        .find(|item| item.id == system_id)
        .cloned();
    if let Some(system) = target {
        pending_remove.set(Some(system));
    }
}

pub fn update_key_for_system(
    systems: Signal<Vec<SystemSummary>>,
    mut pending_update_key: Signal<Option<SystemSummary>>,
    system_id: Uuid,
) {
    let target = systems
        .read()
        .iter()
        .find(|item| item.id == system_id)
        .cloned();
    if let Some(system) = target {
        pending_update_key.set(Some(system));
    }
}

pub fn normalize_optional(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn normalize_policy(value: &str) -> String {
    let normalized = value.trim().to_lowercase();
    if normalized.is_empty() {
        "manual".to_string()
    } else {
        normalized
    }
}

/// Single-select environment filter matching the design's "All environments" select.
pub fn matches_environment(system: &SystemSummary, filter: &str) -> bool {
    if filter == "all" {
        return true;
    }
    system
        .environment
        .as_deref()
        .is_some_and(|env| env.eq_ignore_ascii_case(filter))
}

/// Single-select status filter matching the design's "All statuses" select.
///
/// Design semantics (CrystalForgelatest `SystemsView`):
/// - `online`: anything that is not offline
/// - `warning`: warning / drift
/// - `critical`: critical
/// - `offline`: offline
pub fn matches_status(system: &SystemSummary, filter: &str) -> bool {
    match filter {
        "all" => true,
        "online" => system.health_status != HealthStatus::Offline,
        "warning" => system.health_status == HealthStatus::Warning,
        "critical" => system.health_status == HealthStatus::Critical,
        "offline" => system.health_status == HealthStatus::Offline,
        _ => true,
    }
}

/// Single-select flake filter matching the design's "All flakes" select.
pub fn matches_flake(flake_name: Option<&str>, filter: &str) -> bool {
    if filter == "all" {
        return true;
    }
    flake_name.is_some_and(|name| name.eq_ignore_ascii_case(filter))
}

/// Search across hostname, flake name, and commit, matching the design's
/// "Filter by hostname, commit, or flake…" placeholder behavior.
pub fn matches_search(
    system: &SystemSummary,
    search: &str,
    flake_name: Option<&str>,
    commit: Option<&str>,
) -> bool {
    let query = search.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    if system.hostname.to_lowercase().contains(&query) {
        return true;
    }
    if flake_name.is_some_and(|name| name.to_lowercase().contains(&query)) {
        return true;
    }
    commit.is_some_and(|hash| hash.to_lowercase().contains(&query))
}

pub fn unique_environments(systems: &[SystemSummary]) -> Vec<String> {
    let mut envs: Vec<String> = systems
        .iter()
        .filter_map(|s| s.environment.clone())
        .collect();
    envs.sort();
    envs.dedup();
    envs
}

pub fn systems_missing_flake_count(systems: &[SystemSummary]) -> usize {
    systems
        .iter()
        .filter(|system| system.flake_id.is_none())
        .count()
}

pub fn systems_missing_heartbeat_count(systems: &[SystemSummary]) -> usize {
    systems
        .iter()
        .filter(|system| system.last_seen.is_none())
        .count()
}

pub fn prefers_view_from_query() -> Option<ViewMode> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_system(environment: Option<&str>) -> SystemSummary {
        SystemSummary {
            id: Uuid::parse_str("00000000-0000-0000-0000-0000000000aa").expect("valid uuid"),
            hostname: "sample-host".to_string(),
            system_configuration_name: None,
            environment: environment.map(ToString::to_string),
            flake_id: Some(1),
            primary_ip: None,
            health_status: HealthStatus::Healthy,
            deployment_status: DeploymentStatus::UpToDate,
            pipeline_stage: Some(PipelineStage::BuildComplete),
            cve_counts: CveSummary {
                critical: 0,
                high: 0,
                medium: 0,
                low: 0,
            },
            nixos_version: Some("24.11".to_string()),
            last_seen: Some(Utc::now()),
            deployment_policy: "manual".to_string(),
        }
    }

    #[test]
    fn matches_environment_allows_all_filter() {
        let system = sample_system(Some("production"));
        assert!(matches_environment(&system, "all"));
    }

    #[test]
    fn matches_environment_is_case_insensitive() {
        let system = sample_system(Some("Production"));
        assert!(matches_environment(&system, "production"));
        assert!(matches_environment(&system, "PRODUCTION"));
    }

    #[test]
    fn matches_environment_rejects_non_member_environment() {
        let system = sample_system(Some("staging"));
        assert!(!matches_environment(&system, "production"));
    }

    #[test]
    fn matches_environment_rejects_unscoped_system_when_filtering() {
        let system = sample_system(None);
        assert!(!matches_environment(&system, "production"));
    }

    #[test]
    fn matches_status_follows_design_buckets() {
        let mut system = sample_system(Some("production"));
        assert!(matches_status(&system, "all"));
        assert!(matches_status(&system, "online"));
        assert!(!matches_status(&system, "warning"));

        system.health_status = HealthStatus::Warning;
        assert!(matches_status(&system, "warning"));
        assert!(matches_status(&system, "online"));

        system.health_status = HealthStatus::Critical;
        assert!(matches_status(&system, "critical"));
        assert!(matches_status(&system, "online"));

        system.health_status = HealthStatus::Offline;
        assert!(matches_status(&system, "offline"));
        assert!(!matches_status(&system, "online"));
    }

    #[test]
    fn matches_flake_uses_resolved_flake_name() {
        assert!(matches_flake(Some("infrastructure"), "all"));
        assert!(matches_flake(Some("Infrastructure"), "infrastructure"));
        assert!(!matches_flake(Some("web-services"), "infrastructure"));
        assert!(!matches_flake(None, "infrastructure"));
    }

    #[test]
    fn matches_search_covers_hostname_flake_and_commit() {
        let system = sample_system(Some("production"));
        assert!(matches_search(&system, "", None, None));
        assert!(matches_search(&system, "sample", None, None));
        assert!(matches_search(
            &system,
            "infra",
            Some("infrastructure"),
            None
        ));
        assert!(matches_search(
            &system,
            "a1b2",
            Some("infrastructure"),
            Some("a1b2c3d4")
        ));
        assert!(!matches_search(&system, "nomatch", Some("infra"), Some("ff")));
    }

    #[test]
    fn counts_systems_missing_flake_links() {
        let mut with_missing_flake = sample_system(Some("production"));
        with_missing_flake.flake_id = None;
        let systems = vec![sample_system(Some("production")), with_missing_flake];
        assert_eq!(systems_missing_flake_count(&systems), 1);
    }

    #[test]
    fn counts_systems_missing_heartbeats() {
        let mut with_missing_heartbeat = sample_system(Some("production"));
        with_missing_heartbeat.last_seen = None;
        let systems = vec![sample_system(Some("production")), with_missing_heartbeat];
        assert_eq!(systems_missing_heartbeat_count(&systems), 1);
    }
}
