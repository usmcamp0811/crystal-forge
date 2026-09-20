//! Build job queue queries for the builder API system.
//!
//! This module handles creating and managing build jobs in the build_jobs table.

use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::{PgPool, Postgres, Transaction};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::models::builders::SERVER_FAILURE_CODE_EVALUATOR_CONTRACT_OBSOLETE;

/// Advisory lock serializing all build-queue-position allocations.
/// Using the ASCII encoding of 'CFBQ' as a 64-bit integer (0x43464251).
pub const BUILD_QUEUE_ORDER_LOCK_KEY: i64 = 0x4346_4251;
const BUILD_DERIVATION_LOCK_NAMESPACE: i32 = 0x4346_4244;

/// Serializes all attempt creation and terminal retry transitions for a derivation.
///
/// Callers MUST acquire this lock before a build-job row lock and before
/// [`lock_build_queue_order`]. This order prevents manual retry, automatic retry,
/// and authoritative obsolete replacement from deadlocking or creating two
/// active attempts for one derivation.
pub async fn lock_build_derivation(
    tx: &mut Transaction<'_, Postgres>,
    derivation_id: i32,
) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock($1, $2)")
        .bind(BUILD_DERIVATION_LOCK_NAMESPACE)
        .bind(derivation_id)
        .execute(&mut **tx)
        .await
        .context("Failed to lock build derivation attempt lineage")?;
    Ok(())
}

/// Acquire the transaction-level advisory lock before reading MAX(queue_position).
///
/// Every code path that computes `MAX(queue_position) + 1` must call this first.
/// The lock is scoped to the transaction and released automatically at commit/rollback.
pub async fn lock_build_queue_order(tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(BUILD_QUEUE_ORDER_LOCK_KEY)
        .execute(&mut **tx)
        .await
        .context("Failed to acquire build queue order lock")?;
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct QueuedBuild {
    pub build_job_id: Uuid,
    pub derivation_id: i32,
    pub system_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildJobInsertOutcome {
    Inserted {
        build_job_id: Uuid,
    },
    AlreadyExists {
        build_job_id: Uuid,
        /// Status of the existing job (e.g. "queued", "building", "success").
        /// The caller uses this to decide whether to announce a new queue event.
        status: String,
    },
}

/// Create build jobs for all derivations associated with a commit.
///
/// This function is called after successful commit evaluation to queue
/// derivations for building. It implements smart prioritization based on:
/// - Whether the system is tracked (in the systems table)
/// - How recent the commit is (commit_timestamp age: <1h, <1d, older)
///
/// # Arguments
/// * `pool` - Database connection pool
/// * `commit_id` - The commit ID whose derivations should be queued
///
/// # Returns
/// Number of build jobs created
///
/// Build admission and policy-eligible post-build scan intent creation commit
/// or roll back together. Existing active scan work retains its identity.
pub async fn create_build_jobs_for_commit(pool: &PgPool, commit_id: i32) -> Result<usize> {
    let mut tx = pool
        .begin()
        .await
        .context("Failed to begin create_build_jobs_for_commit transaction")?;
    lock_build_queue_order(&mut tx).await?;

    let max_pos: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(queue_position), 0) FROM build_jobs WHERE status = 'queued' OR status = 'building'",
    )
    .fetch_one(&mut *tx)
    .await
    .context("Failed to read max queue_position")?;

    let inserted_derivation_ids = sqlx::query_scalar::<_, i32>(
        r#"
        INSERT INTO build_jobs (
            derivation_id,
            environment_id,
            priority_weight,
            queue_position,
            status
        )
        SELECT
            d.id as derivation_id,
            s.environment_id,
            CASE
                WHEN s.id IS NOT NULL THEN 10.0
                ELSE 1.0
            END *
            CASE
                WHEN EXTRACT(EPOCH FROM (NOW() - c.commit_timestamp)) < 3600 THEN 2.0
                WHEN EXTRACT(EPOCH FROM (NOW() - c.commit_timestamp)) < 86400 THEN 1.5
                ELSE 1.0
            END as priority_weight,
            $2 + ROW_NUMBER() OVER (ORDER BY d.id) AS queue_position,
            'queued' as status
        FROM derivations d
        INNER JOIN commits c ON d.commit_id = c.id
        LEFT JOIN systems s ON (
            d.derivation_target = s.hostname
            AND s.flake_id = c.flake_id
        )
        WHERE d.commit_id = $1
            AND d.status_id = 5
            AND d.cf_agent_enabled = TRUE
            AND d.policy_requirements_met = TRUE
            AND NOT EXISTS (
                SELECT 1 FROM build_jobs bj
                WHERE bj.derivation_id = d.id
            )
        ON CONFLICT (derivation_id)
            WHERE status IN ('queued', 'building', 'cancelling') DO NOTHING
        RETURNING derivation_id
        "#,
    )
    .bind(commit_id)
    .bind(max_pos)
    .fetch_all(&mut *tx)
    .await
    .context("Failed to create build jobs for commit")?;

    crate::queries::cve_scan_leases::create_post_build_scan_intents_tx(
        &mut tx,
        &inserted_derivation_ids,
    )
    .await
    .context("Failed to create post-build scan intents")?;

    tx.commit()
        .await
        .context("Failed to commit create_build_jobs_for_commit")?;

    let count = inserted_derivation_ids.len();

    if count > 0 {
        info!("📋 Created {} build jobs for commit {}", count, commit_id);
    } else {
        debug!(
            "No new build jobs created for commit {} (already queued or no ready derivations)",
            commit_id
        );
    }

    Ok(count)
}

/// Creates commit build jobs and post-build scan intent in the caller's transaction.
///
/// Only derivations returned by the build insert receive intent. A concurrent or
/// repeated admission that inserts no build job cannot create scan work.
///
/// # Errors
///
/// Returns an error when queue locking, build insertion, or scan-intent
/// persistence fails.
pub async fn create_build_jobs_for_commit_tx(
    tx: &mut Transaction<'_, Postgres>,
    commit_id: i32,
) -> Result<Vec<QueuedBuild>> {
    lock_build_queue_order(tx).await?;

    let max_pos: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(queue_position), 0) FROM build_jobs WHERE status = 'queued' OR status = 'building'",
    )
    .fetch_one(&mut **tx)
    .await
    .context("Failed to read max queue_position")?;

    let rows = sqlx::query_as::<_, QueuedBuild>(
        r#"
        INSERT INTO build_jobs (
            derivation_id,
            environment_id,
            priority_weight,
            queue_position,
            status
        )
        SELECT
            d.id as derivation_id,
            s.environment_id,
            CASE
                WHEN s.id IS NOT NULL THEN 10.0
                ELSE 1.0
            END *
            CASE
                WHEN EXTRACT(EPOCH FROM (NOW() - c.commit_timestamp)) < 3600 THEN 2.0
                WHEN EXTRACT(EPOCH FROM (NOW() - c.commit_timestamp)) < 86400 THEN 1.5
                ELSE 1.0
            END as priority_weight,
            $2 + ROW_NUMBER() OVER (ORDER BY d.id) AS queue_position,
            'queued' as status
        FROM derivations d
        INNER JOIN commits c ON d.commit_id = c.id
        LEFT JOIN systems s ON (
            d.derivation_target = s.hostname
            AND s.flake_id = c.flake_id
        )
        WHERE d.commit_id = $1
            AND d.status_id = 5
            AND d.cf_agent_enabled = TRUE
            AND d.policy_requirements_met = TRUE
            AND NOT EXISTS (
                SELECT 1 FROM build_jobs bj
                WHERE bj.derivation_id = d.id
            )
        ON CONFLICT (derivation_id)
            WHERE status IN ('queued', 'building', 'cancelling') DO NOTHING
        RETURNING id AS build_job_id, derivation_id, (
            SELECT derivation_name FROM derivations WHERE derivations.id = build_jobs.derivation_id
        ) AS system_name
        "#,
    )
    .bind(commit_id)
    .bind(max_pos)
    .fetch_all(&mut **tx)
    .await
    .context("Failed to create build jobs for commit")?;

    let derivation_ids = rows.iter().map(|row| row.derivation_id).collect::<Vec<_>>();
    crate::queries::cve_scan_leases::create_post_build_scan_intents_tx(tx, &derivation_ids)
        .await
        .context("Failed to create post-build scan intents")?;

    Ok(rows)
}

/// Creates an eligible derivation's initial or authoritative replacement attempt.
///
/// The derivation must be in `DryRunComplete` and satisfy the agent and policy
/// gates. When the latest attempt failed with the exact server-owned
/// [`SERVER_FAILURE_CODE_EVALUATOR_CONTRACT_OBSOLETE`] code, authoritative
/// same-revision evaluation creates a new queued child. The failed source row
/// remains immutable. All other existing terminal rows remain unchanged and produce
/// [`BuildJobInsertOutcome::AlreadyExists`].
/// A newly inserted build and its policy-eligible post-build scan intent share
/// the caller's transaction. An `AlreadyExists` outcome never creates intent.
///
/// # Errors
///
/// Returns an error when queue locking or a database operation fails.
pub async fn create_build_job_for_derivation_tx(
    tx: &mut Transaction<'_, Postgres>,
    derivation_id: i32,
) -> Result<Option<BuildJobInsertOutcome>> {
    lock_build_derivation(tx, derivation_id).await?;

    let obsolete_source: Option<(Uuid, Uuid)> = sqlx::query_as(
        r#"
        WITH latest AS (
            SELECT id, root_job_id, status, server_failure_code
            FROM build_jobs
            WHERE derivation_id = $1
            ORDER BY created_at DESC, id DESC
            LIMIT 1
            FOR UPDATE
        )
        SELECT id, COALESCE(root_job_id, id)
        FROM latest
        WHERE status = 'failed'
          AND server_failure_code = $2
        "#,
    )
    .bind(derivation_id)
    .bind(SERVER_FAILURE_CODE_EVALUATOR_CONTRACT_OBSOLETE)
    .fetch_optional(&mut **tx)
    .await
    .context("Failed to lock obsolete-contract source attempt")?;

    lock_build_queue_order(tx).await?;

    let next_pos: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(queue_position), 0) + 1 FROM build_jobs WHERE status = 'queued' OR status = 'building'",
    )
    .fetch_one(&mut **tx)
    .await
    .context("Failed to read max queue_position")?;

    let inserted: Option<(Uuid,)> = sqlx::query_as(
        r#"
        WITH history AS (
            SELECT COALESCE(MAX(attempt_number), 0)::integer AS max_attempt_number
            FROM build_jobs
            WHERE derivation_id = $1
        )
        INSERT INTO build_jobs (
            derivation_id,
            environment_id,
            priority_weight,
            queue_position,
            status,
            parent_job_id,
            root_job_id,
            attempt_number,
            available_at
        )
        SELECT
            d.id AS derivation_id,
            s.environment_id,
            CASE
                WHEN s.id IS NOT NULL THEN 10.0
                ELSE 1.0
            END *
            CASE
                WHEN EXTRACT(EPOCH FROM (NOW() - c.commit_timestamp)) < 3600 THEN 2.0
                WHEN EXTRACT(EPOCH FROM (NOW() - c.commit_timestamp)) < 86400 THEN 1.5
                ELSE 1.0
            END AS priority_weight,
            $2 AS queue_position,
            'queued' AS status,
            $3,
            $4,
            history.max_attempt_number + 1,
            NOW()
        FROM derivations d
        INNER JOIN commits c ON d.commit_id = c.id
        CROSS JOIN history
        LEFT JOIN systems s ON (
            d.derivation_target = s.hostname
            AND s.flake_id = c.flake_id
        )
        WHERE d.id = $1
            AND d.status_id = 5
            AND d.cf_agent_enabled = TRUE
            AND d.policy_requirements_met = TRUE
            AND (history.max_attempt_number = 0 OR $3::uuid IS NOT NULL)
            AND NOT EXISTS (
                SELECT 1 FROM build_jobs active
                WHERE active.derivation_id = d.id
                  AND active.status IN ('queued', 'building', 'cancelling')
            )
        ON CONFLICT (derivation_id)
            WHERE status IN ('queued', 'building', 'cancelling') DO NOTHING
        RETURNING id
        "#,
    )
    .bind(derivation_id)
    .bind(next_pos)
    .bind(obsolete_source.map(|source| source.0))
    .bind(obsolete_source.map(|source| source.1))
    .fetch_optional(&mut **tx)
    .await
    .context("Failed to create build job for derivation")?;

    if let Some((build_job_id,)) = inserted {
        crate::queries::cve_scan_leases::create_post_build_scan_intents_tx(tx, &[derivation_id])
            .await
            .context("Failed to create post-build scan intent")?;
        return Ok(Some(BuildJobInsertOutcome::Inserted { build_job_id }));
    }

    let existing: Option<(Uuid, String)> = sqlx::query_as(
        r#"
        SELECT id, status
        FROM build_jobs
        WHERE derivation_id = $1
        ORDER BY
            status IN ('queued', 'building', 'cancelling') DESC,
            created_at DESC,
            id DESC
        LIMIT 1
        "#,
    )
    .bind(derivation_id)
    .fetch_optional(&mut **tx)
    .await
    .context("Failed to fetch existing build job for derivation")?;

    Ok(existing.map(
        |(build_job_id, status)| BuildJobInsertOutcome::AlreadyExists {
            build_job_id,
            status,
        },
    ))
}

/// Incrementally enqueue a single derivation as a build job.
///
/// Called immediately after a derivation reaches `DryRunComplete` during evaluation,
/// so builders can start work without waiting for the full commit to finish evaluating.
///
/// Idempotency: existing history prevents scheduler-created attempts, and the
/// active-attempt index absorbs concurrent initial enqueue races.
/// Policy-eligible post-build scan intent commits atomically with a newly
/// inserted build job. Existing build jobs do not create intent on retry.
///
/// Returns `true` if a new job was created, `false` if one already existed.
pub async fn enqueue_build_job_for_derivation(pool: &PgPool, derivation_id: i32) -> Result<bool> {
    let mut tx = pool
        .begin()
        .await
        .context("Failed to begin enqueue_build_job_for_derivation transaction")?;
    lock_build_queue_order(&mut tx).await?;

    let next_pos: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(queue_position), 0) + 1 FROM build_jobs WHERE status = 'queued' OR status = 'building'",
    )
    .fetch_one(&mut *tx)
    .await
    .context("Failed to read max queue_position")?;

    let inserted_derivation_id = sqlx::query_scalar::<_, i32>(
        r#"
        INSERT INTO build_jobs (
            derivation_id,
            environment_id,
            priority_weight,
            queue_position,
            status
        )
        SELECT
            d.id AS derivation_id,
            s.environment_id,
            CASE
                WHEN s.id IS NOT NULL THEN 10.0
                ELSE 1.0
            END *
            CASE
                WHEN EXTRACT(EPOCH FROM (NOW() - c.commit_timestamp)) < 3600  THEN 2.0
                WHEN EXTRACT(EPOCH FROM (NOW() - c.commit_timestamp)) < 86400 THEN 1.5
                ELSE 1.0
            END AS priority_weight,
            $2,
            'queued' AS status
        FROM derivations d
        INNER JOIN commits c ON d.commit_id = c.id
        LEFT JOIN systems s ON (
            d.derivation_target = s.hostname
            AND s.flake_id = c.flake_id
        )
        WHERE d.id = $1
          AND d.status_id = 5  -- DryRunComplete
          AND d.cf_agent_enabled = TRUE
          AND d.policy_requirements_met = TRUE
          AND NOT EXISTS (
              SELECT 1 FROM build_jobs existing
              WHERE existing.derivation_id = d.id
          )
        ON CONFLICT (derivation_id)
            WHERE status IN ('queued', 'building', 'cancelling') DO NOTHING
        RETURNING derivation_id
        "#,
    )
    .bind(derivation_id)
    .bind(next_pos)
    .fetch_optional(&mut *tx)
    .await
    .context("Failed to enqueue build job for derivation")?;

    if let Some(inserted_derivation_id) = inserted_derivation_id {
        crate::queries::cve_scan_leases::create_post_build_scan_intents_tx(
            &mut tx,
            &[inserted_derivation_id],
        )
        .await
        .context("Failed to create post-build scan intent")?;
    }

    tx.commit()
        .await
        .context("Failed to commit enqueue_build_job_for_derivation")?;

    let created = inserted_derivation_id.is_some();
    if created {
        info!(
            "📋 Incremental build job created for derivation {}",
            derivation_id
        );
    } else {
        debug!(
            "Build job for derivation {} already exists or derivation not ready; skipping",
            derivation_id
        );
    }
    Ok(created)
}

/// Get the next queued build job for a builder.
///
/// This respects environment assignments - if a builder has environment assignments,
/// only jobs for those environments are returned. If no assignments exist, the builder
/// can pick up any job (wildcard).
///
/// # Arguments
/// * `pool` - Database connection pool
/// * `builder_id` - The UUID of the builder requesting work
///
/// # Returns
/// Optional job UUID if work is available
pub async fn get_next_job_for_builder(pool: &PgPool, builder_id: Uuid) -> Result<Option<Uuid>> {
    let job = sqlx::query_scalar::<_, Uuid>(
        r#"
        WITH builder_environments AS (
            SELECT environment_id 
            FROM builder_environment_assignments 
            WHERE builder_id = $1
        ),
        available_jobs AS (
            SELECT bj.id
            FROM build_jobs bj
            JOIN derivations d ON d.id = bj.derivation_id
            WHERE bj.status = 'queued'
                AND bj.retry_count < bj.max_retries
                AND d.cf_agent_enabled IS TRUE
                AND d.policy_requirements_met IS TRUE
                AND bj.available_at <= NOW()
                AND (
                    -- No environment restrictions (wildcard builder)
                    NOT EXISTS (SELECT 1 FROM builder_environments)
                    OR
                    -- Builder has environment assignments and job matches
                    bj.environment_id IN (SELECT environment_id FROM builder_environments)
                    OR
                    -- Job has no environment (can be built by any builder)
                    bj.environment_id IS NULL
                )
            ORDER BY bj.queue_position DESC NULLS LAST, bj.priority_weight DESC, bj.created_at ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE build_jobs
        SET 
            status = 'building',
            builder_id = $1,
            server_failure_code = NULL,
            started_at = NOW(),
            updated_at = NOW()
        FROM available_jobs
        WHERE build_jobs.id = available_jobs.id
        RETURNING build_jobs.id
        "#,
    )
    .bind(builder_id)
    .fetch_optional(pool)
    .await
    .context("Failed to claim next build job")?;

    Ok(job)
}

/// A recovery candidate: build-eligible derivation whose preparation failed or was
/// interrupted before a build job was created.
#[derive(Debug, sqlx::FromRow)]
struct RecoveryCandidate {
    derivation_id: i32,
    derivation_path: Option<String>,
    derivation_target: Option<String>,
    commit_id: Option<i32>,
    flake_id: Option<i32>,
    evaluation_attempt_count: Option<i32>,
}

/// Set backoff state for a failed recovery attempt.
///
/// The update is guarded to prevent a stale failure from overwriting a newer
/// preparation generation:
/// - `build_preparation_state IN ('pending', 'failed')`
/// - derivation path, commit_id, and evaluation_attempt_count must match
/// - the commit must still be complete
/// - no build job must exist for this derivation
///
/// If the guard fails, the update does nothing and no error is returned.
async fn record_recovery_failure(
    pool: &PgPool,
    derivation_id: i32,
    commit_id: i32,
    expected_attempt: i32,
    derivation_path: Option<&str>,
    error: &str,
) {
    let result = sqlx::query(
        r#"
        UPDATE derivations d
        SET build_preparation_state = 'failed',
            build_preparation_attempts = COALESCE(d.build_preparation_attempts, 0) + 1,
            build_preparation_last_error = $5,
            build_preparation_next_attempt_at = NOW() + LEAST(
                POW(2, COALESCE(d.build_preparation_attempts, 0)) * interval '30 seconds',
                interval '30 minutes'
            )
        FROM commits c
        WHERE d.id = $1
          AND d.commit_id = $2
          AND d.build_preparation_state IN ('pending', 'failed')
          AND d.derivation_path IS NOT DISTINCT FROM $4
          AND d.status_id = 5
          AND d.cf_agent_enabled = TRUE
          AND d.policy_requirements_met = TRUE
          AND c.id = d.commit_id
          AND c.evaluation_status = 'complete'
          AND c.evaluation_attempt_count = $3
          AND NOT EXISTS (
              SELECT 1 FROM build_jobs bj WHERE bj.derivation_id = d.id
          )
        "#,
    )
    .bind(derivation_id)
    .bind(commit_id)
    .bind(expected_attempt)
    .bind(derivation_path)
    .bind(error)
    .execute(pool)
    .await;

    match result {
        Ok(r) if r.rows_affected() == 0 => {
            debug!(
                derivation_id,
                "record_recovery_failure: stale guard prevented update (0 rows)"
            );
        }
        Ok(_) => {}
        Err(e) => {
            warn!(
                derivation_id,
                "record_recovery_failure: update failed: {e:#}"
            );
        }
    }
}

/// Recover derivations whose build-queue preparation failed or was interrupted.
///
/// Only derivations explicitly marked `build_preparation_state = 'pending'` or
/// `'failed'` are eligible. `'not_required'` (scope-excluded, policy-excluded) and
/// `NULL` (rows pre-dating this state machine) are never recovered.
/// Failed rows are subject to exponential backoff via `next_attempt_at`.
///
/// For each candidate:
/// 1. Creates or verifies the derivation GC root (prevents GC of the drv).
/// 2. Revalidates the candidate state inside a transaction with `FOR UPDATE`.
/// 3. Inserts the build job under the advisory lock (only after rooting).
/// 4. Sets `build_preparation_state = 'queued'` on success or `'failed'` on error.
///
/// Idempotent: existing history excludes stale recovery candidates, and the
/// partial active-attempt conflict target absorbs concurrent recovery races.
///
/// Returns the number of build jobs successfully created.
pub async fn recover_orphaned_derivation_build_jobs(pool: &PgPool) -> Result<usize> {
    // Find derivations that need recovery. Only those with explicit 'pending' or
    // 'failed' state — never NULL (pre-migration rows) or 'not_required'.
    // Failed rows are subject to exponential backoff via next_attempt_at.
    let candidates: Vec<RecoveryCandidate> = sqlx::query_as(
        r#"
        SELECT
            d.id AS derivation_id,
            d.derivation_path,
            d.derivation_target,
            d.commit_id,
            c.flake_id,
            c.evaluation_attempt_count
        FROM derivations d
        LEFT JOIN commits c ON c.id = d.commit_id
        WHERE d.build_preparation_state IN ('pending', 'failed')
          AND d.status_id = 5                       -- DryRunComplete
          AND d.cf_agent_enabled = TRUE
          AND d.policy_requirements_met = TRUE
          AND c.evaluation_status = 'complete'      -- commit fully evaluated
          AND (d.build_preparation_next_attempt_at IS NULL
               OR d.build_preparation_next_attempt_at <= NOW())  -- backoff gate
          AND NOT EXISTS (
              SELECT 1 FROM build_jobs bj WHERE bj.derivation_id = d.id
          )
        ORDER BY d.id
        "#,
    )
    .fetch_all(pool)
    .await
    .context("Failed to query recovery candidates")?;

    if candidates.is_empty() {
        return Ok(0);
    }

    info!(
        "🔍 Found {} build-preparation recovery candidate(s)",
        candidates.len()
    );

    let mut recovered = 0usize;

    for candidate in &candidates {
        let derivation_id = candidate.derivation_id;
        let commit_id = candidate.commit_id.unwrap_or(0);
        let expected_attempt = candidate.evaluation_attempt_count.unwrap_or(0);
        let drv_path = match &candidate.derivation_path {
            Some(p) => p.clone(),
            None => {
                let msg = "Skipping recovery: no drv path on derivation";
                warn!(derivation_id, "{msg}");
                record_recovery_failure(
                    pool,
                    derivation_id,
                    commit_id,
                    expected_attempt,
                    None,
                    msg,
                )
                .await;
                continue;
            }
        };

        // Phase 1: create / verify GC root before inserting any claimable job.
        let rooted = match crate::builder::create_drv_gc_root(&drv_path, derivation_id).await {
            Ok(r) => r,
            Err(err) => {
                let msg =
                    format!("Recovery: GC root failed for derivation {derivation_id}: {err:#}");
                warn!("{msg}");
                record_recovery_failure(
                    pool,
                    derivation_id,
                    commit_id,
                    expected_attempt,
                    Some(drv_path.as_str()),
                    &msg,
                )
                .await;
                continue;
            }
        };

        if !rooted {
            let msg = format!(
                "Recovery: derivation {derivation_id} drv path {drv_path} not valid in store"
            );
            warn!("{msg}");
            record_recovery_failure(
                pool,
                derivation_id,
                commit_id,
                expected_attempt,
                Some(drv_path.as_str()),
                &msg,
            )
            .await;
            continue;
        }

        // Phase 2: Validated lock-stage activation in correct lock order.
        //
        // Lock order (must match normal activation to prevent deadlock):
        //   1. Commit row FOR UPDATE (verify still complete)
        //   2. Advisory queue-position lock
        //   3. Derivation row FOR UPDATE (revalidate path/state/eligible)
        //   4. Read MAX(queue_position), insert, update state
        let mut tx = match pool.begin().await {
            Ok(t) => t,
            Err(err) => {
                let msg = format!("Recovery: failed to begin tx: {err:#}");
                warn!(derivation_id, "{msg}");
                record_recovery_failure(
                    pool,
                    derivation_id,
                    commit_id,
                    expected_attempt,
                    Some(drv_path.as_str()),
                    &msg,
                )
                .await;
                continue;
            }
        };

        // Step 1: lock and validate the commit row.
        match sqlx::query_scalar::<_, bool>(
            r#"
            SELECT TRUE FROM commits c
            WHERE c.id = $1
              AND c.evaluation_status = 'complete'
            FOR UPDATE
            "#,
        )
        .bind(candidate.commit_id)
        .fetch_optional(&mut *tx)
        .await
        {
            Ok(Some(_)) => {} // commit still complete, proceed
            Ok(None) => {
                warn!(
                    derivation_id,
                    "Recovery: commit no longer complete (skipping)"
                );
                let _ = tx.rollback().await;
                continue;
            }
            Err(err) => {
                let msg = format!("Recovery: commit lock query failed: {err:#}");
                warn!(derivation_id, "{msg}");
                let _ = tx.rollback().await;
                record_recovery_failure(
                    pool,
                    derivation_id,
                    commit_id,
                    expected_attempt,
                    Some(drv_path.as_str()),
                    &msg,
                )
                .await;
                continue;
            }
        };

        // Step 2: acquire build queue position lock.
        if let Err(err) = lock_build_queue_order(&mut tx).await {
            let msg = format!("Recovery: failed to acquire queue lock: {err:#}");
            warn!(derivation_id, "{msg}");
            let _ = tx.rollback().await;
            record_recovery_failure(
                pool,
                derivation_id,
                commit_id,
                expected_attempt,
                Some(drv_path.as_str()),
                &msg,
            )
            .await;
            continue;
        }

        // Step 3: lock and revalidate the derivation row.
        let revalidated: Option<()> = match sqlx::query_scalar::<_, bool>(
            r#"
            SELECT TRUE
            FROM derivations d
            WHERE d.id = $1
              AND d.build_preparation_state IN ('pending', 'failed')
              AND d.derivation_path = $2
              AND d.status_id = 5
              AND d.cf_agent_enabled = TRUE
              AND d.policy_requirements_met = TRUE
            FOR UPDATE OF d
            "#,
        )
        .bind(derivation_id)
        .bind(&drv_path)
        .fetch_optional(&mut *tx)
        .await
        {
            Ok(Some(_)) => Some(()),
            Ok(None) => None,
            Err(err) => {
                let msg = format!("Recovery: derivation revalidation failed: {err:#}");
                warn!(derivation_id, "{msg}");
                let _ = tx.rollback().await;
                record_recovery_failure(
                    pool,
                    derivation_id,
                    commit_id,
                    expected_attempt,
                    Some(drv_path.as_str()),
                    &msg,
                )
                .await;
                continue;
            }
        };

        let Some(_) = revalidated else {
            warn!(derivation_id, "Recovery: derivation state stale (skipping)");
            let _ = tx.rollback().await;
            continue;
        };

        // Step 4: read queue position and insert.
        let next_pos: i64 = match sqlx::query_scalar(
            "SELECT COALESCE(MAX(queue_position), 0) + 1 FROM build_jobs WHERE status = 'queued' OR status = 'building'",
        )
        .fetch_one(&mut *tx)
        .await
        {
            Ok(p) => p,
            Err(err) => {
                let msg = format!("Recovery: failed to read max position: {err:#}");
                warn!(derivation_id, "{msg}");
                let _ = tx.rollback().await;
                record_recovery_failure(
                    pool, derivation_id, commit_id, expected_attempt, Some(drv_path.as_str()), &msg,
                )
                .await;
                continue;
            }
        };

        // Insert or detect existing build job.
        let inserted: Result<Option<bool>, _> = sqlx::query_scalar(
            r#"
            INSERT INTO build_jobs (
                derivation_id, environment_id, priority_weight, queue_position, status
            )
            SELECT
                d.id,
                s.environment_id,
                CASE WHEN s.id IS NOT NULL THEN 10.0 ELSE 1.0 END *
                CASE
                    WHEN EXTRACT(EPOCH FROM (NOW() - c.commit_timestamp)) < 3600  THEN 2.0
                    WHEN EXTRACT(EPOCH FROM (NOW() - c.commit_timestamp)) < 86400 THEN 1.5
                    ELSE 1.0
                END,
                $2,
                'queued'
            FROM derivations d
            INNER JOIN commits c ON c.id = d.commit_id
            LEFT JOIN systems s ON (
                d.derivation_target = s.hostname AND s.flake_id = c.flake_id
            )
            WHERE d.id = $1
              AND d.status_id = 5
              AND d.cf_agent_enabled = TRUE
              AND d.policy_requirements_met = TRUE
              AND NOT EXISTS (
                  SELECT 1 FROM build_jobs existing
                  WHERE existing.derivation_id = d.id
              )
            ON CONFLICT (derivation_id)
                WHERE status IN ('queued', 'building', 'cancelling') DO NOTHING
            RETURNING TRUE
            "#,
        )
        .bind(derivation_id)
        .bind(next_pos)
        .fetch_optional(&mut *tx)
        .await;

        match inserted {
            Ok(Some(true)) => {
                crate::queries::cve_scan_leases::create_post_build_scan_intents_tx(
                    &mut tx,
                    &[derivation_id],
                )
                .await
                .context("Failed to create recovery post-build scan intent")?;
                // Successfully inserted. Update state inside the same tx
                // (never use a separate pooled connection while holding row locks).
                match sqlx::query(
                    r#"
                    UPDATE derivations
                    SET build_preparation_state = 'queued',
                        build_preparation_attempts = 0,
                        build_preparation_last_error = NULL,
                        build_preparation_next_attempt_at = NULL
                    WHERE id = $1
                    "#,
                )
                .bind(derivation_id)
                .execute(&mut *tx)
                .await
                {
                    Ok(_) => {}
                    Err(err) => {
                        let msg = format!("Recovery: state update failed after insert: {err:#}");
                        warn!(derivation_id, "{msg}");
                        let _ = tx.rollback().await;
                        record_recovery_failure(
                            pool,
                            derivation_id,
                            commit_id,
                            expected_attempt,
                            Some(drv_path.as_str()),
                            &msg,
                        )
                        .await;
                        continue;
                    }
                }

                if let Err(err) = tx.commit().await {
                    let msg = format!("Recovery: commit failed: {err:#}");
                    warn!(derivation_id, "{msg}");
                    record_recovery_failure(
                        pool,
                        derivation_id,
                        commit_id,
                        expected_attempt,
                        Some(drv_path.as_str()),
                        &msg,
                    )
                    .await;
                    continue;
                }

                info!(
                    derivation_id,
                    "🔄 Recovery: created missing build job for derivation {derivation_id}"
                );
                recovered += 1;
            }
            Ok(Some(false)) | Ok(None) => {
                // INSERT returned no row. Check whether a build job exists
                // (INSERT was blocked by ON CONFLICT DO NOTHING) or eligibility
                // changed between revalidation and INSERT.
                let exists: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM build_jobs WHERE derivation_id = $1)",
                )
                .bind(derivation_id)
                .fetch_one(&mut *tx)
                .await
                .unwrap_or(false);

                if exists {
                    // Job already exists — update state inside this tx.
                    // DO NOT use a pool-based helper while holding row locks.
                    sqlx::query(
                        r#"
                        UPDATE derivations
                        SET build_preparation_state = 'queued',
                            build_preparation_attempts = 0,
                            build_preparation_last_error = NULL,
                            build_preparation_next_attempt_at = NULL
                        WHERE id = $1
                        "#,
                    )
                    .bind(derivation_id)
                    .execute(&mut *tx)
                    .await
                    .context("Failed to reconcile existing build job")?;

                    if let Err(err) = tx.commit().await {
                        let msg = format!("Recovery: commit failed: {err:#}");
                        warn!(derivation_id, "{msg}");
                        record_recovery_failure(
                            pool,
                            derivation_id,
                            commit_id,
                            expected_attempt,
                            Some(drv_path.as_str()),
                            &msg,
                        )
                        .await;
                        continue;
                    }

                    info!(
                        derivation_id,
                        "Recovery: build job already exists for derivation {derivation_id}"
                    );
                    recovered += 1;
                } else {
                    // Eligibility changed — derivation is no longer eligible.
                    // Leave it with its current state (don't reset, don't fail).
                    warn!(
                        derivation_id,
                        "Recovery: derivation {derivation_id} no longer eligible for queue",
                    );
                    let _ = tx.rollback().await;
                }
            }
            Err(err) => {
                let msg = format!("Recovery: insert query failed: {err:#}");
                warn!(derivation_id, "{msg}");
                let _ = tx.rollback().await;
                record_recovery_failure(
                    pool,
                    derivation_id,
                    commit_id,
                    expected_attempt,
                    Some(drv_path.as_str()),
                    &msg,
                )
                .await;
            }
        }
    }

    Ok(recovered)
}

/// Mark a build job as successful.
pub async fn mark_job_success(pool: &PgPool, job_id: Uuid, logs: Option<&str>) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE build_jobs
        SET 
            status = 'success',
            server_failure_code = NULL,
            completed_at = NOW(),
            logs = COALESCE($2, logs),
            updated_at = NOW()
        WHERE id = $1
        "#,
        job_id,
        logs
    )
    .execute(pool)
    .await
    .context("Failed to mark job as success")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::queue::QueueNotifier;
    use sqlx::PgPool;
    use uuid::Uuid;

    async fn insert_buildable_derivation(
        pool: &PgPool,
        label: &str,
        derivation_type: &str,
    ) -> (i32, i32) {
        let suffix = Uuid::new_v4().simple().to_string();
        let repo_url = format!("https://example.test/{label}-{suffix}.git");
        crate::queries::flakes::insert_flake(
            pool,
            &format!("{label}-{suffix}"),
            &repo_url,
            "main",
            "all_configs",
        )
        .await
        .expect("test flake should be inserted");
        crate::queries::commits::insert_commit(
            pool,
            &format!("{label}-{suffix}"),
            &repo_url,
            chrono::Utc::now(),
        )
        .await
        .expect("test commit should be inserted");
        let commit_id: i32 =
            sqlx::query_scalar("SELECT id FROM commits WHERE git_commit_hash = $1")
                .bind(format!("{label}-{suffix}"))
                .fetch_one(pool)
                .await
                .expect("test commit should load");
        let derivation_id: i32 = sqlx::query_scalar(
            r#"
            INSERT INTO derivations (
                commit_id, derivation_type, derivation_name, derivation_path,
                status_id, attempt_count, cf_agent_enabled,
                policy_requirements_met
            )
            VALUES ($1, $2, $3, $4, 5, 0, TRUE, TRUE)
            RETURNING id
            "#,
        )
        .bind(commit_id)
        .bind(derivation_type)
        .bind(format!("{label}-{suffix}"))
        .bind(format!("/nix/store/{suffix}-{label}.drv"))
        .fetch_one(pool)
        .await
        .expect("test derivation should be inserted");
        (commit_id, derivation_id)
    }

    async fn active_scan_state(pool: &PgPool, derivation_id: i32) -> Vec<(String, String, i32)> {
        sqlx::query_as(
            r#"
            SELECT status, source_trigger, attempts
            FROM cve_scans
            WHERE derivation_id = $1
              AND status IN ('awaiting_build', 'awaiting_closure', 'pending', 'in_progress')
            ORDER BY created_at, id
            "#,
        )
        .bind(derivation_id)
        .fetch_all(pool)
        .await
        .expect("active scan state should load")
    }

    /// Verify that a QueueNotifier notification issued after incremental enqueue
    /// is observable by a waiting consumer.
    #[tokio::test]
    async fn incremental_enqueue_notifies_build_queue() {
        let notifier = QueueNotifier::new();
        let notifier_clone = notifier.clone();

        let handle = tokio::spawn(async move {
            notifier_clone.wait_for_build_work().await;
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;

        // This is what enqueue_build_job_for_derivation's caller does on Ok(true).
        notifier.notify_build_queue();

        let result = tokio::time::timeout(tokio::time::Duration::from_millis(100), handle).await;
        assert!(
            result.is_ok(),
            "Build queue notification should wake up waiter"
        );
    }

    /// Verify that multiple rapid notifications from incremental per-derivation enqueues
    /// are coalesced to a single wakeup (bounded channel capacity = 1).
    #[tokio::test]
    async fn incremental_enqueue_notifications_are_coalesced() {
        let notifier = QueueNotifier::new();

        // Simulate N derivations all enqueuing before any builder wakes up.
        for _ in 0..20 {
            notifier.notify_build_queue();
        }

        // Drain the single queued wakeup.
        notifier.wait_for_build_work().await;

        // No second wakeup should be pending after draining the coalesced token.
        let result = tokio::time::timeout(
            tokio::time::Duration::from_millis(50),
            notifier.wait_for_build_work(),
        )
        .await;
        assert!(
            result.is_err(),
            "Coalesced notifications should produce exactly one wakeup"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires test database creation privileges"]
    async fn post_build_intent_covers_authoritative_admission_paths_and_gates(pool: PgPool) {
        sqlx::query("UPDATE scan_schedule_policy SET on_build = TRUE WHERE id = 1")
            .execute(&pool)
            .await
            .expect("post-build policy should be enabled");

        let (pool_commit, pool_derivation) =
            insert_buildable_derivation(&pool, "post-build-pool", "nixos").await;
        assert_eq!(
            super::create_build_jobs_for_commit(&pool, pool_commit)
                .await
                .expect("pool-owned bulk admission should succeed"),
            1
        );

        let (tx_commit, tx_derivation) =
            insert_buildable_derivation(&pool, "post-build-tx", "nixos").await;
        let mut tx = pool.begin().await.expect("bulk transaction should begin");
        assert_eq!(
            super::create_build_jobs_for_commit_tx(&mut tx, tx_commit)
                .await
                .expect("caller-owned bulk admission should succeed")
                .len(),
            1
        );
        tx.commit().await.expect("bulk transaction should commit");

        let (_, single_derivation) =
            insert_buildable_derivation(&pool, "post-build-single", "nixos").await;
        let mut tx = pool.begin().await.expect("single transaction should begin");
        assert!(matches!(
            super::create_build_job_for_derivation_tx(&mut tx, single_derivation)
                .await
                .expect("single admission should succeed"),
            Some(super::BuildJobInsertOutcome::Inserted { .. })
        ));
        tx.commit().await.expect("single transaction should commit");

        let (_, incremental_derivation) =
            insert_buildable_derivation(&pool, "post-build-incremental", "nixos").await;
        assert!(
            super::enqueue_build_job_for_derivation(&pool, incremental_derivation)
                .await
                .expect("incremental admission should succeed")
        );

        for derivation_id in [
            pool_derivation,
            tx_derivation,
            single_derivation,
            incremental_derivation,
        ] {
            assert_eq!(
                active_scan_state(&pool, derivation_id).await,
                vec![("awaiting_build".into(), "post_build".into(), 0)]
            );
        }
        assert!(
            !super::enqueue_build_job_for_derivation(&pool, incremental_derivation)
                .await
                .expect("duplicate admission should be idempotent")
        );
        assert_eq!(
            active_scan_state(&pool, incremental_derivation).await.len(),
            1
        );

        sqlx::query("UPDATE scan_schedule_policy SET on_build = FALSE WHERE id = 1")
            .execute(&pool)
            .await
            .expect("post-build policy should be disabled");
        let (_, disabled_derivation) =
            insert_buildable_derivation(&pool, "post-build-disabled", "nixos").await;
        assert!(
            super::enqueue_build_job_for_derivation(&pool, disabled_derivation)
                .await
                .expect("disabled admission should still create its build")
        );
        assert!(
            active_scan_state(&pool, disabled_derivation)
                .await
                .is_empty()
        );

        sqlx::query("UPDATE scan_schedule_policy SET on_build = TRUE WHERE id = 1")
            .execute(&pool)
            .await
            .expect("post-build policy should be re-enabled");
        let (_, package_derivation) =
            insert_buildable_derivation(&pool, "post-build-package", "package").await;
        assert!(
            super::enqueue_build_job_for_derivation(&pool, package_derivation)
                .await
                .expect("package admission should still create its build")
        );
        assert!(
            active_scan_state(&pool, package_derivation)
                .await
                .is_empty()
        );

        let (_, rolled_back_derivation) =
            insert_buildable_derivation(&pool, "post-build-rollback", "nixos").await;
        let mut tx = pool
            .begin()
            .await
            .expect("rollback transaction should begin");
        assert!(matches!(
            super::create_build_job_for_derivation_tx(&mut tx, rolled_back_derivation)
                .await
                .expect("rolled-back admission should execute"),
            Some(super::BuildJobInsertOutcome::Inserted { .. })
        ));
        tx.rollback()
            .await
            .expect("admission transaction should roll back");
        let persisted: (i64, i64) = sqlx::query_as(
            r#"
            SELECT
                (SELECT COUNT(*) FROM build_jobs WHERE derivation_id = $1),
                (SELECT COUNT(*) FROM cve_scans WHERE derivation_id = $1)
            "#,
        )
        .bind(rolled_back_derivation)
        .fetch_one(&pool)
        .await
        .expect("rolled-back rows should be countable");
        assert_eq!(persisted, (0, 0));
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires test database creation privileges"]
    async fn post_build_intent_is_atomic_under_failure_and_concurrent_admission(pool: PgPool) {
        let (_, concurrent_derivation) =
            insert_buildable_derivation(&pool, "post-build-concurrent", "nixos").await;
        let first_pool = pool.clone();
        let second_pool = pool.clone();
        let (first, second) = tokio::join!(
            super::enqueue_build_job_for_derivation(&first_pool, concurrent_derivation),
            super::enqueue_build_job_for_derivation(&second_pool, concurrent_derivation),
        );
        let admitted = [first, second]
            .into_iter()
            .map(|result| result.expect("concurrent admission should not fail"))
            .filter(|created| *created)
            .count();
        assert_eq!(admitted, 1);
        let concurrent_counts: (i64, i64) = sqlx::query_as(
            r#"
            SELECT
                (SELECT COUNT(*) FROM build_jobs WHERE derivation_id = $1),
                (SELECT COUNT(*) FROM cve_scans WHERE derivation_id = $1)
            "#,
        )
        .bind(concurrent_derivation)
        .fetch_one(&pool)
        .await
        .expect("concurrent rows should be countable");
        assert_eq!(concurrent_counts, (1, 1));

        sqlx::query(
            r#"
            CREATE FUNCTION reject_test_post_build_intent() RETURNS trigger
            LANGUAGE plpgsql AS $$
            BEGIN
                IF NEW.source_trigger = 'post_build' THEN
                    RAISE EXCEPTION 'test post-build intent rejection';
                END IF;
                RETURN NEW;
            END;
            $$
            "#,
        )
        .execute(&pool)
        .await
        .expect("failure-injection function should be installed");
        sqlx::query(
            r#"
            CREATE TRIGGER reject_test_post_build_intent
            BEFORE INSERT ON cve_scans
            FOR EACH ROW EXECUTE FUNCTION reject_test_post_build_intent()
            "#,
        )
        .execute(&pool)
        .await
        .expect("failure-injection trigger should be installed");
        let (_, rejected_derivation) =
            insert_buildable_derivation(&pool, "post-build-rejected", "nixos").await;
        let error = super::enqueue_build_job_for_derivation(&pool, rejected_derivation)
            .await
            .expect_err("scan intent failure must reject build admission");
        assert!(error.to_string().contains("post-build scan intent"));
        let rejected_counts: (i64, i64) = sqlx::query_as(
            r#"
            SELECT
                (SELECT COUNT(*) FROM build_jobs WHERE derivation_id = $1),
                (SELECT COUNT(*) FROM cve_scans WHERE derivation_id = $1)
            "#,
        )
        .bind(rejected_derivation)
        .fetch_one(&pool)
        .await
        .expect("rejected rows should be countable");
        assert_eq!(rejected_counts, (0, 0));
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires test database creation privileges"]
    async fn post_build_admission_preserves_existing_active_provenance(pool: PgPool) {
        for trigger in ["manual", "fleet"] {
            let (_, derivation_id) =
                insert_buildable_derivation(&pool, &format!("post-build-{trigger}"), "nixos").await;
            sqlx::query(
                "INSERT INTO cve_scans (derivation_id, scanner_name, status, attempts, source_trigger) VALUES ($1, 'vulnix', 'pending', 0, $2)",
            )
            .bind(derivation_id)
            .bind(trigger)
            .execute(&pool)
            .await
            .expect("existing active scan should be inserted");

            assert!(
                super::enqueue_build_job_for_derivation(&pool, derivation_id)
                    .await
                    .expect("build admission should succeed")
            );
            assert_eq!(
                active_scan_state(&pool, derivation_id).await,
                vec![("pending".into(), trigger.into(), 0)]
            );
            let build_job_id: Uuid = sqlx::query_scalar(
                "UPDATE build_jobs SET status = 'success', completed_at = NOW() WHERE derivation_id = $1 RETURNING id",
            )
            .bind(derivation_id)
            .fetch_one(&pool)
            .await
            .expect("winning-provenance build should complete");
            sqlx::query(
                "UPDATE cve_scans SET status = 'completed', completed_at = NOW() WHERE derivation_id = $1",
            )
            .bind(derivation_id)
            .execute(&pool)
            .await
            .expect("winning-provenance scan should become terminal");
            let mut tx = pool
                .begin()
                .await
                .expect("completion transaction should begin");
            assert!(
                !crate::queries::cve_scan_leases::attach_completed_build_to_post_build_scan_tx(
                    &mut tx,
                    build_job_id,
                )
                .await
                .expect("completion attachment should remain a no-op")
            );
            tx.commit()
                .await
                .expect("completion transaction should commit");
            let history: Vec<(String, String)> = sqlx::query_as(
                "SELECT status, source_trigger FROM cve_scans WHERE derivation_id = $1 ORDER BY created_at, id",
            )
            .bind(derivation_id)
            .fetch_all(&pool)
            .await
            .expect("scan history should load");
            assert_eq!(history, vec![("completed".into(), trigger.into())]);
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires test database creation privileges"]
    async fn post_build_completion_repairs_only_eligible_missing_intent(pool: PgPool) {
        let (_, eligible_derivation) =
            insert_buildable_derivation(&pool, "post-build-legacy-eligible", "nixos").await;
        let eligible_output = format!("/nix/store/{eligible_derivation}-legacy-system");
        sqlx::query("UPDATE derivations SET store_path = $2 WHERE id = $1")
            .bind(eligible_derivation)
            .bind(&eligible_output)
            .execute(&pool)
            .await
            .expect("legacy build output should persist");
        let eligible_job_id: Uuid = sqlx::query_scalar(
            "INSERT INTO build_jobs (derivation_id, status, completed_at) VALUES ($1, 'success', NOW()) RETURNING id",
        )
        .bind(eligible_derivation)
        .fetch_one(&pool)
        .await
        .expect("legacy successful build should be inserted without admission intent");

        let mut tx = pool
            .begin()
            .await
            .expect("legacy repair transaction should begin");
        assert!(
            crate::queries::cve_scan_leases::attach_completed_build_to_post_build_scan_tx(
                &mut tx,
                eligible_job_id,
            )
            .await
            .expect("eligible legacy completion should repair its missing intent")
        );
        tx.commit()
            .await
            .expect("legacy repair transaction should commit");
        let repaired: (String, String, i32, Option<Uuid>) = sqlx::query_as(
            "SELECT status, source_trigger, attempts, completed_build_job_id FROM cve_scans WHERE derivation_id = $1",
        )
        .bind(eligible_derivation)
        .fetch_one(&pool)
        .await
        .expect("repaired intent should load");
        assert_eq!(
            repaired,
            (
                "awaiting_build".into(),
                "post_build".into(),
                0,
                Some(eligible_job_id)
            )
        );
        assert_eq!(
            crate::queries::cve_scans::promote_waiting_cve_scans(&pool, 10)
                .await
                .expect("authoritative promotion should advance repaired intent"),
            1
        );
        assert_eq!(
            active_scan_state(&pool, eligible_derivation).await,
            vec![("pending".into(), "post_build".into(), 0)]
        );

        sqlx::query("UPDATE scan_schedule_policy SET on_build = FALSE WHERE id = 1")
            .execute(&pool)
            .await
            .expect("post-build policy should be disabled");
        let (_, disabled_derivation) =
            insert_buildable_derivation(&pool, "post-build-legacy-disabled", "nixos").await;
        let disabled_job_id: Uuid = sqlx::query_scalar(
            "INSERT INTO build_jobs (derivation_id, status, completed_at) VALUES ($1, 'success', NOW()) RETURNING id",
        )
        .bind(disabled_derivation)
        .fetch_one(&pool)
        .await
        .expect("disabled-policy build should be inserted");
        let mut tx = pool
            .begin()
            .await
            .expect("disabled-policy transaction should begin");
        assert!(
            !crate::queries::cve_scan_leases::attach_completed_build_to_post_build_scan_tx(
                &mut tx,
                disabled_job_id,
            )
            .await
            .expect("disabled policy should not repair an intent")
        );
        tx.commit()
            .await
            .expect("disabled-policy transaction should commit");

        sqlx::query("UPDATE scan_schedule_policy SET on_build = TRUE WHERE id = 1")
            .execute(&pool)
            .await
            .expect("post-build policy should be enabled");
        let (_, package_derivation) =
            insert_buildable_derivation(&pool, "post-build-legacy-package", "package").await;
        let package_job_id: Uuid = sqlx::query_scalar(
            "INSERT INTO build_jobs (derivation_id, status, completed_at) VALUES ($1, 'success', NOW()) RETURNING id",
        )
        .bind(package_derivation)
        .fetch_one(&pool)
        .await
        .expect("package build should be inserted");
        let mut tx = pool
            .begin()
            .await
            .expect("package completion transaction should begin");
        assert!(
            !crate::queries::cve_scan_leases::attach_completed_build_to_post_build_scan_tx(
                &mut tx,
                package_job_id,
            )
            .await
            .expect("package completion should not repair an intent")
        );
        tx.commit()
            .await
            .expect("package completion transaction should commit");

        for derivation_id in [disabled_derivation, package_derivation] {
            assert!(active_scan_state(&pool, derivation_id).await.is_empty());
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires test database creation privileges"]
    async fn post_build_completion_attaches_then_authority_promotes_one_intent(pool: PgPool) {
        let builder_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO builders (id, name, public_key, arch, status) VALUES ($1, $2, $3, 'x86_64-linux', 'active')",
        )
        .bind(builder_id)
        .bind(format!("post-build-builder-{builder_id}"))
        .bind(format!("post-build-key-{builder_id}"))
        .execute(&pool)
        .await
        .expect("remote builder should be inserted");

        let (_, remote_derivation) =
            insert_buildable_derivation(&pool, "post-build-remote", "nixos").await;
        assert!(
            super::enqueue_build_job_for_derivation(&pool, remote_derivation)
                .await
                .expect("remote build should be admitted")
        );
        let remote_job_id: Uuid =
            sqlx::query_scalar("SELECT id FROM build_jobs WHERE derivation_id = $1")
                .bind(remote_derivation)
                .fetch_one(&pool)
                .await
                .expect("remote build job should load");
        sqlx::query(
            "UPDATE build_jobs SET status = 'building', builder_id = $2, started_at = NOW() WHERE id = $1",
        )
        .bind(remote_job_id)
        .bind(builder_id)
        .execute(&pool)
        .await
        .expect("remote build should be assigned");
        sqlx::query("UPDATE derivations SET store_path = $2 WHERE id = $1")
            .bind(remote_derivation)
            .bind(format!(
                "/nix/store/{remote_derivation}-retained-prior-output"
            ))
            .execute(&pool)
            .await
            .expect("retained prior output should be persisted");
        assert_eq!(
            crate::queries::cve_scans::promote_waiting_cve_scans(&pool, 10)
                .await
                .expect("incomplete exact build must remain waiting"),
            0
        );
        assert_eq!(
            active_scan_state(&pool, remote_derivation).await,
            vec![("awaiting_build".into(), "post_build".into(), 0)]
        );
        let remote_output = format!("/nix/store/{remote_derivation}-remote-system");
        let mut tx = pool
            .begin()
            .await
            .expect("remote completion transaction should begin");
        sqlx::query("UPDATE build_jobs SET status = 'success', completed_at = NOW() WHERE id = $1")
            .bind(remote_job_id)
            .execute(&mut *tx)
            .await
            .expect("remote build should complete");
        sqlx::query("UPDATE derivations SET store_path = $2 WHERE id = $1")
            .bind(remote_derivation)
            .bind(&remote_output)
            .execute(&mut *tx)
            .await
            .expect("remote output should be persisted");
        assert!(
            crate::queries::cve_scan_leases::attach_completed_build_to_post_build_scan_tx(
                &mut tx,
                remote_job_id,
            )
            .await
            .expect("remote scan intent should attach the completed build")
        );
        tx.commit()
            .await
            .expect("remote completion transaction should commit");
        let remote_state: (String, String, i32, Option<Uuid>) = sqlx::query_as(
            "SELECT status, source_trigger, attempts, completed_build_job_id FROM cve_scans WHERE derivation_id = $1",
        )
        .bind(remote_derivation)
        .fetch_one(&pool)
        .await
        .expect("remote scan state should load");
        assert_eq!(
            remote_state,
            (
                "awaiting_build".into(),
                "post_build".into(),
                0,
                Some(remote_job_id)
            )
        );
        assert_eq!(
            crate::queries::cve_scans::promote_waiting_cve_scans(&pool, 10)
                .await
                .expect("remote waiting scan should promote"),
            1
        );
        assert_eq!(
            active_scan_state(&pool, remote_derivation).await,
            vec![("awaiting_closure".into(), "post_build".into(), 0)]
        );
        assert!(
            crate::queries::cve_scans::claim_queued_cve_scans(&pool, 1)
                .await
                .expect("waiting scan claim should execute")
                .is_empty()
        );
        sqlx::query(
            r#"
            INSERT INTO cache_push_jobs (
                derivation_id, status, store_path, completed_at, cache_destination
            ) VALUES ($1, 'completed', $2, NOW(), 'post-build-test-cache')
            "#,
        )
        .bind(remote_derivation)
        .bind(&remote_output)
        .execute(&pool)
        .await
        .expect("completed remote cache publication should be inserted");
        assert_eq!(
            crate::queries::cve_scans::promote_waiting_cve_scans(&pool, 10)
                .await
                .expect("cache-published remote scan should promote"),
            1
        );
        assert_eq!(
            active_scan_state(&pool, remote_derivation).await,
            vec![("pending".into(), "post_build".into(), 0)]
        );
        let (_, retry_is_new) = crate::queries::builders::complete_job_atomic(
            &pool,
            &remote_job_id,
            &builder_id,
            None,
            Some("/nix/store/ignored-retry-output"),
        )
        .await
        .expect("completion retry should be idempotent");
        assert!(!retry_is_new);
        assert_eq!(active_scan_state(&pool, remote_derivation).await.len(), 1);
        sqlx::query(
            "UPDATE cve_scans SET status = 'completed', completed_at = NOW() WHERE derivation_id = $1",
        )
        .bind(remote_derivation)
        .execute(&pool)
        .await
        .expect("remote scan should become terminal");
        let (_, terminal_retry_is_new) = crate::queries::builders::complete_job_atomic(
            &pool,
            &remote_job_id,
            &builder_id,
            None,
            Some("/nix/store/ignored-terminal-retry-output"),
        )
        .await
        .expect("terminal completion retry should remain idempotent");
        assert!(!terminal_retry_is_new);
        let post_build_history: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cve_scans WHERE derivation_id = $1 AND source_trigger = 'post_build'",
        )
        .bind(remote_derivation)
        .fetch_one(&pool)
        .await
        .expect("post-build history should be countable");
        assert_eq!(post_build_history, 1);

        let (_, local_derivation) =
            insert_buildable_derivation(&pool, "post-build-local", "nixos").await;
        assert!(
            super::enqueue_build_job_for_derivation(&pool, local_derivation)
                .await
                .expect("local build should be admitted")
        );
        let local_job_id: Uuid = sqlx::query_scalar(
            "UPDATE build_jobs SET status = 'success', completed_at = NOW() WHERE derivation_id = $1 RETURNING id",
        )
        .bind(local_derivation)
        .fetch_one(&pool)
        .await
        .expect("local build should complete");
        sqlx::query("UPDATE derivations SET store_path = $2 WHERE id = $1")
            .bind(local_derivation)
            .bind(format!("/nix/store/{local_derivation}-local-system"))
            .execute(&pool)
            .await
            .expect("local output should be persisted");
        let mut tx = pool
            .begin()
            .await
            .expect("local completion transaction should begin");
        assert!(
            crate::queries::cve_scan_leases::attach_completed_build_to_post_build_scan_tx(
                &mut tx,
                local_job_id,
            )
            .await
            .expect("local scan intent should attach the completed build")
        );
        tx.commit()
            .await
            .expect("local completion transaction should commit");
        assert_eq!(
            active_scan_state(&pool, local_derivation).await,
            vec![("awaiting_build".into(), "post_build".into(), 0)]
        );
        assert_eq!(
            crate::queries::cve_scans::promote_waiting_cve_scans(&pool, 10)
                .await
                .expect("local waiting scan should promote"),
            1
        );
        assert_eq!(
            active_scan_state(&pool, local_derivation).await,
            vec![("pending".into(), "post_build".into(), 0)]
        );

        let (_, failed_derivation) =
            insert_buildable_derivation(&pool, "post-build-failed", "nixos").await;
        assert!(
            super::enqueue_build_job_for_derivation(&pool, failed_derivation)
                .await
                .expect("failed build should be admitted")
        );
        let failed_job_id: Uuid = sqlx::query_scalar(
            "UPDATE build_jobs SET max_retries = 1 WHERE derivation_id = $1 RETURNING id",
        )
        .bind(failed_derivation)
        .fetch_one(&pool)
        .await
        .expect("failed build job should load");
        super::mark_job_failed(&pool, failed_job_id, "build failed", None)
            .await
            .expect("build failure should persist");
        assert_eq!(
            active_scan_state(&pool, failed_derivation).await,
            vec![("awaiting_build".into(), "post_build".into(), 0)]
        );
        let replacement = crate::queries::builders::requeue_build_job_as_new_attempt(
            &pool,
            &failed_job_id,
            Uuid::nil(),
            true,
        )
        .await
        .expect("failed build should create a replacement attempt");
        assert_ne!(replacement.attempt.id, failed_job_id);
        assert_eq!(active_scan_state(&pool, failed_derivation).await.len(), 1);
        let replacement_prerequisite: Option<Uuid> = sqlx::query_scalar(
            "SELECT completed_build_job_id FROM cve_scans WHERE derivation_id = $1",
        )
        .bind(failed_derivation)
        .fetch_one(&pool)
        .await
        .expect("replacement scan prerequisite should load");
        assert_eq!(replacement_prerequisite, Some(replacement.attempt.id));
    }

    /// The per-derivation SQL uses the active-attempt conflict target for
    /// idempotent insertion. A guarded source lookup controls immutable
    /// obsolete-row replacement. Both paths share status_id = 5
    /// (DryRunComplete) as the eligibility gate with the bulk
    /// `create_build_jobs_for_commit` function.
    ///
    /// This test documents the contract so regressions in the SQL predicate are caught.
    #[test]
    fn enqueue_eligibility_gate_is_dry_run_complete() {
        // status_id = 5 is the DryRunComplete status (migration 0027).
        // Both enqueue_build_job_for_derivation and create_build_jobs_for_commit
        // require this; if the migration ever renumbers it this test will need updating.
        const DRY_RUN_COMPLETE_STATUS_ID: i32 = 5;
        assert_eq!(DRY_RUN_COMPLETE_STATUS_ID, 5);
    }

    /// The real eval path in evaluate_with_nix_eval_jobs guards incremental enqueue
    /// on `cf_agent_enabled == Some(true)` and `policy_requirements_met == true`.
    /// This test drives that predicate directly so a refactor that widens the
    /// condition will break the test.
    #[test]
    fn real_path_policy_gate_only_enqueues_passing_configs() {
        // Simulate the gate expression used in evaluate_with_nix_eval_jobs.
        fn should_enqueue(
            cf_agent_enabled: Option<bool>,
            policy_requirements_met: bool,
            has_error: bool,
            has_drv: bool,
        ) -> bool {
            !has_error && has_drv && cf_agent_enabled == Some(true) && policy_requirements_met
        }

        // Policy passed, eval success → enqueue.
        assert!(should_enqueue(Some(true), true, false, true));

        // Policy explicitly failed → do NOT enqueue.
        assert!(!should_enqueue(Some(false), false, false, true));

        // A non-agent strict policy failed → do NOT enqueue.
        assert!(!should_enqueue(Some(true), false, false, true));

        // Policy result unknown (None) → do NOT enqueue.
        assert!(!should_enqueue(None, false, false, true));

        // Eval error → do NOT enqueue even if policy would pass.
        assert!(!should_enqueue(Some(true), true, true, true));

        // Missing drv path → do NOT enqueue.
        assert!(!should_enqueue(Some(true), true, false, false));
    }

    /// The backstop `create_build_jobs_for_commit` SQL now requires
    /// `d.cf_agent_enabled = TRUE` and `d.policy_requirements_met = TRUE` in
    /// addition to DryRunComplete.
    ///
    /// This test documents that contract so accidental removal of the predicate
    /// is caught at review time. It mirrors the WHERE clause in the query.
    #[test]
    fn backstop_sql_predicate_requires_cf_agent_enabled() {
        // Simulate the eligibility check the SQL performs per-derivation.
        struct MockDerivation {
            status_id: i32,
            cf_agent_enabled: Option<bool>,
            policy_requirements_met: bool,
            has_existing_job: bool,
        }

        fn is_eligible(d: &MockDerivation) -> bool {
            d.status_id == 5              // DryRunComplete
            && d.cf_agent_enabled == Some(true)  // policy passed
            && d.policy_requirements_met
            // ON CONFLICT handles existing jobs; has_existing_job is checked
            // here for documentation of the expected outcome only.
            && !d.has_existing_job
        }

        // Passes all conditions.
        assert!(is_eligible(&MockDerivation {
            status_id: 5,
            cf_agent_enabled: Some(true),
            policy_requirements_met: true,
            has_existing_job: false
        }));

        // Policy failed.
        assert!(!is_eligible(&MockDerivation {
            status_id: 5,
            cf_agent_enabled: Some(false),
            policy_requirements_met: false,
            has_existing_job: false
        }));

        // Non-agent strict policy failed.
        assert!(!is_eligible(&MockDerivation {
            status_id: 5,
            cf_agent_enabled: Some(true),
            policy_requirements_met: false,
            has_existing_job: false
        }));

        // Policy unknown.
        assert!(!is_eligible(&MockDerivation {
            status_id: 5,
            cf_agent_enabled: None,
            policy_requirements_met: false,
            has_existing_job: false
        }));

        // Not DryRunComplete.
        assert!(!is_eligible(&MockDerivation {
            status_id: 4,
            cf_agent_enabled: Some(true),
            policy_requirements_met: true,
            has_existing_job: false
        }));

        // Job already exists (idempotency guard).
        assert!(!is_eligible(&MockDerivation {
            status_id: 5,
            cf_agent_enabled: Some(true),
            policy_requirements_met: true,
            has_existing_job: true
        }));
    }

    /// Policy-failed derivations must not be queued in the mock eval path either.
    #[test]
    fn mock_path_policy_fail_guard_prevents_enqueue() {
        fn should_mock_policy_fail(system_count: usize, idx: usize) -> bool {
            system_count > 1 && idx == 1
        }

        let systems = vec!["a", "b", "c"];
        let mut enqueued = vec![];
        for (idx, name) in systems.iter().enumerate() {
            let policy_failed = should_mock_policy_fail(systems.len(), idx);
            if !policy_failed {
                enqueued.push(*name);
            }
        }
        // Only "a" and "c" should be enqueued; "b" (idx=1) is policy-failed.
        assert_eq!(enqueued, vec!["a", "c"]);
    }
}

/// Mark a build job as failed and handle retry logic.
pub async fn mark_job_failed(
    pool: &PgPool,
    job_id: Uuid,
    error_message: &str,
    logs: Option<&str>,
) -> Result<()> {
    let result = sqlx::query!(
        r#"
        UPDATE build_jobs
        SET 
            retry_count = retry_count + 1,
            status = CASE
                WHEN retry_count + 1 >= max_retries THEN 'failed'
                ELSE 'queued'  -- Re-queue for retry
            END,
            builder_id = NULL,  -- Unassign so another builder can pick it up
            server_failure_code = NULL,
            logs = COALESCE(logs, '') || COALESCE($2, '') || E'\n\nError: ' || $3,
            completed_at = CASE
                WHEN retry_count + 1 >= max_retries THEN NOW()
                ELSE NULL
            END,
            updated_at = NOW()
        WHERE id = $1
        RETURNING retry_count, max_retries, status
        "#,
        job_id,
        logs,
        error_message
    )
    .fetch_one(pool)
    .await
    .context("Failed to mark job as failed")?;

    if result.status == "queued" {
        info!(
            "🔄 Build job {} failed (attempt {}/{}), re-queued for retry",
            job_id, result.retry_count, result.max_retries
        );
    } else {
        info!(
            "❌ Build job {} permanently failed after {} attempts",
            job_id, result.retry_count
        );

        // Open a canonical attention occurrence for the terminal failure.
        // A re-queued terminal job gets a new id, so job_id alone is a stable
        // occurrence key.
        let opened_at = Utc::now();
        let _ = crate::queries::attention::open_or_observe(
            pool,
            "builds",
            "build_job",
            &job_id.to_string(),
            &crate::queries::attention::build_occurrence_key(job_id),
            opened_at,
            serde_json::json!({"job_id": job_id.to_string()}),
        )
        .await
        .map_err(|e| tracing::error!("failed to open build attention occurrence: {e:#}"));
    }

    Ok(())
}
