//! Dashboard-related database queries.
//!
//! All SQL for the dashboard summary endpoint lives here, keeping the
//! handler layer free of raw queries.

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::api::models::{
    CveSummary, DeploymentStatus, DeploymentStatusSummary, FleetHealthSummary, RecentDeployment,
};

/// Query `view_fleet_health_status` for system counts by health category.
pub async fn fetch_fleet_health(pool: &PgPool) -> Result<FleetHealthSummary> {
    let rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT health_status, count FROM view_fleet_health_status",
    )
    .fetch_all(pool)
    .await?;

    let mut summary = FleetHealthSummary {
        healthy: 0,
        warning: 0,
        critical: 0,
        offline: 0,
    };

    for (status, count) in rows {
        match status.as_str() {
            "Healthy" => summary.healthy = count,
            "Warning" => summary.warning = count,
            "Critical" => summary.critical = count,
            "Offline" => summary.offline = count,
            _ => {} // Ignore unexpected values
        }
    }

    Ok(summary)
}

/// Query `view_deployment_status` for system counts by deployment category.
pub async fn fetch_deployment_status(pool: &PgPool) -> Result<DeploymentStatusSummary> {
    let rows = sqlx::query_as::<_, (i64, String)>(
        "SELECT count, status_display FROM view_deployment_status",
    )
    .fetch_all(pool)
    .await?;

    let mut summary = DeploymentStatusSummary {
        up_to_date: 0,
        behind: 0,
        never_deployed: 0,
        unknown: 0,
    };

    for (count, status) in rows {
        match status.as_str() {
            "Up to Date" => summary.up_to_date = count,
            "Behind" => summary.behind = count,
            "No Deployment" | "Never Seen" => summary.never_deployed += count,
            "Unknown" | "Evaluation Failed" | "No Evaluation" => summary.unknown += count,
            _ => summary.unknown += count,
        }
    }

    Ok(summary)
}

/// Aggregate CVE counts from `view_systems_cve_summary` across all systems.
pub async fn fetch_cve_summary(pool: &PgPool) -> Result<CveSummary> {
    let row = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        "SELECT \
            COALESCE(SUM(critical_cves), 0), \
            COALESCE(SUM(high_cves), 0), \
            COALESCE(SUM(medium_cves), 0), \
            COALESCE(SUM(low_cves), 0) \
         FROM view_systems_cve_summary",
    )
    .fetch_one(pool)
    .await?;

    Ok(CveSummary {
        critical: row.0,
        high: row.1,
        medium: row.2,
        low: row.3,
    })
}

/// Count total registered systems.
pub async fn fetch_total_systems(pool: &PgPool) -> Result<i64> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM systems")
        .fetch_one(pool)
        .await?;
    Ok(count.0)
}

/// Count active (non-terminal) builds.
pub async fn fetch_active_builds(pool: &PgPool) -> Result<i64> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM derivations d \
         JOIN derivation_statuses ds ON d.status_id = ds.id \
         WHERE ds.is_terminal = false",
    )
    .fetch_one(pool)
    .await?;
    Ok(count.0)
}

/// Fetch recent deployments from `view_system_deployment_status`.
///
/// Returns the 10 most recent deployment events (systems that have a
/// `deployment_time` and `current_commit_hash`).
pub async fn fetch_recent_deployments(pool: &PgPool) -> Result<Vec<RecentDeployment>> {
    let rows = sqlx::query_as::<_, (String, String, DateTime<Utc>, String)>(
        "SELECT hostname, \
                COALESCE(current_commit_hash, ''), \
                deployment_time, \
                deployment_status \
         FROM view_system_deployment_status \
         WHERE deployment_time IS NOT NULL \
         ORDER BY deployment_time DESC \
         LIMIT 10",
    )
    .fetch_all(pool)
    .await?;

    let deployments = rows
        .into_iter()
        .map(|(hostname, commit_hash, deployed_at, status)| {
            let deployment_status = match status.as_str() {
                "up_to_date" => DeploymentStatus::UpToDate,
                "behind" => DeploymentStatus::Behind,
                "ahead" => DeploymentStatus::Ahead,
                "no_deployment" => DeploymentStatus::NeverDeployed,
                _ => DeploymentStatus::Unknown,
            };
            RecentDeployment {
                hostname,
                commit_hash,
                deployed_at,
                status: deployment_status,
            }
        })
        .collect();

    Ok(deployments)
}
