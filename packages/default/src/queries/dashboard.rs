//! Dashboard-related database queries.
//!
//! All SQL for the dashboard summary endpoint lives here, keeping the
//! handler layer free of raw queries.

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::api::models::{
    BuildQueueItem, BuildQueuePageResponse, BuildQueueParams, BuildQueueSummary, BuildStatus,
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
            Option<String>,
            Option<f64>,
            Option<DateTime<Utc>>,
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
            bj.logs,
            e.name AS environment,
            bj.priority_weight,
            c.commit_timestamp
        FROM build_jobs bj
        JOIN derivations d ON d.id = bj.derivation_id
        LEFT JOIN commits c ON c.id = d.commit_id
        LEFT JOIN flakes f ON f.id = c.flake_id
        LEFT JOIN systems s ON s.hostname = d.derivation_target
        LEFT JOIN environments e ON e.id = s.environment_id
        LEFT JOIN builders b ON b.id = bj.builder_id
        WHERE bj.status IN ('queued', 'building')
        ORDER BY
            CASE WHEN bj.status = 'building' THEN 0 ELSE 1 END,
            bj.priority_weight DESC,
            c.commit_timestamp DESC NULLS LAST,
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
                environment,
                _priority_weight,
                _commit_timestamp,
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
                    environment,
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
            bj.logs,
            e.name AS environment
        FROM build_jobs bj
        JOIN derivations d ON d.id = bj.derivation_id
        LEFT JOIN commits c ON c.id = d.commit_id
        LEFT JOIN flakes f ON f.id = c.flake_id
        LEFT JOIN systems s ON s.hostname = d.derivation_target
        LEFT JOIN environments e ON e.id = s.environment_id
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
                environment,
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
                    environment,
                }
            },
        )
        .collect())
}

/// Fetch build jobs with pagination, filtering, and newest-first ordering.
///
/// Supports filtering by status, commit hash, flake name, config/hostname, and time range.
/// Returns a total row count alongside the page of items so the caller can render pagination.
pub async fn list_build_queue_paginated(
    pool: &PgPool,
    params: &BuildQueueParams,
) -> Result<BuildQueuePageResponse> {
    let limit = params.limit.min(200).max(1);
    let page = params.page.max(1);
    let offset = (page - 1) * limit;

    // Build status filter list. Empty means "all statuses".
    let status_filter: Vec<String> = params
        .status
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // Shared type aliases for the raw row tuple.
    type BuildRow = (
        Option<Uuid>,          // job_id
        Option<Uuid>,          // system_id
        Option<String>,        // hostname
        Option<String>,        // flake_name
        Option<String>,        // commit_hash
        Option<String>,        // commit_message (first line)
        String,                // status
        Option<String>,        // builder_name
        DateTime<Utc>,         // queued_at
        Option<DateTime<Utc>>, // started_at
        Option<i64>,           // elapsed_secs
        Option<String>,        // logs
        Option<String>,        // environment
        i64,                   // total_count
    );

    let rows = sqlx::query_as::<_, BuildRow>(
        r#"
        SELECT
            bj.id AS job_id,
            s.id AS system_id,
            COALESCE(s.hostname, d.derivation_target, d.derivation_name) AS hostname,
            f.name AS flake_name,
            c.git_commit_hash AS commit_hash,
            -- First line of commit message; empty string coalesces to NULL so the
            -- frontend sees None rather than an empty summary.
            NULLIF(split_part(COALESCE(c.message, ''), E'\n', 1), '') AS commit_message,
            bj.status,
            b.name AS builder_name,
            bj.created_at AS queued_at,
            bj.started_at,
            CASE
                WHEN bj.status IN ('success', 'failed') THEN
                    CASE
                        WHEN bj.started_at IS NULL OR bj.completed_at IS NULL THEN NULL
                        ELSE EXTRACT(EPOCH FROM (bj.completed_at - bj.started_at))::BIGINT
                    END
                ELSE
                    CASE
                        WHEN bj.started_at IS NULL THEN NULL
                        ELSE EXTRACT(EPOCH FROM (now() - bj.started_at))::BIGINT
                    END
            END AS elapsed_secs,
            bj.logs,
            e.name AS environment,
            COUNT(*) OVER () AS total_count
        FROM build_jobs bj
        JOIN derivations d ON d.id = bj.derivation_id
        LEFT JOIN commits c ON c.id = d.commit_id
        LEFT JOIN flakes f ON f.id = c.flake_id
        -- Use a LATERAL subquery to guarantee at most one system per build job.
        -- Hostname match wins over system_configuration_name match to be deterministic.
        LEFT JOIN LATERAL (
            SELECT id, hostname, environment_id, system_configuration_name
            FROM systems
            WHERE hostname = d.derivation_target
               OR (system_configuration_name IS NOT NULL
                   AND system_configuration_name = d.derivation_target)
            ORDER BY
                CASE WHEN hostname = d.derivation_target THEN 0 ELSE 1 END
            LIMIT 1
        ) s ON TRUE
        LEFT JOIN environments e ON e.id = COALESCE(s.environment_id, bj.environment_id)
        LEFT JOIN builders b ON b.id = bj.builder_id
        WHERE
            -- Status filter: if empty, include all statuses
            (
                $1::text[] IS NULL
                OR cardinality($1::text[]) = 0
                OR bj.status = ANY($1::text[])
            )
            -- Commit hash filter (prefix match)
            AND ($2::text IS NULL OR c.git_commit_hash ILIKE ($2 || '%'))
            -- Flake name filter (partial match)
            AND ($3::text IS NULL OR f.name ILIKE ('%' || $3 || '%'))
            -- Config/hostname filter (partial match on resolved display name or config name)
            AND (
                $4::text IS NULL
                OR COALESCE(s.hostname, d.derivation_target, d.derivation_name) ILIKE ('%' || $4 || '%')
                OR COALESCE(s.system_configuration_name, '') ILIKE ('%' || $4 || '%')
            )
            -- Time range filters on queued_at
            AND ($5::timestamptz IS NULL OR bj.created_at >= $5)
            AND ($6::timestamptz IS NULL OR bj.created_at <= $6)
        ORDER BY
            -- In-progress first, then newest queued/completed
            CASE
                WHEN bj.status = 'building' THEN 0
                WHEN bj.status = 'queued' THEN 1
                ELSE 2
            END,
            bj.created_at DESC NULLS LAST
        LIMIT $7
        OFFSET $8
        "#,
    )
    .bind(if status_filter.is_empty() { None } else { Some(status_filter.clone()) })
    .bind(params.commit_hash.as_deref())
    .bind(params.flake_name.as_deref())
    .bind(params.config_name.as_deref())
    .bind(params.queued_after)
    .bind(params.queued_before)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let total = rows.first().map(|r| r.13).unwrap_or(0);

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
                environment,
                _total,
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
                    environment,
                }
            },
        )
        .collect();

    Ok(BuildQueuePageResponse {
        total,
        page,
        limit,
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_queue_params_defaults() {
        let p: BuildQueueParams = serde_json::from_str("{}").unwrap();
        assert_eq!(p.page, 1);
        assert_eq!(p.limit, 50);
        assert!(p.status.is_none());
        assert!(p.commit_hash.is_none());
    }

    #[test]
    fn build_queue_params_clamps_limit() {
        // The handler will clamp; verify default parse is correct
        let p: BuildQueueParams = serde_json::from_str(r#"{"limit":999}"#).unwrap();
        assert_eq!(p.limit, 999); // clamping happens in query fn
    }

    #[test]
    fn build_queue_status_split() {
        let p: BuildQueueParams =
            serde_json::from_str(r#"{"status":"queued,building"}"#).unwrap();
        let status_filter: Vec<String> = p
            .status
            .as_deref()
            .unwrap_or("")
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        assert_eq!(status_filter, vec!["queued", "building"]);
    }
}
