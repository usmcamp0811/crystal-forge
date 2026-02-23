//! Systems adapter — API fetch with deterministic fallback.
//!
//! # Behaviour
//!
//! | Outcome               | Result                                      |
//! |-----------------------|---------------------------------------------|
//! | API returns 2xx       | Real data, no notice                        |
//! | API returns 401/403   | `redirect_to_login: true`                   |
//! | API 5xx / network err | Fallback mock data, notice shown            |
//! | Empty list from API   | Empty `items` vec (not fallback)            |
//!
//! Views MUST NOT call [`crate::api::client`] directly.
//! All HTTP interactions go through the functions in this module.

use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::api::client::{ApiClientError, fetch_system, fetch_systems};
use crate::api::models::{
    CveSummary, DeploymentStatus, HealthStatus, PipelineStage, PaginatedResponse, SystemDetail,
    SystemHardwareInfo, SystemNetworkInfo, SystemSecurityInfo, SystemSummary, SystemsListParams,
};
use crate::views::systems_mock::mock_system_detail_by_id;

// ─────────────────────────────────────────────────────────────────────────────
// Result Types
// ─────────────────────────────────────────────────────────────────────────────

/// Result of loading the systems list.
#[derive(Debug, Clone)]
pub struct SystemsLoadResult {
    /// The systems to display. May be real data, fallback mock, or empty.
    pub systems: Vec<SystemSummary>,
    /// Human-readable notice shown when using fallback data.
    pub notice: Option<String>,
    /// True when the API returned 401/403 — view should redirect to login.
    pub redirect_to_login: bool,
}

/// Result of loading a single system's detail.
#[derive(Debug, Clone)]
pub struct SystemDetailLoadResult {
    /// The system detail. May be real data or fallback mock.
    pub system: Option<SystemDetail>,
    /// Human-readable notice shown when using fallback data.
    pub notice: Option<String>,
    /// True when the API returned 401/403 — view should redirect to login.
    pub redirect_to_login: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Public Adapter Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Fetch the systems list from the backend, with fallback to deterministic mock data.
pub async fn load_systems_with_fallback(params: &SystemsListParams) -> SystemsLoadResult {
    match fetch_systems(params).await {
        Ok(PaginatedResponse { items, .. }) => SystemsLoadResult {
            systems: items,
            notice: None,
            redirect_to_login: false,
        },
        Err(error) if should_redirect_to_login(&error) => SystemsLoadResult {
            systems: fallback_systems(),
            notice: None,
            redirect_to_login: true,
        },
        Err(error) => SystemsLoadResult {
            systems: fallback_systems(),
            notice: Some(format!(
                "Systems API unavailable, using deterministic fallback data: {error}"
            )),
            redirect_to_login: false,
        },
    }
}

/// Fetch a single system's detail from the backend, with fallback to deterministic mock data.
pub async fn load_system_detail_with_fallback(id: &str) -> SystemDetailLoadResult {
    let uuid = match Uuid::parse_str(id) {
        Ok(uuid) => uuid,
        Err(_) => {
            // Unparseable ID: try mock lookup first, otherwise return not-found.
            let system = mock_system_detail_by_id(id);
            return SystemDetailLoadResult {
                system,
                notice: Some("System ID is not a valid UUID; showing mock data.".to_string()),
                redirect_to_login: false,
            };
        }
    };

    match fetch_system(&uuid).await {
        Ok(detail) => SystemDetailLoadResult {
            system: Some(detail),
            notice: None,
            redirect_to_login: false,
        },
        Err(error) if should_redirect_to_login(&error) => SystemDetailLoadResult {
            system: mock_system_detail_by_id(id),
            notice: None,
            redirect_to_login: true,
        },
        Err(error) => {
            // 404 or other error: try mock, return not-found if also absent.
            let system = mock_system_detail_by_id(id);
            let notice = if system.is_some() {
                Some(format!(
                    "Systems API unavailable, using deterministic fallback data: {error}"
                ))
            } else {
                None
            };
            SystemDetailLoadResult {
                system,
                notice,
                redirect_to_login: false,
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Fallback Data
// ─────────────────────────────────────────────────────────────────────────────

/// Deterministic fallback list of systems used when the API is unavailable.
///
/// Timestamps are anchored to a fixed point so the data is stable across renders.
pub fn fallback_systems() -> Vec<SystemSummary> {
    let now = deterministic_fallback_timestamp();
    vec![
        SystemSummary {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            hostname: "atlas-01".to_string(),
            environment: Some("production".to_string()),
            primary_ip: Some("10.0.1.10".to_string()),
            health_status: HealthStatus::Healthy,
            deployment_status: DeploymentStatus::UpToDate,
            pipeline_stage: Some(PipelineStage::BuildComplete),
            cve_counts: CveSummary { critical: 0, high: 2, medium: 5, low: 12 },
            nixos_version: Some("24.11".to_string()),
            last_seen: Some(now - Duration::minutes(5)),
            deployment_policy: "auto_latest".to_string(),
        },
        SystemSummary {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap(),
            hostname: "atlas-02".to_string(),
            environment: Some("production".to_string()),
            primary_ip: Some("10.0.1.11".to_string()),
            health_status: HealthStatus::Healthy,
            deployment_status: DeploymentStatus::Behind,
            pipeline_stage: Some(PipelineStage::ReadyForDeploy),
            cve_counts: CveSummary { critical: 1, high: 3, medium: 8, low: 15 },
            nixos_version: Some("24.11".to_string()),
            last_seen: Some(now - Duration::minutes(10)),
            deployment_policy: "manual".to_string(),
        },
        SystemSummary {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000003").unwrap(),
            hostname: "staging-01".to_string(),
            environment: Some("staging".to_string()),
            primary_ip: Some("10.0.2.10".to_string()),
            health_status: HealthStatus::Warning,
            deployment_status: DeploymentStatus::Behind,
            pipeline_stage: Some(PipelineStage::ReadyForDeploy),
            cve_counts: CveSummary { critical: 0, high: 0, medium: 2, low: 5 },
            nixos_version: Some("24.11".to_string()),
            last_seen: Some(now - Duration::hours(1)),
            deployment_policy: "manual".to_string(),
        },
        SystemSummary {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000004").unwrap(),
            hostname: "dev-box".to_string(),
            environment: Some("development".to_string()),
            primary_ip: Some("10.0.3.20".to_string()),
            health_status: HealthStatus::Offline,
            deployment_status: DeploymentStatus::NeverDeployed,
            pipeline_stage: Some(PipelineStage::ReadyForBuild),
            cve_counts: CveSummary { critical: 0, high: 0, medium: 0, low: 0 },
            nixos_version: None,
            last_seen: Some(now - Duration::days(3)),
            deployment_policy: "manual".to_string(),
        },
    ]
}

/// Fallback system detail used when no real data and no mock for the given ID.
pub fn fallback_system_detail() -> SystemDetail {
    let now = deterministic_fallback_timestamp();
    SystemDetail {
        id: Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap(),
        hostname: "unknown-system".to_string(),
        environment: None,
        is_active: false,
        deployment_policy: "manual".to_string(),
        health_status: HealthStatus::Offline,
        deployment_status: DeploymentStatus::Unknown,
        pipeline_stage: Some(PipelineStage::Unknown),
        nixos_version: None,
        kernel: None,
        agent_version: None,
        current_store_path: None,
        hardware: SystemHardwareInfo {
            cpu_brand: None,
            cpu_cores: None,
            memory_gb: None,
            uptime_secs: None,
            board_serial: None,
            bios_version: None,
        },
        network: SystemNetworkInfo {
            primary_ip: None,
            primary_mac: None,
            gateway_ip: None,
        },
        security: SystemSecurityInfo {
            tpm_present: None,
            secure_boot_enabled: None,
            fips_mode: None,
            selinux_status: None,
        },
        cve_counts: CveSummary { critical: 0, high: 0, medium: 0, low: 0 },
        flake: None,
        last_seen: None,
        created_at: now,
        updated_at: now,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn should_redirect_to_login(error: &ApiClientError) -> bool {
    matches!(
        error,
        ApiClientError::Status { code: 401 | 403, .. }
    )
}

/// Fixed timestamp used for deterministic fallback data so renders are stable.
fn deterministic_fallback_timestamp() -> chrono::DateTime<Utc> {
    use chrono::TimeZone;
    Utc.with_ymd_and_hms(2025, 1, 15, 12, 0, 0)
        .single()
        .unwrap_or_else(Utc::now)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_redirect_for_auth_errors() {
        assert!(should_redirect_to_login(&ApiClientError::Status {
            code: 401,
            body: "unauthorized".to_string(),
        }));
        assert!(should_redirect_to_login(&ApiClientError::Status {
            code: 403,
            body: "forbidden".to_string(),
        }));
    }

    #[test]
    fn should_not_redirect_for_server_or_network_errors() {
        assert!(!should_redirect_to_login(&ApiClientError::Status {
            code: 500,
            body: "internal server error".to_string(),
        }));
        assert!(!should_redirect_to_login(&ApiClientError::Network(
            "connection refused".to_string()
        )));
    }

    #[test]
    fn fallback_systems_is_deterministic() {
        let a = fallback_systems();
        let b = fallback_systems();
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.id, y.id);
            assert_eq!(x.hostname, y.hostname);
            assert_eq!(x.last_seen, y.last_seen);
        }
    }

    #[test]
    fn fallback_systems_has_expected_entries() {
        let systems = fallback_systems();
        assert_eq!(systems.len(), 4);
        assert_eq!(systems[0].hostname, "atlas-01");
        assert_eq!(systems[1].hostname, "atlas-02");
        assert_eq!(systems[2].hostname, "staging-01");
        assert_eq!(systems[3].hostname, "dev-box");
    }
}
