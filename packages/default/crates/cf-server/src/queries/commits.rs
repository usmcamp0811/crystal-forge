use crate::api::models::{CancelEvalOutcome, EvalHistoryItem, EvalHistoryPage};
use crate::models::commits::Commit;
use crate::models::flakes::Flake;
use crate::queries::attention;
use anyhow::{Context, Result};
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
        SELECT c.id, c.flake_id, c.git_commit_hash, c.commit_timestamp, c.attempt_count
        FROM commits c
        WHERE c.evaluation_status = 'pending'
        AND COALESCE(c.evaluation_attempt_count, 0) < 3
        AND (
            c.evaluation_started_at IS NULL
            OR (
                -- Attempt 1: immediate
                -- Attempt 2: retry after 1 minute
                COALESCE(c.evaluation_attempt_count, 0) = 1
                AND c.evaluation_started_at < NOW() - INTERVAL '1 minute'
            )
            OR (
                -- Attempt 3: retry after 5 minutes
                c.evaluation_attempt_count = 2
                AND c.evaluation_started_at < NOW() - INTERVAL '5 minutes'
            )
        )
        ORDER BY
            COALESCE(c.eval_queue_position, 9223372036854775807),
            c.commit_timestamp DESC,
            c.id DESC
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
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
         WHERE f.repo_url = $1",
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
         WHERE repo_url = $1 
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
    let latest_commit = sqlx::query!(
        r#"
        SELECT id, git_commit_hash
        FROM commits
        WHERE flake_id = $1
        ORDER BY commit_timestamp DESC
        LIMIT 1
        "#,
        flake.id
    )
    .fetch_one(pool)
    .await?;

    // If this is the latest commit, distance is 0
    if latest_commit.id == commit.id {
        return Ok(0);
    }

    // Count commits between this one and HEAD
    let distance = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*)::int as "count!"
        FROM commits
        WHERE flake_id = $1
        AND commit_timestamp > $2
        "#,
        flake.id,
        commit.commit_timestamp
    )
    .fetch_one(pool)
    .await?;

    Ok(distance)
}

/// Reset commits stuck in 'in_progress' or 'cancelling' state (from crashed evaluations).
///
/// NOTE: `cancelled` rows are intentionally left alone — they represent
/// evaluations the user explicitly cancelled and should not be re-queued.
pub async fn reset_stuck_commit_evaluations(pool: &PgPool) -> Result<()> {
    let reset = sqlx::query!(
        r#"
        UPDATE commits
        SET
            evaluation_status = 'pending',
            evaluation_started_at = NULL,
            cancellation_requested = FALSE
        WHERE evaluation_status IN ('in_progress', 'cancelling')
        RETURNING id, git_commit_hash
        "#
    )
    .fetch_all(pool)
    .await?;

    if !reset.is_empty() {
        warn!(
            "🧹 Reset {} in-progress/cancelling commit evaluations on startup",
            reset.len()
        );
        for row in &reset {
            info!("  - Commit {} ({})", row.id, row.git_commit_hash);
        }
    }

    Ok(())
}

/// Mark commit evaluation as started
///
/// This will fail if another commit is already in_progress due to the unique constraint
/// enforced by idx_commits_single_in_progress (migration 0088)
pub async fn mark_commit_evaluation_started(pool: &PgPool, commit_id: i32) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE commits
        SET 
            evaluation_status = 'in_progress',
            evaluation_started_at = NOW(),
            evaluation_completed_at = NULL,
            evaluation_attempt_count = COALESCE(evaluation_attempt_count, 0) + 1
        WHERE id = $1
        "#,
        commit_id
    )
    .execute(pool)
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

    Ok(())
}

/// Mark commit evaluation as successfully completed
pub async fn mark_commit_evaluation_complete(pool: &PgPool, commit_id: i32) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE commits
        SET
            evaluation_status = 'complete',
            evaluation_completed_at = NOW(),
            evaluation_error_message = NULL
        WHERE id = $1
        "#,
    )
    .bind(commit_id)
    .execute(pool)
    .await?;

    resolve_eval_attention_unless_failed(pool, commit_id).await;

    Ok(())
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

/// Mark commit evaluation as failed (with retry logic)
///
/// Retries up to 3 times with exponential backoff:
/// - Attempt 1: immediate
/// - Attempt 2: after 1 minute
/// - Attempt 3: after 5 minutes (from attempt 2)
///
/// After 3 failed attempts, marks as permanently 'failed'.
/// Manual re-evaluation can be triggered via API (resets attempt count).
pub async fn mark_commit_evaluation_failed(
    pool: &PgPool,
    commit_id: i32,
    error: &str,
) -> Result<()> {
    let row = sqlx::query(
        r#"
        UPDATE commits
        SET
            evaluation_status = CASE
                WHEN COALESCE(evaluation_attempt_count, 0) >= 3 THEN 'failed'
                ELSE 'pending'
            END,
            evaluation_completed_at = CASE
                WHEN COALESCE(evaluation_attempt_count, 0) >= 3 THEN NOW()
                ELSE NULL
            END,
            evaluation_error_message = $2
        WHERE id = $1
        RETURNING evaluation_status, evaluation_completed_at
        "#,
    )
    .bind(commit_id)
    .bind(error)
    .fetch_one(pool)
    .await?;

    let status: &str = row.try_get("evaluation_status")?;
    if status == "failed" {
        let completed_at: Option<chrono::DateTime<chrono::Utc>> =
            row.try_get("evaluation_completed_at")?;
        if let Some(completed_at) = completed_at {
            open_eval_attention_if_current(pool, commit_id, completed_at).await;
        }
    }

    Ok(())
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
///
/// Use this for manual re-evaluation after fixing issues.
pub async fn reset_commit_evaluation(pool: &PgPool, commit_id: i32) -> Result<()> {
    #[derive(sqlx::FromRow)]
    struct ResetResult {
        id: i32,
        git_commit_hash: String,
    }

    let result = sqlx::query_as::<_, ResetResult>(
        r#"
        UPDATE commits
        SET 
            evaluation_status = 'pending',
            evaluation_attempt_count = 0,
            evaluation_started_at = NULL,
            evaluation_completed_at = NULL,
            evaluation_error_message = NULL
        WHERE id = $1
        RETURNING id, git_commit_hash
        "#,
    )
    .bind(commit_id)
    .fetch_one(pool)
    .await?;

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
    pub evaluation_status: String,
    pub queue_position: i64,
    pub systems: Vec<String>,
    pub system_count: i64,
    pub passed_count: i64,
    pub policy_failed_count: i64,
    pub eval_failed_count: i64,
    pub active_total_count: i64,
    pub completed_total_count: i64,
    pub failed_total_count: i64,
}

pub async fn list_eval_queue(pool: &PgPool, limit: i64) -> Result<Vec<EvalQueueRow>> {
    let rows = sqlx::query_as::<_, EvalQueueRow>(
        r#"
        SELECT
            c.id AS commit_id,
            c.flake_id,
            f.name AS flake_name,
            COALESCE(f.branch, 'main') AS branch,
            c.git_commit_hash AS commit_hash,
            c.message AS commit_message,
            c.author,
            c.commit_timestamp AS committed_at,
            COALESCE(c.evaluation_status, 'pending') AS evaluation_status,
            COALESCE(c.eval_queue_position, 9223372036854775807) AS queue_position,
            COALESCE(cac.nixos_configurations, ARRAY[]::text[]) AS systems,
            COALESCE(CARDINALITY(cac.nixos_configurations), 0)::bigint AS system_count,
            COALESCE((
                SELECT COUNT(*)::bigint
                FROM derivations d
                WHERE d.commit_id = c.id
                  AND d.cf_agent_enabled IS TRUE
            ), 0) AS passed_count,
            COALESCE((
                SELECT COUNT(*)::bigint
                FROM derivations d
                WHERE d.commit_id = c.id
                  AND d.cf_agent_enabled IS FALSE
            ), 0) AS policy_failed_count,
            COALESCE((
                SELECT COUNT(*)::bigint
                FROM derivations d
                WHERE d.commit_id = c.id
                  AND d.status_id = 6
            ), 0) AS eval_failed_count,
            COUNT(*) FILTER (
                WHERE COALESCE(c.evaluation_status, 'pending') IN ('pending', 'in_progress', 'cancelling')
            ) OVER () AS active_total_count,
            COUNT(*) FILTER (
                WHERE COALESCE(c.evaluation_status, 'pending') NOT IN ('pending', 'in_progress', 'cancelling')
            ) OVER () AS completed_total_count,
            COUNT(*) FILTER (
                WHERE COALESCE(c.evaluation_status, 'pending') = 'failed'
            ) OVER () AS failed_total_count
        FROM commits c
        JOIN flakes f ON f.id = c.flake_id
        LEFT JOIN commit_artifacts_cache cac ON cac.commit_id = c.id
        WHERE COALESCE(c.evaluation_status, 'pending') IN ('pending', 'in_progress', 'cancelling', 'complete', 'failed')
        ORDER BY
            CASE
                WHEN c.evaluation_status = 'in_progress' THEN 0
                WHEN c.evaluation_status = 'cancelling' THEN 0
                WHEN c.evaluation_status = 'pending' THEN 1
                ELSE 2
            END,
            COALESCE(c.eval_queue_position, 9223372036854775807),
            c.commit_timestamp DESC,
            c.id DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows)
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
        ORDER BY
            CASE
                WHEN c.evaluation_status = 'in_progress' THEN 0
                ELSE 1
            END,
            COALESCE(c.eval_queue_position, 9223372036854775807),
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
            SELECT commit_id, ordinality::bigint AS position
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
pub async fn cancel_commit_evaluation(pool: &PgPool, commit_id: i32) -> Result<CancelEvalOutcome> {
    #[derive(sqlx::FromRow)]
    struct Row {
        evaluation_status: Option<String>,
    }

    let current = sqlx::query_as::<_, Row>("SELECT evaluation_status FROM commits WHERE id = $1")
        .bind(commit_id)
        .fetch_optional(pool)
        .await?;

    let Some(row) = current else {
        return Ok(CancelEvalOutcome::NotFound);
    };

    match row.evaluation_status.as_deref().unwrap_or("pending") {
        "pending" => {
            sqlx::query(
                r#"
                UPDATE commits
                SET evaluation_status = 'cancelled',
                    cancellation_requested = FALSE,
                    evaluation_completed_at = NOW()
                WHERE id = $1
                  AND COALESCE(evaluation_status, 'pending') = 'pending'
                "#,
            )
            .bind(commit_id)
            .execute(pool)
            .await?;
            info!("🚫 Cancelled pending evaluation for commit {commit_id}");
            Ok(CancelEvalOutcome::Cancelled)
        }
        "in_progress" => {
            sqlx::query(
                r#"
                UPDATE commits
                SET evaluation_status = 'cancelling',
                    cancellation_requested = TRUE
                WHERE id = $1
                  AND evaluation_status = 'in_progress'
                "#,
            )
            .bind(commit_id)
            .execute(pool)
            .await?;
            info!("🔄 Requested cancellation for in-progress evaluation commit {commit_id}");
            Ok(CancelEvalOutcome::CancellingInProgress)
        }
        "complete" | "failed" | "cancelled" => Ok(CancelEvalOutcome::AlreadyTerminal),
        _ => Ok(CancelEvalOutcome::NotFound),
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
/// Returns `true` if the row was updated, `false` if it was already in a
/// different state (idempotent).
pub async fn force_cancel_commit_evaluation(pool: &PgPool, commit_id: i32) -> Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE commits
        SET evaluation_status = 'cancelled',
            cancellation_requested = FALSE,
            evaluation_completed_at = COALESCE(evaluation_completed_at, NOW())
        WHERE id = $1
          AND evaluation_status = 'cancelling'
        "#,
    )
    .bind(commit_id)
    .execute(pool)
    .await?;

    let updated = result.rows_affected() > 0;
    if updated {
        info!("⚡ Force-cancelled evaluation for commit {commit_id}");
    }
    Ok(updated)
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
    page: i64,
    limit: i64,
    status_filter: Option<&str>,
    flake_filter: Option<&str>,
) -> Result<EvalHistoryPage> {
    let safe_limit = limit.max(1).min(crate::api::models::LIMIT_MAX);
    let safe_page = page.max(1);
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
        evaluation_status: String,
        evaluation_completed_at: Option<chrono::DateTime<chrono::Utc>>,
        evaluation_duration_ms: Option<i64>,
        evaluation_error_message: Option<String>,
        system_count: i64,
        passed_count: i64,
        policy_failed_count: i64,
        eval_failed_count: i64,
        alert_occurrence_id: String,
        total_count: i64,
    }

    let rows = sqlx::query_as::<_, HistoryRow>(
        r#"
        SELECT
            c.id                            AS commit_id,
            c.flake_id,
            f.name                          AS flake_name,
            COALESCE(f.branch, 'main')      AS branch,
            c.git_commit_hash               AS commit_hash,
            c.message                       AS commit_message,
            c.author,
            c.commit_timestamp              AS committed_at,
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
                WHERE d.commit_id = c.id AND d.cf_agent_enabled IS TRUE
            ), 0)                           AS passed_count,
            COALESCE((
                SELECT COUNT(*)::BIGINT FROM derivations d
                WHERE d.commit_id = c.id AND d.cf_agent_enabled IS FALSE
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
            COUNT(*) OVER ()                AS total_count
        FROM commits c
        JOIN flakes f ON f.id = c.flake_id
        LEFT JOIN commit_artifacts_cache cac ON cac.commit_id = c.id
        WHERE c.evaluation_status IN ('complete', 'failed', 'cancelled')
          AND ($1::text IS NULL OR c.evaluation_status = $1)
          AND ($2::text IS NULL OR f.name ILIKE ('%' || $2 || '%'))
        ORDER BY c.evaluation_completed_at DESC NULLS LAST, c.id DESC
        LIMIT $3 OFFSET $4
        "#,
    )
    .bind(status_filter)
    .bind(flake_filter)
    .bind(safe_limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let total_count = rows.first().map(|r| r.total_count).unwrap_or(0);

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
            evaluation_status: r.evaluation_status,
            evaluation_completed_at: r.evaluation_completed_at,
            evaluation_duration_ms: r.evaluation_duration_ms,
            evaluation_error_message: r.evaluation_error_message,
            system_count: r.system_count,
            passed_count: r.passed_count,
            policy_failed_count: r.policy_failed_count,
            eval_failed_count: r.eval_failed_count,
            alert_occurrence_id: r.alert_occurrence_id,
        })
        .collect();

    Ok(EvalHistoryPage {
        total_count,
        page: safe_page,
        limit: safe_limit,
        items,
    })
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EvalPolicySystemRow {
    pub system_name: String,
    pub policy_status: String,
    pub detail: Option<String>,
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
                WHEN d.cf_agent_enabled IS TRUE THEN 'pass'
                WHEN d.cf_agent_enabled IS FALSE THEN 'fail'
                WHEN d.status_id = 6 OR d.error_message IS NOT NULL THEN 'warn'
                ELSE 'warn'
            END AS policy_status,
            CASE
                WHEN d.cf_agent_enabled IS FALSE
                    THEN 'Crystal Forge agent is disabled. Enable with crystal-forge.agent.enable = true in your NixOS configuration.'
                WHEN d.error_message IS NOT NULL
                    THEN d.error_message
                ELSE NULL
            END AS detail
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
    use super::validate_eval_queue_reorder_payload;

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

    // ── Live-database supersession-race regression tests ────────────────────
    //
    // Run against a repository-provided isolated database:
    //   DATABASE_URL=postgres://crystal_forge:password@localhost:3042/crystal_forge \
    //     cargo test -p cf-server --lib queries::commits -- --ignored

    use super::{open_eval_attention_if_current, resolve_eval_attention_unless_failed};
    use sqlx::PgPool;

    async fn test_pool() -> PgPool {
        PgPool::connect(&std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for DB tests"))
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
        sqlx::query_scalar::<_, i32>(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) \
             VALUES ($1, $2, NOW()) RETURNING id",
        )
        .bind(flake_id)
        .bind(hash)
        .fetch_one(pool)
        .await
        .expect("failed to insert throwaway test commit")
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

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE category = 'evals' AND subject_id = $1")
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

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE category = 'evals' AND subject_id = $1")
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

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE category = 'evals' AND subject_id = $1")
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

        let _ = sqlx::query("DELETE FROM attention_occurrences WHERE category = 'evals' AND subject_id = $1")
            .bind(commit_id.to_string())
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }
}
