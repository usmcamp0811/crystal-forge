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
    /// Derivations waiting for a scan through either the persisted operator
    /// queue or the worker's dynamic post-build and stale-rescan selectors.
    pub queued: i64,
    /// Scans waiting for their exact derivation to finish building.
    pub awaiting_build: i64,
    /// Scans waiting for an exact closure to become available from cache.
    pub awaiting_closure: i64,
    pub stale: i64,
    pub never_scanned: i64,
    pub failed: i64,
    pub coverage_percent: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanQueueRow {
    /// Identifies the exact derivation represented by this row.
    pub derivation_id: i32,
    /// Is `true` when the derivation has a built store path that can be scanned.
    pub rescan_eligible: bool,
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
    /// Identifies the persisted source that created the latest scan lifecycle.
    pub source_trigger: Option<String>,
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
    /// Identifies the derivation in the system's latest reported store path.
    pub current_derivation_id: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanActivityRow {
    pub at: Option<DateTime<Utc>>,
    pub name: String,
    pub event: String,
    pub detail: String,
    pub status: String,
}

/// Selects the lifecycle partition returned by the admin scan-record query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanRecordCollection {
    /// Returns nonterminal waiting, queued, and running rows.
    Active,
    /// Returns completed and failed rows.
    Completed,
    /// Returns every persisted lifecycle row.
    History,
}

impl ScanRecordCollection {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Completed => "completed",
            Self::History => "history",
        }
    }
}

/// One exact persisted scan lifecycle for admin scanning views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanRecordRow {
    /// Identifies the immutable scan lifecycle.
    pub scan_id: Uuid,
    /// Identifies the exact scanned derivation.
    pub derivation_id: i32,
    /// Contains the configuration name recorded on the derivation.
    pub hostname: String,
    /// Contains the owning flake name when the derivation belongs to a flake.
    pub flake_name: Option<String>,
    /// Contains the exact commit hash when available.
    pub commit_hash: Option<String>,
    /// Contains the persisted lifecycle state.
    pub status: String,
    /// Contains the canonical presentation trigger.
    pub source_trigger: Option<String>,
    /// Contains the time the lifecycle row was created.
    pub created_at: DateTime<Utc>,
    /// Contains the requested schedule time when available.
    pub scheduled_at: Option<DateTime<Utc>>,
    /// Contains the authoritative execution start time when execution began.
    pub started_at: Option<DateTime<Utc>>,
    /// Contains the terminal time when available.
    pub completed_at: Option<DateTime<Utc>>,
    /// Contains the scanner implementation name.
    pub scanner_name: String,
    /// Contains the scanner version when known.
    pub scanner_version: Option<String>,
    /// Contains the execution identity without exposing lease credentials.
    pub executor: Option<String>,
    /// Contains a bounded redacted terminal failure summary.
    pub failure: Option<String>,
    /// Explains an authoritative waiting state.
    pub wait_reason: Option<String>,
    /// Counts all packages examined by the scanner.
    pub total_packages: i32,
    /// Counts all vulnerability findings.
    pub total_vulnerabilities: i32,
    /// Counts critical findings.
    pub critical_count: i32,
    /// Counts high findings.
    pub high_count: i32,
    /// Counts medium findings.
    pub medium_count: i32,
    /// Counts low findings.
    pub low_count: i32,
    /// Contains scanner duration in milliseconds when recorded.
    pub scan_duration_ms: Option<i32>,
    /// Counts execution attempts.
    pub attempts: i32,
    /// Contains archive time when hidden by an administrator.
    pub archived_at: Option<DateTime<Utc>>,
    /// Is always false until execution ownership supports safe cancellation.
    pub cancellable: bool,
}

/// Returns exact scan records and archive-aware count metadata.
#[derive(Debug, Clone)]
pub struct ScanRecordResult {
    /// Contains deterministically ordered records.
    pub rows: Vec<ScanRecordRow>,
    /// Counts matching records before the response limit and archive filter.
    pub total: i64,
    /// Counts matching archived records hidden from this response.
    pub hidden_archived: i64,
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
            (
                SELECT COUNT(DISTINCT derivation_id)::BIGINT
                FROM cve_scans
                WHERE status = 'in_progress'
            ) AS scanning,
            (SELECT COUNT(*) FROM cve_scans WHERE status = 'pending')::BIGINT AS queued,
            (SELECT COUNT(*) FROM cve_scans WHERE status = 'awaiting_build')::BIGINT AS awaiting_build,
            (SELECT COUNT(*) FROM cve_scans WHERE status = 'awaiting_closure')::BIGINT AS awaiting_closure,
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
        awaiting_build: row.get("awaiting_build"),
        awaiting_closure: row.get("awaiting_closure"),
        stale: row.get("stale"),
        never_scanned: row.get("never_scanned"),
        failed: row.get("failed"),
        coverage_percent: row.get("coverage_percent"),
    })
}

/// Returns one lifecycle partition with complete exact history and archive state.
///
/// `system_id` scopes rows to the active system's exact flake and effective
/// configuration. Archive filtering never deletes or mutates scan evidence.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot load the records or counts.
pub async fn get_scan_records(
    pool: &PgPool,
    collection: ScanRecordCollection,
    include_archived: bool,
    system_id: Option<Uuid>,
    limit: i64,
) -> Result<ScanRecordResult> {
    let base = r#"
        FROM cve_scans scan
        JOIN derivations derivation ON derivation.id = scan.derivation_id
        LEFT JOIN commits commit ON commit.id = derivation.commit_id
        LEFT JOIN flakes flake ON flake.id = commit.flake_id
        LEFT JOIN builders builder ON builder.id = scan.lease_builder_id
        LEFT JOIN cve_scan_archives archive ON archive.scan_id = scan.id
        WHERE derivation.derivation_type = 'nixos'
          AND (
              $1 = 'history'
              OR ($1 = 'active' AND scan.status IN (
                  'awaiting_build', 'awaiting_closure', 'pending', 'in_progress'
              ))
              OR ($1 = 'completed' AND scan.status IN ('completed', 'failed'))
          )
          AND ($2::uuid IS NULL OR EXISTS (
              SELECT 1
              FROM systems system
              WHERE system.id = $2
                AND system.is_active = TRUE
                AND system.flake_id = commit.flake_id
                AND COALESCE(
                    NULLIF(BTRIM(system.system_configuration_name), ''),
                    system.hostname
                ) = derivation.derivation_name
          ))
    "#;
    let count_sql = format!(
        "SELECT COUNT(*)::bigint AS total, COUNT(*) FILTER (WHERE archive.scan_id IS NOT NULL)::bigint AS archived {base}"
    );
    let counts = sqlx::query(&count_sql)
        .bind(collection.as_str())
        .bind(system_id)
        .fetch_one(pool)
        .await?;
    let total: i64 = counts.get("total");
    let archived: i64 = counts.get("archived");

    let rows_sql = format!(
        r#"
        SELECT
            scan.id AS scan_id, scan.derivation_id,
            derivation.derivation_name AS hostname,
            flake.name AS flake_name, commit.git_commit_hash AS commit_hash,
            scan.status, scan.source_trigger, scan.created_at, scan.scheduled_at,
            COALESCE(
                scan.lease_started_at,
                (scan.scan_metadata ->> 'execution_started_at')::timestamptz
            ) AS started_at,
            scan.completed_at, scan.scanner_name, scan.scanner_version,
            COALESCE(builder.name,
                CASE WHEN scan.scan_metadata ? 'execution_id' THEN 'server-local' END
            ) AS executor,
            scan.scan_metadata ->> 'error' AS failure,
            CASE scan.status
                WHEN 'awaiting_build' THEN 'Build output is not available.'
                WHEN 'awaiting_closure' THEN 'A completed cache closure is not available.'
            END AS wait_reason,
            scan.total_packages, scan.total_vulnerabilities,
            scan.critical_count, scan.high_count, scan.medium_count, scan.low_count,
            scan.scan_duration_ms, scan.attempts, archive.archived_at
        {base}
          AND ($3 OR archive.scan_id IS NULL)
        ORDER BY
            CASE scan.status
                WHEN 'in_progress' THEN 0
                WHEN 'pending' THEN 1
                WHEN 'awaiting_closure' THEN 2
                WHEN 'awaiting_build' THEN 3
                WHEN 'failed' THEN 4
                WHEN 'completed' THEN 5
                ELSE 6
            END,
            COALESCE(scan.completed_at, scan.scheduled_at, scan.created_at) DESC,
            scan.id DESC
        LIMIT $4
        "#
    );
    let rows = sqlx::query(&rows_sql)
        .bind(collection.as_str())
        .bind(system_id)
        .bind(include_archived)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    let rows = rows
        .into_iter()
        .map(|row| {
            let failure = row
                .get::<Option<String>, _>("failure")
                .map(|value| crate::security::snapshot_redaction::redact_text(&value))
                .map(|value| value.chars().take(2048).collect());
            ScanRecordRow {
                scan_id: row.get("scan_id"),
                derivation_id: row.get("derivation_id"),
                hostname: row.get("hostname"),
                flake_name: row.get("flake_name"),
                commit_hash: row.get("commit_hash"),
                status: row.get("status"),
                source_trigger: crate::queries::cve_scans::present_scan_trigger(
                    row.get::<Option<String>, _>("source_trigger").as_deref(),
                ),
                created_at: row.get("created_at"),
                scheduled_at: row.get("scheduled_at"),
                started_at: row.get("started_at"),
                completed_at: row.get("completed_at"),
                scanner_name: row.get("scanner_name"),
                scanner_version: row.get("scanner_version"),
                executor: row.get("executor"),
                failure,
                wait_reason: row.get("wait_reason"),
                total_packages: row.get("total_packages"),
                total_vulnerabilities: row.get("total_vulnerabilities"),
                critical_count: row.get("critical_count"),
                high_count: row.get("high_count"),
                medium_count: row.get("medium_count"),
                low_count: row.get("low_count"),
                scan_duration_ms: row.get("scan_duration_ms"),
                attempts: row.get("attempts"),
                archived_at: row.get("archived_at"),
                cancellable: false,
            }
        })
        .collect();
    Ok(ScanRecordResult {
        rows,
        total,
        hidden_archived: if include_archived { 0 } else { archived },
    })
}

/// Sets archive presentation state for a bounded set of eligible terminal scans.
///
/// The operation is idempotent. It writes only `cve_scan_archives`, never the
/// immutable scan row, findings, or diagnostics.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot apply the archive-state change.
pub async fn set_scan_archive_state(
    pool: &PgPool,
    scan_ids: &[Uuid],
    archived: bool,
    actor_id: Uuid,
) -> Result<u64> {
    if archived {
        let result = sqlx::query(
            r#"
            INSERT INTO cve_scan_archives (scan_id, archived_by)
            SELECT id, $2
            FROM cve_scans
            WHERE id = ANY($1) AND status IN ('completed', 'failed')
            ON CONFLICT (scan_id) DO NOTHING
            "#,
        )
        .bind(scan_ids)
        .bind(actor_id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    } else {
        let result = sqlx::query("DELETE FROM cve_scan_archives WHERE scan_id = ANY($1)")
            .bind(scan_ids)
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }
}

/// Returns the latest scan lifecycle for each NixOS derivation.
///
/// Standalone derivations remain visible with no flake or commit display data.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot load the queue.
pub async fn get_scan_queue(pool: &PgPool, limit: i64) -> Result<Vec<ScanQueueRow>> {
    let rows = sqlx::query(
        r#"
        WITH latest_commit_per_flake AS (
            SELECT snapshot.flake_id, snapshot.commit_id
            FROM flake_branch_commit_snapshot snapshot
            JOIN flakes f ON f.id = snapshot.flake_id
            WHERE f.snapshot_ready_at IS NOT NULL
              AND snapshot.position = 0
        ),
        latest_per_derivation AS (
            SELECT DISTINCT ON (d.id)
                cs.id AS scan_id,
                d.id AS derivation_id,
                (d.store_path IS NOT NULL AND BTRIM(d.store_path) <> '') AS rescan_eligible,
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
                , cs.source_trigger
            FROM derivations d
            LEFT JOIN commits c ON c.id = d.commit_id
            LEFT JOIN cve_scans cs ON cs.derivation_id = d.id
            LEFT JOIN flakes f ON f.id = c.flake_id
            WHERE d.derivation_type = 'nixos'
            ORDER BY d.id, COALESCE(cs.completed_at, cs.scheduled_at, cs.created_at) DESC NULLS LAST
        )
        SELECT
            scan_id,
            derivation_id,
            rescan_eligible,
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
            , source_trigger
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
            derivation_id: row.get("derivation_id"),
            rescan_eligible: row.get("rescan_eligible"),
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
            source_trigger: crate::queries::cve_scans::present_scan_trigger(
                row.get::<Option<String>, _>("source_trigger").as_deref(),
            ),
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

/// Returns cursor-paginated derivations deployed on active systems.
///
/// The cursor is a base64url-encoded `{hostname}\x00{derivation_id}` payload so
/// that the cursor is opaque, unambiguous, and handles config names with `:`.
/// The latest reported store path determines each system's current derivation.
/// The normalized configuration name permits a NixOS configuration name to
/// differ from its system hostname. Systems without a scan remain present with
/// safe default counts.
///
/// # Errors
///
/// Returns an error when the cursor is malformed or PostgreSQL cannot load the
/// deployed configurations.
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
            SELECT snapshot.flake_id, snapshot.commit_id
            FROM flake_branch_commit_snapshot snapshot
            JOIN flakes f ON f.id = snapshot.flake_id
            WHERE f.snapshot_ready_at IS NOT NULL
              AND snapshot.position = 0
        ),
        deployed_derivations AS (
            SELECT DISTINCT ON (d.id)
                cs.id                      AS scan_id,
                d.id                       AS derivation_id,
                (BTRIM(d.store_path) <> '') AS rescan_eligible,
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
                , cs.source_trigger
            FROM systems s
            LEFT JOIN latest_system_state lss ON lss.system_id = s.id
            JOIN derivations d
              ON d.derivation_name =
                     COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname)
              AND d.store_path IS NOT NULL
              AND BTRIM(d.store_path) <> ''
              AND d.store_path = lss.current_store_path
            LEFT JOIN cve_scans cs ON cs.derivation_id = d.id
            JOIN commits c ON c.id = d.commit_id AND c.flake_id = s.flake_id
            LEFT JOIN flakes f ON f.id = c.flake_id
            WHERE s.is_active = TRUE
              AND d.derivation_type = 'nixos'
            ORDER BY d.id,
                     COALESCE(cs.completed_at, cs.scheduled_at, cs.created_at) DESC NULLS LAST
        )
        SELECT
            scan_id,
            derivation_id,
            rescan_eligible,
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
            , source_trigger
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
                    derivation_id: row.get("derivation_id"),
                    rescan_eligible: row.get("rescan_eligible"),
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
                    source_trigger: crate::queries::cve_scans::present_scan_trigger(
                        row.get::<Option<String>, _>("source_trigger").as_deref(),
                    ),
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
          AND BTRIM(d.store_path) <> ''
          AND d.store_path = lss.current_store_path
        JOIN commits c ON c.id = d.commit_id AND c.flake_id = s.flake_id
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

/// Returns scan history for one active system's exact flake and configuration.
///
/// The latest reported `system_states.store_path` determines `is_current`.
/// A newer derivation from another flake cannot enter this history.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot load the system or its scan history.
pub async fn get_scan_queue_for_system(
    pool: &PgPool,
    system_id: Uuid,
    limit: i64,
) -> Result<Vec<ScanQueueRow>> {
    let rows = sqlx::query(
        r#"
        WITH latest_commit_per_flake AS (
            SELECT snapshot.flake_id, snapshot.commit_id
            FROM flake_branch_commit_snapshot snapshot
            JOIN flakes f ON f.id = snapshot.flake_id
            WHERE f.snapshot_ready_at IS NOT NULL
              AND snapshot.position = 0
        ),
        selected_system AS (
            SELECT
                s.id,
                s.flake_id,
                s.hostname,
                COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname) AS config_name,
                (
                    SELECT ss.store_path
                    FROM system_states ss
                    WHERE ss.hostname = s.hostname
                    ORDER BY ss.timestamp DESC NULLS LAST, ss.id DESC
                    LIMIT 1
                ) AS current_store_path
            FROM systems s
            WHERE s.id = $1
              AND s.is_active = TRUE
        ),
        latest_per_derivation AS (
            SELECT DISTINCT ON (d.id)
                cs.id AS scan_id,
                d.id AS derivation_id,
                (d.store_path IS NOT NULL AND BTRIM(d.store_path) <> '') AS rescan_eligible,
                ss.hostname,
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
                COALESCE(cs.completed_at, cs.scheduled_at, cs.created_at) AS lifecycle_at,
                COALESCE(d.store_path = ss.current_store_path, FALSE) AS is_current,
                cs.source_trigger
            FROM derivations d
            JOIN selected_system ss ON d.derivation_name = ss.config_name
            JOIN commits c ON c.id = d.commit_id AND c.flake_id = ss.flake_id
            LEFT JOIN cve_scans cs ON cs.derivation_id = d.id
            LEFT JOIN flakes f ON f.id = c.flake_id
            WHERE d.derivation_type = 'nixos'
            ORDER BY d.id, COALESCE(cs.completed_at, cs.scheduled_at, cs.created_at) DESC NULLS LAST
        )
        SELECT
            scan_id,
            derivation_id,
            rescan_eligible,
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
            is_current,
            (lc.commit_id IS NOT NULL AND lpd.commit_db_id = lc.commit_id) AS is_latest_per_flake
            , source_trigger
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
            derivation_id: row.get("derivation_id"),
            rescan_eligible: row.get("rescan_eligible"),
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
            source_trigger: crate::queries::cve_scans::present_scan_trigger(
                row.get::<Option<String>, _>("source_trigger").as_deref(),
            ),
        })
        .collect())
}

/// Returns aggregate scan state for active systems with exact current identities.
///
/// Each aggregate includes only derivations from the system's flake and
/// configuration. `current_derivation_id` matches the latest reported store
/// path when that path resolves to an eligible derivation.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot load the system aggregates.
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
            SELECT DISTINCT ON (s.id, d.id)
                s.id AS system_id,
                d.id AS derivation_id,
                s.hostname,
                d.store_path,
                cs.status,
                cs.scheduled_at,
                cs.created_at,
                cs.completed_at
            FROM systems s
            JOIN commits c ON c.flake_id = s.flake_id
            JOIN derivations d
              ON d.commit_id = c.id
             AND d.derivation_name = COALESCE(
                    NULLIF(BTRIM(s.system_configuration_name), ''),
                    s.hostname
                 )
             AND d.derivation_type = 'nixos'
            LEFT JOIN cve_scans cs ON cs.derivation_id = d.id
            WHERE s.is_active = TRUE
            ORDER BY s.id, d.id,
                     COALESCE(cs.completed_at, cs.scheduled_at, cs.created_at) DESC NULLS LAST
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
        ),
        current_derivation AS (
            SELECT DISTINCT ON (s.id)
                s.id AS system_id,
                d.id AS derivation_id
            FROM systems s
            JOIN LATERAL (
                SELECT ss.store_path
                FROM system_states ss
                WHERE ss.hostname = s.hostname
                ORDER BY ss.timestamp DESC NULLS LAST, ss.id DESC
                LIMIT 1
            ) state ON TRUE
            JOIN derivations d
              ON d.store_path = state.store_path
             AND d.derivation_name = COALESCE(
                    NULLIF(BTRIM(s.system_configuration_name), ''),
                    s.hostname
                 )
             AND d.derivation_type = 'nixos'
            JOIN commits c ON c.id = d.commit_id AND c.flake_id = s.flake_id
            WHERE s.is_active = TRUE
            ORDER BY s.id, d.completed_at DESC NULLS LAST, d.id DESC
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
            COALESCE(MAX(lc.critical_count) FILTER (
                WHERE ll.derivation_id = cd.derivation_id
            ), 0)::BIGINT AS current_crit,
            COALESCE(MAX(lc.high_count) FILTER (
                WHERE ll.derivation_id = cd.derivation_id
            ), 0)::BIGINT AS current_high
            , cd.derivation_id AS current_derivation_id
        FROM latest_lifecycle_per_derivation ll
        LEFT JOIN latest_completed_per_derivation lc ON lc.derivation_id = ll.derivation_id
        JOIN systems s ON s.id = ll.system_id
        LEFT JOIN environments e ON e.id = s.environment_id
        LEFT JOIN current_derivation cd ON cd.system_id = s.id
        WHERE s.is_active = TRUE
        GROUP BY s.id, ll.hostname, cd.derivation_id
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
            current_derivation_id: row.get("current_derivation_id"),
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
