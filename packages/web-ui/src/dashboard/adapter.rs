use chrono::Utc;

use crate::api::client::{ApiClientError, fetch_dashboard, fetch_dashboard_flake_timelines};
use crate::api::models::{
    BuildQueueSummary, CveSummary, DashboardSummary, DeploymentStatusSummary, FlakeTimeline,
    FleetHealthSummary,
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
            summary: empty_dashboard_summary(),
            notice: None,
            redirect_to_login: true,
        },
        Err(error) => DashboardLoadResult {
            summary: empty_dashboard_summary(),
            notice: Some(format!("Dashboard API unavailable: {error}")),
            redirect_to_login: false,
        },
    }
}

pub fn empty_dashboard_summary() -> DashboardSummary {
    let now = Utc::now();
    DashboardSummary {
        fleet_health: FleetHealthSummary {
            healthy: 0,
            warning: 0,
            critical: 0,
            offline: 0,
        },
        deployment_status: DeploymentStatusSummary {
            up_to_date: 0,
            behind: 0,
            never_deployed: 0,
            unknown: 0,
        },
        cve_summary: CveSummary {
            critical: 0,
            high: 0,
            medium: 0,
            low: 0,
        },
        total_systems: 0,
        active_builds: 0,
        build_queue: Some(BuildQueueSummary {
            building_count: 0,
            queued_count: 0,
            failed_24h_count: 0,
            active_workers: 0,
            total_workers: 0,
            used_slots: 0,
            total_slots: 0,
            items: vec![],
            timestamp: now,
        }),
        cache_health: None,
        recent_deployments: vec![],
        timestamp: now,
    }
}

fn should_redirect_to_login(error: &ApiClientError) -> bool {
    matches!(
        error,
        ApiClientError::Status { code, .. } if *code == 401 || *code == 403
    )
}

/// Load result for flake timelines.
#[derive(Debug, Clone)]
pub struct FlakeTimelinesLoadResult {
    pub timelines: Vec<FlakeTimeline>,
    pub notice: Option<String>,
    pub redirect_to_login: bool,
}

/// Load flake timelines from the API.
///
/// On a non-auth error the production path renders a genuine empty state
/// (no fabricated timelines) plus a notice describing the failure.
pub async fn load_flake_timelines_with_fallback() -> FlakeTimelinesLoadResult {
    match fetch_dashboard_flake_timelines().await {
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
            // Return empty instead of mock data
            FlakeTimelinesLoadResult {
                timelines: vec![],
                notice: Some(format!("Flake timelines API unavailable: {error}")),
                redirect_to_login: false,
            }
        }
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
    fn empty_dashboard_summary_has_no_fabricated_values() {
        let summary = empty_dashboard_summary();
        assert_eq!(summary.total_systems, 0);
        assert_eq!(summary.active_builds, 0);
        assert_eq!(summary.fleet_health.healthy, 0);
        assert_eq!(summary.fleet_health.warning, 0);
        assert_eq!(summary.fleet_health.critical, 0);
        assert_eq!(summary.fleet_health.offline, 0);
        assert_eq!(summary.cve_summary.critical, 0);
        assert_eq!(summary.cve_summary.high, 0);
        assert_eq!(summary.cve_summary.medium, 0);
        assert_eq!(summary.cve_summary.low, 0);
        assert!(summary.recent_deployments.is_empty());
        let queue = summary
            .build_queue
            .expect("empty summary has a build queue");
        assert_eq!(queue.building_count, 0);
        assert_eq!(queue.queued_count, 0);
        assert!(queue.items.is_empty());
    }
}
