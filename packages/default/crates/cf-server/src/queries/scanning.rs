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
    /// `None` when the system has been deployed but never scanned.
    pub scan_id: Option<Uuid>,
    pub hostname: String,
    pub flake_name: Option<String>,
    pub commit_hash: Option<String>,
    /// Normalized scan status; `"never_scanned"` when no scan row exists.
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
    /// True when this derivation's commit is the latest known commit for its flake.
    pub is_latest_per_flake: bool,
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
        WITH latest_commit_per_flake AS (
            SELECT DISTINCT ON (flake_id) id AS commit_id, flake_id
            FROM commits
            ORDER BY flake_id, commit_timestamp DESC NULLS LAST, id DESC
        ),
        latest_per_derivation AS (
            SELECT DISTINCT ON (d.id)
                cs.id AS scan_id,
                d.derivation_name AS hostname,
                f.name AS flake_name,
                c.git_commit_hash AS commit_hash,
                c.flake_id,
                c.id AS commit_db_id,
                COALESCE(cs.status, 'never_scanned') AS status,
                cs.completed_at,
                cs.scheduled_at,
                COALESCE(cs.critical_count, 0)::int AS critical_count,
                COALESCE(cs.high_count, 0)::int AS high_count,
                COALESCE(cs.medium_count, 0)::int AS medium_count,
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
            TRUE AS is_current,
            (lc.commit_id IS NOT NULL AND lpd.commit_db_id = lc.commit_id) AS is_latest_per_flake
        FROM latest_per_derivation lpd
        LEFT JOIN latest_commit_per_flake lc ON lc.flake_id = lpd.flake_id
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
            is_latest_per_flake: row.get("is_latest_per_flake"),
        })
        .collect())
}

/// Result type for `get_scan_deployed` including pagination metadata.
pub struct ScanDeployedResult {
    pub rows: Vec<ScanQueueRow>,
    /// Total deployed configurations known to the server (without limit/cursor).
    pub total: i64,
    /// True when the result was capped.
    pub has_more: bool,
    /// Opaque composite cursor: `{hostname}:{derivation_id}` of the last row.
    pub next_cursor: Option<String>,
}

/// Returns all derivations currently deployed on at least one active system,
/// with complete flake history used to derive `is_latest_per_flake`.
///
/// Uses the same normalized configuration-name expression as the evaluation
/// path (`COALESCE(NULLIF(BTRIM(system_configuration_name), ''), hostname)`)
/// so systems whose NixOS config name differs from their hostname are included.
///
/// Nullable scan columns are COALESCE'd to safe defaults so systems that have
/// never been scanned are included without NULL-decode panics.
/// Marker for a malformed deployed-scan cursor.
#[derive(Debug)]
pub struct InvalidCursorError;
impl std::fmt::Display for InvalidCursorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid deployed scan cursor")
    }
}
impl std::error::Error for InvalidCursorError {}

/// Decode an opaque base64url cursor into (hostname, derivation_id).
pub fn decode_deployed_cursor(cursor: &str) -> Result<(String, i32), InvalidCursorError> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| InvalidCursorError)?;
    let s = std::str::from_utf8(&bytes).map_err(|_| InvalidCursorError)?;
    let mut parts = s.splitn(2, '\x00');
    let host = parts.next().ok_or(InvalidCursorError)?.to_string();
    let id: i32 = parts
        .next()
        .ok_or(InvalidCursorError)?
        .parse()
        .map_err(|_| InvalidCursorError)?;
    Ok((host, id))
}

/// Encode (hostname, derivation_id) into an opaque base64url cursor.
pub fn encode_deployed_cursor(hostname: &str, derivation_id: i32) -> String {
    use base64::Engine;
    // NUL-separated; hostname cannot contain NUL in practice.
    let raw = format!("{}\x00{}", hostname, derivation_id);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw.as_bytes())
}

/// Cursor-based pagination for the deployed scan list.
///
/// The cursor is a base64url-encoded `{hostname}\x00{derivation_id}` payload so
/// that the cursor is opaque, unambiguous, and handles config names with `:`.
pub async fn get_scan_deployed(
    pool: &PgPool,
    limit: i64,
    after_cursor: Option<&str>,
) -> Result<ScanDeployedResult> {
    // Decode the composite cursor.
    let (cursor_hostname, cursor_derivation_id): (String, i32) = match after_cursor {
        None => (String::new(), 0),
        Some(c) => decode_deployed_cursor(c).map_err(anyhow::Error::new)?,
    };
    let rows = sqlx::query(
        r#"
        WITH latest_system_state AS (
            SELECT DISTINCT ON (s.id)
                s.id AS system_id,
                ss.store_path AS current_store_path
            FROM systems s
            LEFT JOIN system_states ss ON ss.hostname = s.hostname
            ORDER BY s.id, ss.timestamp DESC NULLS LAST, ss.id DESC
        ),
        latest_commit_per_flake AS (
            -- Order by commit_timestamp DESC first (newest git commit wins),
            -- then by id DESC as a tiebreaker for identical timestamps.
            SELECT DISTINCT ON (flake_id) id AS commit_id, flake_id
            FROM commits
            ORDER BY flake_id, commit_timestamp DESC NULLS LAST, id DESC
        ),
        deployed_derivations AS (
            SELECT DISTINCT ON (d.id)
                cs.id                      AS scan_id,
                d.id                       AS derivation_id,
                COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname) AS hostname,
                f.name                     AS flake_name,
                c.git_commit_hash          AS commit_hash,
                c.flake_id,
                c.id                       AS commit_db_id,
                COALESCE(cs.status, 'never_scanned')  AS status,
                cs.completed_at,
                cs.scheduled_at,
                COALESCE(cs.critical_count, 0)::int   AS critical_count,
                COALESCE(cs.high_count, 0)::int        AS high_count,
                COALESCE(cs.medium_count, 0)::int      AS medium_count,
                COALESCE(cs.completed_at, cs.scheduled_at, cs.created_at) AS lifecycle_at
            FROM systems s
            LEFT JOIN latest_system_state lss ON lss.system_id = s.id
            JOIN derivations d
              ON d.derivation_name =
                     COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname)
              AND d.store_path IS NOT NULL
              AND d.store_path = lss.current_store_path
            LEFT JOIN cve_scans cs ON cs.derivation_id = d.id
            LEFT JOIN commits c ON c.id = d.commit_id
            LEFT JOIN flakes f ON f.id = c.flake_id
            WHERE s.is_active = TRUE
              AND d.derivation_type = 'nixos'
            ORDER BY d.id,
                     COALESCE(cs.completed_at, cs.scheduled_at, cs.created_at) DESC NULLS LAST
        )
        SELECT
            scan_id,
            derivation_id,
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
                WHEN completed_at IS NULL THEN 'never_scanned'
                WHEN completed_at >= NOW() - INTERVAL '24 hours' THEN 'deployed'
                WHEN completed_at >= NOW() - INTERVAL '30 days' THEN 'recent'
                ELSE 'archived'
            END AS freshness,
            TRUE AS is_current,
            (lc.commit_id IS NOT NULL
             AND dd.commit_db_id = lc.commit_id) AS is_latest_per_flake
        FROM deployed_derivations dd
        LEFT JOIN latest_commit_per_flake lc ON lc.flake_id = dd.flake_id
        -- Composite keyset cursor: (hostname, derivation_id).
        WHERE (dd.hostname, dd.derivation_id) > ($2, $3)
        ORDER BY dd.hostname ASC, dd.derivation_id ASC
        LIMIT $1
        "#,
    )
    .bind(limit + 1) // Fetch one extra to detect has_more (P2 #7).
    .bind(&cursor_hostname)
    .bind(cursor_derivation_id)
    .fetch_all(pool)
    .await?;

    // Build items from the raw rows, collecting (row, derivation_id) pairs so
    // we can construct the composite cursor from the last item.
    let mut raw_items: Vec<(ScanQueueRow, i32)> = rows
        .into_iter()
        .map(|row| {
            (
                ScanQueueRow {
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
                    is_latest_per_flake: row.get("is_latest_per_flake"),
                },
                row.get::<i32, _>("derivation_id"),
            )
        })
        .collect();

    // limit+1 pattern: has_more iff we got the extra row; trim it off (P2 #7).
    let has_more = raw_items.len() as i64 > limit;
    if has_more {
        raw_items.truncate(limit as usize);
    }

    let next_cursor = if has_more {
        raw_items
            .last()
            .map(|(row, did)| encode_deployed_cursor(&row.hostname, *did))
    } else {
        None
    };

    // Total count query for informational display (not used for has_more).
    let total: i64 = sqlx::query_scalar(
        r#"
        WITH latest_system_state AS (
            SELECT DISTINCT ON (s.id)
                s.id AS system_id,
                ss.store_path AS current_store_path
            FROM systems s
            LEFT JOIN system_states ss ON ss.hostname = s.hostname
            ORDER BY s.id, ss.timestamp DESC NULLS LAST, ss.id DESC
        )
        SELECT COUNT(DISTINCT d.id)
        FROM systems s
        LEFT JOIN latest_system_state lss ON lss.system_id = s.id
        JOIN derivations d
          ON d.derivation_name =
                 COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname)
          AND d.store_path IS NOT NULL
          AND d.store_path = lss.current_store_path
        WHERE s.is_active = TRUE
          AND d.derivation_type = 'nixos'
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(ScanDeployedResult {
        has_more,
        total,
        next_cursor,
        rows: raw_items.into_iter().map(|(r, _)| r).collect(),
    })
}

pub async fn get_scan_queue_for_system(
    pool: &PgPool,
    system_id: Uuid,
    limit: i64,
) -> Result<Vec<ScanQueueRow>> {
    let rows = sqlx::query(
        r#"
        WITH latest_commit_per_flake AS (
            SELECT DISTINCT ON (flake_id) id AS commit_id, flake_id
            FROM commits
            ORDER BY flake_id, commit_timestamp DESC NULLS LAST, id DESC
        ),
        latest_per_derivation AS (
            SELECT DISTINCT ON (d.id)
                cs.id AS scan_id,
                d.derivation_name AS hostname,
                f.name AS flake_name,
                c.git_commit_hash AS commit_hash,
                c.flake_id,
                c.id AS commit_db_id,
                COALESCE(cs.status, 'never_scanned') AS status,
                cs.completed_at,
                cs.scheduled_at,
                COALESCE(cs.critical_count, 0)::int AS critical_count,
                COALESCE(cs.high_count, 0)::int AS high_count,
                COALESCE(cs.medium_count, 0)::int AS medium_count,
                COALESCE(cs.completed_at, cs.scheduled_at, cs.created_at) AS lifecycle_at
            FROM derivations d
            JOIN systems s
              ON d.derivation_name =
                     COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname)
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
            ) = 1) AS is_current,
            (lc.commit_id IS NOT NULL AND lpd.commit_db_id = lc.commit_id) AS is_latest_per_flake
        FROM latest_per_derivation lpd
        LEFT JOIN latest_commit_per_flake lc ON lc.flake_id = lpd.flake_id
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
            is_latest_per_flake: row.get("is_latest_per_flake"),
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
