use crate::models::commits::Commit;
use crate::models::flakes::Flake;
use anyhow::{Context, Result};
use sqlx::PgPool;
use tracing::{debug, error, info, warn};

pub async fn insert_commit(
    pool: &PgPool,
    commit_hash: &str,
    repo_url: &str,
    commit_timestamp: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    let flake_id: (i32,) = sqlx::query_as("SELECT id FROM flakes WHERE repo_url = $1")
        .bind(repo_url)
        .fetch_optional(pool)
        .await?
        .context("No flake entry found")?;

    sqlx::query(
        "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp)
         VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(flake_id.0)
    .bind(commit_hash)
    .bind(commit_timestamp)
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
    let rows = sqlx::query_as!(
        Commit,
        r#"
        SELECT c.id, c.flake_id, c.git_commit_hash, c.commit_timestamp, c.attempt_count
        FROM commits c
        LEFT JOIN derivations d ON c.id = d.commit_id
        WHERE d.commit_id IS NULL
        AND c.attempt_count < 5
        ORDER BY c.commit_timestamp DESC
        "#
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
    let reset = sqlx::query!(
        r#"
        UPDATE commits
        SET evaluation_status = 'pending'
        WHERE evaluation_status = 'in_progress'
          AND evaluation_started_at < NOW() - INTERVAL '30 minutes'
        RETURNING id, git_commit_hash
        "#
    )
    .fetch_all(pool)
    .await?;

    if !reset.is_empty() {
        warn!("🧹 Reset {} stuck commit evaluations", reset.len());
        for row in &reset {
            info!("  - Commit {} ({})", row.id, row.git_commit_hash);
        }
    }

    Ok(())
}

/// Mark commit evaluation as started
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
    .await?;

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
pub async fn mark_commit_evaluation_failed(
    pool: &PgPool,
    commit_id: i32,
    error: &str,
) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE commits
        SET 
            evaluation_status = CASE 
                WHEN COALESCE(evaluation_attempt_count, 0) >= 5 THEN 'failed'
                ELSE 'pending'
            END,
            evaluation_error_message = $2
        WHERE id = $1
        "#,
        commit_id,
        error
    )
    .execute(pool)
    .await?;

    Ok(())
}

