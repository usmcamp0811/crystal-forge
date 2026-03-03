use crate::models::commits::Commit;
use crate::models::flakes::Flake;
use anyhow::{Context, Result};
use sqlx::PgPool;
use std::collections::HashSet;
use tracing::{debug, error, info, warn};

pub async fn insert_commit(
    pool: &PgPool,
    commit_hash: &str,
    repo_url: &str,
    commit_timestamp: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    insert_commit_with_metadata(pool, commit_hash, repo_url, commit_timestamp, None, None).await
}

pub async fn insert_commit_with_metadata(
    pool: &PgPool,
    commit_hash: &str,
    repo_url: &str,
    commit_timestamp: chrono::DateTime<chrono::Utc>,
    message: Option<&str>,
    author: Option<&str>,
) -> Result<()> {
    let flake_id: (i32,) = sqlx::query_as("SELECT id FROM flakes WHERE repo_url = $1")
        .bind(repo_url)
        .fetch_optional(pool)
        .await?
        .context("No flake entry found")?;

    sqlx::query(
        r#"
        WITH next_position AS (
            SELECT COALESCE(MAX(eval_queue_position), 0) + 1 AS position
            FROM commits
            WHERE COALESCE(evaluation_status, 'pending') IN ('pending', 'in_progress')
        )
        INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, message, author, eval_queue_position)
        SELECT $1, $2, $3, $4, $5, position
        FROM next_position
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(flake_id.0)
    .bind(commit_hash)
    .bind(commit_timestamp)
    .bind(message)
    .bind(author)
    .execute(pool)
    .await?;

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
    let rows = sqlx::query_as::<_, Commit>(
        r#"
        SELECT c.id, c.flake_id, c.git_commit_hash, c.commit_timestamp, c.attempt_count
        FROM commits c
        LEFT JOIN derivations d ON c.id = d.commit_id
        WHERE d.commit_id IS NULL
        AND c.evaluation_status = 'pending'
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
            c.commit_timestamp DESC
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

/// Reset commits stuck in 'in_progress' state (from crashed evaluations)
pub async fn reset_stuck_commit_evaluations(pool: &PgPool) -> Result<()> {
    // Reset ALL in_progress commits on startup (not just stuck ones)
    // This ensures clean state and enforces single-eval-at-a-time invariant
    let reset = sqlx::query!(
        r#"
        UPDATE commits
        SET 
            evaluation_status = 'pending',
            evaluation_started_at = NULL
        WHERE evaluation_status = 'in_progress'
        RETURNING id, git_commit_hash
        "#
    )
    .fetch_all(pool)
    .await?;

    if !reset.is_empty() {
        warn!("🧹 Reset {} in-progress commit evaluations on startup", reset.len());
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
    sqlx::query!(
        r#"
        UPDATE commits
        SET 
            evaluation_status = 'complete',
            evaluation_completed_at = NOW(),
            evaluation_error_message = NULL
        WHERE id = $1
        "#,
        commit_id
    )
    .execute(pool)
    .await?;

    Ok(())
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
    sqlx::query(
        r#"
        UPDATE commits
        SET 
            evaluation_status = CASE 
                WHEN COALESCE(evaluation_attempt_count, 0) >= 3 THEN 'failed'
                ELSE 'pending'
            END,
            evaluation_error_message = $2
        WHERE id = $1
        "#,
    )
    .bind(commit_id)
    .bind(error)
    .execute(pool)
    .await?;

    Ok(())
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
            ), 0) AS eval_failed_count
        FROM commits c
        JOIN flakes f ON f.id = c.flake_id
        LEFT JOIN commit_artifacts_cache cac ON cac.commit_id = c.id
        WHERE COALESCE(c.evaluation_status, 'pending') IN ('pending', 'in_progress', 'complete', 'failed')
        ORDER BY
            CASE
                WHEN c.evaluation_status = 'in_progress' THEN 0
                WHEN c.evaluation_status = 'pending' THEN 1
                ELSE 2
            END,
            COALESCE(c.eval_queue_position, 9223372036854775807),
            c.commit_timestamp DESC
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
            c.commit_timestamp DESC
        "#,
    )
    .fetch_all(&mut *tx)
    .await?;

    if active_commit_ids.is_empty() {
        if ordered_commit_ids.is_empty() {
            tx.commit().await?;
            return Ok(());
        }

        return Err(anyhow::anyhow!(
            "invalid eval queue reorder request: no active queue items exist"
        ));
    }

    if ordered_commit_ids.len() != active_commit_ids.len() {
        return Err(anyhow::anyhow!(
            "invalid eval queue reorder request: payload size {} does not match active queue size {}",
            ordered_commit_ids.len(),
            active_commit_ids.len()
        ));
    }

    let payload_set: HashSet<i32> = ordered_commit_ids.iter().copied().collect();
    if payload_set.len() != ordered_commit_ids.len() {
        return Err(anyhow::anyhow!(
            "invalid eval queue reorder request: duplicate commit IDs are not allowed"
        ));
    }

    let active_set: HashSet<i32> = active_commit_ids.iter().copied().collect();
    if payload_set != active_set {
        return Err(anyhow::anyhow!(
            "invalid eval queue reorder request: payload must be a full permutation of active queue IDs"
        ));
    }

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
