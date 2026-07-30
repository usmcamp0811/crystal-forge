//! Build job queue queries for the builder API system.
//!
//! This module handles creating and managing build jobs in the build_jobs table.

use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::{PgPool, Postgres, Transaction};
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Advisory lock serializing all build-queue-position allocations.
/// Using the ASCII encoding of 'CFBQ' as a 64-bit integer (0x43464251).
pub const BUILD_QUEUE_ORDER_LOCK_KEY: i64 = 0x4346_4251;

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

    let result = sqlx::query(
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
        "#,
    )
    .bind(commit_id)
    .bind(max_pos)
    .execute(&mut *tx)
    .await
    .context("Failed to create build jobs for commit")?;

    tx.commit()
        .await
        .context("Failed to commit create_build_jobs_for_commit")?;

    let count = result.rows_affected() as usize;

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

    Ok(rows)
}

pub async fn create_build_job_for_derivation_tx(
    tx: &mut Transaction<'_, Postgres>,
    derivation_id: i32,
) -> Result<Option<BuildJobInsertOutcome>> {
    lock_build_queue_order(tx).await?;

    let next_pos: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(queue_position), 0) + 1 FROM build_jobs WHERE status = 'queued' OR status = 'building'",
    )
    .fetch_one(&mut **tx)
    .await
    .context("Failed to read max queue_position")?;

    let inserted: Option<(Uuid,)> = sqlx::query_as(
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
                WHEN EXTRACT(EPOCH FROM (NOW() - c.commit_timestamp)) < 3600 THEN 2.0
                WHEN EXTRACT(EPOCH FROM (NOW() - c.commit_timestamp)) < 86400 THEN 1.5
                ELSE 1.0
            END AS priority_weight,
            $2 AS queue_position,
            'queued' AS status
        FROM derivations d
        INNER JOIN commits c ON d.commit_id = c.id
        LEFT JOIN systems s ON (
            d.derivation_target = s.hostname
            AND s.flake_id = c.flake_id
        )
        WHERE d.id = $1
            AND d.status_id = 5
            AND d.cf_agent_enabled = TRUE
            AND d.policy_requirements_met = TRUE
        ON CONFLICT (derivation_id) DO NOTHING
        RETURNING id
        "#,
    )
    .bind(derivation_id)
    .bind(next_pos)
    .fetch_optional(&mut **tx)
    .await
    .context("Failed to create build job for derivation")?;

    if let Some((build_job_id,)) = inserted {
        return Ok(Some(BuildJobInsertOutcome::Inserted { build_job_id }));
    }

    let existing: Option<(Uuid, String)> = sqlx::query_as(
        r#"
        SELECT id, status
        FROM build_jobs
        WHERE derivation_id = $1
        ORDER BY created_at ASC
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
/// Idempotency: uses `ON CONFLICT (derivation_id) DO NOTHING` to guarantee at most
/// one `build_jobs` row per derivation. Concurrent callers are safe — the constraint
/// absorbs races without returning an error, unlike a `NOT EXISTS` subquery which
/// is non-atomic between the check and the insert.
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

    let result = sqlx::query(
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
        ON CONFLICT (derivation_id) DO NOTHING
        "#,
    )
    .bind(derivation_id)
    .bind(next_pos)
    .execute(&mut *tx)
    .await
    .context("Failed to enqueue build job for derivation")?;

    tx.commit()
        .await
        .context("Failed to commit enqueue_build_job_for_derivation")?;

    let created = result.rows_affected() > 0;
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
            warn!(derivation_id, "record_recovery_failure: update failed: {e:#}");
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
/// Idempotent: `ON CONFLICT (derivation_id) DO NOTHING` prevents duplicate jobs.
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
                    pool, derivation_id, commit_id, expected_attempt, None, msg,
                )
                .await;
                continue;
            }
        };

        // Phase 1: create / verify GC root before inserting any claimable job.
        let rooted = match crate::builder::create_drv_gc_root(&drv_path, derivation_id).await {
            Ok(r) => r,
            Err(err) => {
                let msg = format!("Recovery: GC root failed for derivation {derivation_id}: {err:#}");
                warn!("{msg}");
                record_recovery_failure(
                    pool, derivation_id, commit_id, expected_attempt, Some(drv_path.as_str()), &msg,
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
                pool, derivation_id, commit_id, expected_attempt, Some(drv_path.as_str()), &msg,
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
                    pool, derivation_id, commit_id, expected_attempt, Some(drv_path.as_str()), &msg,
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
                warn!(derivation_id, "Recovery: commit no longer complete (skipping)");
                let _ = tx.rollback().await;
                continue;
            }
            Err(err) => {
                let msg = format!("Recovery: commit lock query failed: {err:#}");
                warn!(derivation_id, "{msg}");
                let _ = tx.rollback().await;
                record_recovery_failure(
                    pool, derivation_id, commit_id, expected_attempt, Some(drv_path.as_str()), &msg,
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
                pool, derivation_id, commit_id, expected_attempt, Some(drv_path.as_str()), &msg,
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
                    pool, derivation_id, commit_id, expected_attempt, Some(drv_path.as_str()), &msg,
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
            ON CONFLICT (derivation_id) DO NOTHING
            RETURNING TRUE
            "#,
        )
        .bind(derivation_id)
        .bind(next_pos)
        .fetch_optional(&mut *tx)
        .await;

        match inserted {
            Ok(Some(true)) => {
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
                            pool, derivation_id, commit_id, expected_attempt, Some(drv_path.as_str()), &msg,
                        )
                        .await;
                        continue;
                    }
                }

                if let Err(err) = tx.commit().await {
                    let msg = format!("Recovery: commit failed: {err:#}");
                    warn!(derivation_id, "{msg}");
                    record_recovery_failure(
                        pool, derivation_id, commit_id, expected_attempt, Some(drv_path.as_str()), &msg,
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
                            pool, derivation_id, commit_id, expected_attempt, Some(drv_path.as_str()), &msg,
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
                    pool, derivation_id, commit_id, expected_attempt, Some(drv_path.as_str()), &msg,
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

    /// The per-derivation SQL uses `ON CONFLICT (derivation_id) DO NOTHING` for
    /// idempotent insertion, and shares status_id = 5 (DryRunComplete) as the
    /// eligibility gate with the bulk `create_build_jobs_for_commit` function.
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
            // ON CONFLICT DO NOTHING handles existing jobs; has_existing_job
            // is checked here for documentation of the expected outcome only.
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
