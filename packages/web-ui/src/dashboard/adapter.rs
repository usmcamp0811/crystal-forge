use chrono::{Duration, TimeZone, Utc};

use crate::api::client::{ApiClientError, fetch_dashboard, fetch_flake_timelines};
use crate::api::models::{
    BuildQueueItem, BuildQueueSummary, BuildStatus, CveSummary, DashboardSummary, DeploymentStatus,
    DeploymentStatusSummary, FleetHealthSummary, FlakeTimeline, RecentDeployment,
};

#[derive(Debug, Clone)]
pub struct DashboardLoadResult {
    pub summary: DashboardSummary,
    pub notice: Option<String>,
    pub redirect_to_login: bool,
}

pub async fn load_dashboard_with_fallback() -> DashboardLoadResult {
    match fetch_dashboard().await {
        Ok(summary) => DashboardLoadResult {
            summary,
            notice: None,
            redirect_to_login: false,
        },
        Err(error) if should_redirect_to_login(&error) => DashboardLoadResult {
            summary: fallback_dashboard_summary(),
            notice: None,
            redirect_to_login: true,
        },
        Err(error) => DashboardLoadResult {
            summary: fallback_dashboard_summary(),
            notice: Some(format!(
                "Dashboard API unavailable, using deterministic fallback data: {error}"
            )),
            redirect_to_login: false,
        },
    }
}

pub fn fallback_dashboard_summary() -> DashboardSummary {
    let now = deterministic_mock_timestamp();
    let build_queue = fallback_build_queue_summary(now);

    DashboardSummary {
        fleet_health: FleetHealthSummary {
            healthy: 17,
            warning: 2,
            critical: 0,
            offline: 2,
        },
        deployment_status: DeploymentStatusSummary {
            up_to_date: 7,
            behind: 0,
            never_deployed: 12,
            unknown: 2,
        },
        cve_summary: CveSummary {
            critical: 5,
            high: 23,
            medium: 67,
            low: 142,
        },
        total_systems: 21,
        active_builds: build_queue.building_count,
        build_queue: Some(build_queue),
        recent_deployments: vec![
            RecentDeployment {
                hostname: "atlas-01".to_string(),
                commit_hash: "a1b2c3d4e5f6789".to_string(),
                commit_message: Some("fix: update nginx config for TLS 1.3".to_string()),
                deployed_at: now - Duration::minutes(15),
                status: DeploymentStatus::UpToDate,
            },
            RecentDeployment {
                hostname: "nova-05".to_string(),
                commit_hash: "f9e8d7c6b5a4321".to_string(),
                commit_message: Some("feat: add prometheus metrics endpoint".to_string()),
                deployed_at: now - Duration::hours(2),
                status: DeploymentStatus::UpToDate,
            },
            RecentDeployment {
                hostname: "luna-02".to_string(),
                commit_hash: "1234567890abcdef".to_string(),
                commit_message: Some("chore: bump nixpkgs to 24.11".to_string()),
                deployed_at: now - Duration::hours(5),
                status: DeploymentStatus::Behind,
            },
            RecentDeployment {
                hostname: "orion-03".to_string(),
                commit_hash: "deadbeefcafe1234".to_string(),
                commit_message: Some("refactor: migrate to systemd hardening options".to_string()),
                deployed_at: now - Duration::days(1),
                status: DeploymentStatus::UpToDate,
            },
            RecentDeployment {
                hostname: "vega-04".to_string(),
                commit_hash: "cafe1234deadbeef".to_string(),
                commit_message: Some("fix: resolve CVE-2024-1234 in openssl".to_string()),
                deployed_at: now - Duration::days(2),
                status: DeploymentStatus::Behind,
            },
        ],
        timestamp: now,
    }
}

pub fn deterministic_mock_timestamp() -> chrono::DateTime<chrono::Utc> {
    Utc.with_ymd_and_hms(2025, 1, 15, 12, 0, 0)
        .single()
        .unwrap_or_else(Utc::now)
}

fn should_redirect_to_login(error: &ApiClientError) -> bool {
    matches!(
        error,
        ApiClientError::Status {
            code: 401 | 403,
            ..
        }
    )
}

/// Load result for flake timelines.
#[derive(Debug, Clone)]
pub struct FlakeTimelinesLoadResult {
    pub timelines: Vec<FlakeTimeline>,
    pub notice: Option<String>,
    pub redirect_to_login: bool,
}

/// Load flake timelines from API with fallback to mock data.
pub async fn load_flake_timelines_with_fallback() -> FlakeTimelinesLoadResult {
    match fetch_flake_timelines().await {
        Ok(timelines) => FlakeTimelinesLoadResult {
            timelines,
            notice: None,
            redirect_to_login: false,
        },
        Err(error) if should_redirect_to_login(&error) => FlakeTimelinesLoadResult {
            timelines: vec![],
            notice: None,
            redirect_to_login: true,
        },
        Err(error) => {
            // Fall back to mock data if API is unavailable
            FlakeTimelinesLoadResult {
                timelines: crate::views::dashboard::mock_flake_timelines(),
                notice: Some(format!(
                    "Flake timelines API unavailable, using mock data: {error}"
                )),
                redirect_to_login: false,
            }
        }
    }
}

pub fn fallback_build_queue_summary(now: chrono::DateTime<chrono::Utc>) -> BuildQueueSummary {
    let items = vec![
        BuildQueueItem {
            hostname: "atlas-02".to_string(),
            flake_name: "infrastructure".to_string(),
            commit_hash: "a1b2c3d".to_string(),
            commit_message: Some("feat: add monitoring stack".to_string()),
            status: BuildStatus::Building,
            queued_at: now - Duration::minutes(14),
            started_at: Some(now - Duration::minutes(9)),
            elapsed_secs: Some(9 * 60),
        },
        BuildQueueItem {
            hostname: "ws-009".to_string(),
            flake_name: "workstations".to_string(),
            commit_hash: "a2b3c4d".to_string(),
            commit_message: Some("fix: bluetooth audio".to_string()),
            status: BuildStatus::Queued,
            queued_at: now - Duration::minutes(6),
            started_at: None,
            elapsed_secs: None,
        },
        BuildQueueItem {
            hostname: "edge-us-west".to_string(),
            flake_name: "edge-nodes".to_string(),
            commit_hash: "1234567".to_string(),
            commit_message: Some("fix: wireguard tunnel".to_string()),
            status: BuildStatus::Queued,
            queued_at: now - Duration::minutes(22),
            started_at: None,
            elapsed_secs: None,
        },
        BuildQueueItem {
            hostname: "luna-01".to_string(),
            flake_name: "infrastructure".to_string(),
            commit_hash: "b2c3d4e".to_string(),
            commit_message: Some("fix: nginx config reload".to_string()),
            status: BuildStatus::Queued,
            queued_at: now - Duration::minutes(3),
            started_at: None,
            elapsed_secs: None,
        },
    ];

    let building_count = items
        .iter()
        .filter(|item| item.status == BuildStatus::Building)
        .count() as i64;
    let queued_count = items
        .iter()
        .filter(|item| item.status == BuildStatus::Queued)
        .count() as i64;

    BuildQueueSummary {
        building_count,
        queued_count,
        items,
        timestamp: now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirects_to_login_for_auth_errors() {
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
    fn does_not_redirect_for_server_or_network_errors() {
        assert!(!should_redirect_to_login(&ApiClientError::Status {
            code: 500,
            body: "boom".to_string(),
        }));
        assert!(!should_redirect_to_login(&ApiClientError::Network(
            "offline".to_string()
        )));
    }

    #[test]
    fn deterministic_fallback_timestamp_is_stable() {
        let a = deterministic_mock_timestamp();
        let b = deterministic_mock_timestamp();
        assert_eq!(a, b);
    }
}
