use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSchedulePolicyRow {
    pub on_build: bool,
    pub deployed_interval: String,
    pub recent_interval: String,
    pub archived_interval: String,
    pub archived_enabled: bool,
    pub rebuild_to_scan: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanStatsRow {
    pub scanning: i64,
    pub queued: i64,
    pub stale: i64,
    pub never_scanned: i64,
    pub failed: i64,
    pub coverage_percent: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanQueueRow {
    pub scan_id: Uuid,
    pub hostname: String,
    pub flake_name: Option<String>,
    pub commit_hash: Option<String>,
    pub status: String,
    pub completed_at: Option<DateTime<Utc>>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub critical_count: i32,
    pub high_count: i32,
    pub medium_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSystemRow {
    pub system_id: Uuid,
    pub hostname: String,
    pub environment: Option<String>,
    pub total_configs: i64,
    pub scanned: i64,
    pub stale: i64,
    pub needs_build: i64,
    pub unscanned: i64,
    pub current_crit: i64,
    pub current_high: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanActivityRow {
    pub at: Option<DateTime<Utc>>,
    pub name: String,
    pub event: String,
    pub detail: String,
    pub status: String,
}

pub async fn get_scan_schedule_policy(pool: &PgPool) -> Result<ScanSchedulePolicyRow> {
    let row = sqlx::query(
        r#"
        SELECT on_build, deployed_interval, recent_interval, archived_interval,
               archived_enabled, rebuild_to_scan, updated_at
        FROM scan_schedule_policy
        WHERE id = 1
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(ScanSchedulePolicyRow {
        on_build: row.get("on_build"),
        deployed_interval: row.get("deployed_interval"),
        recent_interval: row.get("recent_interval"),
        archived_interval: row.get("archived_interval"),
        archived_enabled: row.get("archived_enabled"),
        rebuild_to_scan: row.get("rebuild_to_scan"),
        updated_at: row.get("updated_at"),
    })
}

pub async fn update_scan_schedule_policy(
    pool: &PgPool,
    policy: &ScanSchedulePolicyRow,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE scan_schedule_policy
        SET on_build = $1,
            deployed_interval = $2,
            recent_interval = $3,
            archived_interval = $4,
            archived_enabled = $5,
            rebuild_to_scan = $6,
            updated_at = NOW()
        WHERE id = 1
        "#,
    )
    .bind(policy.on_build)
    .bind(&policy.deployed_interval)
    .bind(&policy.recent_interval)
    .bind(&policy.archived_interval)
    .bind(policy.archived_enabled)
    .bind(policy.rebuild_to_scan)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_scan_stats(pool: &PgPool) -> Result<ScanStatsRow> {
    let row = sqlx::query(
        r#"
        WITH latest AS (
            SELECT DISTINCT ON (et.target_name)
                et.target_name,
                cs.status,
                cs.completed_at,
                cs.scheduled_at
            FROM evaluation_targets et
            LEFT JOIN cve_scans cs ON cs.evaluation_target_id = et.id
            WHERE et.target_type = 'nixos'
            ORDER BY et.target_name, cs.completed_at DESC NULLS LAST, cs.created_at DESC NULLS LAST
        )
        SELECT
            COUNT(*) FILTER (WHERE status = 'in_progress')::BIGINT AS scanning,
            COUNT(*) FILTER (WHERE status = 'pending')::BIGINT AS queued,
            COUNT(*) FILTER (WHERE status = 'failed')::BIGINT AS failed,
            COUNT(*) FILTER (WHERE completed_at IS NULL)::BIGINT AS never_scanned,
            COUNT(*) FILTER (WHERE completed_at IS NOT NULL AND completed_at < NOW() - INTERVAL '24 hours')::BIGINT AS stale,
            CASE
                WHEN COUNT(*) = 0 THEN 0
                ELSE ROUND((COUNT(*) FILTER (WHERE completed_at IS NOT NULL)::numeric / COUNT(*)::numeric) * 100)
            END::BIGINT AS coverage_percent
        FROM latest
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(ScanStatsRow {
        scanning: row.get("scanning"),
        queued: row.get("queued"),
        stale: row.get("stale"),
        never_scanned: row.get("never_scanned"),
        failed: row.get("failed"),
        coverage_percent: row.get("coverage_percent"),
    })
}

pub async fn get_scan_queue(pool: &PgPool, limit: i64) -> Result<Vec<ScanQueueRow>> {
    let rows = sqlx::query(
        r#"
        SELECT
            cs.id AS scan_id,
            et.target_name AS hostname,
            f.name AS flake_name,
            c.git_commit_hash AS commit_hash,
            cs.status,
            cs.completed_at,
            cs.scheduled_at,
            cs.critical_count,
            cs.high_count,
            cs.medium_count
        FROM cve_scans cs
        JOIN evaluation_targets et ON et.id = cs.evaluation_target_id
        LEFT JOIN commits c ON c.id = et.commit_id
        LEFT JOIN flakes f ON f.id = c.flake_id
        WHERE et.target_type = 'nixos'
        ORDER BY
            CASE WHEN cs.status = 'in_progress' THEN 0 WHEN cs.status = 'pending' THEN 1 ELSE 2 END,
            COALESCE(cs.completed_at, cs.scheduled_at, cs.created_at) DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| ScanQueueRow {
            scan_id: row.get("scan_id"),
            hostname: row.get("hostname"),
            flake_name: row.get("flake_name"),
            commit_hash: row.get("commit_hash"),
            status: row.get("status"),
            completed_at: row.get("completed_at"),
            scheduled_at: row.get("scheduled_at"),
            critical_count: row.get("critical_count"),
            high_count: row.get("high_count"),
            medium_count: row.get("medium_count"),
        })
        .collect())
}

pub async fn get_scan_systems(pool: &PgPool, limit: i64) -> Result<Vec<ScanSystemRow>> {
    let rows = sqlx::query(
        r#"
        WITH latest_per_target AS (
            SELECT DISTINCT ON (et.id)
                et.id AS evaluation_target_id,
                et.target_name AS hostname,
                et.derivation_path,
                e.name AS environment,
                cs.completed_at,
                cs.critical_count,
                cs.high_count
            FROM evaluation_targets et
            LEFT JOIN systems s ON s.hostname = et.target_name
            LEFT JOIN environments e ON e.id = s.environment_id
            LEFT JOIN cve_scans cs ON cs.evaluation_target_id = et.id
            WHERE et.target_type = 'nixos'
            ORDER BY et.id, cs.completed_at DESC NULLS LAST, cs.created_at DESC NULLS LAST
        )
        SELECT
            s.id AS system_id,
            l.hostname,
            MAX(l.environment) AS environment,
            COUNT(*)::BIGINT AS total_configs,
            COUNT(*) FILTER (WHERE l.completed_at >= NOW() - INTERVAL '30 days')::BIGINT AS scanned,
            COUNT(*) FILTER (WHERE l.completed_at IS NOT NULL AND l.completed_at < NOW() - INTERVAL '30 days')::BIGINT AS stale,
            COUNT(*) FILTER (WHERE l.derivation_path IS NULL)::BIGINT AS needs_build,
            COUNT(*) FILTER (WHERE l.completed_at IS NULL)::BIGINT AS unscanned,
            COALESCE(MAX(l.critical_count), 0)::BIGINT AS current_crit,
            COALESCE(MAX(l.high_count), 0)::BIGINT AS current_high
        FROM latest_per_target l
        JOIN systems s ON s.hostname = l.hostname
        WHERE s.is_active = TRUE
        GROUP BY s.id, l.hostname
        ORDER BY total_configs DESC, l.hostname ASC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| ScanSystemRow {
            system_id: row.get("system_id"),
            hostname: row.get("hostname"),
            environment: row.get("environment"),
            total_configs: row.get("total_configs"),
            scanned: row.get("scanned"),
            stale: row.get("stale"),
            needs_build: row.get("needs_build"),
            unscanned: row.get("unscanned"),
            current_crit: row.get("current_crit"),
            current_high: row.get("current_high"),
        })
        .collect())
}

pub async fn get_scan_activity(pool: &PgPool, limit: i64) -> Result<Vec<ScanActivityRow>> {
    let rows = sqlx::query(
        r#"
        SELECT
            COALESCE(cs.completed_at, cs.scheduled_at, cs.created_at) AS at,
            et.target_name AS name,
            CASE
                WHEN cs.status = 'in_progress' THEN 'Scan started'
                WHEN cs.status = 'completed' THEN 'Scan completed'
                WHEN cs.status = 'failed' THEN 'Scan failed'
                WHEN cs.status = 'pending' THEN 'Scan queued'
                ELSE 'Scan update'
            END AS event,
            CASE
                WHEN cs.status = 'completed' THEN CONCAT(cs.critical_count, ' critical, ', cs.high_count, ' high, ', cs.medium_count, ' medium')
                WHEN cs.status = 'failed' THEN COALESCE(cs.scan_metadata->>'error', 'scan failed')
                ELSE 'vulnix scan lifecycle update'
            END AS detail,
            cs.status
        FROM cve_scans cs
        JOIN evaluation_targets et ON et.id = cs.evaluation_target_id
        WHERE et.target_type = 'nixos'
        ORDER BY COALESCE(cs.completed_at, cs.scheduled_at, cs.created_at) DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| ScanActivityRow {
            at: row.get("at"),
            name: row.get("name"),
            event: row.get("event"),
            detail: row.get("detail"),
            status: row.get("status"),
        })
        .collect())
}
