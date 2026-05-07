

//! Database queries for evaluation logs.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// A single evaluation log entry from the database.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EvalLogRow {
    pub id: Uuid,
    pub commit_id: i32,
    pub log_timestamp: DateTime<Utc>,
    pub log_sequence: i32,
    pub log_level: Option<String>,
    pub log_message: String,
}

/// Insert a single evaluation log entry.
pub async fn insert_eval_log(
    pool: &PgPool,
    commit_id: i32,
    sequence: i32,
    level: Option<&str>,
    message: &str,
) -> Result<Uuid, sqlx::Error> {
    let rec = sqlx::query!(
        r#"
        INSERT INTO eval_logs (commit_id, log_sequence, log_level, log_message)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
        commit_id,
        sequence,
        level,
        message
    )
    .fetch_one(pool)
    .await?;

    Ok(rec.id)
}

/// Batch insert multiple evaluation log entries for performance.
pub async fn insert_eval_logs_batch(
    pool: &PgPool,
    commit_id: i32,
    logs: &[(i32, Option<String>, String)], // (sequence, level, message)
) -> Result<u64, sqlx::Error> {
    if logs.is_empty() {
        return Ok(0);
    }

    let mut sequences = Vec::with_capacity(logs.len());
    let mut levels: Vec<Option<&str>> = Vec::with_capacity(logs.len());
    let mut messages = Vec::with_capacity(logs.len());

    for (seq, lvl, msg) in logs {
        sequences.push(*seq);
        levels.push(lvl.as_deref());
        messages.push(msg.as_str());
    }

    let result = sqlx::query!(
        r#"
        INSERT INTO eval_logs (commit_id, log_sequence, log_level, log_message)
        SELECT $1, * FROM UNNEST($2::int[], $3::text[], $4::text[])
        ON CONFLICT (commit_id, log_sequence) DO NOTHING
        "#,
        commit_id,
        &sequences,
        &levels as &[Option<&str>],
        &messages as &[&str]
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

/// Fetch all evaluation logs for a commit, ordered by sequence.
pub async fn fetch_eval_logs_by_commit(
    pool: &PgPool,
    commit_id: i32,
) -> Result<Vec<EvalLogRow>, sqlx::Error> {
    sqlx::query_as!(
        EvalLogRow,
        r#"
        SELECT id, commit_id, log_timestamp, log_sequence, log_level, log_message
        FROM eval_logs
        WHERE commit_id = $1
        ORDER BY log_sequence ASC
        "#,
        commit_id
    )
    .fetch_all(pool)
    .await
}

/// Delete all evaluation logs for a specific commit.
pub async fn delete_eval_logs_by_commit(
    pool: &PgPool,
    commit_id: i32,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        DELETE FROM eval_logs WHERE commit_id = $1
        "#,
        commit_id
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

/// Delete evaluation logs older than a given date (for retention policy).
pub async fn delete_eval_logs_before(
    pool: &PgPool,
    before: DateTime<Utc>,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        DELETE FROM eval_logs WHERE log_timestamp < $1
        "#,
        before
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

/// Count total evaluation log entries across all commits.
pub async fn count_eval_logs(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let rec = sqlx::query!(
        r#"
        SELECT COUNT(*)::bigint as count FROM eval_logs
        "#
    )
    .fetch_one(pool)
    .await?;

    Ok(rec.count.unwrap_or(0))
}
