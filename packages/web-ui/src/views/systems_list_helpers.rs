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

pub fn matches_environment(system: &SystemSummary, filters: &[String]) -> bool {
    if filters.is_empty() {
        return true;
    }
    system
        .environment
        .as_deref()
        .is_some_and(|env| filters.iter().any(|f| env.eq_ignore_ascii_case(f)))
}

pub fn matches_health(system: &SystemSummary, filters: &[HealthStatus]) -> bool {
    filters.is_empty() || filters.contains(&system.health_status)
}

pub fn matches_deployment(system: &SystemSummary, filters: &[DeploymentStatus]) -> bool {
    filters.is_empty() || filters.contains(&system.deployment_status)
}

pub fn matches_search(system: &SystemSummary, search: &str) -> bool {
    if search.is_empty() {
        return true;
    }
    system
        .hostname
        .to_lowercase()
        .contains(&search.to_lowercase())
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
    fn matches_environment_allows_when_filters_empty() {
        let system = sample_system(Some("production"));
        assert!(matches_environment(&system, &[]));
    }

    #[test]
    fn matches_environment_is_case_insensitive() {
        let system = sample_system(Some("Production"));
        assert!(matches_environment(&system, &["production".to_string()]));
        assert!(matches_environment(&system, &["PRODUCTION".to_string()]));
    }

    #[test]
    fn matches_environment_rejects_non_member_environment() {
        let system = sample_system(Some("staging"));
        assert!(!matches_environment(&system, &["production".to_string()]));
    }

    #[test]
    fn matches_environment_rejects_unscoped_system_when_filtering() {
        let system = sample_system(None);
        assert!(!matches_environment(&system, &["production".to_string()]));
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
