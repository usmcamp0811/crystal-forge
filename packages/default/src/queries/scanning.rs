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
    /// Freshness class derived from the most recent completed scan:
    /// `deployed` (<=24h), `recent` (<=30d), or `archived` (older/never).
    pub freshness: String,
    /// True when this is the latest scan row for its derivation.
    pub is_current: bool,
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
        WITH policy AS (
            SELECT
                GREATEST(
                    1,
                    COALESCE(NULLIF(regexp_replace(deployed_interval, '[^0-9]', '', 'g'), '')::INT, 24)
                ) AS deployed_hours
            FROM scan_schedule_policy
            WHERE id = 1
        ),
        latest_lifecycle AS (
            SELECT DISTINCT ON (d.id)
                d.id AS derivation_id,
                cs.status,
                cs.scheduled_at,
                cs.created_at,
                cs.completed_at
            FROM derivations d
            LEFT JOIN cve_scans cs ON cs.derivation_id = d.id
            WHERE d.derivation_type = 'nixos'
            ORDER BY d.id, COALESCE(cs.completed_at, cs.scheduled_at, cs.created_at) DESC NULLS LAST
        ),
        latest_completed AS (
            SELECT DISTINCT ON (d.id)
                d.id AS derivation_id,
                cs.completed_at
            FROM derivations d
            LEFT JOIN cve_scans cs ON cs.derivation_id = d.id
            WHERE d.derivation_type = 'nixos'
              AND cs.completed_at IS NOT NULL
            ORDER BY d.id, cs.completed_at DESC
        )
        SELECT
            COUNT(*) FILTER (WHERE ll.status = 'in_progress')::BIGINT AS scanning,
            COUNT(*) FILTER (WHERE ll.status = 'pending')::BIGINT AS queued,
            COUNT(*) FILTER (WHERE ll.status = 'failed')::BIGINT AS failed,
            COUNT(*) FILTER (WHERE lc.completed_at IS NULL)::BIGINT AS never_scanned,
            COUNT(*) FILTER (
                WHERE lc.completed_at IS NOT NULL
                AND lc.completed_at < NOW() - (SELECT deployed_hours * INTERVAL '1 hour' FROM policy)
            )::BIGINT AS stale,
            CASE
                WHEN COUNT(*) = 0 THEN 0
                ELSE ROUND((COUNT(*) FILTER (WHERE lc.completed_at IS NOT NULL)::numeric / COUNT(*)::numeric) * 100)
            END::BIGINT AS coverage_percent
        FROM latest_lifecycle ll
        LEFT JOIN latest_completed lc ON lc.derivation_id = ll.derivation_id
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
        WITH latest_per_derivation AS (
            SELECT DISTINCT ON (d.id)
                cs.id AS scan_id,
                d.derivation_name AS hostname,
                f.name AS flake_name,
                c.git_commit_hash AS commit_hash,
                cs.status,
                cs.completed_at,
                cs.scheduled_at,
                cs.critical_count,
                cs.high_count,
                cs.medium_count,
                COALESCE(cs.completed_at, cs.scheduled_at, cs.created_at) AS lifecycle_at
            FROM derivations d
            LEFT JOIN cve_scans cs ON cs.derivation_id = d.id
            LEFT JOIN commits c ON c.id = d.commit_id
            LEFT JOIN flakes f ON f.id = c.flake_id
            WHERE d.derivation_type = 'nixos'
            ORDER BY d.id, COALESCE(cs.completed_at, cs.scheduled_at, cs.created_at) DESC NULLS LAST
        )
        SELECT
            scan_id,
            hostname,
            flake_name,
            commit_hash,
            status,
            completed_at,
            scheduled_at,
            critical_count,
            high_count,
            medium_count,
            CASE
                WHEN completed_at IS NULL THEN 'archived'
                WHEN completed_at >= NOW() - INTERVAL '24 hours' THEN 'deployed'
                WHEN completed_at >= NOW() - INTERVAL '30 days' THEN 'recent'
                ELSE 'archived'
            END AS freshness,
            TRUE AS is_current
        FROM latest_per_derivation
        ORDER BY
            CASE WHEN status = 'in_progress' THEN 0 WHEN status = 'pending' THEN 1 ELSE 2 END,
            lifecycle_at DESC NULLS LAST
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
            freshness: row.get("freshness"),
            is_current: row.get("is_current"),
        })
        .collect())
}

pub async fn get_scan_queue_for_system(
    pool: &PgPool,
    system_id: Uuid,
    limit: i64,
) -> Result<Vec<ScanQueueRow>> {
    let rows = sqlx::query(
        r#"
        WITH latest_per_derivation AS (
            SELECT DISTINCT ON (d.id)
                cs.id AS scan_id,
                d.derivation_name AS hostname,
                f.name AS flake_name,
                c.git_commit_hash AS commit_hash,
                cs.status,
                cs.completed_at,
                cs.scheduled_at,
                cs.critical_count,
                cs.high_count,
                cs.medium_count,
                COALESCE(cs.completed_at, cs.scheduled_at, cs.created_at) AS lifecycle_at
            FROM derivations d
            JOIN systems s ON s.hostname = d.derivation_name
            LEFT JOIN cve_scans cs ON cs.derivation_id = d.id
            LEFT JOIN commits c ON c.id = d.commit_id
            LEFT JOIN flakes f ON f.id = c.flake_id
            WHERE d.derivation_type = 'nixos'
              AND s.id = $1
              AND s.is_active = TRUE
            ORDER BY d.id, COALESCE(cs.completed_at, cs.scheduled_at, cs.created_at) DESC NULLS LAST
        )
        SELECT
            scan_id,
            hostname,
            flake_name,
            commit_hash,
            status,
            completed_at,
            scheduled_at,
            critical_count,
            high_count,
            medium_count,
            CASE
                WHEN completed_at IS NULL THEN 'archived'
                WHEN completed_at >= NOW() - INTERVAL '24 hours' THEN 'deployed'
                WHEN completed_at >= NOW() - INTERVAL '30 days' THEN 'recent'
                ELSE 'archived'
            END AS freshness,
            (ROW_NUMBER() OVER (
                ORDER BY lifecycle_at DESC NULLS LAST
            ) = 1) AS is_current
        FROM latest_per_derivation
        ORDER BY
            CASE WHEN status = 'in_progress' THEN 0 WHEN status = 'pending' THEN 1 ELSE 2 END,
            lifecycle_at DESC NULLS LAST
        LIMIT $2
        "#,
    )
    .bind(system_id)
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
            freshness: row.get("freshness"),
            is_current: row.get("is_current"),
        })
        .collect())
}

pub async fn get_scan_systems(pool: &PgPool, limit: i64) -> Result<Vec<ScanSystemRow>> {
    let rows = sqlx::query(
        r#"
        WITH policy AS (
            SELECT
                GREATEST(
                    1,
                    COALESCE(NULLIF(regexp_replace(deployed_interval, '[^0-9]', '', 'g'), '')::INT, 24)
                ) AS deployed_hours
            FROM scan_schedule_policy
            WHERE id = 1
        ),
        latest_lifecycle_per_derivation AS (
            SELECT DISTINCT ON (d.id)
                d.id AS derivation_id,
                d.derivation_name AS hostname,
                d.store_path,
                cs.status,
                cs.scheduled_at,
                cs.created_at,
                cs.completed_at
            FROM derivations d
            LEFT JOIN cve_scans cs ON cs.derivation_id = d.id
            WHERE d.derivation_type = 'nixos'
            ORDER BY d.id, COALESCE(cs.completed_at, cs.scheduled_at, cs.created_at) DESC NULLS LAST
        ),
        latest_completed_per_derivation AS (
            SELECT DISTINCT ON (d.id)
                d.id AS derivation_id,
                cs.completed_at,
                cs.critical_count,
                cs.high_count
            FROM derivations d
            LEFT JOIN cve_scans cs ON cs.derivation_id = d.id
            WHERE d.derivation_type = 'nixos'
              AND cs.completed_at IS NOT NULL
            ORDER BY d.id, cs.completed_at DESC
        )
        SELECT
            s.id AS system_id,
            ll.hostname,
            MAX(e.name) AS environment,
            COUNT(*)::BIGINT AS total_configs,
            COUNT(*) FILTER (
                WHERE lc.completed_at IS NOT NULL
                AND lc.completed_at >= NOW() - (SELECT deployed_hours * INTERVAL '1 hour' FROM policy)
            )::BIGINT AS scanned,
            COUNT(*) FILTER (
                WHERE lc.completed_at IS NOT NULL
                AND lc.completed_at < NOW() - (SELECT deployed_hours * INTERVAL '1 hour' FROM policy)
            )::BIGINT AS stale,
            COUNT(*) FILTER (WHERE ll.store_path IS NULL)::BIGINT AS needs_build,
            COUNT(*) FILTER (WHERE lc.completed_at IS NULL)::BIGINT AS unscanned,
            COALESCE(MAX(lc.critical_count), 0)::BIGINT AS current_crit,
            COALESCE(MAX(lc.high_count), 0)::BIGINT AS current_high
        FROM latest_lifecycle_per_derivation ll
        LEFT JOIN latest_completed_per_derivation lc ON lc.derivation_id = ll.derivation_id
        JOIN systems s ON s.hostname = ll.hostname
        LEFT JOIN environments e ON e.id = s.environment_id
        WHERE s.is_active = TRUE
        GROUP BY s.id, ll.hostname
        ORDER BY total_configs DESC, ll.hostname ASC
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
            d.derivation_name AS name,
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
        JOIN derivations d ON d.id = cs.derivation_id
        WHERE d.derivation_type = 'nixos'
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
