use crate::api::models::{
    CancelEvalOutcome, EvalHistoryItem, EvalHistoryPage, EvalHistoryParams, EvalQueueParams,
};
use crate::models::commits::Commit;
use crate::models::flakes::Flake;
use crate::models::retry_policy::{
    AutomaticRetryPolicy, RetryFailureClass, automatic_retry_budget_remaining,
    automatic_retry_eligible,
};
use crate::queries::attention;
use anyhow::{Context, Result, bail};
use sqlx::{PgPool, Row};
use std::collections::{BTreeSet, HashSet};
use tracing::{debug, error, info, warn};

const EVAL_QUEUE_ADVISORY_LOCK_KEY: i64 = 1_600_001;

pub async fn insert_commit(
    pool: &PgPool,
    commit_hash: &str,
    repo_url: &str,
    commit_timestamp: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    insert_commit_with_metadata(pool, commit_hash, repo_url, commit_timestamp, None, None).await?;
    Ok(())
}

/// Insert a commit or return 0 if already present (`ON CONFLICT DO NOTHING`).
///
/// Returns `Ok(1)` when the commit was newly inserted, `Ok(0)` when it already
/// existed.  This lets callers count actual insertions even under concurrent
/// sync races where the pre-filter misses a duplicate.
pub async fn insert_commit_with_metadata(
    pool: &PgPool,
    commit_hash: &str,
    repo_url: &str,
    commit_timestamp: chrono::DateTime<chrono::Utc>,
    message: Option<&str>,
    author: Option<&str>,
) -> Result<u64> {
    let flake_id: (i32,) = sqlx::query_as("SELECT id FROM flakes WHERE repo_url = $1")
        .bind(repo_url)
        .fetch_optional(pool)
        .await?
        .context("No flake entry found")?;

    let result = sqlx::query_scalar::<_, i32>(
        r#"
        WITH queue_lock AS (
            SELECT pg_advisory_xact_lock($6)
        ),
        next_position AS (
            SELECT COALESCE(MAX(eval_queue_position), 0) + 1 AS position
            FROM commits
            WHERE COALESCE(evaluation_status, 'pending') IN ('pending', 'in_progress', 'cancelling')
        )
        INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, message, author, eval_queue_position)
        SELECT $1, $2, $3, $4, $5, position
        FROM next_position, queue_lock
        ON CONFLICT DO NOTHING
        RETURNING 1
        "#,
    )
    .bind(flake_id.0)
    .bind(commit_hash)
    .bind(commit_timestamp)
    .bind(message)
    .bind(author)
    .bind(EVAL_QUEUE_ADVISORY_LOCK_KEY)
    .fetch_optional(pool)
    .await?;

    Ok(if result.is_some() { 1 } else { 0 })
}

/// Advisory lock key space for per-flake sync/reset serialization.
/// The key is `SYNC_LOCK_BASE + flake_id`.
pub const SYNC_LOCK_BASE: i64 = 1_700_000;

/// Insert a commit by flake_id inside an open transaction.
///
/// Uses `flake_id` directly (no `repo_url` lookup), so the caller can verify
/// source identity before calling this function.  Returns 1 if inserted, 0 if
/// `ON CONFLICT DO NOTHING` fired.
pub async fn insert_commit_by_flake_id_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    flake_id: i32,
    commit_hash: &str,
    commit_timestamp: chrono::DateTime<chrono::Utc>,
    message: Option<&str>,
    author: Option<&str>,
) -> Result<u64> {
    let result = sqlx::query_scalar::<_, i32>(
        r#"
        WITH queue_lock AS (
            SELECT pg_advisory_xact_lock($6)
        ),
        next_position AS (
            SELECT COALESCE(MAX(eval_queue_position), 0) + 1 AS position
            FROM commits
            WHERE COALESCE(evaluation_status, 'pending') IN ('pending', 'in_progress', 'cancelling')
        )
        INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, message, author, eval_queue_position)
        SELECT $1, $2, $3, $4, $5, position
        FROM next_position, queue_lock
        ON CONFLICT DO NOTHING
        RETURNING 1
        "#,
    )
    .bind(flake_id)
    .bind(commit_hash)
    .bind(commit_timestamp)
    .bind(message)
    .bind(author)
    .bind(EVAL_QUEUE_ADVISORY_LOCK_KEY)
    .fetch_optional(&mut **tx)
    .await?;

    Ok(if result.is_some() { 1 } else { 0 })
}

/// Persists the full Git first-parent identity found during an authoritative sync.
///
/// A `None` parent identifies a root commit. The sync transaction updates both
/// new and existing rows so commits first learned from webhooks gain ancestry
/// metadata without requiring a duplicate evaluation.
///
/// # Errors
///
/// Returns an error when the commit is absent or PostgreSQL cannot persist the
/// parent identity in the caller's transaction.
pub async fn set_commit_first_parent_by_flake_id_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    flake_id: i32,
    commit_hash: &str,
    first_parent_sha: Option<&str>,
) -> Result<()> {
    let updated = sqlx::query(
        "UPDATE commits
         SET first_parent_sha = $3, first_parent_resolved = true, source_archived = false
         WHERE flake_id = $1 AND git_commit_hash = $2",
    )
    .bind(flake_id)
    .bind(commit_hash)
    .bind(first_parent_sha)
    .execute(&mut **tx)
    .await?;
    if updated.rows_affected() != 1 {
        bail!("commit {commit_hash} was not present for flake {flake_id}");
    }
    Ok(())
}

/// Persists first-parent identity for a commit inserted outside a sync transaction.
///
/// # Errors
///
/// Returns an error when the commit is absent or PostgreSQL cannot persist the
/// parent identity.
pub async fn set_commit_first_parent_by_repo_url(
    pool: &PgPool,
    repo_url: &str,
    commit_hash: &str,
    first_parent_sha: Option<&str>,
) -> Result<()> {
    let updated = sqlx::query(
        r#"
        UPDATE commits c
        SET first_parent_sha = $3, first_parent_resolved = true, source_archived = false
        FROM flakes f
        WHERE f.id = c.flake_id AND f.repo_url = $1 AND c.git_commit_hash = $2
        "#,
    )
    .bind(repo_url)
    .bind(commit_hash)
    .bind(first_parent_sha)
    .execute(pool)
    .await?;
    if updated.rows_affected() != 1 {
        bail!("commit {commit_hash} was not present for repository {repo_url}");
    }
    Ok(())
}

pub async fn get_commit_by_hash(pool: &PgPool, commit_hash: &str) -> Result<Commit> {
    let commit = sqlx::query_as::<_, Commit>("SELECT * FROM commits WHERE git_commit_hash = $1")
        .bind(commit_hash)
        .fetch_one(pool)
        .await?;
    Ok(commit)
}

pub async fn get_commit_by_id(pool: &PgPool, id: i32) -> Result<Commit> {
    let commit = sqlx::query_as::<_, Commit>("SELECT * FROM commits WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok(commit)
}

pub async fn get_commits_pending_evaluation(pool: &PgPool) -> Result<Vec<Commit>> {
    // NOTE: We no longer check for d.commit_id IS NULL because partial evaluations
    // (where some derivations exist but eval crashed mid-way) would get stuck.
    // The evaluation_status = 'pending' is the authoritative check.
    let rows = sqlx::query_as::<_, Commit>(
        r#"
        SELECT c.id, c.flake_id, c.git_commit_hash, c.commit_timestamp, c.attempt_count,
               ea.id AS evaluation_attempt_id,
               ea.attempt_number AS evaluation_attempt_number,
               ea.parent_attempt_id AS evaluation_parent_attempt_id,
               ea.root_attempt_id AS evaluation_root_attempt_id,
               ea.available_at AS evaluation_available_at
        FROM commits c
        JOIN evaluation_attempts ea ON ea.commit_id = c.id AND ea.status = 'queued'
        WHERE c.evaluation_status = 'pending'
        AND c.source_archived = false
        AND ea.available_at <= NOW()
        ORDER BY
            COALESCE(c.eval_queue_position, 0) DESC,
            c.commit_timestamp DESC,
            c.id DESC
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn next_evaluation_available_at(
    pool: &PgPool,
) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
    sqlx::query_scalar(
        r#"
        SELECT MIN(ea.available_at)
        FROM evaluation_attempts ea
        JOIN commits c ON c.id = ea.commit_id
         WHERE ea.status = 'queued'
           AND COALESCE(c.evaluation_status, 'pending') = 'pending'
           AND c.source_archived = false
        "#,
    )
    .fetch_one(pool)
    .await
    .context("Failed to load next evaluation due time")
}

pub async fn increment_commit_list_attempt_count(pool: &PgPool, commit: &Commit) -> Result<()> {
    let _updated = sqlx::query!(
        r#"
        UPDATE commits
        SET attempt_count = attempt_count + 1
        WHERE id = $1
        "#,
        commit.id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Check if a flake already has commits in the database
pub async fn flake_has_commits(pool: &PgPool, repo_url: &str) -> Result<bool> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM commits c 
         JOIN flakes f ON c.flake_id = f.id 
         WHERE f.repo_url = $1 AND c.source_archived = false",
    )
    .bind(repo_url)
    .fetch_one(pool)
    .await?;
    Ok(count.0 > 0)
}

pub async fn flake_last_commit(pool: &PgPool, repo_url: &str) -> Result<Commit> {
    let commit = sqlx::query_as::<_, Commit>(
        "SELECT * FROM COMMITS c 
         JOIN flakes f ON c.flake_id = f.id 
         WHERE repo_url = $1 AND c.source_archived = false
         ORDER BY commit_timestamp DESC 
         LIMIT 1;",
    )
    .bind(repo_url)
    .fetch_one(pool)
    .await?;
    Ok(commit)
}

pub async fn get_commit_distance_from_head(
    pool: &PgPool,
    flake: &Flake,
    commit: &Commit,
) -> Result<i32> {
    // Get the latest commit for this flake
    let latest_commit = sqlx::query_as::<_, (i32, String)>(
        r#"
        SELECT id, git_commit_hash
        FROM commits
         WHERE flake_id = $1 AND source_archived = false
        ORDER BY commit_timestamp DESC
        LIMIT 1
        "#,
    )
    .bind(flake.id)
    .fetch_one(pool)
    .await?;

    // If this is the latest commit, distance is 0
    if latest_commit.0 == commit.id {
        return Ok(0);
    }

    // Count commits between this one and HEAD
    let distance = sqlx::query_scalar::<_, i32>(
        r#"
        SELECT COUNT(*)::int as "count!"
        FROM commits
         WHERE flake_id = $1
         AND commit_timestamp > $2
         AND source_archived = false
        "#,
    )
    .bind(flake.id)
    .bind(commit.commit_timestamp)
    .fetch_one(pool)
    .await?;

    Ok(distance)
}

/// Recover orphaned evaluations after a server restart.
///
/// NOTE: `cancelled` rows are intentionally left alone — they represent
/// evaluations the user explicitly cancelled and should not be re-queued.
pub async fn reset_stuck_commit_evaluations(pool: &PgPool) -> Result<()> {
    let mut tx = pool.begin().await?;
    let cancelled = sqlx::query!(
        r#"
        UPDATE commits
        SET
            evaluation_status = 'cancelled',
            evaluation_completed_at = COALESCE(evaluation_completed_at, NOW()),
            cancellation_requested = FALSE
        WHERE evaluation_status = 'cancelling'
        RETURNING id, git_commit_hash
        "#
    )
    .fetch_all(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        UPDATE evaluation_attempts
        SET status = 'cancelled', completed_at = COALESCE(completed_at, NOW()), updated_at = NOW()
        WHERE status = 'in_progress'
          AND commit_id = ANY($1)
        "#,
    )
    .bind(cancelled.iter().map(|row| row.id).collect::<Vec<_>>())
    .execute(&mut *tx)
    .await?;

    let reset = sqlx::query!(
        r#"
        UPDATE commits
        SET evaluation_status = 'pending', evaluation_started_at = NULL
        WHERE evaluation_status = 'in_progress'
        RETURNING id, git_commit_hash
        "#
    )
    .fetch_all(&mut *tx)
    .await?;

    sqlx::query(
        "UPDATE evaluation_attempts SET status = 'queued', started_at = NULL, available_at = NOW(), updated_at = NOW() WHERE status = 'in_progress' AND commit_id = ANY($1)",
    )
    .bind(reset.iter().map(|row| row.id).collect::<Vec<_>>())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    if !reset.is_empty() {
        warn!(
            "🧹 Reset {} orphaned in-progress commit evaluations on startup",
            reset.len()
        );
        for row in &reset {
            info!("  - Commit {} ({})", row.id, row.git_commit_hash);
        }
    }
    if !cancelled.is_empty() {
        info!(
            "🚫 Finalized {} cancelling evaluations as cancelled on startup",
            cancelled.len()
        );
    }

    Ok(())
}

/// Outcome of attempting to start a commit evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalStartOutcome {
    /// Evaluation started successfully; includes the current attempt number.
    Started { attempt: i32 },
    /// The commit is no longer pending (already started, cancelled, or
    /// completed by another worker).
    NoLongerPending,
}

/// Atomically claim a pending commit for evaluation.
///
/// Uses a compare-and-set pattern: only transitions `pending` → `in_progress`.
/// Returns `Started` with the attempt count when the claim succeeds, or
/// `NoLongerPending` when the commit is no longer in a startable state.
/// This prevents resurrecting a cancelled evaluation (Race C in the review).
pub async fn mark_commit_evaluation_started(
    pool: &PgPool,
    commit_id: i32,
) -> Result<EvalStartOutcome> {
    let mut tx = pool.begin().await?;
    let attempt = sqlx::query_as::<_, (uuid::Uuid, i32)>(
        r#"
        WITH next_attempt AS (
            SELECT id
            FROM evaluation_attempts
            WHERE commit_id = $1 AND status = 'queued' AND available_at <= NOW()
            ORDER BY attempt_number ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE evaluation_attempts ea
        SET status = 'in_progress', started_at = NOW(), updated_at = NOW()
        FROM next_attempt
        WHERE ea.id = next_attempt.id
        RETURNING ea.id, ea.attempt_number
        "#,
    )
    .bind(commit_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((attempt_id, attempt)) = attempt else {
        tx.rollback().await?;
        return Ok(EvalStartOutcome::NoLongerPending);
    };

    let started = sqlx::query(
        r#"
        UPDATE commits
        SET 
            evaluation_status = 'in_progress',
            evaluation_started_at = NOW(),
            evaluation_completed_at = NULL,
            evaluation_error_message = NULL,
            evaluation_attempt_count = $2,
            cancellation_requested = FALSE
        WHERE id = $1
          AND COALESCE(evaluation_status, 'pending') = 'pending'
        "#,
    )
    .bind(commit_id)
    .bind(attempt)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        // Check if this is a unique constraint violation (another commit is in_progress)
        if let sqlx::Error::Database(ref db_err) = e {
            if db_err.code().as_deref() == Some("23505") {
                return anyhow::anyhow!(
                    "Cannot start evaluation for commit {}: another commit is already being evaluated",
                    commit_id
                );
            }
        }
        anyhow::anyhow!("Failed to mark commit {} as in_progress: {}", commit_id, e)
    })?;
    if started.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(EvalStartOutcome::NoLongerPending);
    }

    crate::services::composite_enforcement::reset_eval_passed_assessments_for_started_attempt_in_tx(
        &mut tx, commit_id, attempt_id, attempt,
    )
    .await?;

    tx.commit().await?;
    Ok(EvalStartOutcome::Started { attempt })
}

/// Outcome of attempting to mark a commit evaluation as complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalCompleteOutcome {
    /// The evaluation was successfully marked complete.
    Completed,
    /// The row could not be updated — the evaluation may have been
    /// cancelled or superseded by another transition.
    SupersededOrCancelled,
}

/// Atomically mark a commit evaluation as complete (compare-and-set).
///
/// Only transitions `in_progress` → `complete` when cancellation has not
/// been requested and the attempt count matches.  Returns
/// `SupersededOrCancelled` if the row was already in a different state
/// (e.g. cancelled), preventing a completion broadcast from overwriting
/// a concurrent cancellation (Race B).
pub async fn mark_commit_evaluation_complete(
    pool: &PgPool,
    commit_id: i32,
    expected_attempt: i32,
) -> Result<EvalCompleteOutcome> {
    let mut tx = pool.begin().await?;
    let attempt_rows = sqlx::query(
        r#"
        UPDATE evaluation_attempts
        SET status = 'complete', completed_at = NOW(), error_message = NULL, updated_at = NOW()
        WHERE commit_id = $1 AND attempt_number = $2 AND status = 'in_progress'
        "#,
    )
    .bind(commit_id)
    .bind(expected_attempt)
    .execute(&mut *tx)
    .await?;
    if attempt_rows.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(EvalCompleteOutcome::SupersededOrCancelled);
    }
    let result = sqlx::query(
        r#"
        UPDATE commits
        SET
            evaluation_status = 'complete',
            evaluation_completed_at = NOW(),
            evaluation_error_message = NULL,
            cancellation_requested = FALSE
        WHERE id = $1
          AND evaluation_status = 'in_progress'
          AND COALESCE(cancellation_requested, FALSE) = FALSE
          AND evaluation_attempt_count = $2
        "#,
    )
    .bind(commit_id)
    .bind(expected_attempt)
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(EvalCompleteOutcome::SupersededOrCancelled);
    }

    tx.commit().await?;
    resolve_eval_attention_unless_failed(pool, commit_id).await;
    Ok(EvalCompleteOutcome::Completed)
}

/// Resolve the evaluation-failure attention occurrence for a commit, but
/// only if the commit's CURRENT `evaluation_status` is not `'failed'`.
///
/// The domain status update (complete/reset) and this attention action are
/// two separate best-effort operations, so a delay between them can leave a
/// window in which a NEWER failure is recorded. Without this recheck, a
/// delayed completion or reset resolve could wipe out that newer failure's
/// still-valid attention occurrence. Acquires the same per-subject advisory
/// lock used by [`open_eval_attention_if_current`], so the recheck-then-act
/// sequence is atomic with respect to any concurrent attention transition
/// for this commit.
async fn resolve_eval_attention_unless_failed(pool: &PgPool, commit_id: i32) {
    let subject_id = commit_id.to_string();

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::warn!("failed to begin eval attention resolve transaction: {e:#}");
            return;
        }
    };

    let lock_key = format!("attention_occurrence:evals:{subject_id}");
    if let Err(e) = sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&lock_key)
        .execute(&mut *tx)
        .await
    {
        tracing::warn!("failed to acquire eval attention lock: {e:#}");
        let _ = tx.rollback().await;
        return;
    }

    let current_status: Option<String> =
        match sqlx::query_scalar("SELECT evaluation_status FROM commits WHERE id = $1")
            .bind(commit_id)
            .fetch_optional(&mut *tx)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("failed to recheck commit status before resolving attention: {e:#}");
                let _ = tx.rollback().await;
                return;
            }
        };

    if current_status.as_deref() == Some("failed") {
        // A newer failure has since been recorded on this commit; do not
        // resolve its still-valid attention occurrence.
        let _ = tx.commit().await;
        return;
    }

    if let Err(e) = sqlx::query(
        "UPDATE attention_occurrences SET resolved_at = NOW() \
         WHERE category = 'evals' AND subject_id = $1 AND resolved_at IS NULL",
    )
    .bind(&subject_id)
    .execute(&mut *tx)
    .await
    {
        tracing::warn!("failed to resolve evaluation attention occurrence: {e:#}");
        let _ = tx.rollback().await;
        return;
    }

    if let Err(e) = tx.commit().await {
        tracing::warn!("failed to commit eval attention resolve: {e:#}");
    }
}

/// Outcome of attempting to mark a commit evaluation as failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalFailureOutcome {
    /// The evaluation was marked for retry (will become `pending`).
    RetryScheduled,
    /// The evaluation is permanently failed (attempt limit reached).
    PermanentlyFailed,
    /// The evaluation state was superseded or cancelled — no failure
    /// metadata or broadcast should be emitted.
    SupersededOrCancelled,
}

/// Mark commit evaluation as failed (with retry logic)
///
/// Atomically transitions `in_progress` → `pending`/`failed` only when
/// cancellation has not been requested and the attempt count matches.
/// Returns `SupersededOrCancelled` if the row is not in `in_progress` or
/// cancellation was requested, preventing the generic failure handler
/// from overwriting a concurrent cancellation.
///
/// Retries up to 3 times with exponential backoff:
/// - Attempt 1: immediate
/// - Attempt 2: after 1 minute
/// - Attempt 3: after 5 minutes (from attempt 2)
///
/// After 3 failed attempts, marks as permanently 'failed'.
/// Manual re-evaluation can be triggered via API (resets attempt count).
/// Terminally fail the active evaluation attempt and schedule at most one child.
pub async fn mark_commit_evaluation_failed(
    pool: &PgPool,
    commit_id: i32,
    error: &str,
    expected_attempt: i32,
    failure_class: RetryFailureClass,
) -> Result<EvalFailureOutcome> {
    // SECURITY: Commit and attempt errors are API-visible and persisted. Raw
    // evaluator diagnostics must not cross this boundary.
    let error = crate::security::snapshot_redaction::redact_evaluation_error(error);
    let mut tx = pool.begin().await?;
    // CONCURRENCY: Terminal failure publishes failed artifacts and can take
    // attempt, POA&M, commit, derivation, system, and deployment locks later.
    crate::queries::evaluation_snapshots::lock_snapshot_writer_tx(&mut tx).await?;
    #[derive(sqlx::FromRow)]
    struct FailedAttempt {
        id: uuid::Uuid,
        root_attempt_id: Option<uuid::Uuid>,
        attempt_number: i32,
        automatic_retry_count: i32,
    }

    let class_name = match failure_class {
        RetryFailureClass::Transient => "transient",
        RetryFailureClass::Deterministic | RetryFailureClass::DerivationMismatch => "deterministic",
        RetryFailureClass::Cancelled => "cancelled",
        RetryFailureClass::Authorization => "authorization",
        RetryFailureClass::Unknown => "unknown",
    };
    let failed = sqlx::query_as::<_, FailedAttempt>(
        r#"
        UPDATE evaluation_attempts
        SET status = 'failed', completed_at = NOW(), error_message = $2,
            failure_class = $3, updated_at = NOW()
        WHERE commit_id = $1 AND status = 'in_progress'
          AND attempt_number = $4
        RETURNING id, root_attempt_id, attempt_number, automatic_retry_count
        "#,
    )
    .bind(commit_id)
    .bind(&error)
    .bind(class_name)
    .bind(expected_attempt)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(failed) = failed else {
        tx.rollback().await?;
        return Ok(EvalFailureOutcome::SupersededOrCancelled);
    };

    crate::services::composite_enforcement::fail_eval_passed_attempt_in_tx(
        &mut tx, failed.id, &error, class_name,
    )
    .await?;

    let policy = sqlx::query_as::<_, AutomaticRetryPolicy>(
        "SELECT max_build_retries, max_evaluation_retries, backoff_seconds, transient_only FROM automatic_retry_policy WHERE id = 1",
    )
    .fetch_optional(&mut *tx)
    .await?
    .unwrap_or_default();

    let retry_scheduled = automatic_retry_budget_remaining(
        failed.automatic_retry_count,
        i32::from(policy.max_evaluation_retries),
    ) && automatic_retry_eligible(policy.transient_only, failure_class);
    if retry_scheduled {
        // Move this commit to the front of the eval queue (LIFO) so that
        // retried evaluations are picked up promptly rather than waiting
        // behind every newer commit that was discovered in the meantime.
        sqlx::query(
            r#"
            INSERT INTO evaluation_attempts (
                commit_id, parent_attempt_id, root_attempt_id, automatic_retry_source_id,
                attempt_number, automatic_retry_count, available_at
            )
            VALUES ($1, $2, $3, $2, $4, $5, NOW() + make_interval(secs => $6))
            ON CONFLICT (automatic_retry_source_id)
                WHERE automatic_retry_source_id IS NOT NULL DO NOTHING
            "#,
        )
        .bind(commit_id)
        .bind(failed.id)
        .bind(failed.root_attempt_id.unwrap_or(failed.id))
        .bind(failed.attempt_number + 1)
        .bind(failed.automatic_retry_count + 1)
        .bind(policy.backoff_seconds)
        .execute(&mut *tx)
        .await?;

        // Bump eval_queue_position to front under the advisory lock.
        sqlx::query(
            r#"
            WITH queue_lock AS (
                SELECT pg_advisory_xact_lock($2)
            ),
            next_position AS (
                SELECT COALESCE(MAX(eval_queue_position), 0) + 1 AS position
                FROM commits
                WHERE COALESCE(evaluation_status, 'pending')
                    IN ('pending', 'in_progress', 'cancelling')
            )
            UPDATE commits
            SET eval_queue_position = next_position.position
            FROM queue_lock, next_position
            WHERE id = $1
            "#,
        )
        .bind(commit_id)
        .bind(EVAL_QUEUE_ADVISORY_LOCK_KEY)
        .execute(&mut *tx)
        .await?;
        crate::queries::evaluation_snapshots::recompute_host_deltas_tx(&mut tx, commit_id).await?;
    }

    let row = sqlx::query(
        r#"
        UPDATE commits
        SET evaluation_status = $2,
            evaluation_completed_at = CASE WHEN $2 = 'failed' THEN NOW() ELSE NULL END,
            evaluation_error_message = $3
         WHERE id = $1 AND evaluation_status = 'in_progress'
           AND COALESCE(cancellation_requested, FALSE) = FALSE
           AND evaluation_attempt_count = $4
        RETURNING evaluation_completed_at
        "#,
    )
    .bind(commit_id)
    .bind(if retry_scheduled { "pending" } else { "failed" })
    .bind(&error)
    .bind(expected_attempt)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        tx.rollback().await?;
        return Ok(EvalFailureOutcome::SupersededOrCancelled);
    };

    if !retry_scheduled {
        // PERSISTENCE: A terminal commit failure records an explicit lifecycle
        // for each known configuration instead of collapsing every Config read
        // into one commit-global error.
        let configuration_names = sqlx::query_scalar::<_, String>(
            r#"
            WITH configuration_names AS (
                SELECT DISTINCT unnest(cac.nixos_configurations) AS name
                FROM commit_artifacts_cache cac WHERE cac.commit_id = $1
                UNION
                SELECT DISTINCT d.derivation_name
                FROM derivations d
                WHERE d.commit_id = $1 AND d.derivation_type = 'nixos'
            )
            SELECT name FROM configuration_names WHERE btrim(name) <> ''
            "#,
        )
        .bind(commit_id)
        .fetch_all(&mut *tx)
        .await?;
        for configuration_name in configuration_names {
            crate::queries::evaluation_snapshots::persist_failed_snapshot_deferred_tx(
                &mut tx,
                commit_id,
                &configuration_name,
                &error,
            )
            .await?;
        }
        crate::queries::evaluation_snapshots::recompute_host_deltas_tx(&mut tx, commit_id).await?;
    }

    let completed_at: Option<chrono::DateTime<chrono::Utc>> =
        row.try_get("evaluation_completed_at")?;
    tx.commit().await?;

    if let Some(completed_at) = completed_at {
        open_eval_attention_if_current(pool, commit_id, completed_at).await;
    }

    if retry_scheduled {
        Ok(EvalFailureOutcome::RetryScheduled)
    } else {
        Ok(EvalFailureOutcome::PermanentlyFailed)
    }
}

/// Open (or observe) the evaluation-failure attention occurrence for a
/// commit, but only if this failure is still the current evaluation
/// outcome recorded on the commit (`evaluation_status = 'failed'` AND
/// `evaluation_completed_at` matches `completed_at`).
///
/// The domain status update and this attention action are two separate
/// best-effort operations, so a delay between them (e.g. this async call is
/// scheduled late) leaves a window in which a manual reset or a
/// re-evaluation can change the commit's state. Without this recheck, a
/// delayed handler would open a stale failure occurrence for a commit that
/// is no longer failed. Acquires the same per-subject advisory lock used by
/// [`resolve_eval_attention_unless_failed`], so the recheck-then-act
/// sequence is atomic with respect to any concurrent attention transition
/// for this commit.
async fn open_eval_attention_if_current(
    pool: &PgPool,
    commit_id: i32,
    completed_at: chrono::DateTime<chrono::Utc>,
) {
    let subject_id = commit_id.to_string();

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::warn!("failed to begin eval attention open transaction: {e:#}");
            return;
        }
    };

    let lock_key = format!("attention_occurrence:evals:{subject_id}");
    if let Err(e) = sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&lock_key)
        .execute(&mut *tx)
        .await
    {
        tracing::warn!("failed to acquire eval attention lock: {e:#}");
        let _ = tx.rollback().await;
        return;
    }

    let still_current: bool = match sqlx::query_scalar(
        "SELECT evaluation_status = 'failed' AND evaluation_completed_at = $2 FROM commits WHERE id = $1",
    )
    .bind(commit_id)
    .bind(completed_at)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(Some(v)) => v,
        Ok(None) => false,
        Err(e) => {
            tracing::warn!("failed to recheck commit status before opening attention: {e:#}");
            let _ = tx.rollback().await;
            return;
        }
    };

    if !still_current {
        // Superseded by a reset or re-evaluation — commit the (no-op)
        // transaction to release the lock.
        let _ = tx.commit().await;
        return;
    }

    let key = attention::eval_occurrence_key(commit_id, completed_at);
    if let Err(e) = sqlx::query(
        r#"
        INSERT INTO attention_occurrences (
            category, subject_type, subject_id, source_occurrence_key,
            opened_at, last_observed_at, metadata
        )
        VALUES ('evals', 'commit_eval', $1, $2, $3, $3, $4)
        ON CONFLICT (category, source_occurrence_key) DO UPDATE
        SET last_observed_at = GREATEST(attention_occurrences.last_observed_at, EXCLUDED.last_observed_at)
        WHERE attention_occurrences.resolved_at IS NULL
        "#,
    )
    .bind(&subject_id)
    .bind(&key)
    .bind(completed_at)
    .bind(serde_json::json!({"commit_id": commit_id}))
    .execute(&mut *tx)
    .await
    {
        tracing::warn!("failed to open evaluation attention occurrence: {e:#}");
        let _ = tx.rollback().await;
        return;
    }

    if let Err(e) = tx.commit().await {
        tracing::warn!("failed to commit eval attention open: {e:#}");
    }
}

/// Reset commit evaluation status to allow manual retry
///
/// This resets:
/// - evaluation_status → 'pending'
/// - evaluation_attempt_count → 0
/// - evaluation_error_message → NULL
/// - cancellation_requested → FALSE (so stale finalizer cannot cancel the reset evaluation)
/// - stale active attempt rows on terminal commits → `cancelled`
///
/// Use this for manual re-evaluation after fixing issues.
pub async fn reset_commit_evaluation(pool: &PgPool, commit_id: i32) -> Result<()> {
    #[derive(sqlx::FromRow)]
    struct ResetResult {
        id: i32,
        git_commit_hash: String,
    }

    let mut tx = pool.begin().await?;
    // CONCURRENCY: A worker can fail the commit after it claims an attempt but
    // before it marks that attempt terminal. Retire only these orphaned rows.
    // An active attempt on a non-terminal commit remains authoritative.
    sqlx::query(
        r#"
        UPDATE evaluation_attempts attempt
        SET status = 'cancelled',
            completed_at = COALESCE(completed_at, NOW()),
            error_message = COALESCE(error_message, 'Superseded by manual re-evaluation'),
            failure_class = COALESCE(failure_class, 'cancelled'),
            updated_at = NOW()
        FROM commits commit_row
        WHERE attempt.commit_id = commit_row.id
          AND commit_row.id = $1
          AND commit_row.evaluation_status IN ('complete', 'failed', 'cancelled')
          AND attempt.status IN ('queued', 'in_progress')
        "#,
    )
    .bind(commit_id)
    .execute(&mut *tx)
    .await?;
    let result = sqlx::query_as::<_, ResetResult>(
        r#"
        UPDATE commits
        SET 
            evaluation_status = 'pending',
            evaluation_attempt_count = 0,
            evaluation_started_at = NULL,
            evaluation_completed_at = NULL,
            evaluation_error_message = NULL,
            cancellation_requested = FALSE
        WHERE id = $1
        RETURNING id, git_commit_hash
        "#,
    )
    .bind(commit_id)
    .fetch_one(&mut *tx)
    .await?;

    // Insert a fresh evaluation attempt.
    sqlx::query(
        r#"
        WITH source AS (
            SELECT id, COALESCE(root_attempt_id, id) AS root_attempt_id, attempt_number
            FROM evaluation_attempts
            WHERE commit_id = $1
            ORDER BY attempt_number DESC, created_at DESC
            LIMIT 1
        )
        INSERT INTO evaluation_attempts (
            commit_id, parent_attempt_id, root_attempt_id, attempt_number,
            automatic_retry_count, available_at
        )
        SELECT $1, id, root_attempt_id, attempt_number + 1, 0, NOW()
        FROM source
        "#,
    )
    .bind(commit_id)
    .execute(&mut *tx)
    .await?;

    // Bump eval_queue_position to front (LIFO) under the advisory lock.
    sqlx::query(
        r#"
        WITH queue_lock AS (
            SELECT pg_advisory_xact_lock($2)
        ),
        next_position AS (
            SELECT COALESCE(MAX(eval_queue_position), 0) + 1 AS position
            FROM commits
            WHERE COALESCE(evaluation_status, 'pending')
                IN ('pending', 'in_progress', 'cancelling')
        )
        UPDATE commits
        SET eval_queue_position = next_position.position
        FROM queue_lock, next_position
        WHERE id = $1
        "#,
    )
    .bind(commit_id)
    .bind(EVAL_QUEUE_ADVISORY_LOCK_KEY)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    info!(
        "🔄 Reset evaluation for commit {} ({})",
        result.id, result.git_commit_hash
    );

    resolve_eval_attention_unless_failed(pool, commit_id).await;

    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EvalQueueRow {
    pub commit_id: i32,
    pub flake_id: i32,
    pub flake_name: String,
    pub branch: String,
    pub commit_hash: String,
    pub commit_message: Option<String>,
    pub author: Option<String>,
    pub committed_at: chrono::DateTime<chrono::Utc>,
    pub enqueued_at: chrono::DateTime<chrono::Utc>,
    pub is_latest_per_flake: bool,
    pub evaluation_status: String,
    pub queue_position: i64,
    pub systems: Vec<String>,
    pub system_count: i64,
    pub passed_count: i64,
    pub policy_failed_count: i64,
    pub eval_failed_count: i64,
    pub attempt_number: i32,
    pub parent_attempt_id: Option<uuid::Uuid>,
    pub root_attempt_id: Option<uuid::Uuid>,
    pub available_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub struct EvalQueueResult {
    pub rows: Vec<EvalQueueRow>,
    pub active_count: i64,
    pub completed_count: i64,
    pub successful_count: i64,
    pub failed_count: i64,
    pub domain_total: i64,
    pub filtered_total: i64,
}

pub async fn list_eval_queue(pool: &PgPool, params: &EvalQueueParams) -> Result<EvalQueueResult> {
    list_eval_queue_for_user(pool, params, None).await
}

pub async fn list_eval_queue_for_user(
    pool: &PgPool,
    params: &EvalQueueParams,
    user_id: Option<uuid::Uuid>,
) -> Result<EvalQueueResult> {
    let limit = params.limit.max(1).min(crate::api::models::LIMIT_MAX);
    let status_filter: Vec<String> = params
        .status
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|status| !status.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    let counts: (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        r#"
        WITH domain AS (
            SELECT
                c.id,
                c.flake_id,
                f.name AS flake_name,
                COALESCE(f.branch, 'main') AS branch,
                c.git_commit_hash,
                c.message,
                c.author,
                COALESCE(c.evaluation_status, 'pending') AS evaluation_status,
                COALESCE(cac.nixos_configurations, ARRAY[]::text[]) AS systems,
                ROW_NUMBER() OVER (
                    PARTITION BY c.flake_id,
                        COALESCE(c.evaluation_status, 'pending') IN ('pending', 'in_progress', 'cancelling')
                    ORDER BY c.evaluation_enqueued_at DESC, c.id DESC
                ) AS latest_rank
            FROM commits c
            JOIN flakes f ON f.id = c.flake_id
            LEFT JOIN commit_artifacts_cache cac ON cac.commit_id = c.id
            WHERE COALESCE(c.evaluation_status, 'pending') IN ('pending', 'in_progress', 'cancelling', 'complete', 'failed', 'cancelled')
              AND c.source_archived = false
              AND ($5::uuid IS NULL OR EXISTS (
                SELECT 1 FROM systems s JOIN user_environment_memberships uem ON uem.environment_id = s.environment_id
                WHERE s.flake_id = c.flake_id AND uem.user_id = $5
              ))
        ), filtered AS (
            SELECT * FROM domain
            WHERE ($1::text[] IS NULL OR cardinality($1::text[]) = 0 OR evaluation_status = ANY($1::text[]))
              AND ($2::text IS NULL OR flake_name ILIKE ('%' || $2 || '%'))
              AND ($3::text IS NULL OR flake_name ILIKE ('%' || $3 || '%') OR branch ILIKE ('%' || $3 || '%')
                   OR git_commit_hash ILIKE ('%' || $3 || '%') OR COALESCE(message, '') ILIKE ('%' || $3 || '%')
                   OR COALESCE(author, '') ILIKE ('%' || $3 || '%') OR evaluation_status ILIKE ('%' || $3 || '%')
                   OR EXISTS (SELECT 1 FROM unnest(systems) system_name WHERE system_name ILIKE ('%' || $3 || '%')))
              AND (NOT $4 OR latest_rank = 1)
        )
        SELECT
            COUNT(*) FILTER (WHERE evaluation_status IN ('pending', 'in_progress', 'cancelling')),
            COUNT(*) FILTER (WHERE evaluation_status NOT IN ('pending', 'in_progress', 'cancelling')),
            COUNT(*) FILTER (WHERE evaluation_status = 'complete'),
            COUNT(*) FILTER (WHERE evaluation_status = 'failed'),
            (SELECT COUNT(*) FROM domain WHERE evaluation_status IN ('pending', 'in_progress', 'cancelling')),
            COUNT(*) FILTER (WHERE evaluation_status IN ('pending', 'in_progress', 'cancelling'))
        FROM filtered
        "#,
    )
    .bind(if status_filter.is_empty() { None } else { Some(status_filter.clone()) })
    .bind(params.flake.as_deref())
    .bind(params.search.as_deref())
    .bind(params.latest_only)
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    let rows = sqlx::query_as::<_, EvalQueueRow>(
        r#"
        WITH domain AS (
        SELECT
            c.id AS commit_id,
            c.flake_id,
            f.name AS flake_name,
            COALESCE(f.branch, 'main') AS branch,
            c.git_commit_hash AS commit_hash,
            c.message AS commit_message,
            c.author,
            c.commit_timestamp AS committed_at,
            c.evaluation_enqueued_at AS enqueued_at,
            COALESCE(c.evaluation_status, 'pending') AS evaluation_status,
            COALESCE(c.eval_queue_position, 9223372036854775807) AS queue_position,
            COALESCE(cac.nixos_configurations, ARRAY[]::text[]) AS systems,
            COALESCE(CARDINALITY(cac.nixos_configurations), 0)::bigint AS system_count,
            COALESCE((
                SELECT COUNT(*)::bigint
                FROM derivations d
                WHERE d.commit_id = c.id
                  AND d.cf_agent_enabled IS TRUE
                  AND d.policy_requirements_met IS TRUE
                  AND d.status_id <> 6
            ), 0) AS passed_count,
            COALESCE((
                SELECT COUNT(*)::bigint
                FROM derivations d
                WHERE d.commit_id = c.id
                  AND d.status_id <> 6
                  AND (
                    d.cf_agent_enabled IS FALSE
                    OR d.policy_requirements_met IS FALSE
                  )
            ), 0) AS policy_failed_count,
            COALESCE((
                SELECT COUNT(*)::bigint
                FROM derivations d
                WHERE d.commit_id = c.id
                  AND d.status_id = 6
            ), 0) AS eval_failed_count,
            COALESCE(ea.attempt_number, 1) AS attempt_number,
            ea.parent_attempt_id,
            ea.root_attempt_id,
            ea.available_at,
            ROW_NUMBER() OVER (
                PARTITION BY c.flake_id,
                    COALESCE(c.evaluation_status, 'pending') IN ('pending', 'in_progress', 'cancelling')
                ORDER BY c.evaluation_enqueued_at DESC, c.id DESC
            ) AS latest_rank
        FROM commits c
        JOIN flakes f ON f.id = c.flake_id
        LEFT JOIN commit_artifacts_cache cac ON cac.commit_id = c.id
        LEFT JOIN LATERAL (
            SELECT attempt_number, parent_attempt_id, root_attempt_id, available_at
            FROM evaluation_attempts
            WHERE commit_id = c.id
            ORDER BY attempt_number DESC, created_at DESC
            LIMIT 1
        ) ea ON TRUE
        WHERE COALESCE(c.evaluation_status, 'pending') IN ('pending', 'in_progress', 'cancelling', 'complete', 'failed', 'cancelled')
          AND c.source_archived = false
          AND ($5::uuid IS NULL OR EXISTS (
            SELECT 1 FROM systems s JOIN user_environment_memberships uem ON uem.environment_id = s.environment_id
            WHERE s.flake_id = c.flake_id AND uem.user_id = $5
          ))
        ), filtered AS (
            SELECT * FROM domain
            WHERE ($1::text[] IS NULL OR cardinality($1::text[]) = 0 OR evaluation_status = ANY($1::text[]))
              AND ($2::text IS NULL OR flake_name ILIKE ('%' || $2 || '%'))
              AND ($3::text IS NULL OR flake_name ILIKE ('%' || $3 || '%') OR branch ILIKE ('%' || $3 || '%')
                   OR commit_hash ILIKE ('%' || $3 || '%') OR COALESCE(commit_message, '') ILIKE ('%' || $3 || '%')
                   OR COALESCE(author, '') ILIKE ('%' || $3 || '%') OR evaluation_status ILIKE ('%' || $3 || '%')
                   OR EXISTS (SELECT 1 FROM unnest(systems) system_name WHERE system_name ILIKE ('%' || $3 || '%')))
              AND (NOT $4 OR latest_rank = 1)
        )
        SELECT *, latest_rank = 1 AS is_latest_per_flake
        FROM filtered
        ORDER BY
            CASE
                WHEN evaluation_status = 'in_progress' THEN 0
                WHEN evaluation_status = 'cancelling' THEN 0
                WHEN evaluation_status = 'pending' THEN 1
                ELSE 2
            END,
            queue_position DESC NULLS LAST,
            committed_at DESC,
            commit_id DESC
        LIMIT $6
        "#,
    )
    .bind(if status_filter.is_empty() { None } else { Some(status_filter) })
    .bind(params.flake.as_deref())
    .bind(params.search.as_deref())
    .bind(params.latest_only)
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(EvalQueueResult {
        rows,
        active_count: counts.0,
        completed_count: counts.1,
        successful_count: counts.2,
        failed_count: counts.3,
        domain_total: counts.4,
        filtered_total: counts.5,
    })
}

pub async fn reorder_eval_queue(pool: &PgPool, ordered_commit_ids: &[i32]) -> Result<()> {
    let mut tx = pool.begin().await?;

    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(EVAL_QUEUE_ADVISORY_LOCK_KEY)
        .execute(&mut *tx)
        .await?;

    let active_commit_ids: Vec<i32> = sqlx::query_scalar(
        r#"
        SELECT c.id
        FROM commits c
        WHERE COALESCE(c.evaluation_status, 'pending') IN ('pending', 'in_progress')
          AND c.source_archived = false
        ORDER BY
            CASE
                WHEN c.evaluation_status = 'in_progress' THEN 0
                ELSE 1
            END,
            COALESCE(c.eval_queue_position, 0) DESC,
            c.commit_timestamp DESC,
            c.id DESC
        FOR UPDATE
        "#,
    )
    .fetch_all(&mut *tx)
    .await?;

    validate_eval_queue_reorder_payload(&active_commit_ids, ordered_commit_ids)?;

    sqlx::query(
        r#"
        WITH ordered AS (
            SELECT commit_id,
                   MAX(ordinality) OVER () - ordinality + 1 AS position
            FROM UNNEST($1::int[]) WITH ORDINALITY AS t(commit_id, ordinality)
        )
        UPDATE commits c
        SET eval_queue_position = o.position
        FROM ordered o
        WHERE c.id = o.commit_id
          AND COALESCE(c.evaluation_status, 'pending') IN ('pending', 'in_progress')
        "#,
    )
    .bind(ordered_commit_ids)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

fn validate_eval_queue_reorder_payload(
    active_commit_ids: &[i32],
    ordered_commit_ids: &[i32],
) -> Result<()> {
    if active_commit_ids.is_empty() && ordered_commit_ids.is_empty() {
        return Ok(());
    }

    let mut seen = HashSet::new();
    let mut duplicates = BTreeSet::new();
    for commit_id in ordered_commit_ids {
        if !seen.insert(*commit_id) {
            duplicates.insert(*commit_id);
        }
    }

    let payload_set: HashSet<i32> = ordered_commit_ids.iter().copied().collect();
    let active_set: HashSet<i32> = active_commit_ids.iter().copied().collect();

    let missing_ids = active_set
        .difference(&payload_set)
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let extra_ids = payload_set
        .difference(&active_set)
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    if duplicates.is_empty() && missing_ids.is_empty() && extra_ids.is_empty() {
        return Ok(());
    }

    let duplicate_ids = duplicates.into_iter().collect::<Vec<_>>();
    Err(anyhow::anyhow!(
        "invalid eval queue reorder request: duplicate IDs: {:?}; missing IDs: {:?}; extra IDs: {:?}",
        duplicate_ids,
        missing_ids,
        extra_ids
    ))
}

/// Cancel an evaluation by commit ID.
///
/// - `pending → cancelled` immediately (sets cancellation_requested = FALSE)
/// - `in_progress → cancelling` (sets cancellation_requested = TRUE so the loop kills the subprocess)
/// - Returns `NotFound` if no matching row, `AlreadyTerminal` for complete/failed/cancelled rows.
///
/// NOTE: Uses `UPDATE ... RETURNING` so the returned outcome reflects an actual
/// row transition, avoiding the TOCTOU race between a SELECT and subsequent UPDATE.
pub async fn cancel_commit_evaluation(pool: &PgPool, commit_id: i32) -> Result<CancelEvalOutcome> {
    let mut tx = pool.begin().await?;

    // Try pending -> cancelled first (no in-flight worker to coordinate with).
    let updated = sqlx::query_scalar::<_, i32>(
        r#"
        WITH cancelled_attempts AS (
            UPDATE evaluation_attempts
            SET status = 'cancelled',
                completed_at = COALESCE(completed_at, NOW()),
                updated_at = NOW()
            WHERE commit_id = $1
              AND status = 'queued'
            RETURNING id
        )
        UPDATE commits c
        SET evaluation_status = 'cancelled',
            cancellation_requested = FALSE,
            evaluation_completed_at = NOW(),
            evaluation_error_message = NULL
        WHERE id = $1
          AND COALESCE(evaluation_status, 'pending') = 'pending'
          AND EXISTS (SELECT 1 FROM cancelled_attempts)
        RETURNING id
        "#,
    )
    .bind(commit_id)
    .fetch_optional(&mut *tx)
    .await?;

    if updated.is_some() {
        tx.commit().await?;
        info!("🚫 Cancelled pending evaluation for commit {commit_id}");
        return Ok(CancelEvalOutcome::Cancelled);
    }

    // Try in_progress -> cancelling (worker will see cancellation_requested
    // in its poll loop and kill the subprocess cooperatively).
    let updated = sqlx::query_scalar::<_, i32>(
        r#"
        UPDATE commits
        SET evaluation_status = 'cancelling',
            cancellation_requested = TRUE
        WHERE id = $1
          AND evaluation_status = 'in_progress'
        RETURNING id
        "#,
    )
    .bind(commit_id)
    .fetch_optional(&mut *tx)
    .await?;

    if updated.is_some() {
        tx.commit().await?;
        info!("🔄 Requested cancellation for in-progress evaluation commit {commit_id}");
        return Ok(CancelEvalOutcome::CancellingInProgress);
    }

    // No transition occurred — determine the current state for a meaningful response.
    let current: Option<String> =
        sqlx::query_scalar("SELECT evaluation_status FROM commits WHERE id = $1")
            .bind(commit_id)
            .fetch_optional(&mut *tx)
            .await?
            .flatten();

    tx.commit().await?;

    match current.as_deref() {
        None => Ok(CancelEvalOutcome::NotFound),
        Some("complete" | "failed" | "cancelled") => Ok(CancelEvalOutcome::AlreadyTerminal),
        // cancelling means a prior cancellation took effect between our updates.
        Some("cancelling") => Ok(CancelEvalOutcome::CancellingInProgress),
        Some(_) => Ok(CancelEvalOutcome::AlreadyTerminal),
    }
}

/// Force-cancel an evaluation stuck in 'cancelling' state.
///
/// Immediately transitions `cancelling → cancelled` without waiting for the
/// eval loop to confirm subprocess death. Use this for evals that have been
/// stuck in 'cancelling' for longer than expected.
///
/// IMPORTANT: only operates on rows in `cancelling` state. It does NOT accept
/// `in_progress` rows to avoid orphaning a still-running nix-eval-jobs subprocess.
/// To cancel an in_progress eval, use `cancel_commit_evaluation` first (which
/// sets cancellation_requested and lets the loop kill the process cooperatively),
/// then use force-cancel only if it gets stuck in `cancelling`.
///
/// NOTE: does NOT clear `cancellation_requested` so a still-running evaluator
/// can detect the cancellation via its poll loop and return a typed
/// `EvaluationCancelled` error, which the outer handler treats as a
/// cancellation rather than a generic failure.
///
/// Returns `true` if the row was updated, `false` if it was already in a
/// different state (idempotent).
pub async fn force_cancel_commit_evaluation_attempt(
    pool: &PgPool,
    commit_id: i32,
    attempt_id: uuid::Uuid,
) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let attempt = sqlx::query(
        "UPDATE evaluation_attempts SET status = 'cancelled', completed_at = COALESCE(completed_at, NOW()), updated_at = NOW() WHERE id = $1 AND commit_id = $2 AND status = 'in_progress'",
    )
    .bind(attempt_id)
    .bind(commit_id)
    .execute(&mut *tx)
    .await?;
    if attempt.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(false);
    }

    let result = sqlx::query(
        r#"
        UPDATE commits
        SET evaluation_status = 'cancelled',
            evaluation_completed_at = COALESCE(evaluation_completed_at, NOW())
        WHERE id = $1
          AND evaluation_status = 'cancelling'
          AND EXISTS (SELECT 1 FROM evaluation_attempts WHERE id = $2 AND commit_id = $1 AND status = 'cancelled')
        "#,
    )
    .bind(commit_id)
    .bind(attempt_id)
    .execute(&mut *tx)
    .await?;

    let updated = result.rows_affected() > 0;
    if updated {
        tx.commit().await?;
        info!("⚡ Force-cancelled evaluation for commit {commit_id}");
    } else {
        tx.rollback().await?;
    }
    Ok(updated)
}

pub async fn force_cancel_commit_evaluation(pool: &PgPool, commit_id: i32) -> Result<bool> {
    let attempt_id: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM evaluation_attempts WHERE commit_id = $1 AND status = 'in_progress' ORDER BY started_at DESC NULLS LAST LIMIT 1",
    )
    .bind(commit_id)
    .fetch_optional(pool)
    .await?;
    match attempt_id {
        Some(attempt_id) => {
            force_cancel_commit_evaluation_attempt(pool, commit_id, attempt_id).await
        }
        None => Ok(false),
    }
}

/// Check whether cancellation has been requested for the given commit.
///
/// Called periodically from inside `evaluate_with_nix_eval_jobs` to allow
/// cooperative cancellation without holding a lock.
pub async fn check_cancellation_requested(pool: &PgPool, commit_id: i32) -> Result<bool> {
    let flag: Option<bool> =
        sqlx::query_scalar("SELECT cancellation_requested FROM commits WHERE id = $1")
            .bind(commit_id)
            .fetch_optional(pool)
            .await?;
    Ok(flag.unwrap_or(false))
}

/// Outcome of attempting to finalize an evaluation cancellation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalCancellationOutcome {
    /// The cancellation was finalized (row transitioned to cancelled).
    Cancelled,
    /// Already in a terminal cancelled state (idempotent).
    AlreadyCancelled,
    /// The evaluation attempt does not match (stale worker) or
    /// the commit is in a terminal complete/failed state.
    Superseded,
}

/// Atomically finalize a user-requested evaluation cancellation.
///
/// Transitions `cancelling` or `in_progress` (with `cancellation_requested`)
/// commits to `cancelled`, clears the request flag, and records completion
/// time — but only when the evaluation attempt matches. This prevents a
/// stale worker from finalizing a cancellation against a newer attempt
/// (P1-4 fix).
///
/// Returns:
/// - `Cancelled` when a transition actually occurred.
/// - `AlreadyCancelled` when the commit is already in `cancelled` state.
/// - `Superseded` when the attempt does not match or the commit is in a
///    terminal complete/failed state.
pub async fn finalize_requested_commit_evaluation_cancellation(
    pool: &PgPool,
    commit_id: i32,
    expected_attempt: i32,
) -> Result<EvalCancellationOutcome> {
    let updated = sqlx::query_scalar::<_, i32>(
        r#"
        WITH cancelled_attempt AS (
            UPDATE evaluation_attempts
            SET status = 'cancelled',
                completed_at = COALESCE(completed_at, NOW()),
                updated_at = NOW()
            WHERE commit_id = $1
              AND attempt_number = $2
              AND status = 'in_progress'
            RETURNING id
        )
        UPDATE commits
        SET evaluation_status = 'cancelled',
            cancellation_requested = FALSE,
            evaluation_completed_at = COALESCE(
                evaluation_completed_at,
                NOW()
            ),
            evaluation_error_message = NULL
        WHERE id = $1
          AND evaluation_attempt_count = $2
          AND (
              evaluation_status = 'cancelling'
              OR (
                  evaluation_status = 'in_progress'
                  AND cancellation_requested IS TRUE
              )
          )
          AND EXISTS (SELECT 1 FROM cancelled_attempt)
        RETURNING id
        "#,
    )
    .bind(commit_id)
    .bind(expected_attempt)
    .fetch_optional(pool)
    .await?;

    if updated.is_some() {
        return Ok(EvalCancellationOutcome::Cancelled);
    }

    // No transition — check current state for a meaningful response.
    let current: Option<String> =
        sqlx::query_scalar("SELECT evaluation_status FROM commits WHERE id = $1")
            .bind(commit_id)
            .fetch_optional(pool)
            .await?
            .flatten();

    match current.as_deref() {
        Some("cancelled") => Ok(EvalCancellationOutcome::AlreadyCancelled),
        // Any other existing state means this worker's attempt was superseded.
        _ => Ok(EvalCancellationOutcome::Superseded),
    }
}

/// Clean up partial derivations for a specific commit.
///
/// Called inline after cooperative eval cancellation to remove derivation rows
/// that were inserted mid-eval but never completed (no derivation_path).
/// Mirrors the startup-time `cleanup_partial_derivations` but scoped to one commit.
pub async fn cleanup_partial_derivations_for_commit(pool: &PgPool, commit_id: i32) -> Result<()> {
    sqlx::query(
        r#"
        DELETE FROM derivations
        WHERE commit_id = $1
          AND derivation_path IS NULL
          AND status_id IN (3, 4)
        "#,
    )
    .bind(commit_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Paginated query for evaluation history (complete, failed, cancelled).
///
/// Results are ordered by `evaluation_completed_at DESC` so the most
/// recently finished eval appears first.
pub async fn list_eval_history(
    pool: &PgPool,
    params: &EvalHistoryParams,
) -> Result<EvalHistoryPage> {
    let safe_limit = params.limit.max(1).min(crate::api::models::LIMIT_MAX);
    let safe_page = params.page.max(1);
    let offset = (safe_page - 1).checked_mul(safe_limit).ok_or_else(|| {
        anyhow::anyhow!("offset overflow: page={} limit={}", safe_page, safe_limit)
    })?;

    #[derive(sqlx::FromRow)]
    struct HistoryRow {
        commit_id: i32,
        flake_id: i32,
        flake_name: String,
        branch: String,
        commit_hash: String,
        commit_message: Option<String>,
        author: Option<String>,
        committed_at: chrono::DateTime<chrono::Utc>,
        enqueued_at: chrono::DateTime<chrono::Utc>,
        is_latest_per_flake: bool,
        evaluation_status: String,
        evaluation_completed_at: Option<chrono::DateTime<chrono::Utc>>,
        evaluation_duration_ms: Option<i64>,
        evaluation_error_message: Option<String>,
        system_count: i64,
        passed_count: i64,
        policy_failed_count: i64,
        eval_failed_count: i64,
        alert_occurrence_id: String,
        attempt_number: i32,
        parent_attempt_id: Option<uuid::Uuid>,
        root_attempt_id: Option<uuid::Uuid>,
    }

    let counts: (i64, i64) = sqlx::query_as(
        r#"
        WITH domain AS (
            SELECT c.id, c.flake_id, f.name AS flake_name, COALESCE(f.branch, 'main') AS branch,
                   c.git_commit_hash, c.message, c.author, c.evaluation_status,
                   c.evaluation_enqueued_at,
                   ROW_NUMBER() OVER (
                       PARTITION BY c.flake_id
                       ORDER BY c.evaluation_enqueued_at DESC, c.id DESC
                   ) AS latest_rank
            FROM commits c
            JOIN flakes f ON f.id = c.flake_id
            WHERE c.evaluation_status IN ('complete', 'failed', 'cancelled')
        ), filtered AS (
            SELECT * FROM domain
            WHERE ($1::text IS NULL OR evaluation_status = $1)
              AND ($2::text IS NULL OR flake_name ILIKE ('%' || $2 || '%'))
              AND ($3::text IS NULL OR flake_name ILIKE ('%' || $3 || '%') OR branch ILIKE ('%' || $3 || '%')
                   OR git_commit_hash ILIKE ('%' || $3 || '%') OR COALESCE(message, '') ILIKE ('%' || $3 || '%')
                   OR COALESCE(author, '') ILIKE ('%' || $3 || '%') OR evaluation_status ILIKE ('%' || $3 || '%'))
              AND (NOT $4 OR latest_rank = 1)
        )
        SELECT (SELECT COUNT(*) FROM domain), (SELECT COUNT(*) FROM filtered)
        "#,
    )
    .bind(params.status.as_deref())
    .bind(params.flake.as_deref())
    .bind(params.search.as_deref())
    .bind(params.latest_only)
    .fetch_one(pool)
    .await?;

    let rows = sqlx::query_as::<_, HistoryRow>(
        r#"
        WITH domain AS (
        SELECT
            c.id                            AS commit_id,
            c.flake_id,
            f.name                          AS flake_name,
            COALESCE(f.branch, 'main')      AS branch,
            c.git_commit_hash               AS commit_hash,
            c.message                       AS commit_message,
            c.author,
            c.commit_timestamp              AS committed_at,
            c.evaluation_enqueued_at         AS enqueued_at,
            c.evaluation_status,
            c.evaluation_completed_at,
            CASE
                WHEN c.evaluation_started_at IS NOT NULL
                 AND c.evaluation_completed_at IS NOT NULL
                THEN EXTRACT(EPOCH FROM (c.evaluation_completed_at - c.evaluation_started_at))::BIGINT * 1000
                ELSE NULL
            END                             AS evaluation_duration_ms,
            c.evaluation_error_message,
            COALESCE(CARDINALITY(cac.nixos_configurations), 0)::BIGINT AS system_count,
            COALESCE((
                SELECT COUNT(*)::BIGINT FROM derivations d
                WHERE d.commit_id = c.id
                  AND d.cf_agent_enabled IS TRUE
                  AND d.policy_requirements_met IS TRUE
                  AND d.status_id <> 6
            ), 0)                           AS passed_count,
            COALESCE((
                SELECT COUNT(*)::BIGINT FROM derivations d
                WHERE d.commit_id = c.id
                  AND d.status_id <> 6
                  AND (
                    d.cf_agent_enabled IS FALSE
                    OR d.policy_requirements_met IS FALSE
                  )
            ), 0)                           AS policy_failed_count,
            COALESCE((
                SELECT COUNT(*)::BIGINT FROM derivations d
                WHERE d.commit_id = c.id AND d.status_id = 6
            ), 0)                           AS eval_failed_count,
            concat_ws(
                ':',
                'eval',
                c.id::text,
                COALESCE(
                    (EXTRACT(EPOCH FROM c.evaluation_completed_at) * 1000000)::bigint::text,
                    'unknown'
                )
            )                               AS alert_occurrence_id,
            COALESCE(ea.attempt_number, 1)  AS attempt_number,
            ea.parent_attempt_id,
            ea.root_attempt_id,
            ROW_NUMBER() OVER (
                PARTITION BY c.flake_id
                ORDER BY c.evaluation_enqueued_at DESC, c.id DESC
            ) AS latest_rank
        FROM commits c
        JOIN flakes f ON f.id = c.flake_id
        LEFT JOIN commit_artifacts_cache cac ON cac.commit_id = c.id
        LEFT JOIN LATERAL (
            SELECT attempt_number, parent_attempt_id, root_attempt_id
            FROM evaluation_attempts
            WHERE commit_id = c.id
            ORDER BY attempt_number DESC, created_at DESC
            LIMIT 1
        ) ea ON TRUE
        WHERE c.evaluation_status IN ('complete', 'failed', 'cancelled')
        ), filtered AS (
            SELECT * FROM domain
            WHERE ($1::text IS NULL OR evaluation_status = $1)
              AND ($2::text IS NULL OR flake_name ILIKE ('%' || $2 || '%'))
              AND ($3::text IS NULL OR flake_name ILIKE ('%' || $3 || '%') OR branch ILIKE ('%' || $3 || '%')
                   OR commit_hash ILIKE ('%' || $3 || '%') OR COALESCE(commit_message, '') ILIKE ('%' || $3 || '%')
                   OR COALESCE(author, '') ILIKE ('%' || $3 || '%') OR evaluation_status ILIKE ('%' || $3 || '%'))
              AND (NOT $4 OR latest_rank = 1)
        )
        SELECT *, latest_rank = 1 AS is_latest_per_flake
        FROM filtered
        ORDER BY evaluation_completed_at DESC NULLS LAST, commit_id DESC
        LIMIT $5 OFFSET $6
        "#,
    )
    .bind(params.status.as_deref())
    .bind(params.flake.as_deref())
    .bind(params.search.as_deref())
    .bind(params.latest_only)
    .bind(safe_limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let (domain_total, total_count) = counts;

    let items = rows
        .into_iter()
        .map(|r| EvalHistoryItem {
            commit_id: r.commit_id,
            flake_id: r.flake_id,
            flake_name: r.flake_name,
            branch: r.branch,
            commit_hash: r.commit_hash,
            commit_message: r.commit_message,
            author: r.author,
            committed_at: r.committed_at,
            enqueued_at: r.enqueued_at,
            is_latest_per_flake: r.is_latest_per_flake,
            evaluation_status: r.evaluation_status,
            evaluation_completed_at: r.evaluation_completed_at,
            evaluation_duration_ms: r.evaluation_duration_ms,
            evaluation_error_message: r.evaluation_error_message,
            system_count: r.system_count,
            passed_count: r.passed_count,
            policy_failed_count: r.policy_failed_count,
            eval_failed_count: r.eval_failed_count,
            alert_occurrence_id: r.alert_occurrence_id,
            attempt_number: r.attempt_number,
            parent_attempt_id: r.parent_attempt_id,
            root_attempt_id: r.root_attempt_id,
        })
        .collect();

    Ok(EvalHistoryPage {
        total_count,
        domain_total,
        page: safe_page,
        limit: safe_limit,
        items,
    })
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EvalPolicySystemRow {
    pub system_name: String,
    pub eval_status: String,
    pub error_message: Option<String>,
    pub policy_results: serde_json::Value,
    /// NULL for rows written before the `policy_results` document existed
    /// (migration 0185 backfill). A NULL value combined with an empty
    /// `policy_results` document means the row has never been evaluated
    /// under the new policy-result model and must be surfaced as
    /// "legacy_unknown" rather than silently treated as passing or as an
    /// infrastructure error.
    pub policy_requirements_met: Option<bool>,
}

pub async fn fetch_eval_policy_matrix(
    pool: &PgPool,
    commit_id: i32,
) -> Result<Vec<EvalPolicySystemRow>> {
    let rows = sqlx::query_as::<_, EvalPolicySystemRow>(
        r#"
        SELECT
            d.derivation_name AS system_name,
            CASE
                WHEN d.status_id = 6 THEN 'eval_failed'
                ELSE 'evaluated'
            END AS eval_status,
            d.error_message,
            d.policy_results,
            d.policy_requirements_met
        FROM derivations d
        WHERE d.commit_id = $1
          AND d.derivation_type = 'nixos'
        ORDER BY d.derivation_name ASC
        "#,
    )
    .bind(commit_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EvalDependencyPackageRow {
    pub package_name: String,
    pub closure_counted: bool,
    pub ready_count: i64,
    pub pending_count: i64,
    pub failed_count: i64,
}

pub async fn fetch_eval_dependency_breakdown(
    pool: &PgPool,
    commit_id: i32,
) -> Result<Vec<EvalDependencyPackageRow>> {
    // Evaluation writes one nixos-type derivation per NixOS system config.
    // Closure counts are populated asynchronously after eval via
    // `nix-store --query --requisites <system.drv>`.
    //
    // We map each system to a row showing package-closure counts when present:
    //   ready_count   = packages already available in the local/cached store
    //   pending_count = packages still requiring build/substitution
    //   failed_count  = failed system marker when no package count is available
    //
    // If closure counts are not available yet, fall back to the historical
    // one-row system status so the graph still renders while counts are pending.
    //
    // status_id reference:
    //   3 = DryRunPending, 4 = DryRunInProgress, 5 = DryRunComplete
    //   6 = DryRunFailed,  7 = BuildPending,     8 = BuildInProgress
    //  10 = BuildComplete, 12 = BuildFailed
    let rows = sqlx::query_as::<_, EvalDependencyPackageRow>(
        r#"
        SELECT
            COALESCE(NULLIF(BTRIM(d.derivation_name), ''), 'unknown') AS package_name,
            (d.closure_total IS NOT NULL) AS closure_counted,
            CASE
                WHEN d.closure_total IS NOT NULL
                    THEN COALESCE(d.closure_cached, 0)::BIGINT
                WHEN d.status_id = 10 OR (d.store_path IS NOT NULL AND d.store_path != '')
                    THEN 1::BIGINT
                ELSE 0::BIGINT
            END AS ready_count,
            CASE
                WHEN d.closure_total IS NOT NULL
                    THEN GREATEST(d.closure_total - COALESCE(d.closure_cached, 0), 0)::BIGINT
                WHEN d.status_id IN (5, 7, 8)
                  AND (d.store_path IS NULL OR d.store_path = '')
                    THEN 1::BIGINT
                ELSE 0::BIGINT
            END AS pending_count,
            CASE
                WHEN d.status_id IN (6, 12)
                    THEN COALESCE(NULLIF(d.closure_total, 0), 1)::BIGINT
                ELSE 0::BIGINT
            END AS failed_count
        FROM derivations d
        WHERE d.commit_id = $1
          AND d.derivation_type = 'nixos'
        ORDER BY package_name ASC
        "#,
    )
    .bind(commit_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::{fetch_eval_policy_matrix, validate_eval_queue_reorder_payload};

    #[test]
    fn reorder_validation_rejects_duplicates() {
        let active = [10, 11, 12];
        let payload = [10, 11, 11];
        let err = validate_eval_queue_reorder_payload(&active, &payload)
            .expect_err("duplicates must be rejected")
            .to_string();

        assert!(err.contains("duplicate IDs: [11]"));
        assert!(err.contains("missing IDs: [12]"));
        assert!(err.contains("extra IDs: []"));
    }

    #[test]
    fn reorder_validation_rejects_missing() {
        let active = [10, 11, 12];
        let payload = [10, 12];
        let err = validate_eval_queue_reorder_payload(&active, &payload)
            .expect_err("missing IDs must be rejected")
            .to_string();

        assert!(err.contains("duplicate IDs: []"));
        assert!(err.contains("missing IDs: [11]"));
        assert!(err.contains("extra IDs: []"));
    }

    #[test]
    fn reorder_validation_rejects_extra() {
        let active = [10, 11, 12];
        let payload = [10, 11, 12, 99];
        let err = validate_eval_queue_reorder_payload(&active, &payload)
            .expect_err("extra IDs must be rejected")
            .to_string();

        assert!(err.contains("duplicate IDs: []"));
        assert!(err.contains("missing IDs: []"));
        assert!(err.contains("extra IDs: [99]"));
    }

    #[test]
    fn reorder_validation_accepts_full_permutation_and_positions_are_dense() {
        let active = [10, 11, 12, 13];
        let payload = [13, 10, 12, 11];

        validate_eval_queue_reorder_payload(&active, &payload)
            .expect("full permutation must be accepted");

        let positions = payload
            .iter()
            .enumerate()
            .map(|(index, commit_id)| (*commit_id, index as i64 + 1))
            .collect::<Vec<_>>();

        assert_eq!(positions, vec![(13, 1), (10, 2), (12, 3), (11, 4)]);
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires test database creation privileges"]
    async fn latest_evaluations_rank_before_filters_and_keep_tab_domains_separate(pool: PgPool) {
        let flake_id = insert_throwaway_flake(&pool).await;
        let tie_time = chrono::Utc::now() - chrono::Duration::minutes(5);

        let old_active: i32 = sqlx::query_scalar(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, evaluation_enqueued_at, evaluation_status) \
             VALUES ($1, $2, $3, $3, 'pending') RETURNING id",
        )
        .bind(flake_id)
        .bind(format!("needle-{}", uuid::Uuid::new_v4()))
        .bind(tie_time)
        .fetch_one(&pool)
        .await
        .unwrap();
        let latest_active: i32 = sqlx::query_scalar(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, evaluation_enqueued_at, evaluation_status) \
             VALUES ($1, $2, $3, $3, 'pending') RETURNING id",
        )
        .bind(flake_id)
        .bind(format!("winner-{}", uuid::Uuid::new_v4()))
        .bind(tie_time)
        .fetch_one(&pool)
        .await
        .unwrap();
        let history_id: i32 = sqlx::query_scalar(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, evaluation_enqueued_at, evaluation_status, evaluation_completed_at) \
             VALUES ($1, $2, $3, $3, 'complete', NOW()) RETURNING id",
        )
        .bind(flake_id)
        .bind(format!("history-{}", uuid::Uuid::new_v4()))
        .bind(tie_time + chrono::Duration::minutes(1))
        .fetch_one(&pool)
        .await
        .unwrap();

        let filtered = super::list_eval_queue(
            &pool,
            &crate::api::models::EvalQueueParams {
                limit: 20,
                search: Some("needle".to_string()),
                latest_only: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(filtered.domain_total, 2);
        assert_eq!(filtered.filtered_total, 0);
        assert!(filtered.rows.is_empty());

        let latest = super::list_eval_queue(
            &pool,
            &crate::api::models::EvalQueueParams {
                limit: 20,
                latest_only: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(latest.rows.iter().any(|row| row.commit_id == latest_active));
        assert!(latest.rows.iter().any(|row| row.commit_id == history_id));
        assert!(!latest.rows.iter().any(|row| row.commit_id == old_active));
        assert!(latest.rows.iter().all(|row| row.is_latest_per_flake));

        let mutation =
            sqlx::query("UPDATE commits SET evaluation_enqueued_at = NOW() WHERE id = $1")
                .bind(old_active)
                .execute(&pool)
                .await;
        assert!(
            mutation.is_err(),
            "evaluation enqueue time must be immutable"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires test database creation privileges"]
    async fn evaluation_history_totals_survive_empty_pagination_boundary(pool: PgPool) {
        for offset in 0..2 {
            let flake_id = insert_throwaway_flake(&pool).await;
            sqlx::query(
                "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, evaluation_enqueued_at, evaluation_status, evaluation_completed_at) \
                 VALUES ($1, $2, NOW(), NOW(), 'complete', NOW() + ($3 * INTERVAL '1 second'))",
            )
            .bind(flake_id)
            .bind(format!("history-page-{}", uuid::Uuid::new_v4()))
            .bind(offset)
            .execute(&pool)
            .await
            .unwrap();
        }

        let page = super::list_eval_history(
            &pool,
            &crate::api::models::EvalHistoryParams {
                page: 3,
                limit: 1,
                latest_only: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(page.domain_total, 2);
        assert_eq!(page.total_count, 2);
        assert!(page.items.is_empty());
    }

    // ── Live-database supersession-race regression tests ────────────────────
    //
    // Run against a repository-provided isolated database:
    //   DATABASE_URL=postgres://crystal_forge:password@localhost:3042/crystal_forge \
    //     cargo test -p cf-server --lib queries::commits -- --ignored

    use super::{
        EvalCancellationOutcome, EvalCompleteOutcome, EvalFailureOutcome, EvalStartOutcome,
        cancel_commit_evaluation, finalize_requested_commit_evaluation_cancellation,
        force_cancel_commit_evaluation, force_cancel_commit_evaluation_attempt,
        get_commits_pending_evaluation, list_eval_queue, mark_commit_evaluation_complete,
        mark_commit_evaluation_failed, mark_commit_evaluation_started,
        next_evaluation_available_at, open_eval_attention_if_current, reset_commit_evaluation,
        reset_stuck_commit_evaluations, resolve_eval_attention_unless_failed,
    };
    use crate::api::models::CancelEvalOutcome;
    use crate::api::models::EvalQueueParams;
    use crate::models::retry_policy::RetryFailureClass;
    use sqlx::PgPool;

    fn test_database_url() -> String {
        std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .expect(
                "CRYSTAL_FORGE_TEST_DATABASE_URL or DATABASE_URL must be set for database tests",
            )
    }

    async fn test_pool() -> PgPool {
        PgPool::connect(&test_database_url())
            .await
            .expect("failed to connect to test database")
    }

    async fn insert_throwaway_flake(pool: &PgPool) -> i32 {
        let short = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        sqlx::query_scalar::<_, i32>(
            "INSERT INTO flakes (name, repo_url, branch) VALUES ($1, $2, 'main') RETURNING id",
        )
        .bind(format!("att-eval-flake-{short}"))
        .bind(format!("https://git.example/att-eval-flake-{short}.git"))
        .fetch_one(pool)
        .await
        .expect("failed to insert throwaway test flake")
    }

    async fn insert_throwaway_commit(pool: &PgPool, flake_id: i32) -> i32 {
        let hash = uuid::Uuid::new_v4().simple().to_string();
        let commit_id = sqlx::query_scalar::<_, i32>(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) \
             VALUES ($1, $2, NOW()) RETURNING id",
        )
        .bind(flake_id)
        .bind(hash)
        .fetch_one(pool)
        .await
        .expect("failed to insert throwaway test commit");
        commit_id
    }

    async fn open_eval_count(pool: &PgPool, commit_id: i32) -> i64 {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM attention_occurrences \
             WHERE category = 'evals' AND subject_id = $1 AND resolved_at IS NULL",
        )
        .bind(commit_id.to_string())
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires test database creation privileges"]
    async fn evaluation_failure_schedules_one_delayed_linked_attempt_from_current_policy(
        pool: PgPool,
    ) {
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = match mark_commit_evaluation_started(&pool, commit_id)
            .await
            .expect("start attempt")
        {
            EvalStartOutcome::Started { attempt } => attempt,
            EvalStartOutcome::NoLongerPending => panic!("new commit should be due"),
        };
        sqlx::query(
            "UPDATE automatic_retry_policy SET max_evaluation_retries = 1, backoff_seconds = 10, transient_only = TRUE WHERE id = 1",
        )
        .execute(&pool)
        .await
        .expect("set policy governing the observed failure");
        mark_commit_evaluation_failed(
            &pool,
            commit_id,
            "temporary evaluator source failure",
            attempt,
            RetryFailureClass::Transient,
        )
        .await
        .expect("schedule retry");

        let next_due = next_evaluation_available_at(&pool)
            .await
            .unwrap()
            .expect("delayed retry due time");
        assert!(next_due > chrono::Utc::now() + chrono::Duration::seconds(8));
        assert!(
            get_commits_pending_evaluation(&pool)
                .await
                .unwrap()
                .iter()
                .all(|commit| commit.id != commit_id)
        );

        #[derive(sqlx::FromRow)]
        struct AttemptRow {
            id: uuid::Uuid,
            parent_attempt_id: Option<uuid::Uuid>,
            root_attempt_id: Option<uuid::Uuid>,
            attempt_number: i32,
            automatic_retry_count: i32,
            status: String,
            available_at: chrono::DateTime<chrono::Utc>,
        }
        let attempts = sqlx::query_as::<_, AttemptRow>(
            "SELECT id, parent_attempt_id, root_attempt_id, attempt_number, automatic_retry_count, status, available_at FROM evaluation_attempts WHERE commit_id = $1 ORDER BY attempt_number",
        )
        .bind(commit_id)
        .fetch_all(&pool)
        .await
        .expect("load attempts");
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0].status, "failed");
        assert_eq!(attempts[1].parent_attempt_id, Some(attempts[0].id));
        assert_eq!(attempts[1].root_attempt_id, Some(attempts[0].id));
        assert_eq!(attempts[1].attempt_number, 2);
        assert_eq!(attempts[1].automatic_retry_count, 1);
        assert_eq!(attempts[1].status, "queued");
        assert!(attempts[1].available_at > chrono::Utc::now());

        let duplicate = mark_commit_evaluation_failed(
            &pool,
            commit_id,
            "duplicate event",
            attempt,
            RetryFailureClass::Unknown,
        )
        .await;
        assert_eq!(
            duplicate.expect("duplicate is idempotent"),
            EvalFailureOutcome::SupersededOrCancelled
        );
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM evaluation_attempts WHERE automatic_retry_source_id = $1",
        )
        .bind(attempts[0].id)
        .fetch_one(&pool)
        .await
        .expect("count retry children");
        assert_eq!(count, 1);

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires test database creation privileges"]
    async fn evaluation_attempt_transitions_cancel_and_recover_without_stale_mutation(
        pool: PgPool,
    ) {
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let attempt = match mark_commit_evaluation_started(&pool, commit_id)
            .await
            .unwrap()
        {
            EvalStartOutcome::Started { attempt } => attempt,
            EvalStartOutcome::NoLongerPending => panic!("commit should be pending"),
        };
        let attempt_id: uuid::Uuid = sqlx::query_scalar(
            "SELECT id FROM evaluation_attempts WHERE commit_id = $1 AND attempt_number = $2",
        )
        .bind(commit_id)
        .bind(attempt)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(
            cancel_commit_evaluation(&pool, commit_id).await.unwrap(),
            CancelEvalOutcome::CancellingInProgress
        );
        assert!(
            force_cancel_commit_evaluation_attempt(&pool, commit_id, attempt_id)
                .await
                .unwrap()
        );
        reset_commit_evaluation(&pool, commit_id).await.unwrap();
        let _newer_attempt = match mark_commit_evaluation_started(&pool, commit_id)
            .await
            .unwrap()
        {
            EvalStartOutcome::Started { attempt } => attempt,
            EvalStartOutcome::NoLongerPending => panic!("commit should be pending after reset"),
        };
        assert!(
            mark_commit_evaluation_complete(&pool, commit_id, attempt)
                .await
                .unwrap()
                == EvalCompleteOutcome::SupersededOrCancelled
        );
        assert_eq!(
            mark_commit_evaluation_failed(
                &pool,
                commit_id,
                "stale failure",
                attempt,
                RetryFailureClass::Transient,
            )
            .await
            .unwrap(),
            EvalFailureOutcome::SupersededOrCancelled
        );

        let current_status: String =
            sqlx::query_scalar("SELECT evaluation_status FROM commits WHERE id = $1")
                .bind(commit_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(current_status, "in_progress");

        cancel_commit_evaluation(&pool, commit_id).await.unwrap();
        reset_stuck_commit_evaluations(&pool).await.unwrap();

        let status: String =
            sqlx::query_scalar("SELECT evaluation_status FROM commits WHERE id = $1")
                .bind(commit_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "cancelled");
        let summary = list_eval_queue(&pool, &EvalQueueParams::default())
            .await
            .unwrap();
        assert_eq!(summary.completed_count, 1);
        assert_eq!(summary.successful_count, 0);
        assert_eq!(summary.failed_count, 0);
        assert!(
            get_commits_pending_evaluation(&pool)
                .await
                .unwrap()
                .iter()
                .all(|commit| commit.id != commit_id)
        );

        let orphan_id = insert_throwaway_commit(&pool, flake_id).await;
        get_commits_pending_evaluation(&pool)
            .await
            .unwrap()
            .into_iter()
            .find(|commit| commit.id == orphan_id)
            .unwrap();
        assert!(matches!(
            mark_commit_evaluation_started(&pool, orphan_id)
                .await
                .unwrap(),
            EvalStartOutcome::Started { .. }
        ));
        reset_stuck_commit_evaluations(&pool).await.unwrap();
        let orphan_status: String =
            sqlx::query_scalar("SELECT evaluation_status FROM commits WHERE id = $1")
                .bind(orphan_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(orphan_status, "pending");
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn open_eval_attention_if_current_skips_when_commit_was_reset() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;

        let stale_completed_at = chrono::Utc::now() - chrono::Duration::minutes(5);

        // Simulate: the commit has since been manually reset to pending —
        // it is no longer 'failed', and evaluation_completed_at no longer
        // matches the stale value a delayed failure handler captured.
        sqlx::query(
            "UPDATE commits SET evaluation_status = 'pending', evaluation_completed_at = NULL WHERE id = $1",
        )
        .bind(commit_id)
        .execute(&pool)
        .await
        .unwrap();

        open_eval_attention_if_current(&pool, commit_id, stale_completed_at).await;
        assert_eq!(
            open_eval_count(&pool, commit_id).await,
            0,
            "a delayed failure handler must not open an occurrence for a reset commit"
        );

        let _ = sqlx::query(
            "DELETE FROM attention_occurrences WHERE category = 'evals' AND subject_id = $1",
        )
        .bind(commit_id.to_string())
        .execute(&pool)
        .await;
        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn open_eval_attention_if_current_opens_when_still_current() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let completed_at = chrono::Utc::now();

        sqlx::query(
            "UPDATE commits SET evaluation_status = 'failed', evaluation_completed_at = $2 WHERE id = $1",
        )
        .bind(commit_id)
        .bind(completed_at)
        .execute(&pool)
        .await
        .unwrap();

        open_eval_attention_if_current(&pool, commit_id, completed_at).await;
        assert_eq!(
            open_eval_count(&pool, commit_id).await,
            1,
            "a still-current failure must open its attention occurrence"
        );

        let _ = sqlx::query(
            "DELETE FROM attention_occurrences WHERE category = 'evals' AND subject_id = $1",
        )
        .bind(commit_id.to_string())
        .execute(&pool)
        .await;
        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn resolve_eval_attention_unless_failed_preserves_newer_failure() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let completed_at = chrono::Utc::now();

        // A newer failure has since been recorded on this commit.
        sqlx::query(
            "UPDATE commits SET evaluation_status = 'failed', evaluation_completed_at = $2 WHERE id = $1",
        )
        .bind(commit_id)
        .bind(completed_at)
        .execute(&pool)
        .await
        .unwrap();
        open_eval_attention_if_current(&pool, commit_id, completed_at).await;
        assert_eq!(open_eval_count(&pool, commit_id).await, 1);

        // A delayed completion/reset resolve action must not wipe out the
        // newer failure's still-valid occurrence.
        resolve_eval_attention_unless_failed(&pool, commit_id).await;
        assert_eq!(
            open_eval_count(&pool, commit_id).await,
            1,
            "resolve must not clear a newer failure's occurrence"
        );

        let _ = sqlx::query(
            "DELETE FROM attention_occurrences WHERE category = 'evals' AND subject_id = $1",
        )
        .bind(commit_id.to_string())
        .execute(&pool)
        .await;
        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn resolve_eval_attention_unless_failed_resolves_when_not_failed() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let completed_at = chrono::Utc::now();

        sqlx::query(
            "UPDATE commits SET evaluation_status = 'failed', evaluation_completed_at = $2 WHERE id = $1",
        )
        .bind(commit_id)
        .bind(completed_at)
        .execute(&pool)
        .await
        .unwrap();
        open_eval_attention_if_current(&pool, commit_id, completed_at).await;
        assert_eq!(open_eval_count(&pool, commit_id).await, 1);

        // Commit later completes successfully.
        sqlx::query("UPDATE commits SET evaluation_status = 'complete' WHERE id = $1")
            .bind(commit_id)
            .execute(&pool)
            .await
            .unwrap();

        resolve_eval_attention_unless_failed(&pool, commit_id).await;
        assert_eq!(
            open_eval_count(&pool, commit_id).await,
            0,
            "resolve must clear the occurrence once the commit is no longer failed"
        );

        let _ = sqlx::query(
            "DELETE FROM attention_occurrences WHERE category = 'evals' AND subject_id = $1",
        )
        .bind(commit_id.to_string())
        .execute(&pool)
        .await;
        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── P1 state-machine regression tests ───────────────────────────────────
    //
    // These tests cover the race scenarios identified in the P1 review of
    // MR !309.  Run against an isolated database:
    //
    //   DATABASE_URL=postgres://crystal_forge:password@localhost:3042/crystal_forge \
    //     cargo test -p cf-server --lib queries::commits::tests \
    //     -- --ignored --test-threads=1

    async fn start_eval(pool: &PgPool, commit_id: i32) -> i32 {
        match mark_commit_evaluation_started(pool, commit_id)
            .await
            .expect("start should succeed")
        {
            EvalStartOutcome::Started { attempt } => attempt,
            EvalStartOutcome::NoLongerPending => panic!("commit should be pending"),
        }
    }

    // ── Test 1: cancel-API pending race ────────────────────────────────────
    // The cancel API reads pending, the worker wins first (pending→in_progress).
    // The cancel UPDATE must affect zero rows and must NOT return Cancelled.
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn cancel_api_pending_race_does_not_return_cancelled_when_worker_wins() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;

        // Worker claims the commit first.
        let _attempt = start_eval(&pool, commit_id).await;

        // Cancel API now tries to cancel a "pending" commit — but it's already
        // in_progress.  The outcome must be CancellingInProgress (not Cancelled).
        let outcome = cancel_commit_evaluation(&pool, commit_id)
            .await
            .expect("cancel should not error");

        assert_ne!(
            outcome,
            CancelEvalOutcome::Cancelled,
            "cancel must not claim success when the worker already claimed in_progress"
        );

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Test 2: cancel-API completion race ────────────────────────────────
    // Cancel reads in_progress; evaluation completes before the cancel UPDATE.
    // Cancel must return AlreadyTerminal, not CancellingInProgress.
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn cancel_api_completion_race_returns_already_terminal_when_eval_wins() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;

        let attempt = start_eval(&pool, commit_id).await;

        // Evaluation completes first (CAS succeeds).
        let complete = mark_commit_evaluation_complete(&pool, commit_id, attempt)
            .await
            .expect("complete should not error");
        assert_eq!(complete, EvalCompleteOutcome::Completed);

        // Cancel API must now see the terminal state.
        let cancel = cancel_commit_evaluation(&pool, commit_id)
            .await
            .expect("cancel should not error");
        assert_eq!(
            cancel,
            CancelEvalOutcome::AlreadyTerminal,
            "cancel must not transition a complete commit"
        );

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Test 3: force-cancel then manual reset clears cancellation_requested ─
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn force_cancel_then_reset_clears_cancellation_requested() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;

        let _attempt = start_eval(&pool, commit_id).await;

        // cancel API: in_progress → cancelling
        let cancel = cancel_commit_evaluation(&pool, commit_id)
            .await
            .expect("cancel should not error");
        assert_eq!(cancel, CancelEvalOutcome::CancellingInProgress);

        // force-cancel: cancelling → cancelled
        let forced = force_cancel_commit_evaluation(&pool, commit_id)
            .await
            .expect("force cancel should not error");
        assert!(forced, "force cancel should transition the row");

        // Manual reset: cancelled → pending
        reset_commit_evaluation(&pool, commit_id)
            .await
            .expect("reset should not error");

        // Verify cancellation_requested is now FALSE.
        let flag: bool = sqlx::query_scalar(
            "SELECT COALESCE(cancellation_requested, FALSE) FROM commits WHERE id = $1",
        )
        .bind(commit_id)
        .fetch_one(&pool)
        .await
        .expect("query should succeed");

        assert!(
            !flag,
            "cancellation_requested must be FALSE after manual reset"
        );

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Test 4: stale typed cancellation does not affect newer attempt ────
    // Attempt 1 is force-cancelled. Manual reset intentionally resets the
    // attempt counter, then a new attempt starts. The old finalizer (still
    // carrying expected_attempt=1) must not cancel the reset evaluation.
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn stale_cancellation_finalizer_does_not_affect_newer_attempt() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;

        // Attempt 1 starts, is cancelled.
        let attempt1 = start_eval(&pool, commit_id).await;
        cancel_commit_evaluation(&pool, commit_id)
            .await
            .expect("cancel should not error");
        force_cancel_commit_evaluation(&pool, commit_id)
            .await
            .expect("force cancel should not error");

        // Manual reset → pending.
        reset_commit_evaluation(&pool, commit_id)
            .await
            .expect("reset should not error");

        // A new attempt starts. Because manual reset resets the attempt
        // counter, this may reuse attempt number 1; stale cancellation is
        // still prevented by status/cancellation_requested guards.
        let attempt2 = start_eval(&pool, commit_id).await;
        assert_eq!(
            attempt1, attempt2,
            "manual reset intentionally resets attempts"
        );

        // The stale worker for attempt 1 calls the finalizer with the old attempt.
        let outcome = finalize_requested_commit_evaluation_cancellation(
            &pool, commit_id, attempt1, // stale expected_attempt
        )
        .await
        .expect("finalizer should not error");

        assert_eq!(
            outcome,
            EvalCancellationOutcome::Superseded,
            "stale finalizer must not affect the newer attempt"
        );

        // Verify attempt 2 is still in_progress.
        let status: String =
            sqlx::query_scalar("SELECT evaluation_status FROM commits WHERE id = $1")
                .bind(commit_id)
                .fetch_one(&pool)
                .await
                .expect("query should succeed");
        assert_eq!(
            status, "in_progress",
            "newer attempt must remain in_progress"
        );

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Test 5: complete CAS returns SupersededOrCancelled when cancellation wins ─
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn complete_cas_superseded_when_cancellation_wins() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;

        let attempt = start_eval(&pool, commit_id).await;

        // Cancellation wins — sets cancelling + cancellation_requested.
        cancel_commit_evaluation(&pool, commit_id)
            .await
            .expect("cancel should not error");

        // Completion CAS must fail because cancellation_requested = TRUE.
        let complete = mark_commit_evaluation_complete(&pool, commit_id, attempt)
            .await
            .expect("complete should not error");
        assert_eq!(
            complete,
            EvalCompleteOutcome::SupersededOrCancelled,
            "complete CAS must fail when cancellation_requested is set"
        );

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Test 6: failure CAS returns SupersededOrCancelled when cancellation wins ─
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn failure_cas_superseded_when_cancellation_wins() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;

        let attempt = start_eval(&pool, commit_id).await;

        // Cancellation wins.
        cancel_commit_evaluation(&pool, commit_id)
            .await
            .expect("cancel should not error");

        // Failure CAS must fail because cancellation_requested = TRUE.
        let fail = mark_commit_evaluation_failed(
            &pool,
            commit_id,
            "test error",
            attempt,
            RetryFailureClass::Transient,
        )
        .await
        .expect("fail should not error");
        assert_eq!(
            fail,
            EvalFailureOutcome::SupersededOrCancelled,
            "failure CAS must fail when cancellation_requested is set"
        );

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Test 7: finalize cancellation sets status=cancelled and clears flag ──
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn finalize_cancellation_transitions_cancelling_to_cancelled() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;

        let attempt = start_eval(&pool, commit_id).await;

        cancel_commit_evaluation(&pool, commit_id)
            .await
            .expect("cancel should not error");

        // Finalizer with correct attempt must succeed.
        let outcome = finalize_requested_commit_evaluation_cancellation(&pool, commit_id, attempt)
            .await
            .expect("finalizer should not error");
        assert_eq!(outcome, EvalCancellationOutcome::Cancelled);

        // Verify the DB state.
        #[derive(sqlx::FromRow)]
        struct Row {
            evaluation_status: String,
            cancellation_requested: Option<bool>,
        }
        let row = sqlx::query_as::<_, Row>(
            "SELECT evaluation_status, cancellation_requested FROM commits WHERE id = $1",
        )
        .bind(commit_id)
        .fetch_one(&pool)
        .await
        .expect("query should succeed");

        assert_eq!(row.evaluation_status, "cancelled");
        assert!(
            !row.cancellation_requested.unwrap_or(false),
            "cancellation_requested must be cleared after finalization"
        );

        let attempt_status: String = sqlx::query_scalar(
            "SELECT status FROM evaluation_attempts WHERE commit_id = $1 AND attempt_number = $2",
        )
        .bind(commit_id)
        .bind(attempt)
        .fetch_one(&pool)
        .await
        .expect("query attempt status");
        assert_eq!(attempt_status, "cancelled");

        reset_commit_evaluation(&pool, commit_id)
            .await
            .expect("reset after cooperative cancellation should insert a new queued attempt");
        let queued_attempts: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM evaluation_attempts WHERE commit_id = $1 AND status = 'queued'",
        )
        .bind(commit_id)
        .fetch_one(&pool)
        .await
        .expect("count queued attempts");
        assert_eq!(queued_attempts, 1);

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Test 8: finalize cancellation is idempotent (AlreadyCancelled) ───────
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn finalize_cancellation_is_idempotent_when_already_cancelled() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;

        let attempt = start_eval(&pool, commit_id).await;
        cancel_commit_evaluation(&pool, commit_id)
            .await
            .expect("cancel should not error");

        // First finalization.
        finalize_requested_commit_evaluation_cancellation(&pool, commit_id, attempt)
            .await
            .expect("first finalization should not error");

        // Second finalization (same attempt) must be idempotent.
        let second = finalize_requested_commit_evaluation_cancellation(&pool, commit_id, attempt)
            .await
            .expect("second finalization should not error");
        assert_eq!(
            second,
            EvalCancellationOutcome::AlreadyCancelled,
            "repeated finalization must return AlreadyCancelled"
        );

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Test 9: mark_commit_evaluation_started skips cancelled commits ────────
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn mark_evaluation_started_skips_cancelled_commit() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;

        // Commit is directly cancelled (no in-progress phase).
        cancel_commit_evaluation(&pool, commit_id)
            .await
            .expect("cancel should not error");

        let outcome = mark_commit_evaluation_started(&pool, commit_id)
            .await
            .expect("start should not error");
        assert_eq!(
            outcome,
            EvalStartOutcome::NoLongerPending,
            "cancelled commit must not be resurrected by mark_evaluation_started"
        );

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Test 11: cancel is idempotent on already-cancelled commit ─────────────
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn cancel_api_idempotent_on_already_cancelled() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;

        // First cancel: pending → cancelled
        let first = cancel_commit_evaluation(&pool, commit_id)
            .await
            .expect("first cancel should not error");
        assert_eq!(first, CancelEvalOutcome::Cancelled);

        // Second cancel: already terminal
        let second = cancel_commit_evaluation(&pool, commit_id)
            .await
            .expect("second cancel should not error");
        assert_eq!(
            second,
            CancelEvalOutcome::AlreadyTerminal,
            "repeated cancel must return AlreadyTerminal"
        );

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Test 12: complete CAS wins before cancellation flag set ───────────────
    // If mark_commit_evaluation_complete is called before any cancellation
    // request is made, it must succeed.
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn complete_cas_wins_when_no_cancellation_requested() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;

        let attempt = start_eval(&pool, commit_id).await;

        // Complete without any cancellation → should win.
        let result = mark_commit_evaluation_complete(&pool, commit_id, attempt)
            .await
            .expect("complete should not error");
        assert_eq!(result, EvalCompleteOutcome::Completed);

        let status: String =
            sqlx::query_scalar("SELECT evaluation_status FROM commits WHERE id = $1")
                .bind(commit_id)
                .fetch_one(&pool)
                .await
                .expect("query should succeed");
        assert_eq!(status, "complete");

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Test 13: finalizer Superseded when commit is already complete ──────────
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn finalizer_superseded_when_commit_already_complete() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;

        let attempt = start_eval(&pool, commit_id).await;
        // Completion wins.
        mark_commit_evaluation_complete(&pool, commit_id, attempt)
            .await
            .expect("complete should not error");

        // Stale finalizer should see Superseded (not Cancelled).
        let outcome = finalize_requested_commit_evaluation_cancellation(&pool, commit_id, attempt)
            .await
            .expect("finalizer should not error");
        assert_eq!(
            outcome,
            EvalCancellationOutcome::Superseded,
            "finalizer must return Superseded when commit already completed"
        );

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Test 14: mark_evaluation_failed retry increments attempt count ────────
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn failed_attempt_retry_increments_count() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;

        // Attempt 1 starts and fails (with retry).
        let attempt1 = start_eval(&pool, commit_id).await;
        let outcome1 = mark_commit_evaluation_failed(
            &pool,
            commit_id,
            "error1",
            attempt1,
            RetryFailureClass::Transient,
        )
        .await
        .expect("fail should not error");
        // With attempt_count = 1 (< 3), should retry.
        assert_eq!(outcome1, EvalFailureOutcome::RetryScheduled);

        // Attempt 2 starts.
        let attempt2 = start_eval(&pool, commit_id).await;
        assert_eq!(
            attempt2,
            attempt1 + 1,
            "attempt counter must increment on retry"
        );

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Test 10: complete CAS returns SupersededOrCancelled for wrong attempt ─
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn complete_cas_superseded_for_stale_attempt() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;

        let attempt = start_eval(&pool, commit_id).await;

        // Simulate a stale worker using attempt - 1.
        let stale = attempt.saturating_sub(1);
        let complete = mark_commit_evaluation_complete(&pool, commit_id, stale)
            .await
            .expect("complete should not error");
        assert_eq!(
            complete,
            EvalCompleteOutcome::SupersededOrCancelled,
            "CAS must fail for a stale attempt number"
        );

        // Actual attempt still wins.
        let correct = mark_commit_evaluation_complete(&pool, commit_id, attempt)
            .await
            .expect("complete should not error");
        assert_eq!(correct, EvalCompleteOutcome::Completed);

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    /// Verify that the eval_status CASE expression in
    /// fetch_eval_policy_matrix distinguishes DryRunFailed (status_id = 6)
    /// from all build-state statuses (7, 8, 10, 12) regardless of
    /// error_message content.  Only status_id = 6 proves evaluation
    /// failure; build errors must not render as nix_eval_failure.
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn fetch_policy_matrix_distinguishes_eval_failure_from_build_errors() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;

        // Insert derivations with varying status_id and error_message combinations.
        async fn insert_deriv(
            pool: &PgPool,
            commit_id: i32,
            name: &str,
            status: i32,
            err: Option<&str>,
        ) {
            sqlx::query(
                r#"
                INSERT INTO derivations (
                    commit_id, derivation_type, derivation_name, derivation_target,
                    status_id, attempt_count, cf_agent_enabled, policy_requirements_met,
                    policy_results, error_message
                ) VALUES ($1, 'nixos', $2, $2, $3, 0, TRUE, TRUE, '{}'::jsonb, $4)
                "#,
            )
            .bind(commit_id)
            .bind(name)
            .bind(status)
            .bind(err)
            .execute(pool)
            .await
            .unwrap();
        }

        insert_deriv(
            &pool,
            commit_id,
            "eval-failed",
            6,
            Some("undefined variable 'foobar'"),
        )
        .await;
        insert_deriv(&pool, commit_id, "build-queued", 7, None).await;
        insert_deriv(&pool, commit_id, "build-building", 8, None).await;
        insert_deriv(&pool, commit_id, "build-complete", 10, None).await;
        insert_deriv(&pool, commit_id, "build-failed", 12, Some("gcc segfault")).await;
        insert_deriv(
            &pool,
            commit_id,
            "build-failed-witherr",
            12,
            Some("out of disk space"),
        )
        .await;
        insert_deriv(&pool, commit_id, "build-complete-noerr", 10, None).await;

        let rows = fetch_eval_policy_matrix(&pool, commit_id)
            .await
            .expect("fetch should succeed");

        for row in &rows {
            match row.system_name.as_str() {
                "eval-failed" => {
                    assert_eq!(
                        row.eval_status, "eval_failed",
                        "status_id=6 must produce eval_failed"
                    );
                }
                other => {
                    assert_eq!(
                        row.eval_status, "evaluated",
                        "status_id for {other} must produce 'evaluated', not 'eval_failed'"
                    );
                }
            }
        }
        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }
}
