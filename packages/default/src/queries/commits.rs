use crate::models::commits::Commit;
use crate::models::flakes::Flake;
use anyhow::{Context, Result};
use sqlx::PgPool;
use std::collections::{BTreeSet, HashSet};
use tracing::{debug, error, info, warn};

const EVAL_QUEUE_ADVISORY_LOCK_KEY: i64 = 1_600_001;

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
        WITH queue_lock AS (
            SELECT pg_advisory_xact_lock($6)
        ),
        next_position AS (
            SELECT COALESCE(MAX(eval_queue_position), 0) + 1 AS position
            FROM commits
            WHERE COALESCE(evaluation_status, 'pending') IN ('pending', 'in_progress')
        )
        INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, message, author, eval_queue_position)
        SELECT $1, $2, $3, $4, $5, position
        FROM next_position, queue_lock
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(flake_id.0)
    .bind(commit_hash)
    .bind(commit_timestamp)
    .bind(message)
    .bind(author)
    .bind(EVAL_QUEUE_ADVISORY_LOCK_KEY)
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
        warn!(
            "🧹 Reset {} in-progress commit evaluations on startup",
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

pub async fn promote_pending_commits_by_hashes(
    pool: &PgPool,
    flake_id: i32,
    inserted_commit_hashes: &[String],
) -> Result<()> {
    if inserted_commit_hashes.is_empty() {
        return Ok(());
    }

    let mut tx = pool.begin().await?;

    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(EVAL_QUEUE_ADVISORY_LOCK_KEY)
        .execute(&mut *tx)
        .await?;

    let promoted_ids: Vec<i32> = sqlx::query_scalar(
        r#"
        SELECT c.id
        FROM commits c
        WHERE c.flake_id = $1
          AND COALESCE(c.evaluation_status, 'pending') = 'pending'
          AND c.git_commit_hash = ANY($2)
        ORDER BY c.commit_timestamp DESC, c.id DESC
        FOR UPDATE
        "#,
    )
    .bind(flake_id)
    .bind(inserted_commit_hashes)
    .fetch_all(&mut *tx)
    .await?;

    if promoted_ids.is_empty() {
        tx.commit().await?;
        return Ok(());
    }

    let in_progress_ids: Vec<i32> = sqlx::query_scalar(
        r#"
        SELECT c.id
        FROM commits c
        WHERE COALESCE(c.evaluation_status, 'pending') = 'in_progress'
        ORDER BY COALESCE(c.eval_queue_position, 9223372036854775807), c.commit_timestamp DESC, c.id DESC
        FOR UPDATE
        "#,
    )
    .fetch_all(&mut *tx)
    .await?;

    let pending_ids: Vec<i32> = sqlx::query_scalar(
        r#"
        SELECT c.id
        FROM commits c
        WHERE COALESCE(c.evaluation_status, 'pending') = 'pending'
        ORDER BY COALESCE(c.eval_queue_position, 9223372036854775807), c.commit_timestamp DESC, c.id DESC
        FOR UPDATE
        "#,
    )
    .fetch_all(&mut *tx)
    .await?;

    let ordered_commit_ids = merge_promoted_pending_ids(&in_progress_ids, &pending_ids, &promoted_ids);

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
    .bind(&ordered_commit_ids)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

fn merge_promoted_pending_ids(
    in_progress_ids: &[i32],
    pending_ids: &[i32],
    promoted_ids: &[i32],
) -> Vec<i32> {
    let promoted: HashSet<i32> = promoted_ids.iter().copied().collect();
    let mut ordered = Vec::with_capacity(in_progress_ids.len() + pending_ids.len());

    ordered.extend_from_slice(in_progress_ids);
    ordered.extend_from_slice(promoted_ids);
    ordered.extend(
        pending_ids
            .iter()
            .copied()
            .filter(|commit_id| !promoted.contains(commit_id)),
    );

    ordered
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

#[cfg(test)]
mod tests {
    use super::{merge_promoted_pending_ids, validate_eval_queue_reorder_payload};

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

    #[test]
    fn merge_promoted_pending_keeps_in_progress_first_then_new_sync_commits() {
        let in_progress = vec![50];
        let pending = vec![10, 11, 12, 13];
        let promoted = vec![13, 12];

        let ordered = merge_promoted_pending_ids(&in_progress, &pending, &promoted);
        assert_eq!(ordered, vec![50, 13, 12, 10, 11]);
    }
}
