use anyhow::Result;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row};
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
    /// Counts persisted runnable scans that await a worker claim.
    pub queued: i64,
    /// Scans waiting for their exact derivation to finish building.
    pub awaiting_build: i64,
    /// Scans waiting for an exact closure to become available from cache.
    pub awaiting_closure: i64,
    /// Counts operational derivations with completed evidence past the
    /// deployed freshness interval; `never` disables this count.
    pub stale: i64,
    /// Counts operational derivations without completed scan evidence.
    pub never_scanned: i64,
    /// Counts latest unarchived failed scans in the operational population.
    pub failed: i64,
    /// Reports completed-evidence coverage of the operational population.
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
    pub flake_name: Option<String>,
    pub environment: Option<String>,
    pub total_configs: i64,
    pub scanned: i64,
    pub stale: i64,
    pub needs_build: i64,
    pub unscanned: i64,
    pub current_crit: i64,
    pub current_high: i64,
    pub current_medium: i64,
    pub current_low: i64,
    /// Identifies exact schema-1 evidence for the current deployment.
    pub current_scan_id: Option<Uuid>,
    /// Is true when completed evidence exists only for non-current revisions.
    pub historical_evidence: bool,
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
    /// Is true when this derivation is the exact current deployment of an
    /// active system with matching flake and configuration identity.
    pub is_current: bool,
    /// Is true when this derivation belongs to the position-0 commit in its
    /// flake's ready branch snapshot.
    pub is_latest_per_flake: bool,
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
    /// Is true when another request-bound keyset page exists.
    pub has_more: bool,
    /// Continues after the last returned stable scan identity.
    pub next_cursor: Option<String>,
}

/// Selects a validated Completed status filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanRecordStatus {
    /// Includes every terminal status.
    All,
    /// Includes successful terminal scans.
    Completed,
    /// Includes failed terminal scans.
    Failed,
}

/// Selects a validated derivation revision class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanRecordRevision {
    /// Includes every revision class.
    All,
    /// Includes exact currently deployed revisions.
    Deployed,
    /// Includes the latest ready flake revision that is not deployed.
    Recent,
    /// Includes older flake revisions.
    Superseded,
}

/// Selects a validated Completed ordering key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanRecordSort {
    /// Orders by configuration name.
    Configuration,
    /// Orders by commit revision.
    Revision,
    /// Orders by terminal lifecycle status.
    Status,
    /// Orders lexicographically by critical, high, medium, and low counts.
    Severity,
    /// Orders by the authoritative terminal timestamp.
    Timestamp,
}

/// Selects a validated primary sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanRecordDirection {
    /// Orders the primary key from low to high.
    Asc,
    /// Orders the primary key from high to low.
    Desc,
}

/// Contains validated inputs for one scan-record page.
#[derive(Debug, Clone)]
pub struct ScanRecordRequest {
    /// Selects the lifecycle collection.
    pub collection: ScanRecordCollection,
    /// Includes archived terminal records when true.
    pub include_archived: bool,
    /// Restricts history to one active system.
    pub system_id: Option<Uuid>,
    /// Bounds returned rows to 1 through 500.
    pub limit: u16,
    /// Contains the normalized case-insensitive search value.
    pub search: Option<String>,
    /// Selects a terminal status.
    pub status: ScanRecordStatus,
    /// Selects a revision class.
    pub revision: ScanRecordRevision,
    /// Requires the latest ready flake revision when true.
    pub latest_only: bool,
    /// Selects the primary ordering key.
    pub sort: ScanRecordSort,
    /// Selects the primary ordering direction.
    pub direction: ScanRecordDirection,
    /// Continues from an opaque cursor returned by the previous page.
    pub after: Option<String>,
}

impl ScanRecordRequest {
    fn fingerprint(&self) -> String {
        let mut digest = Sha256::new();
        for component in [
            self.collection.as_str().to_string(),
            self.include_archived.to_string(),
            self.system_id.map(|id| id.to_string()).unwrap_or_default(),
            self.limit.to_string(),
            self.search.clone().unwrap_or_default(),
            scan_record_status_value(self.status).to_string(),
            scan_record_revision_value(self.revision).to_string(),
            self.latest_only.to_string(),
            scan_record_sort_value(self.sort).to_string(),
            scan_record_direction_value(self.direction).to_string(),
        ] {
            digest.update(component);
            digest.update([0]);
        }
        hex::encode(digest.finalize())
    }

    fn search_pattern(&self) -> Option<String> {
        self.search.as_ref().map(|value| {
            let escaped = value
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            format!("%{escaped}%")
        })
    }
}

/// Identifies malformed or request-incompatible scan-record cursors.
#[derive(Debug)]
pub struct InvalidScanRecordCursor;

impl std::fmt::Display for InvalidScanRecordCursor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("invalid scan record cursor")
    }
}

impl std::error::Error for InvalidScanRecordCursor {}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScanRecordCursor {
    version: u8,
    fingerprint: String,
    high_water_at: DateTime<Utc>,
    high_water_id: Uuid,
    configuration: String,
    revision: String,
    status_rank: i16,
    critical_count: i32,
    high_count: i32,
    medium_count: i32,
    low_count: i32,
    terminal_at: DateTime<Utc>,
    scan_id: Uuid,
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

/// Returns the persisted limit for recovering scans after a successful build.
///
/// # Errors
///
/// Returns a database error when the policy row cannot be read.
pub async fn get_post_build_recovery_window(pool: &PgPool) -> Result<String> {
    Ok(sqlx::query_scalar(
        "SELECT post_build_recovery_window FROM scan_schedule_policy WHERE id = 1",
    )
    .fetch_one(pool)
    .await?)
}

/// Updates the existing policy fields while preserving the stored recovery window.
///
/// # Errors
/// Returns a database error when the singleton row cannot be updated.
pub async fn update_scan_schedule_policy(
    pool: &PgPool,
    policy: &ScanSchedulePolicyRow,
) -> Result<()> {
    update_scan_schedule_policy_with_recovery(pool, policy, None).await
}

/// Updates the singleton policy without replacing the recovery window when omitted.
///
/// The `COALESCE` expression reads the stored value in the same row update. A
/// concurrent writer cannot lose a recovery-window change through an old PUT.
///
/// # Errors
/// Returns a database error when the row update fails, including a violated
/// recovery-window constraint.
pub async fn update_scan_schedule_policy_with_recovery(
    pool: &PgPool,
    policy: &ScanSchedulePolicyRow,
    post_build_recovery_window: Option<&str>,
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
            post_build_recovery_window = COALESCE($7, post_build_recovery_window),
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
    .bind(post_build_recovery_window)
    .execute(pool)
    .await?;
    Ok(())
}

/// Returns operational scan counts without removing historical scan evidence.
///
/// Current deployments, recoverable successful builds, and active scan requests
/// form the coverage population. A terminal failure is actionable only while
/// its derivation remains in that population and its latest scan is unarchived.
///
/// # Errors
///
/// Returns a database error if scan policy or scan history cannot be read.
pub async fn get_scan_stats(pool: &PgPool) -> Result<ScanStatsRow> {
    let row = sqlx::query(
        r#"
        WITH policy AS (
            SELECT CASE WHEN deployed_interval = 'never' THEN NULL::interval
                        ELSE deployed_interval::interval END AS deployed_window,
                   post_build_recovery_window::interval AS recovery_window,
                   on_build
            FROM scan_schedule_policy
            WHERE id = 1
        ),
        current_deployed AS (
            SELECT DISTINCT derivation.id AS derivation_id
            FROM systems system
            JOIN LATERAL (
                SELECT state.store_path
                FROM system_states state
                WHERE state.hostname = system.hostname
                ORDER BY state.timestamp DESC NULLS LAST, state.id DESC
                LIMIT 1
            ) current_state ON TRUE
            JOIN commits commit ON commit.flake_id = system.flake_id
            JOIN derivations derivation ON derivation.commit_id = commit.id
                AND derivation.derivation_type = 'nixos'
                AND derivation.derivation_name = COALESCE(
                    NULLIF(BTRIM(system.system_configuration_name), ''), system.hostname
                )
                AND NULLIF(BTRIM(derivation.store_path), '') = current_state.store_path
            WHERE system.is_active
        ),
        recoverable_builds AS (
            SELECT DISTINCT job.derivation_id
            FROM build_jobs job
            JOIN derivations derivation ON derivation.id = job.derivation_id
            CROSS JOIN policy
            WHERE policy.on_build
              AND derivation.derivation_type = 'nixos'
              AND job.status = 'success'
              AND job.completed_at IS NOT NULL
              AND job.completed_at > NOW() - policy.recovery_window
              AND NOT EXISTS (
                  SELECT 1 FROM build_jobs newer
                  WHERE newer.derivation_id = job.derivation_id
                    AND (newer.created_at, newer.id) > (job.created_at, job.id)
              )
              AND NOT EXISTS (
                  SELECT 1 FROM cve_scans evidence
                  WHERE evidence.derivation_id = job.derivation_id
                    AND evidence.status = 'completed'
                    AND evidence.completed_at IS NOT NULL
                    AND (evidence.completed_build_job_id = job.id
                        OR (evidence.source_trigger IS DISTINCT FROM 'post_build'
                            AND evidence.completed_at >= job.completed_at))
              )
        ),
        operational AS (
            SELECT derivation_id FROM current_deployed
            UNION
            SELECT derivation_id FROM recoverable_builds
            UNION
            SELECT scan.derivation_id FROM cve_scans scan
            JOIN derivations derivation ON derivation.id = scan.derivation_id
            WHERE derivation.derivation_type = 'nixos'
              AND scan.status IN ('awaiting_build', 'awaiting_closure', 'pending', 'in_progress')
        ),
        latest_lifecycle AS (
            SELECT DISTINCT ON (scan.derivation_id)
                scan.derivation_id, scan.status, archive.scan_id AS archived_scan_id
            FROM cve_scans scan
            JOIN operational scope ON scope.derivation_id = scan.derivation_id
            LEFT JOIN cve_scan_archives archive ON archive.scan_id = scan.id
            -- A late maintenance terminalization must not make an old intent
            -- supersede a newer manual or periodic scan's actual lifecycle.
            ORDER BY scan.derivation_id, scan.created_at DESC, scan.id DESC
        ),
        latest_completed AS (
            SELECT scan.derivation_id, MAX(scan.completed_at) AS completed_at
            FROM cve_scans scan
            JOIN operational scope ON scope.derivation_id = scan.derivation_id
            WHERE scan.status = 'completed' AND scan.completed_at IS NOT NULL
            GROUP BY scan.derivation_id
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
            COUNT(*) FILTER (WHERE ll.status = 'failed' AND ll.archived_scan_id IS NULL)::BIGINT AS failed,
            COUNT(*) FILTER (WHERE lc.completed_at IS NULL)::BIGINT AS never_scanned,
            COUNT(*) FILTER (
                WHERE lc.completed_at IS NOT NULL
                AND lc.completed_at < NOW() - (SELECT deployed_window FROM policy)
            )::BIGINT AS stale,
            CASE
                WHEN COUNT(*) = 0 THEN 0
                ELSE ROUND((COUNT(*) FILTER (WHERE lc.completed_at IS NOT NULL)::numeric / COUNT(*)::numeric) * 100)
            END::BIGINT AS coverage_percent
        FROM operational scope
        LEFT JOIN latest_lifecycle ll ON ll.derivation_id = scope.derivation_id
        LEFT JOIN latest_completed lc ON lc.derivation_id = scope.derivation_id
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
/// Returns an error when a continuation cursor is malformed or belongs to a
/// different request, or when PostgreSQL cannot load the records or counts.
pub async fn get_scan_records(
    pool: &PgPool,
    request: &ScanRecordRequest,
) -> Result<ScanRecordResult> {
    if request.collection == ScanRecordCollection::Completed {
        return get_completed_scan_records(pool, request).await;
    }
    if request.after.is_some() {
        return Err(InvalidScanRecordCursor.into());
    }
    let collection = request.collection;
    let include_archived = request.include_archived;
    let system_id = request.system_id;
    let limit = i64::from(request.limit);
    let base = r#"
        FROM cve_scans scan
        JOIN derivations derivation ON derivation.id = scan.derivation_id
        LEFT JOIN commits commit ON commit.id = derivation.commit_id
        LEFT JOIN flakes flake ON flake.id = commit.flake_id
        LEFT JOIN flake_branch_commit_snapshot latest_snapshot
          ON latest_snapshot.flake_id=commit.flake_id AND latest_snapshot.position=0
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
            EXISTS (
              SELECT 1 FROM systems system
              WHERE system.is_active
                AND system.flake_id=commit.flake_id
                AND COALESCE(NULLIF(BTRIM(system.system_configuration_name), ''), system.hostname)=derivation.derivation_name
                AND derivation.store_path IS NOT NULL AND BTRIM(derivation.store_path)<>''
                AND derivation.store_path=(
                  SELECT state.store_path FROM system_states state
                  WHERE state.hostname=system.hostname
                  ORDER BY state.timestamp DESC NULLS LAST,state.id DESC LIMIT 1)
            ) AS is_current,
            COALESCE(flake.snapshot_ready_at IS NOT NULL
              AND latest_snapshot.commit_id=commit.id,FALSE) AS is_latest_per_flake,
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
                is_current: row.get("is_current"),
                is_latest_per_flake: row.get("is_latest_per_flake"),
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
        has_more: false,
        next_cursor: None,
    })
}

const COMPLETED_RECORDS_CTE: &str = r#"
    WITH records AS (
        SELECT
            scan.id AS scan_id, scan.derivation_id,
            derivation.derivation_name AS hostname,
            derivation.derivation_path,
            flake.name AS flake_name, commit.git_commit_hash AS commit_hash,
            EXISTS (
              SELECT 1 FROM systems system
              WHERE system.is_active
                AND system.flake_id=commit.flake_id
                AND COALESCE(NULLIF(BTRIM(system.system_configuration_name), ''), system.hostname)=derivation.derivation_name
                AND derivation.store_path IS NOT NULL AND BTRIM(derivation.store_path)<>''
                AND derivation.store_path=(
                  SELECT state.store_path FROM system_states state
                  WHERE state.hostname=system.hostname
                  ORDER BY state.timestamp DESC NULLS LAST,state.id DESC LIMIT 1)
            ) AS is_current,
            COALESCE(flake.snapshot_ready_at IS NOT NULL
              AND latest_snapshot.commit_id=commit.id,FALSE) AS is_latest_per_flake,
            scan.status,
            CASE scan.status WHEN 'completed' THEN 0 WHEN 'failed' THEN 1 ELSE 2 END::smallint
              AS status_rank,
            scan.source_trigger, scan.created_at, scan.scheduled_at,
            COALESCE(
                scan.lease_started_at,
                (scan.scan_metadata ->> 'execution_started_at')::timestamptz
            ) AS started_at,
            scan.completed_at, scan.completed_at AS terminal_at,
            scan.scanner_name, scan.scanner_version,
            COALESCE(builder.name,
                CASE WHEN scan.scan_metadata ? 'execution_id' THEN 'server-local' END
            ) AS executor,
            scan.scan_metadata ->> 'error' AS failure,
            NULL::text AS wait_reason,
            scan.total_packages, scan.total_vulnerabilities,
            scan.critical_count, scan.high_count, scan.medium_count, scan.low_count,
            scan.scan_duration_ms, scan.attempts, archive.archived_at
        FROM cve_scans scan
        JOIN derivations derivation ON derivation.id = scan.derivation_id
        LEFT JOIN commits commit ON commit.id = derivation.commit_id
        LEFT JOIN flakes flake ON flake.id = commit.flake_id
        LEFT JOIN flake_branch_commit_snapshot latest_snapshot
          ON latest_snapshot.flake_id=commit.flake_id AND latest_snapshot.position=0
        LEFT JOIN builders builder ON builder.id = scan.lease_builder_id
        LEFT JOIN cve_scan_archives archive ON archive.scan_id = scan.id
        WHERE derivation.derivation_type = 'nixos'
          AND scan.status IN ('completed', 'failed')
          AND scan.completed_at IS NOT NULL
          AND ($1::uuid IS NULL OR EXISTS (
              SELECT 1
              FROM systems system
              WHERE system.id = $1
                AND system.is_active = TRUE
                AND system.flake_id = commit.flake_id
                AND COALESCE(
                    NULLIF(BTRIM(system.system_configuration_name), ''),
                    system.hostname
                ) = derivation.derivation_name
          ))
    ), classified AS (
        SELECT records.*,
               lower(hostname) AS configuration_key,
               COALESCE(commit_hash, '') AS revision_key,
               CASE
                 WHEN is_current THEN 'deployed'
                 WHEN is_latest_per_flake THEN 'recent'
                 ELSE 'superseded'
               END AS revision_class
        FROM records
    ), filtered AS (
        SELECT * FROM classified
        WHERE ($2::text IS NULL
               OR hostname ILIKE $2 ESCAPE '\'
               OR COALESCE(flake_name, '') ILIKE $2 ESCAPE '\'
               OR COALESCE(commit_hash, '') ILIKE $2 ESCAPE '\'
               OR scan_id::text ILIKE $2 ESCAPE '\'
               OR derivation_id::text ILIKE $2 ESCAPE '\'
               OR COALESCE(derivation_path, '') ILIKE $2 ESCAPE '\')
          AND ($3 = 'all' OR status = $3)
          AND ($4 = 'all' OR revision_class = $4)
          AND (NOT $5 OR is_latest_per_flake)
          AND ($6::timestamptz IS NULL OR
               (terminal_at, scan_id) <= ($6, $7::uuid))
    )
"#;

/// Returns a filtered, request-bound keyset page of terminal scan records.
///
/// A repeatable-read transaction keeps page metadata and rows coherent. The
/// cursor's high-water tuple excludes newer terminal inserts from every
/// continuation page. Archive state remains mutable and separate from scan
/// evidence.
///
/// # Errors
///
/// Returns an error when the cursor is malformed, belongs to a different
/// request, or PostgreSQL cannot load a coherent page.
async fn get_completed_scan_records(
    pool: &PgPool,
    request: &ScanRecordRequest,
) -> Result<ScanRecordResult> {
    let fingerprint = request.fingerprint();
    let cursor = request
        .after
        .as_deref()
        .map(decode_scan_record_cursor)
        .transpose()?;
    if cursor
        .as_ref()
        .is_some_and(|cursor| cursor.fingerprint != fingerprint)
    {
        return Err(InvalidScanRecordCursor.into());
    }

    let mut transaction = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *transaction)
        .await?;
    let search_pattern = request.search_pattern();
    let status = scan_record_status_value(request.status);
    let revision = scan_record_revision_value(request.revision);
    let cursor_high_water_at = cursor.as_ref().map(|cursor| cursor.high_water_at);
    let cursor_high_water_id = cursor.as_ref().map(|cursor| cursor.high_water_id);
    let metadata_sql = format!(
        r#"{COMPLETED_RECORDS_CTE}
           SELECT COUNT(*)::bigint AS total,
                  COUNT(*) FILTER (WHERE archived_at IS NOT NULL)::bigint AS archived,
                  (SELECT terminal_at FROM filtered
                   ORDER BY terminal_at DESC,scan_id DESC LIMIT 1) AS high_water_at,
                  (SELECT scan_id FROM filtered
                   ORDER BY terminal_at DESC,scan_id DESC LIMIT 1) AS high_water_id
           FROM filtered"#
    );
    let metadata = bind_completed_base(
        sqlx::query(&metadata_sql),
        request,
        search_pattern.as_deref(),
        status,
        revision,
        cursor_high_water_at,
        cursor_high_water_id,
    )
    .fetch_one(&mut *transaction)
    .await?;
    let total: i64 = metadata.get("total");
    let archived: i64 = metadata.get("archived");
    let high_water_at: Option<DateTime<Utc>> =
        cursor_high_water_at.or_else(|| metadata.get::<Option<DateTime<Utc>>, _>("high_water_at"));
    let high_water_id: Option<Uuid> =
        cursor_high_water_id.or_else(|| metadata.get::<Option<Uuid>, _>("high_water_id"));

    let (keyset, order_by) = completed_order_sql(request.sort, request.direction);
    let rows_sql = format!(
        r#"{COMPLETED_RECORDS_CTE}
           SELECT * FROM filtered
           WHERE ($8 OR archived_at IS NULL)
             AND ({keyset})
           ORDER BY {order_by}
           LIMIT $18"#
    );
    let query = bind_completed_base(
        sqlx::query(&rows_sql),
        request,
        search_pattern.as_deref(),
        status,
        revision,
        high_water_at,
        high_water_id,
    )
    .bind(request.include_archived)
    .bind(cursor.as_ref().map(|cursor| cursor.configuration.as_str()))
    .bind(cursor.as_ref().map(|cursor| cursor.revision.as_str()))
    .bind(cursor.as_ref().map(|cursor| cursor.status_rank))
    .bind(cursor.as_ref().map(|cursor| cursor.critical_count))
    .bind(cursor.as_ref().map(|cursor| cursor.high_count))
    .bind(cursor.as_ref().map(|cursor| cursor.medium_count))
    .bind(cursor.as_ref().map(|cursor| cursor.low_count))
    .bind(cursor.as_ref().map(|cursor| cursor.terminal_at))
    .bind(cursor.as_ref().map(|cursor| cursor.scan_id))
    .bind(i64::from(request.limit) + 1);
    let mut raw_rows = query.fetch_all(&mut *transaction).await?;
    let has_more = raw_rows.len() > usize::from(request.limit);
    if has_more {
        raw_rows.pop();
    }
    let next_cursor = if has_more {
        raw_rows
            .last()
            .map(|row| {
                encode_scan_record_cursor(&ScanRecordCursor {
                    version: 1,
                    fingerprint: fingerprint.clone(),
                    high_water_at: high_water_at.ok_or(InvalidScanRecordCursor)?,
                    high_water_id: high_water_id.ok_or(InvalidScanRecordCursor)?,
                    configuration: row.get("configuration_key"),
                    revision: row.get("revision_key"),
                    status_rank: row.get("status_rank"),
                    critical_count: row.get("critical_count"),
                    high_count: row.get("high_count"),
                    medium_count: row.get("medium_count"),
                    low_count: row.get("low_count"),
                    terminal_at: row.get("terminal_at"),
                    scan_id: row.get("scan_id"),
                })
            })
            .transpose()?
    } else {
        None
    };
    let rows = raw_rows.into_iter().map(scan_record_from_row).collect();
    transaction.commit().await?;
    Ok(ScanRecordResult {
        rows,
        total,
        hidden_archived: if request.include_archived {
            0
        } else {
            archived
        },
        has_more,
        next_cursor,
    })
}

fn bind_completed_base<'q>(
    query: sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>,
    request: &'q ScanRecordRequest,
    search_pattern: Option<&'q str>,
    status: &'static str,
    revision: &'static str,
    high_water_at: Option<DateTime<Utc>>,
    high_water_id: Option<Uuid>,
) -> sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments> {
    query
        .bind(request.system_id)
        .bind(search_pattern)
        .bind(status)
        .bind(revision)
        .bind(request.latest_only)
        .bind(high_water_at)
        .bind(high_water_id)
}

fn scan_record_status_value(status: ScanRecordStatus) -> &'static str {
    match status {
        ScanRecordStatus::All => "all",
        ScanRecordStatus::Completed => "completed",
        ScanRecordStatus::Failed => "failed",
    }
}

fn scan_record_revision_value(revision: ScanRecordRevision) -> &'static str {
    match revision {
        ScanRecordRevision::All => "all",
        ScanRecordRevision::Deployed => "deployed",
        ScanRecordRevision::Recent => "recent",
        ScanRecordRevision::Superseded => "superseded",
    }
}

fn scan_record_sort_value(sort: ScanRecordSort) -> &'static str {
    match sort {
        ScanRecordSort::Configuration => "configuration",
        ScanRecordSort::Revision => "revision",
        ScanRecordSort::Status => "status",
        ScanRecordSort::Severity => "severity",
        ScanRecordSort::Timestamp => "timestamp",
    }
}

fn scan_record_direction_value(direction: ScanRecordDirection) -> &'static str {
    match direction {
        ScanRecordDirection::Asc => "asc",
        ScanRecordDirection::Desc => "desc",
    }
}

fn completed_order_sql(
    sort: ScanRecordSort,
    direction: ScanRecordDirection,
) -> (&'static str, &'static str) {
    match (sort, direction) {
        (ScanRecordSort::Timestamp, ScanRecordDirection::Desc) => (
            "$16::timestamptz IS NULL OR (terminal_at,scan_id)<($16,$17::uuid)",
            "terminal_at DESC,scan_id DESC",
        ),
        (ScanRecordSort::Timestamp, ScanRecordDirection::Asc) => (
            "$16::timestamptz IS NULL OR (terminal_at,scan_id)>($16,$17::uuid)",
            "terminal_at ASC,scan_id ASC",
        ),
        (ScanRecordSort::Configuration, ScanRecordDirection::Asc) => (
            "$9::text IS NULL OR configuration_key COLLATE \"C\">$9 COLLATE \"C\" OR (configuration_key=$9 AND (terminal_at,scan_id)<($16,$17::uuid))",
            "configuration_key COLLATE \"C\" ASC,terminal_at DESC,scan_id DESC",
        ),
        (ScanRecordSort::Configuration, ScanRecordDirection::Desc) => (
            "$9::text IS NULL OR configuration_key COLLATE \"C\"<$9 COLLATE \"C\" OR (configuration_key=$9 AND (terminal_at,scan_id)<($16,$17::uuid))",
            "configuration_key COLLATE \"C\" DESC,terminal_at DESC,scan_id DESC",
        ),
        (ScanRecordSort::Revision, ScanRecordDirection::Asc) => (
            "$10::text IS NULL OR revision_key COLLATE \"C\">$10 COLLATE \"C\" OR (revision_key=$10 AND (terminal_at,scan_id)<($16,$17::uuid))",
            "revision_key COLLATE \"C\" ASC,terminal_at DESC,scan_id DESC",
        ),
        (ScanRecordSort::Revision, ScanRecordDirection::Desc) => (
            "$10::text IS NULL OR revision_key COLLATE \"C\"<$10 COLLATE \"C\" OR (revision_key=$10 AND (terminal_at,scan_id)<($16,$17::uuid))",
            "revision_key COLLATE \"C\" DESC,terminal_at DESC,scan_id DESC",
        ),
        (ScanRecordSort::Status, ScanRecordDirection::Asc) => (
            "$11::smallint IS NULL OR status_rank>$11 OR (status_rank=$11 AND (terminal_at,scan_id)<($16,$17::uuid))",
            "status_rank ASC,terminal_at DESC,scan_id DESC",
        ),
        (ScanRecordSort::Status, ScanRecordDirection::Desc) => (
            "$11::smallint IS NULL OR status_rank<$11 OR (status_rank=$11 AND (terminal_at,scan_id)<($16,$17::uuid))",
            "status_rank DESC,terminal_at DESC,scan_id DESC",
        ),
        (ScanRecordSort::Severity, ScanRecordDirection::Asc) => (
            "$12::integer IS NULL OR (critical_count,high_count,medium_count,low_count)>($12,$13,$14,$15) OR ((critical_count,high_count,medium_count,low_count)=($12,$13,$14,$15) AND (terminal_at,scan_id)<($16,$17::uuid))",
            "critical_count ASC,high_count ASC,medium_count ASC,low_count ASC,terminal_at DESC,scan_id DESC",
        ),
        (ScanRecordSort::Severity, ScanRecordDirection::Desc) => (
            "$12::integer IS NULL OR (critical_count,high_count,medium_count,low_count)<($12,$13,$14,$15) OR ((critical_count,high_count,medium_count,low_count)=($12,$13,$14,$15) AND (terminal_at,scan_id)<($16,$17::uuid))",
            "critical_count DESC,high_count DESC,medium_count DESC,low_count DESC,terminal_at DESC,scan_id DESC",
        ),
    }
}

fn decode_scan_record_cursor(value: &str) -> Result<ScanRecordCursor> {
    if value.is_empty() || value.len() > 8_192 {
        return Err(InvalidScanRecordCursor.into());
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| InvalidScanRecordCursor)?;
    let cursor: ScanRecordCursor =
        serde_json::from_slice(&bytes).map_err(|_| InvalidScanRecordCursor)?;
    if cursor.version != 1
        || cursor.fingerprint.len() != 64
        || cursor.configuration.len() > 4_096
        || cursor.revision.len() > 4_096
        || !(0..=2).contains(&cursor.status_rank)
    {
        return Err(InvalidScanRecordCursor.into());
    }
    Ok(cursor)
}

fn encode_scan_record_cursor(cursor: &ScanRecordCursor) -> Result<String> {
    let encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(cursor)?);
    if encoded.len() > 8_192 {
        return Err(InvalidScanRecordCursor.into());
    }
    Ok(encoded)
}

fn scan_record_from_row(row: sqlx::postgres::PgRow) -> ScanRecordRow {
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
        is_current: row.get("is_current"),
        is_latest_per_flake: row.get("is_latest_per_flake"),
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
                d.store_path,
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
            EXISTS (
              SELECT 1 FROM systems system
              WHERE system.is_active
                AND system.flake_id=lpd.flake_id
                AND COALESCE(NULLIF(BTRIM(system.system_configuration_name), ''), system.hostname)=lpd.hostname
                AND lpd.store_path IS NOT NULL AND BTRIM(lpd.store_path)<>''
                AND lpd.store_path=(SELECT state.store_path FROM system_states state
                  WHERE state.hostname=system.hostname
                  ORDER BY state.timestamp DESC NULLS LAST,state.id DESC LIMIT 1)
            ) AS is_current,
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
            SELECT deployed_interval,recent_interval,archived_interval,archived_enabled
            FROM scan_schedule_policy
            WHERE id = 1
        ),
        derivation_scope AS (
            SELECT
                s.id AS system_id,
                d.id AS derivation_id,
                s.hostname,
                f.name AS flake_name,
                d.store_path,
                CASE
                  WHEN d.store_path IS NOT NULL AND BTRIM(d.store_path)<>''
                    AND d.store_path=(SELECT state.store_path FROM system_states state
                      WHERE state.hostname=s.hostname
                      ORDER BY state.timestamp DESC NULLS LAST,state.id DESC LIMIT 1)
                    THEN 'deployed'
                  WHEN d.completed_at >= NOW()-INTERVAL '30 days' THEN 'recent'
                  ELSE 'archived'
                END AS lifecycle_class
            FROM systems s
            JOIN flakes f ON f.id=s.flake_id
            JOIN commits c ON c.flake_id=s.flake_id
            JOIN derivations d
              ON d.commit_id = c.id
             AND d.derivation_name = COALESCE(
                    NULLIF(BTRIM(s.system_configuration_name), ''),
                    s.hostname
                 )
             AND d.derivation_type = 'nixos'
            WHERE s.is_active = TRUE
        ),
        latest_completed_per_derivation AS (
            SELECT DISTINCT ON (d.id)
                d.id AS derivation_id,
                cs.id AS scan_id,
                cs.completed_at,
                cs.critical_count,
                cs.high_count,
                cs.medium_count,
                cs.low_count
            FROM derivations d
            JOIN cve_scans cs ON cs.derivation_id = d.id
            WHERE d.derivation_type = 'nixos'
              AND cs.status='completed' AND cs.completed_at IS NOT NULL
            ORDER BY d.id, cs.completed_at DESC,cs.id DESC
        ),
        latest_state AS (
            SELECT s.id AS system_id,state.store_path,state.generation,
                   state.generation_matches_current_store_path
            FROM systems s
            LEFT JOIN LATERAL (
                SELECT ss.store_path,ss.generation,ss.generation_matches_current_store_path
                FROM system_states ss
                WHERE ss.hostname = s.hostname
                ORDER BY ss.timestamp DESC NULLS LAST, ss.id DESC
                LIMIT 1
            ) state ON TRUE
            WHERE s.is_active = TRUE
        ),
        current_derivation AS (
            SELECT DISTINCT ON (system.id)
                   system.id AS system_id,derivation.id AS derivation_id
            FROM systems system
            JOIN latest_state state ON state.system_id=system.id
            JOIN derivations derivation
              ON derivation.store_path=state.store_path
             AND derivation.derivation_name=COALESCE(
                   NULLIF(BTRIM(system.system_configuration_name), ''),system.hostname)
             AND derivation.derivation_type='nixos'
            JOIN commits commit ON commit.id=derivation.commit_id
              AND commit.flake_id=system.flake_id
            WHERE system.is_active
            ORDER BY system.id,derivation.completed_at DESC NULLS LAST,derivation.id DESC
        ),
        current_exact AS (
            SELECT system.id AS system_id,derivation.id AS derivation_id,
                   scan.id AS scan_id,scan.critical_count,scan.high_count,
                   scan.medium_count,scan.low_count
            FROM systems system
            JOIN latest_state state ON state.system_id=system.id
              AND state.generation IS NOT NULL
              AND state.store_path IS NOT NULL AND BTRIM(state.store_path)<>''
              AND state.generation_matches_current_store_path IS TRUE
            JOIN evaluation_generation_snapshots retained
              ON retained.system_id=system.id AND retained.generation=state.generation
              AND retained.source_store_path=state.store_path
              AND retained.lineage_verified IS TRUE
            JOIN evaluation_snapshots artifact ON artifact.id=retained.snapshot_id
              AND artifact.commit_id=retained.commit_id
              AND artifact.configuration_name=retained.configuration_name
              AND artifact.lifecycle='available' AND artifact.integrity_version=1
            JOIN derivations derivation ON derivation.id=retained.derivation_id
              AND derivation.commit_id=retained.commit_id
              AND derivation.derivation_name=retained.configuration_name
              AND derivation.derivation_type='nixos'
              AND COALESCE(derivation.store_path,derivation.expected_store_path)=retained.source_store_path
            LEFT JOIN LATERAL (
              SELECT candidate.id,candidate.critical_count,candidate.high_count,
                     candidate.medium_count,candidate.low_count
              FROM cve_scans candidate
              WHERE candidate.derivation_id=derivation.id
                AND candidate.status='completed' AND candidate.completed_at IS NOT NULL
                AND candidate.evidence_schema_version=1
              ORDER BY candidate.completed_at DESC,candidate.id DESC LIMIT 1
            ) scan ON TRUE
        )
        SELECT
            s.id AS system_id,
            scope.hostname,
            MAX(scope.flake_name) AS flake_name,
            MAX(e.name) AS environment,
            COUNT(*)::BIGINT AS total_configs,
            COUNT(*) FILTER (WHERE scope.store_path IS NOT NULL AND BTRIM(scope.store_path)<>''
              AND completed.completed_at IS NOT NULL
              AND NOT CASE scope.lifecycle_class
                WHEN 'deployed' THEN policy.deployed_interval<>'never'
                  AND NOW()-completed.completed_at>policy.deployed_interval::interval
                WHEN 'recent' THEN policy.recent_interval<>'never'
                  AND NOW()-completed.completed_at>policy.recent_interval::interval
                ELSE policy.archived_enabled AND policy.archived_interval<>'never'
                  AND NOW()-completed.completed_at>policy.archived_interval::interval
              END)::BIGINT AS scanned,
            COUNT(*) FILTER (WHERE scope.store_path IS NOT NULL AND BTRIM(scope.store_path)<>''
              AND completed.completed_at IS NOT NULL
              AND CASE scope.lifecycle_class
                WHEN 'deployed' THEN policy.deployed_interval<>'never'
                  AND NOW()-completed.completed_at>policy.deployed_interval::interval
                WHEN 'recent' THEN policy.recent_interval<>'never'
                  AND NOW()-completed.completed_at>policy.recent_interval::interval
                ELSE policy.archived_enabled AND policy.archived_interval<>'never'
                  AND NOW()-completed.completed_at>policy.archived_interval::interval
              END)::BIGINT AS stale,
            COUNT(*) FILTER (WHERE scope.store_path IS NULL OR BTRIM(scope.store_path)='')::BIGINT AS needs_build,
            COUNT(*) FILTER (WHERE scope.store_path IS NOT NULL AND BTRIM(scope.store_path)<>''
              AND completed.completed_at IS NULL)::BIGINT AS unscanned,
            COALESCE(MAX(current_exact.critical_count),0)::BIGINT AS current_crit,
            COALESCE(MAX(current_exact.high_count),0)::BIGINT AS current_high,
            COALESCE(MAX(current_exact.medium_count),0)::BIGINT AS current_medium,
            COALESCE(MAX(current_exact.low_count),0)::BIGINT AS current_low,
            (ARRAY_AGG(current_exact.scan_id) FILTER (WHERE current_exact.scan_id IS NOT NULL))[1]
              AS current_scan_id,
            (BOOL_OR(completed.scan_id IS NOT NULL)
              AND COUNT(current_exact.scan_id)=0) AS historical_evidence,
            MAX(current_derivation.derivation_id) AS current_derivation_id
        FROM derivation_scope scope
        LEFT JOIN latest_completed_per_derivation completed ON completed.derivation_id=scope.derivation_id
        JOIN systems s ON s.id=scope.system_id
        LEFT JOIN environments e ON e.id = s.environment_id
        LEFT JOIN current_exact ON current_exact.system_id=s.id
        LEFT JOIN current_derivation ON current_derivation.system_id=s.id
        CROSS JOIN policy
        WHERE s.is_active = TRUE
        GROUP BY s.id,scope.hostname
        ORDER BY total_configs DESC,scope.hostname ASC
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
            flake_name: row.get("flake_name"),
            environment: row.get("environment"),
            total_configs: row.get("total_configs"),
            scanned: row.get("scanned"),
            stale: row.get("stale"),
            needs_build: row.get("needs_build"),
            unscanned: row.get("unscanned"),
            current_crit: row.get("current_crit"),
            current_high: row.get("current_high"),
            current_medium: row.get("current_medium"),
            current_low: row.get("current_low"),
            current_scan_id: row.get("current_scan_id"),
            historical_evidence: row.get("historical_evidence"),
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
