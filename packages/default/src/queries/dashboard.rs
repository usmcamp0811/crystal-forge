//! Dashboard-related database queries.
//!
//! All SQL for the dashboard summary endpoint lives here, keeping the
//! handler layer free of raw queries.

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::api::models::{
    BuildQueueItem, BuildQueueSummary, BuildStatus, CveSummary, DeploymentStatus,
    DeploymentStatusSummary, FleetHealthSummary, RecentDeployment,
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

/// Fetch the active build queue (building + queued) from build_jobs.
pub async fn fetch_build_queue(pool: &PgPool, limit: i64) -> Result<BuildQueueSummary> {
    let rows = sqlx::query_as::<
        _,
        (
            Option<Uuid>,
            Option<Uuid>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            Option<String>,
            DateTime<Utc>,
            Option<DateTime<Utc>>,
            Option<i64>,
            Option<String>,
        ),
    >(
        r#"
        SELECT
            bj.id AS job_id,
            s.id AS system_id,
            COALESCE(s.hostname, d.derivation_target, d.derivation_name) AS hostname,
            f.name AS flake_name,
            c.git_commit_hash AS commit_hash,
            NULL::TEXT AS commit_message,
            bj.status,
            b.name AS builder_name,
            bj.created_at AS queued_at,
            bj.started_at,
            CASE
                WHEN bj.started_at IS NULL THEN NULL
                ELSE EXTRACT(EPOCH FROM (now() - bj.started_at))::BIGINT
            END AS elapsed_secs,
            bj.logs
        FROM build_jobs bj
        JOIN derivations d ON d.id = bj.derivation_id
        LEFT JOIN commits c ON c.id = d.commit_id
        LEFT JOIN flakes f ON f.id = c.flake_id
        LEFT JOIN systems s ON s.hostname = d.derivation_target
        LEFT JOIN builders b ON b.id = bj.builder_id
        WHERE bj.status IN ('queued', 'building')
        ORDER BY
            CASE WHEN bj.status = 'building' THEN 0 ELSE 1 END,
            bj.priority_weight DESC,
            bj.created_at ASC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let items = rows
        .into_iter()
        .map(
            |(
                job_id,
                system_id,
                hostname,
                flake_name,
                commit_hash,
                commit_message,
                status,
                builder_name,
                queued_at,
                started_at,
                elapsed_secs,
                logs,
            )| {
                let status = match status.as_str() {
                    "queued" => BuildStatus::Queued,
                    "building" => BuildStatus::Building,
                    "failed" => BuildStatus::Failed,
                    "success" => BuildStatus::Complete,
                    _ => BuildStatus::Idle,
                };

                BuildQueueItem {
                    job_id,
                    system_id,
                    hostname: hostname.unwrap_or_else(|| "unknown".to_string()),
                    flake_name: flake_name.unwrap_or_else(|| "unknown".to_string()),
                    commit_hash: commit_hash.unwrap_or_else(|| "unknown".to_string()),
                    commit_message,
                    status,
                    builder_name,
                    queued_at,
                    started_at,
                    elapsed_secs,
                    logs,
                }
            },
        )
        .collect::<Vec<_>>();

    let building_count = items
        .iter()
        .filter(|item| item.status == BuildStatus::Building)
        .count() as i64;
    let queued_count = items
        .iter()
        .filter(|item| item.status == BuildStatus::Queued)
        .count() as i64;

    Ok(BuildQueueSummary {
        building_count,
        queued_count,
        items,
        timestamp: Utc::now(),
    })
}

/// Fetch recent completed/failed builds for history views.
pub async fn fetch_recent_build_history(pool: &PgPool, limit: i64) -> Result<Vec<BuildQueueItem>> {
    let rows = sqlx::query_as::<
        _,
        (
            Option<Uuid>,
            Option<Uuid>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            Option<String>,
            DateTime<Utc>,
            Option<DateTime<Utc>>,
            Option<i64>,
            Option<String>,
        ),
    >(
        r#"
        SELECT
            bj.id AS job_id,
            s.id AS system_id,
            COALESCE(s.hostname, d.derivation_target, d.derivation_name) AS hostname,
            f.name AS flake_name,
            c.git_commit_hash AS commit_hash,
            NULL::TEXT AS commit_message,
            bj.status,
            b.name AS builder_name,
            bj.created_at AS queued_at,
            bj.started_at,
            CASE
                WHEN bj.started_at IS NULL OR bj.completed_at IS NULL THEN NULL
                ELSE EXTRACT(EPOCH FROM (bj.completed_at - bj.started_at))::BIGINT
            END AS elapsed_secs,
            bj.logs
        FROM build_jobs bj
        JOIN derivations d ON d.id = bj.derivation_id
        LEFT JOIN commits c ON c.id = d.commit_id
        LEFT JOIN flakes f ON f.id = c.flake_id
        LEFT JOIN systems s ON s.hostname = d.derivation_target
        LEFT JOIN builders b ON b.id = bj.builder_id
        WHERE bj.status IN ('success', 'failed')
        ORDER BY COALESCE(bj.completed_at, bj.updated_at, bj.created_at) DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(
                job_id,
                system_id,
                hostname,
                flake_name,
                commit_hash,
                commit_message,
                status,
                builder_name,
                queued_at,
                started_at,
                elapsed_secs,
                logs,
            )| {
                let status = match status.as_str() {
                    "failed" => BuildStatus::Failed,
                    "success" => BuildStatus::Complete,
                    "building" => BuildStatus::Building,
                    "queued" => BuildStatus::Queued,
                    _ => BuildStatus::Idle,
                };

                BuildQueueItem {
                    job_id,
                    system_id,
                    hostname: hostname.unwrap_or_else(|| "unknown".to_string()),
                    flake_name: flake_name.unwrap_or_else(|| "unknown".to_string()),
                    commit_hash: commit_hash.unwrap_or_else(|| "unknown".to_string()),
                    commit_message,
                    status,
                    builder_name,
                    queued_at,
                    started_at,
                    elapsed_secs,
                    logs,
                }
            },
        )
        .collect())
}
