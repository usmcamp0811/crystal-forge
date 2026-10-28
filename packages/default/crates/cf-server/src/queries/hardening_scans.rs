//! Database queries for hardening scans.

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::api::models::SystemCveInventorySelection;
use crate::hardening::scanner::ScanResult;
use crate::hardening::types::{
    FleetHardeningSummary, HardeningJustification, HardeningScan, RiskLevel,
    ServiceHardeningResult, SystemHardeningPosture, TopVulnerableService,
};
use crate::models::hardening_scans::ScanStatus;

/// Bounds the sanitized failure text returned with a hardening attempt.
///
/// The limit keeps one attempt summary small enough to embed in every exact
/// inventory response without turning the response into a log transport.
const MAX_HARDENING_FAILURE_CHARS: usize = 400;

/// Names the durable reason a hardening scan row exists.
///
/// The value is immutable for the life of the row and is persisted in
/// `hardening_scans.source_trigger` by migration 0274. It separates work a
/// person requested from work the server admitted on its own, so operators can
/// tell evidence they asked for from evidence the build pipeline produced.
/// Existing rows use the persisted `legacy` value because their original
/// admission path cannot be proved; new code never creates that value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardeningScanTrigger {
    /// A person or an API client requested this scan.
    Manual,
    /// A successful exact NixOS build admitted this scan in the transaction
    /// that recorded the build success.
    PostBuild,
    /// The hardening worker admitted this scan for an already successfully
    /// built target that had no hardening evidence.
    Backfill,
}

impl HardeningScanTrigger {
    /// Returns the exact persisted `source_trigger` value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::PostBuild => "post_build",
            Self::Backfill => "backfill",
        }
    }
}

/// Names the lifecycle state of the newest hardening attempt for a derivation.
///
/// The absence of an attempt is represented by `None` in
/// [`SystemHardeningInventory::attempt`] and means "never scanned". The states
/// below therefore describe only derivations that have at least one attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardeningAttemptState {
    /// The attempt is admitted and waits for the serial worker.
    Queued,
    /// The serial worker owns the attempt and is running `nix eval`.
    Scanning,
    /// The attempt reached a terminal failure. Earlier completed evidence, when
    /// it exists, is still reported separately and is not discarded.
    Failed,
    /// The attempt produced evidence.
    Completed,
}

impl HardeningAttemptState {
    /// Returns the stable wire value for this state.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Scanning => "scanning",
            Self::Failed => "failed",
            Self::Completed => "completed",
        }
    }

    /// Returns true while the attempt can still change without a new request.
    ///
    /// Callers use this to suppress a duplicate scan request and to decide
    /// whether a bounded status poll is still useful.
    pub fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Scanning)
    }

    fn from_status(status: &str) -> Option<Self> {
        match status {
            "pending" => Some(Self::Queued),
            "in_progress" => Some(Self::Scanning),
            "failed" => Some(Self::Failed),
            "completed" => Some(Self::Completed),
            _ => None,
        }
    }
}

/// Describes the newest hardening attempt for one exact derivation.
///
/// This is lifecycle information, not evidence. A `Failed` or `Queued` attempt
/// never replaces or invalidates the completed evidence reported by
/// [`SystemHardeningInventory::source`].
#[derive(Debug, Clone)]
pub struct SystemHardeningAttempt {
    /// Identifies the newest attempt row.
    pub scan_id: Uuid,
    /// Gives the lifecycle state of that attempt.
    pub state: HardeningAttemptState,
    /// Gives the immutable admission reason recorded for that attempt.
    pub source_trigger: String,
    /// Gives the admission time.
    pub scheduled_at: Option<DateTime<Utc>>,
    /// Gives the execution start time, when execution started.
    pub started_at: Option<DateTime<Utc>>,
    /// Gives the terminal time, when the attempt reached a terminal state.
    pub completed_at: Option<DateTime<Utc>>,
    /// Counts execution attempts recorded on the row.
    pub attempts: i32,
    /// Gives redacted, bounded failure text for a failed attempt.
    ///
    /// The value is `None` for every non-failed state and may also be `None`
    /// for a failed attempt that persisted no message.
    pub error: Option<String>,
}

/// Contains one exact hardening inventory selection and its completed evidence.
#[derive(Debug)]
pub struct SystemHardeningInventory {
    /// Gives the normalized server-validated target identity.
    pub selection: SystemCveInventorySelection,
    /// Identifies the exact derivation when target resolution succeeds.
    pub derivation_id: Option<i32>,
    /// Gives the latest completed scan for the exact derivation.
    pub source: Option<HardeningScan>,
    /// Contains service rows belonging to exactly `source`.
    pub services: Vec<ServiceHardeningResult>,
    /// Gives the newest attempt for the exact derivation, or `None` when the
    /// derivation was never scanned.
    ///
    /// This field is independent of `source`. A later failed attempt leaves an
    /// earlier completed `source` in place.
    pub attempt: Option<SystemHardeningAttempt>,
    /// Is true for every historical target.
    pub read_only: bool,
}

/// Reports that a historical hardening target is not owned by the system.
#[derive(Debug, PartialEq, Eq)]
pub struct SystemHardeningTargetUnavailable;

impl std::fmt::Display for SystemHardeningTargetUnavailable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("system hardening target unavailable")
    }
}

impl std::error::Error for SystemHardeningTargetUnavailable {}

/// Parses the shared system inventory target syntax.
///
/// # Errors
///
/// Returns a static client-facing message when the target and target identity
/// do not form a supported selection.
pub fn parse_system_hardening_selection(
    target: Option<&str>,
    target_id: Option<&str>,
) -> std::result::Result<SystemCveInventorySelection, &'static str> {
    SystemCveInventorySelection::from_target_params(target, target_id)
}

/// Fetches hardening evidence for one exact server-authorized target.
///
/// Current selection uses only the latest system state. Its store path must
/// equal the derivation's realized store path, and the derivation must match the
/// system flake and effective configuration name. The query never falls back to
/// an older derivation or scan. Historical selections revalidate the same
/// system, flake, configuration, and NixOS boundaries as CVE inventory.
///
/// # Errors
///
/// Returns a database error when the consistent snapshot cannot be read.
/// Returns [`SystemHardeningTargetUnavailable`] when a historical identity is
/// absent or belongs outside the selected system boundary.
pub async fn fetch_system_hardening_inventory(
    pool: &PgPool,
    system_id: Uuid,
    selection: SystemCveInventorySelection,
) -> Result<SystemHardeningInventory> {
    let mut transaction = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *transaction)
        .await?;
    let inventory =
        fetch_system_hardening_inventory_tx(&mut transaction, system_id, selection).await?;
    transaction.commit().await?;
    Ok(inventory)
}

async fn fetch_system_hardening_inventory_tx(
    transaction: &mut Transaction<'_, Postgres>,
    system_id: Uuid,
    selection: SystemCveInventorySelection,
) -> Result<SystemHardeningInventory> {
    // SECURITY: Browser-provided identities only narrow server-owned system,
    // flake, effective configuration, and NixOS relationships.
    let derivation_id = match selection {
        SystemCveInventorySelection::Current => {
            sqlx::query_scalar::<_, i32>(
                r#"WITH selected_system AS (
                     SELECT system.hostname,system.flake_id,
                            COALESCE(NULLIF(BTRIM(system.system_configuration_name),''),
                                     system.hostname) AS configuration_name
                     FROM systems system WHERE system.id=$1
                   ), latest_state AS (
                     SELECT state.store_path
                     FROM selected_system system
                     LEFT JOIN LATERAL (
                       SELECT candidate.store_path
                       FROM system_states candidate
                       WHERE candidate.hostname=system.hostname
                       ORDER BY candidate.timestamp DESC NULLS LAST,candidate.id DESC
                       LIMIT 1
                     ) state ON true
                   )
                   SELECT derivation.id
                   FROM selected_system system CROSS JOIN latest_state state
                   JOIN derivations derivation
                     ON derivation.store_path=state.store_path
                    AND derivation.derivation_type='nixos'
                    AND derivation.derivation_name=system.configuration_name
                   JOIN commits commit ON commit.id=derivation.commit_id
                    AND commit.flake_id=system.flake_id"#,
            )
            .bind(system_id)
            .fetch_optional(&mut **transaction)
            .await?
        }
        SystemCveInventorySelection::RetainedGeneration {
            generation_snapshot_id,
        } => {
            sqlx::query_scalar::<_, i32>(
                r#"SELECT derivation.id
                   FROM evaluation_generation_snapshots retained
                   JOIN systems system ON system.id=retained.system_id
                   JOIN derivations derivation ON derivation.id=retained.derivation_id
                    AND derivation.derivation_type='nixos'
                    AND derivation.derivation_name=COALESCE(
                      NULLIF(BTRIM(system.system_configuration_name),''),system.hostname)
                   JOIN commits commit ON commit.id=derivation.commit_id
                    AND commit.flake_id=system.flake_id
                   WHERE retained.id=$2 AND retained.system_id=$1"#,
            )
            .bind(system_id)
            .bind(generation_snapshot_id)
            .fetch_optional(&mut **transaction)
            .await?
        }
        SystemCveInventorySelection::ExactDerivation { derivation_id } => {
            sqlx::query_scalar::<_, i32>(
                r#"SELECT derivation.id
                   FROM systems system
                   JOIN derivations derivation
                     ON derivation.id=$2 AND derivation.derivation_type='nixos'
                    AND derivation.derivation_name=COALESCE(
                      NULLIF(BTRIM(system.system_configuration_name),''),system.hostname)
                   JOIN commits commit ON commit.id=derivation.commit_id
                    AND commit.flake_id=system.flake_id
                   WHERE system.id=$1"#,
            )
            .bind(system_id)
            .bind(derivation_id)
            .fetch_optional(&mut **transaction)
            .await?
        }
    };

    if selection != SystemCveInventorySelection::Current && derivation_id.is_none() {
        return Err(SystemHardeningTargetUnavailable.into());
    }
    let Some(derivation_id) = derivation_id else {
        return Ok(SystemHardeningInventory {
            selection,
            derivation_id: None,
            source: None,
            services: Vec::new(),
            attempt: None,
            read_only: false,
        });
    };

    let source = sqlx::query_as::<_, HardeningScan>(
        r#"SELECT id,derivation_id,scheduled_at,started_at,completed_at,
                  status,attempts,total_services,well_hardened_count,
                  moderately_hardened_count,poorly_hardened_count,
                  vulnerable_count,overall_score,scan_duration_ms,
                  scan_metadata,created_at
           FROM hardening_scans
           WHERE derivation_id=$1 AND status='completed' AND completed_at IS NOT NULL
           ORDER BY completed_at DESC,id DESC LIMIT 1"#,
    )
    .bind(derivation_id)
    .fetch_optional(&mut **transaction)
    .await?;
    let services = match source.as_ref() {
        Some(scan) => {
            sqlx::query_as::<_, ServiceHardeningResult>(
                r#"SELECT id,scan_id,service_name,service_type,hardening_score,
                      risk_level,directives_detail,enabled_directives_count,
                      disabled_directives_count,missing_directives_count,created_at
               FROM service_hardening_results
               WHERE scan_id=$1
               ORDER BY hardening_score ASC,service_name ASC,id ASC"#,
            )
            .bind(scan.id)
            .fetch_all(&mut **transaction)
            .await?
        }
        None => Vec::new(),
    };
    let attempt = fetch_latest_hardening_attempt_tx(transaction, derivation_id).await?;
    Ok(SystemHardeningInventory {
        selection,
        derivation_id: Some(derivation_id),
        source,
        services,
        attempt,
        read_only: selection != SystemCveInventorySelection::Current,
    })
}

/// Reads the newest hardening attempt for one exact derivation.
///
/// The newest row is authoritative for lifecycle reporting regardless of its
/// status. The caller must have already proved that `derivation_id` belongs to
/// the requesting system; this helper performs no authorization.
///
/// Rows whose persisted status is outside the supported lifecycle are reported
/// as absent rather than guessed, so an unknown future state can never be
/// displayed as a known one.
///
/// # Errors
///
/// Returns a database error when the attempt row cannot be read.
async fn fetch_latest_hardening_attempt_tx(
    transaction: &mut Transaction<'_, Postgres>,
    derivation_id: i32,
) -> Result<Option<SystemHardeningAttempt>> {
    let row = sqlx::query(
        r#"SELECT id,status,source_trigger,scheduled_at,started_at,completed_at,
                  attempts,scan_metadata->>'error' AS error_text
           FROM hardening_scans
           WHERE derivation_id=$1
           ORDER BY created_at DESC,id DESC
           LIMIT 1"#,
    )
    .bind(derivation_id)
    .fetch_optional(&mut **transaction)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };
    let status: String = row.try_get("status")?;
    let Some(state) = HardeningAttemptState::from_status(&status) else {
        return Ok(None);
    };
    let error = match state {
        HardeningAttemptState::Failed => row
            .try_get::<Option<String>, _>("error_text")?
            .map(|text| sanitize_hardening_failure(&text))
            .filter(|text| !text.is_empty()),
        _ => None,
    };

    Ok(Some(SystemHardeningAttempt {
        scan_id: row.try_get("id")?,
        state,
        source_trigger: row.try_get("source_trigger")?,
        scheduled_at: row.try_get("scheduled_at")?,
        started_at: row.try_get("started_at")?,
        completed_at: row.try_get("completed_at")?,
        attempts: row.try_get("attempts")?,
        error,
    }))
}

/// Redacts and bounds persisted hardening failure text for browser display.
///
/// SECURITY: Scanner failure text can quote Nix evaluation output that contains
/// credentials from a flake reference. The shared snapshot redaction runs before
/// any truncation so a secret cannot survive by sitting past the length bound.
fn sanitize_hardening_failure(value: &str) -> String {
    crate::security::snapshot_redaction::redact_text(value)
        .chars()
        .filter(|character| !character.is_control() || *character == '\n')
        .take(MAX_HARDENING_FAILURE_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Idempotently enqueue a hardening scan for a derivation.
///
/// The `ON CONFLICT ... DO NOTHING` clause relies on the partial unique index
/// added by migration 0188:
///
///   UNIQUE (derivation_id) WHERE status IN ('pending', 'in_progress')
///
/// This means concurrent callers can never create two active rows for the same
/// derivation.  If a row already exists, the INSERT is silently skipped and
/// the function returns the existing scan ID instead.
///
/// IMPORTANT: This function only writes a database row.  It does NOT spawn a
/// task or start a subprocess.  The actual `nix eval` is performed later by
/// `run_hardening_scan_queue` in `services/hardening_scans.rs`.
///
/// This is the manual admission path. The row inherits the `'manual'`
/// `source_trigger` default from migration 0274 and carries no build
/// provenance, which keeps it outside the automatic idempotency slot owned by
/// [`enqueue_post_build_hardening_scan_tx`].
pub async fn create_hardening_scan(pool: &PgPool, derivation_id: i32) -> Result<Uuid> {
    let scan_id = Uuid::new_v4();

    let inserted = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO hardening_scans (
            id, derivation_id, status, attempts,
            total_services, well_hardened_count, moderately_hardened_count,
            poorly_hardened_count, vulnerable_count
        ) VALUES ($1, $2, 'pending', 0, 0, 0, 0, 0, 0)
        ON CONFLICT (derivation_id)
          WHERE status IN ('pending', 'in_progress')
        DO NOTHING
        RETURNING id
        "#,
    )
    .bind(scan_id)
    .bind(derivation_id)
    .fetch_optional(pool)
    .await?;

    if let Some(id) = inserted {
        return Ok(id);
    }

    get_active_scan_for_derivation(pool, derivation_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("active hardening scan disappeared during enqueue"))
}

/// Columns every admission path inserts, in a single shared order.
///
/// Keeping one literal prevents an admission path from silently omitting a
/// `NOT NULL` counter and inserting a row the worker cannot summarize.
const HARDENING_ADMISSION_COLUMNS: &str = "id, derivation_id, status, attempts,
     total_services, well_hardened_count, moderately_hardened_count,
     poorly_hardened_count, vulnerable_count,
     source_trigger, source_build_job_id";

/// Admits one automatic hardening scan for a successfully built NixOS target.
///
/// ATOMICITY: The caller must invoke this helper inside the same transaction
/// that records the build success, so admission and build success commit or roll
/// back together. The helper performs exactly one `INSERT` and never spawns a
/// task, a subprocess, or a `nix eval`. A production incident on 2026-07-28 was
/// caused by admission spawning work; do not add a `tokio::spawn` here.
///
/// ELIGIBILITY: A row is inserted only when the derivation is a `nixos`
/// derivation with a realized `store_path`. An unrealized derivation, a package
/// derivation, and a build that never succeeded therefore admit nothing. When
/// `source_build_job_id` is supplied it must name a `success` build job for the
/// same derivation; the predicate is re-checked in SQL so a caller cannot admit
/// an event for a failed or cancelled attempt.
///
/// IDEMPOTENCY: The insert uses an untargeted `ON CONFLICT DO NOTHING`, so it
/// absorbs both relevant unique indexes. The automatic-event index from 0274
/// makes repeated and concurrent completion of the same build job produce at
/// most one row, forever. The active-scan index from 0188 makes an existing
/// pending or in-progress scan win; an active manual scan is preserved and is
/// never replaced, superseded, or restarted.
///
/// # Parameters
///
/// `auto_hardening_scans` mirrors the deployment's `server.auto_hardening_scans`
/// value and must be read from configuration by the caller. When it is `false`
/// this function executes no statement and returns `Ok(None)`. The parameter
/// exists so that admission policy stays owned by configuration instead of by a
/// process-global or a database mirror.
///
/// # Returns
///
/// The new scan ID when this call admitted a scan, or `None` when admission was
/// disabled, the target was ineligible, or an existing row already covered it.
///
/// # Errors
///
/// Returns a database error when the insert cannot run.
pub async fn enqueue_post_build_hardening_scan_tx(
    connection: &mut PgConnection,
    derivation_id: i32,
    source_build_job_id: Option<Uuid>,
    auto_hardening_scans: bool,
) -> Result<Option<Uuid>> {
    if !auto_hardening_scans {
        return Ok(None);
    }

    let inserted = sqlx::query_scalar::<_, Uuid>(&format!(
        r#"
        INSERT INTO hardening_scans ({HARDENING_ADMISSION_COLUMNS})
        SELECT gen_random_uuid(), derivation.id, 'pending', 0, 0, 0, 0, 0, 0, $3, $2
        FROM derivations derivation
        WHERE derivation.id = $1
          AND derivation.derivation_type = 'nixos'
          AND derivation.store_path IS NOT NULL
          AND BTRIM(derivation.store_path) <> ''
          AND (
                $2::uuid IS NULL
                OR EXISTS (
                    SELECT 1
                    FROM build_jobs job
                    WHERE job.id = $2
                      AND job.derivation_id = derivation.id
                      AND job.status = 'success'
                )
          )
        ON CONFLICT DO NOTHING
        RETURNING id
        "#
    ))
    .bind(derivation_id)
    .bind(source_build_job_id)
    .bind(HardeningScanTrigger::PostBuild.as_str())
    .fetch_optional(connection)
    .await?;

    Ok(inserted)
}

/// Bounds one backfill cycle to a small fixed number of admitted scans.
///
/// The hardening worker runs one `nix eval` at a time, so a larger batch adds
/// only queue depth and delays manual and post-build work behind historical
/// work. Five keeps a backlog draining steadily without starving newer requests.
pub const HARDENING_BACKFILL_BATCH_SIZE: i64 = 5;

/// Admits hardening scans for already built targets that have no evidence.
///
/// OWNERSHIP: This pass belongs to the hardening worker. Nothing on a request
/// path may call it, because a large historical backlog must never be attached
/// to a user request or to a build completion.
///
/// ELIGIBILITY: A candidate must be a `nixos` derivation with a realized
/// `store_path` and a `success` build job. A derivation that was evaluated but
/// never built, and a derivation whose only build attempts failed, are therefore
/// skipped. A derivation whose successful build predates `build_jobs` has no
/// source identity and is also skipped rather than admitted without provenance.
///
/// PRIORITY: Candidates are ordered as currently deployed exact targets, then
/// the newest derivation of each configuration in each flake, then remaining
/// history newest first. Operators see evidence for what is running now before
/// evidence for what ran before.
///
/// IDEMPOTENCY: A derivation with any pending, in-progress, or completed scan is
/// not a candidate, and a build job that already has an automatic event is not a
/// candidate even after that event failed. A failed automatic event is therefore
/// never retried silently by this pass; a person must request a new scan. The
/// untargeted `ON CONFLICT DO NOTHING` absorbs races with concurrent admission.
///
/// # Parameters
///
/// `auto_hardening_scans` mirrors `server.auto_hardening_scans`. When it is
/// `false` this function executes no statement and returns `Ok(0)`. `limit`
/// caps the batch and must be positive.
///
/// # Returns
///
/// The number of scans admitted by this cycle.
///
/// # Errors
///
/// Returns a database error when candidate discovery or insertion fails.
pub async fn enqueue_hardening_backfill_batch(
    pool: &PgPool,
    limit: i64,
    auto_hardening_scans: bool,
) -> Result<u64> {
    if !auto_hardening_scans || limit <= 0 {
        return Ok(0);
    }

    let result = sqlx::query(&format!(
        r#"
        WITH deployed AS (
            SELECT DISTINCT derivation.id AS derivation_id
            FROM systems system
            JOIN LATERAL (
                SELECT state.store_path
                FROM system_states state
                WHERE state.hostname = system.hostname
                ORDER BY state.timestamp DESC NULLS LAST, state.id DESC
                LIMIT 1
            ) state ON TRUE
            JOIN commits commit ON commit.flake_id = system.flake_id
            JOIN derivations derivation
              ON derivation.commit_id = commit.id
             AND derivation.store_path = state.store_path
             AND derivation.derivation_type = 'nixos'
             AND derivation.derivation_name = COALESCE(
                   NULLIF(BTRIM(system.system_configuration_name), ''), system.hostname)
            WHERE system.is_active = TRUE
        ), latest_per_flake AS (
            SELECT DISTINCT ON (commit.flake_id, derivation.derivation_name)
                   derivation.id AS derivation_id
            FROM derivations derivation
            JOIN commits commit ON commit.id = derivation.commit_id
            WHERE derivation.derivation_type = 'nixos'
            ORDER BY commit.flake_id, derivation.derivation_name,
                     commit.commit_timestamp DESC, derivation.id DESC
        ), candidate AS (
            SELECT derivation.id AS derivation_id,
                   job.id AS build_job_id,
                   CASE
                       WHEN deployed.derivation_id IS NOT NULL THEN 0
                       WHEN latest_per_flake.derivation_id IS NOT NULL THEN 1
                       ELSE 2
                   END AS priority,
                   job.completed_at AS built_at
            FROM derivations derivation
            JOIN LATERAL (
                SELECT attempt.id, attempt.completed_at
                FROM build_jobs attempt
                WHERE attempt.derivation_id = derivation.id
                  AND attempt.status = 'success'
                ORDER BY attempt.completed_at DESC NULLS LAST, attempt.id DESC
                LIMIT 1
            ) job ON TRUE
            LEFT JOIN deployed ON deployed.derivation_id = derivation.id
            LEFT JOIN latest_per_flake ON latest_per_flake.derivation_id = derivation.id
            WHERE derivation.derivation_type = 'nixos'
              AND derivation.store_path IS NOT NULL
              AND BTRIM(derivation.store_path) <> ''
              AND NOT EXISTS (
                  SELECT 1 FROM hardening_scans scan
                  WHERE scan.derivation_id = derivation.id
                    AND scan.status IN ('pending', 'in_progress', 'completed')
              )
              AND NOT EXISTS (
                  SELECT 1 FROM hardening_scans scan
                  WHERE scan.source_build_job_id = job.id
              )
            ORDER BY priority ASC, job.completed_at DESC NULLS LAST, derivation.id DESC
            LIMIT $1
        )
        INSERT INTO hardening_scans ({HARDENING_ADMISSION_COLUMNS})
        SELECT gen_random_uuid(), candidate.derivation_id, 'pending', 0, 0, 0, 0, 0, 0,
               $2, candidate.build_job_id
        FROM candidate
        ON CONFLICT DO NOTHING
        "#
    ))
    .bind(limit)
    .bind(HardeningScanTrigger::Backfill.as_str())
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ClaimedHardeningScan {
    pub id: Uuid,
    pub derivation_id: i32,
    pub config_name: String,
    pub repo_url: String,
    pub commit_hash: String,
    pub attempts: i32,
}

/// PostgreSQL advisory lock key used to serialize hardening scan claims and
/// stale recovery across multiple server processes.  Held only for the duration
/// of the claim transaction (milliseconds), NOT for the duration of the scan.
///
/// This is distinct from `HEAVY_NIX_ADVISORY_LOCK` in `evaluate_with_policies.rs`,
/// which is held for the entire `nix eval` subprocess lifetime to prevent bulk
/// evaluation and hardening from overlapping.  Using separate keys lets the
/// claim step itself remain lightweight.
const HARDENING_CLAIM_ADVISORY_LOCK: i64 = 0x4346_4841_5244;

/// Atomically claim the oldest pending hardening scan.
///
/// Uses a transaction-scoped PostgreSQL advisory lock to serialize concurrent
/// claim attempts across multiple server or worker processes.  `SKIP LOCKED`
/// makes the inner SELECT non-blocking: if another transaction already holds a
/// row lock (e.g. during stale recovery), this call simply returns `None` and
/// the caller will retry on the next poll tick.
///
/// The global `in_progress` partial unique index (migration 0188) provides an
/// additional database-level guard: even if two processes race past the advisory
/// lock, only one UPDATE can succeed.
pub async fn claim_next_hardening_scan(pool: &PgPool) -> Result<Option<ClaimedHardeningScan>> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(HARDENING_CLAIM_ADVISORY_LOCK)
        .execute(&mut *tx)
        .await?;

    let claimed = sqlx::query_as::<_, ClaimedHardeningScan>(
        r#"
        WITH candidate AS (
            SELECT hs.id
            FROM hardening_scans hs
            WHERE hs.status = 'pending'
              AND NOT EXISTS (
                  SELECT 1 FROM hardening_scans active
                  WHERE active.status = 'in_progress'
              )
            ORDER BY hs.scheduled_at ASC, hs.id ASC
            LIMIT 1
            FOR UPDATE OF hs SKIP LOCKED
        ), claimed AS (
            UPDATE hardening_scans hs
            SET status = 'in_progress',
                started_at = NOW(),
                completed_at = NULL,
                attempts = hs.attempts + 1
            FROM candidate
            WHERE hs.id = candidate.id
            RETURNING hs.id, hs.derivation_id, hs.attempts
        )
        SELECT claimed.id,
               claimed.derivation_id,
               d.derivation_name AS config_name,
               f.repo_url,
               c.git_commit_hash AS commit_hash,
               claimed.attempts
        FROM claimed
        JOIN derivations d ON d.id = claimed.derivation_id
        JOIN commits c ON c.id = d.commit_id
        JOIN flakes f ON f.id = c.flake_id
        "#,
    )
    .fetch_optional(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(claimed)
}

/// Recover work abandoned by a crashed worker. A five-minute subprocess
/// deadline plus a two-minute cleanup margin defines staleness.
pub async fn recover_stale_hardening_scans(pool: &PgPool) -> Result<u64> {
    let result = sqlx::query(
        r#"
        UPDATE hardening_scans
        SET status = CASE WHEN attempts < 3 THEN 'pending' ELSE 'failed' END,
            started_at = NULL,
            completed_at = CASE WHEN attempts < 3 THEN NULL ELSE NOW() END,
            scan_metadata = COALESCE(scan_metadata, '{}'::jsonb)
              || jsonb_build_object(
                  'recovered_at', NOW(),
                  'recovery_reason', 'stale hardening worker claim'
              )
        WHERE status = 'in_progress'
          AND started_at < NOW() - INTERVAL '7 minutes'
        "#,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn hardening_queue_depth(pool: &PgPool) -> Result<i64> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM hardening_scans WHERE status = 'pending'",
    )
    .fetch_one(pool)
    .await?)
}

/// Counts hardening work that is queued or currently executing.
///
/// The backfill owner uses this value to admit historical work only while the
/// serial worker is idle. A concurrent manual or post-build admission can race
/// with the check, but one bounded backfill batch can then precede it at most;
/// subsequent cycles wait until all admitted work drains.
///
/// # Errors
///
/// Returns a database error when the work count cannot be read.
pub async fn hardening_active_work_count(pool: &PgPool) -> Result<i64> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM hardening_scans WHERE status IN ('pending', 'in_progress')",
    )
    .fetch_one(pool)
    .await?)
}

/// Mark a scan as in progress.
pub async fn mark_scan_in_progress(pool: &PgPool, scan_id: Uuid) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE hardening_scans
        SET status = $1, started_at = NOW(), attempts = attempts + 1
        WHERE id = $2
        "#,
        "in_progress" as &str,
        scan_id
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Complete a hardening scan with results.
pub async fn complete_hardening_scan(
    pool: &PgPool,
    scan_id: Uuid,
    total_services: i32,
    well_hardened_count: i32,
    moderately_hardened_count: i32,
    poorly_hardened_count: i32,
    vulnerable_count: i32,
    overall_score: Option<i32>,
    scan_duration_ms: Option<i32>,
) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE hardening_scans
        SET
            status = $1,
            completed_at = NOW(),
            total_services = $2,
            well_hardened_count = $3,
            moderately_hardened_count = $4,
            poorly_hardened_count = $5,
            vulnerable_count = $6,
            overall_score = $7,
            scan_duration_ms = $8
        WHERE id = $9
        "#,
        "completed" as &str,
        total_services,
        well_hardened_count,
        moderately_hardened_count,
        poorly_hardened_count,
        vulnerable_count,
        overall_score,
        scan_duration_ms,
        scan_id
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Mark a scan as failed.
pub async fn mark_scan_failed(pool: &PgPool, scan_id: Uuid, error_message: &str) -> Result<()> {
    let metadata = serde_json::json!({ "error": error_message });

    sqlx::query!(
        r#"
        UPDATE hardening_scans
        SET
            status = $1,
            completed_at = NOW(),
            scan_metadata = $2
        WHERE id = $3
        "#,
        "failed" as &str,
        metadata,
        scan_id
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Persist all service rows and the completed scan summary atomically using a
/// single database connection and one batched UNNEST insert.
pub async fn persist_completed_hardening_scan(
    pool: &PgPool,
    scan_id: Uuid,
    scan: &ScanResult,
    scan_duration_ms: i32,
) -> Result<()> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM service_hardening_results WHERE scan_id = $1")
        .bind(scan_id)
        .execute(&mut *tx)
        .await?;

    let mut names = Vec::with_capacity(scan.services.len());
    let mut service_types = Vec::with_capacity(scan.services.len());
    let mut scores = Vec::with_capacity(scan.services.len());
    let mut risk_levels = Vec::with_capacity(scan.services.len());
    let mut directives = Vec::with_capacity(scan.services.len());
    let mut enabled = Vec::with_capacity(scan.services.len());
    let mut disabled = Vec::with_capacity(scan.services.len());
    let mut missing = Vec::with_capacity(scan.services.len());

    for service in &scan.services {
        names.push(service.name.clone());
        service_types.push(service.service_type.clone());
        scores.push(service.score_result.score);
        risk_levels.push(match service.score_result.risk_level {
            RiskLevel::WellHardened => "well_hardened".to_string(),
            RiskLevel::ModeratelyHardened => "moderately_hardened".to_string(),
            RiskLevel::PoorlyHardened => "poorly_hardened".to_string(),
            RiskLevel::Vulnerable => "vulnerable".to_string(),
        });
        directives.push(serde_json::to_value(&service.score_result.directives)?);
        enabled.push(service.score_result.enabled_count);
        disabled.push(service.score_result.disabled_count);
        missing.push(service.score_result.missing_count);
    }

    if !names.is_empty() {
        sqlx::query(
            r#"
            INSERT INTO service_hardening_results (
                id, scan_id, service_name, service_type, hardening_score,
                risk_level, directives_detail, enabled_directives_count,
                disabled_directives_count, missing_directives_count
            )
            SELECT gen_random_uuid(), $1, rows.*
            FROM UNNEST(
                $2::text[], $3::text[], $4::integer[], $5::text[],
                $6::jsonb[], $7::integer[], $8::integer[], $9::integer[]
            ) AS rows(
                service_name, service_type, hardening_score, risk_level,
                directives_detail, enabled_directives_count,
                disabled_directives_count, missing_directives_count
            )
            "#,
        )
        .bind(scan_id)
        .bind(&names)
        .bind(&service_types)
        .bind(&scores)
        .bind(&risk_levels)
        .bind(&directives)
        .bind(&enabled)
        .bind(&disabled)
        .bind(&missing)
        .execute(&mut *tx)
        .await?;
    }

    let updated = sqlx::query(
        r#"
        UPDATE hardening_scans
        SET status = 'completed',
            completed_at = NOW(),
            total_services = $2,
            well_hardened_count = $3,
            moderately_hardened_count = $4,
            poorly_hardened_count = $5,
            vulnerable_count = $6,
            overall_score = $7,
            scan_duration_ms = $8
        WHERE id = $1 AND status = 'in_progress'
        "#,
    )
    .bind(scan_id)
    .bind(scan.total_services)
    .bind(scan.well_hardened_count)
    .bind(scan.moderately_hardened_count)
    .bind(scan.poorly_hardened_count)
    .bind(scan.vulnerable_count)
    .bind(scan.overall_score)
    .bind(scan_duration_ms)
    .execute(&mut *tx)
    .await?;

    if updated.rows_affected() != 1 {
        anyhow::bail!("hardening scan {scan_id} lost its in-progress claim before completion");
    }

    tx.commit().await?;
    Ok(())
}

/// Insert a service hardening result.
pub async fn insert_service_result(
    pool: &PgPool,
    scan_id: Uuid,
    service_name: &str,
    service_type: Option<&str>,
    hardening_score: i32,
    risk_level: RiskLevel,
    directives_detail: serde_json::Value,
    enabled_directives_count: i32,
    disabled_directives_count: i32,
    missing_directives_count: i32,
) -> Result<Uuid> {
    let result_id = Uuid::new_v4();
    let risk_level_str = match risk_level {
        RiskLevel::WellHardened => "well_hardened",
        RiskLevel::ModeratelyHardened => "moderately_hardened",
        RiskLevel::PoorlyHardened => "poorly_hardened",
        RiskLevel::Vulnerable => "vulnerable",
    };

    sqlx::query!(
        r#"
        INSERT INTO service_hardening_results (
            id, scan_id, service_name, service_type,
            hardening_score, risk_level, directives_detail,
            enabled_directives_count, disabled_directives_count, missing_directives_count
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        "#,
        result_id,
        scan_id,
        service_name,
        service_type,
        hardening_score,
        risk_level_str,
        directives_detail,
        enabled_directives_count,
        disabled_directives_count,
        missing_directives_count
    )
    .execute(pool)
    .await?;

    Ok(result_id)
}

/// Get a hardening scan by ID.
pub async fn get_scan_by_id(pool: &PgPool, scan_id: Uuid) -> Result<Option<HardeningScan>> {
    let scan = sqlx::query_as!(
        HardeningScan,
        r#"
        SELECT
            id,
            derivation_id as "derivation_id!",
            scheduled_at,
            started_at,
            completed_at,
            status as "status!: ScanStatus",
            attempts as "attempts!",
            total_services as "total_services!",
            well_hardened_count as "well_hardened_count!",
            moderately_hardened_count as "moderately_hardened_count!",
            poorly_hardened_count as "poorly_hardened_count!",
            vulnerable_count as "vulnerable_count!",
            overall_score,
            scan_duration_ms,
            scan_metadata,
            created_at as "created_at!"
        FROM hardening_scans
        WHERE id = $1
        "#,
        scan_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(scan)
}

/// List environment IDs for active systems associated with a hardening scan's derivation.
pub async fn list_scan_environment_ids(pool: &PgPool, scan_id: Uuid) -> Result<Vec<Option<Uuid>>> {
    let rows = sqlx::query_scalar::<_, Option<Uuid>>(
        r#"
        SELECT DISTINCT s.environment_id
        FROM hardening_scans hs
        JOIN derivations d ON d.id = hs.derivation_id
        JOIN commits c ON c.id = d.commit_id
        JOIN systems s
          ON s.flake_id = c.flake_id
         AND COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname) = d.derivation_name
        WHERE hs.id = $1
          AND s.is_active = TRUE
        "#,
    )
    .bind(scan_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Get the latest hardening scan for a derivation.
pub async fn get_latest_scan(pool: &PgPool, derivation_id: i32) -> Result<Option<HardeningScan>> {
    let scan = sqlx::query_as!(
        HardeningScan,
        r#"
        SELECT
            id,
            derivation_id as "derivation_id!",
            scheduled_at,
            started_at,
            completed_at,
            status as "status!: ScanStatus",
            attempts as "attempts!",
            total_services as "total_services!",
            well_hardened_count as "well_hardened_count!",
            moderately_hardened_count as "moderately_hardened_count!",
            poorly_hardened_count as "poorly_hardened_count!",
            vulnerable_count as "vulnerable_count!",
            overall_score,
            scan_duration_ms,
            scan_metadata,
            created_at as "created_at!"
        FROM hardening_scans
        WHERE derivation_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        derivation_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(scan)
}

/// Get service results for a scan.
pub async fn get_service_results(
    pool: &PgPool,
    scan_id: Uuid,
) -> Result<Vec<ServiceHardeningResult>> {
    let results = sqlx::query_as!(
        ServiceHardeningResult,
        r#"
        SELECT
            id,
            scan_id,
            service_name as "service_name!",
            service_type,
            hardening_score as "hardening_score!",
            risk_level as "risk_level!: RiskLevel",
            directives_detail as "directives_detail!",
            enabled_directives_count as "enabled_directives_count!",
            disabled_directives_count as "disabled_directives_count!",
            missing_directives_count as "missing_directives_count!",
            created_at as "created_at!"
        FROM service_hardening_results
        WHERE scan_id = $1
        ORDER BY hardening_score ASC, service_name ASC
        "#,
        scan_id
    )
    .fetch_all(pool)
    .await?;

    Ok(results)
}

/// Get fleet-wide hardening summary.
pub async fn get_fleet_summary(pool: &PgPool) -> Result<FleetHardeningSummary> {
    let row = sqlx::query!(
        r#"
        SELECT
            COALESCE(total_systems_scanned, 0) as "total_systems_scanned!",
            avg_fleet_score,
            COALESCE(total_well_hardened_services, 0) as "total_well_hardened_services!",
            COALESCE(total_moderately_hardened_services, 0) as "total_moderately_hardened_services!",
            COALESCE(total_poorly_hardened_services, 0) as "total_poorly_hardened_services!",
            COALESCE(total_vulnerable_services, 0) as "total_vulnerable_services!",
            COALESCE(total_services_scanned, 0) as "total_services_scanned!",
            last_scan_completed
        FROM view_hardening_fleet_summary
        "#
    )
    .fetch_optional(pool)
    .await?;

    Ok(match row {
        Some(r) => FleetHardeningSummary {
            total_systems_scanned: r.total_systems_scanned,
            avg_fleet_score: r
                .avg_fleet_score
                .map(|d| d.to_string().parse().unwrap_or(0.0)),
            total_well_hardened_services: r.total_well_hardened_services,
            total_moderately_hardened_services: r.total_moderately_hardened_services,
            total_poorly_hardened_services: r.total_poorly_hardened_services,
            total_vulnerable_services: r.total_vulnerable_services,
            total_services_scanned: r.total_services_scanned,
            last_scan_completed: r.last_scan_completed,
        },
        None => FleetHardeningSummary {
            total_systems_scanned: 0,
            avg_fleet_score: None,
            total_well_hardened_services: 0,
            total_moderately_hardened_services: 0,
            total_poorly_hardened_services: 0,
            total_vulnerable_services: 0,
            total_services_scanned: 0,
            last_scan_completed: None,
        },
    })
}

/// Get top vulnerable services across fleet.
pub async fn get_top_vulnerable_services(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<TopVulnerableService>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            service_name as "service_name!",
            affected_systems_count as "affected_systems_count!",
            avg_score as "avg_score!",
            min_score as "min_score!",
            max_score as "max_score!"
        FROM view_hardening_top_vulnerable_services
        LIMIT $1
        "#,
        limit
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| TopVulnerableService {
            service_name: r.service_name,
            affected_systems_count: r.affected_systems_count,
            avg_score: r.avg_score.to_string().parse().unwrap_or(0.0),
            min_score: r.min_score,
            max_score: r.max_score,
        })
        .collect())
}

/// List system hardening posture rows for all systems with completed scans.
pub async fn list_system_postures(pool: &PgPool) -> Result<Vec<SystemHardeningPosture>> {
    let rows = sqlx::query_as::<_, SystemHardeningPosture>(
        r#"
        SELECT
            v.derivation_id,
            v.config_name,
            v.system_id,
            v.hostname,
            e.name AS environment_name,
            v.latest_scan_id,
            v.overall_score,
            v.risk_level,
            v.total_services,
            v.well_hardened_count,
            v.moderately_hardened_count,
            v.poorly_hardened_count,
            v.vulnerable_count,
            v.last_scan_at,
            v.scan_duration_ms
        FROM view_system_hardening_posture v
        LEFT JOIN systems s ON s.id = v.system_id
        LEFT JOIN environments e ON e.id = s.environment_id
        WHERE v.latest_scan_id IS NOT NULL
        ORDER BY v.overall_score ASC NULLS LAST, v.config_name ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Get the latest hardening posture row for a single system.
pub async fn get_system_posture(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<Option<SystemHardeningPosture>> {
    let row = sqlx::query_as::<_, SystemHardeningPosture>(
        r#"
        SELECT
            v.derivation_id,
            v.config_name,
            v.system_id,
            v.hostname,
            e.name AS environment_name,
            v.latest_scan_id,
            v.overall_score,
            v.risk_level,
            v.total_services,
            v.well_hardened_count,
            v.moderately_hardened_count,
            v.poorly_hardened_count,
            v.vulnerable_count,
            v.last_scan_at,
            v.scan_duration_ms
        FROM view_system_hardening_posture v
        LEFT JOIN systems s ON s.id = v.system_id
        LEFT JOIN environments e ON e.id = s.environment_id
        WHERE v.system_id = $1
        ORDER BY v.last_scan_at DESC NULLS LAST
        LIMIT 1
        "#,
    )
    .bind(system_id)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// Check if there's an active (pending/in_progress) scan for a derivation.
pub async fn get_active_scan_for_derivation(
    pool: &PgPool,
    derivation_id: i32,
) -> Result<Option<Uuid>> {
    let row = sqlx::query!(
        r#"
        SELECT id
        FROM hardening_scans
        WHERE derivation_id = $1
          AND status IN ('pending', 'in_progress')
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        derivation_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| r.id))
}

/// Create or update a hardening justification.
pub async fn upsert_justification(
    pool: &PgPool,
    system_id: Uuid,
    service_name: &str,
    directive_name: Option<&str>,
    category: Option<&str>,
    reason: &str,
    user_id: Option<Uuid>,
) -> Result<Uuid> {
    let inserted_id = Uuid::new_v4();

    if let Some(directive_name) = directive_name {
        let id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO hardening_justifications (
                id, system_id, service_name, directive_name,
                category, reason, created_by, updated_by
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $7)
            ON CONFLICT (system_id, service_name, directive_name) WHERE directive_name IS NOT NULL DO UPDATE SET
                category = EXCLUDED.category,
                reason = EXCLUDED.reason,
                updated_by = EXCLUDED.updated_by,
                updated_at = NOW()
            RETURNING id
            "#,
        )
        .bind(inserted_id)
        .bind(system_id)
        .bind(service_name)
        .bind(directive_name)
        .bind(category)
        .bind(reason)
        .bind(user_id)
        .fetch_one(pool)
        .await?;

        Ok(id)
    } else {
        let id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO hardening_justifications (
                id, system_id, service_name, directive_name,
                category, reason, created_by, updated_by
            ) VALUES ($1, $2, $3, NULL, $4, $5, $6, $6)
            ON CONFLICT (system_id, service_name) WHERE directive_name IS NULL DO UPDATE SET
                category = EXCLUDED.category,
                reason = EXCLUDED.reason,
                updated_by = EXCLUDED.updated_by,
                updated_at = NOW()
            RETURNING id
            "#,
        )
        .bind(inserted_id)
        .bind(system_id)
        .bind(service_name)
        .bind(category)
        .bind(reason)
        .bind(user_id)
        .fetch_one(pool)
        .await?;

        Ok(id)
    }
}

/// Get justifications for a system.
pub async fn get_justifications_for_system(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<Vec<HardeningJustification>> {
    let justifications = sqlx::query_as!(
        HardeningJustification,
        r#"
        SELECT
            id,
            system_id,
            service_name as "service_name!",
            directive_name,
            category,
            reason as "reason!",
            created_by,
            updated_by,
            created_at as "created_at!",
            updated_at as "updated_at!",
            expires_at
        FROM hardening_justifications
        WHERE system_id = $1
        ORDER BY service_name, directive_name NULLS FIRST
        "#,
        system_id
    )
    .fetch_all(pool)
    .await?;

    Ok(justifications)
}

/// Delete a justification.
pub async fn delete_justification(pool: &PgPool, justification_id: Uuid) -> Result<bool> {
    let result = sqlx::query!(
        r#"
        DELETE FROM hardening_justifications
        WHERE id = $1
        "#,
        justification_id
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Resolve a system's derivation for hardening scan (similar to CVE scan pattern).
pub async fn resolve_system_hardening_scan_target(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<Option<HardeningScanTarget>> {
    let row = sqlx::query_as::<_, HardeningScanTarget>(
        r#"
        WITH selected_system AS (
            SELECT
                s.id,
                s.hostname,
                s.flake_id,
                COALESCE(NULLIF(BTRIM(s.system_configuration_name), ''), s.hostname) AS config_name
            FROM systems s
            WHERE s.id = $1
              AND s.is_active = TRUE
        ),
        latest_state AS (
            SELECT ss.store_path
            FROM selected_system s
            JOIN system_states ss ON ss.hostname = s.hostname
            ORDER BY ss.timestamp DESC
            LIMIT 1
        )
        SELECT
            d.id AS derivation_id,
            ss.config_name as config_name,
            ss.hostname as hostname,
            COALESCE(f.repo_url, '') as repo_url,
            COALESCE(c.git_commit_hash, '') as commit_hash,
            CASE
                WHEN f.repo_url IS NULL OR BTRIM(f.repo_url) = ''
                  OR c.git_commit_hash IS NULL OR BTRIM(c.git_commit_hash) = ''
                THEN 'Flake source metadata is unavailable for this system configuration.'
                ELSE NULL
            END AS blocked_reason
        FROM selected_system ss
        JOIN latest_state ls ON TRUE
        JOIN derivations d
          ON COALESCE(d.store_path, d.expected_store_path) = ls.store_path
        JOIN commits c ON c.id = d.commit_id AND c.flake_id = ss.flake_id
        JOIN flakes f ON f.id = c.flake_id
        WHERE d.derivation_type = 'nixos'
          AND d.derivation_name = ss.config_name
        ORDER BY d.completed_at DESC NULLS LAST, d.id DESC
        LIMIT 1
        "#,
    )
    .bind(system_id)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// Target for a hardening scan.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct HardeningScanTarget {
    pub derivation_id: i32,
    pub config_name: String,
    pub hostname: String,
    pub repo_url: String,
    pub commit_hash: String,
    pub blocked_reason: Option<String>,
}
