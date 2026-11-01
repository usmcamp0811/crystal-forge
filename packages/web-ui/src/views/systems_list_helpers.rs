use crate::api::models::{
    CveSummary, DeploymentStatus, HealthStatus, ManagementMode, PipelineStage, SystemSummary,
};
use crate::components::filters::ViewMode;
use crate::components::system::management_mode::EvidenceStatus;
use chrono::Utc;
use dioxus::prelude::*;
use uuid::Uuid;

/// Identifies actionable configuration failures without treating unknown runtime as outage.
pub fn evidence_issue(status: Option<EvidenceStatus>) -> bool {
    matches!(
        status,
        Some(
            EvidenceStatus::ConfigurationMissing
                | EvidenceStatus::EvaluationFailed
                | EvidenceStatus::BuildFailed
                | EvidenceStatus::Stale
                | EvidenceStatus::Failed
                | EvidenceStatus::Unavailable
        )
    )
}

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

/// Matches managed operational status; nonmanaged resources have no heartbeat health.
pub fn matches_status(system: &SystemSummary, filter: &str) -> bool {
    if filter == "all" {
        return true;
    }
    if system.management_mode != ManagementMode::Managed {
        return false;
    }
    match filter {
        "all" => true,
        "online" => !matches!(
            system.health_status,
            HealthStatus::Offline | HealthStatus::Unknown
        ),
        "warning" => {
            system.health_status == HealthStatus::Warning
                || system.deployment_status == DeploymentStatus::Behind
        }
        "attention" => {
            matches!(
                system.health_status,
                HealthStatus::Warning | HealthStatus::Critical | HealthStatus::Offline
            ) || system.deployment_status == DeploymentStatus::Behind
        }
        "evidence" => false,
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

/// Searches persisted identity, configuration, FQDN, flake, and revision.
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
    if [
        system.system_configuration_name.as_deref(),
        system.fqdn.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|value| value.to_lowercase().contains(&query))
    {
        return true;
    }
    if flake_name.is_some_and(|name| name.to_lowercase().contains(&query)) {
        return true;
    }
    commit.is_some_and(|hash| hash.to_lowercase().contains(&query))
}

/// Searches only the associated parent's visible registry identity.
pub fn matches_parent_search(
    system: &SystemSummary,
    systems: &[SystemSummary],
    search: &str,
) -> bool {
    let query = search.trim().to_lowercase();
    system
        .parent_host_id
        .and_then(|id| systems.iter().find(|parent| parent.id == id))
        .is_some_and(|parent| parent.hostname.to_lowercase().contains(&query))
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
        .filter(|system| {
            system.management_mode == ManagementMode::Managed && system.last_seen.is_none()
        })
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
            tags: Vec::new(),
            description: None,
            management_mode: Default::default(),
            parent_host_id: None,
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
            fqdn: None,
            heartbeat_interval_secs: None,
            effective_heartbeat_interval_secs: 600,
            boot_id: None,
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
    fn unknown_health_is_neither_online_nor_an_outage() {
        let mut system = sample_system(Some("production"));
        system.health_status = HealthStatus::Unknown;
        for mode in [
            ManagementMode::Managed,
            ManagementMode::External,
            ManagementMode::Reference,
        ] {
            system.management_mode = mode;
            assert!(matches_status(&system, "all"));
            for filter in ["online", "offline", "critical", "warning", "attention"] {
                assert!(!matches_status(&system, filter));
            }
            let stats =
                crate::components::systems_stat_strip::SystemsStats::from_systems(
                    &[system.clone()],
                );
            assert_eq!(
                (stats.healthy, stats.warning, stats.critical, stats.offline),
                (0, 0, 0, 0)
            );
        }
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
        assert!(!matches_search(
            &system,
            "nomatch",
            Some("infra"),
            Some("ff")
        ));
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

    #[test]
    fn nonmanaged_resources_never_enter_operational_filters_or_denominators() {
        let managed = sample_system(Some("production"));
        let mut external = managed.clone();
        external.management_mode = ManagementMode::External;
        external.health_status = HealthStatus::Offline;
        external.last_seen = None;
        external.cve_counts.critical = 88;
        let mut reference = external.clone();
        reference.management_mode = ManagementMode::Reference;
        reference.health_status = HealthStatus::Critical;
        for resource in [&external, &reference] {
            for filter in ["online", "warning", "critical", "offline", "attention"] {
                assert!(!matches_status(resource, filter));
            }
            assert!(matches_status(resource, "all"));
        }
        let resources = vec![managed, external, reference];
        let stats = crate::components::systems_stat_strip::SystemsStats::from_systems(&resources);
        assert_eq!(
            (stats.total, stats.healthy, stats.critical, stats.offline),
            (1, 1, 0, 0)
        );
        assert_eq!(stats.critical_cves, 0);
        assert_eq!(systems_missing_heartbeat_count(&resources), 0);
    }

    #[test]
    fn configuration_and_fqdn_are_searchable_without_changing_other_filters() {
        let mut system = sample_system(Some("production"));
        system.system_configuration_name = Some("Golden-Workstation".into());
        system.fqdn = Some("sample.example.org".into());
        assert!(matches_search(&system, "  GOLDEN  ", None, None));
        assert!(matches_search(&system, "example.org", None, None));
        assert!(!matches_environment(&system, "staging"));
    }

    #[test]
    fn attention_requires_an_actual_evidence_problem() {
        use EvidenceStatus::*;
        for status in [
            ConfigurationMissing,
            EvaluationFailed,
            BuildFailed,
            Stale,
            Failed,
            Unavailable,
        ] {
            assert!(evidence_issue(Some(status)));
        }
        for status in [
            Missing,
            EvaluationUnavailable,
            Current,
            FreshnessUnknown,
            Scanning,
            AwaitingBuild,
        ] {
            assert!(!evidence_issue(Some(status)));
        }
        assert!(!evidence_issue(None));
    }

    #[test]
    fn parent_search_resolves_association_not_an_unrelated_host() {
        let mut guest = sample_system(Some("production"));
        guest.id = Uuid::from_u128(10);
        let mut parent = sample_system(Some("production"));
        parent.hostname = "Hypervisor-Alpha".into();
        guest.parent_host_id = Some(parent.id);
        assert!(matches_parent_search(
            &guest,
            &[parent.clone()],
            " hypervisor "
        ));
        guest.parent_host_id = Some(Uuid::from_u128(99));
        assert!(!matches_parent_search(&guest, &[parent], "hypervisor"));
    }
}
