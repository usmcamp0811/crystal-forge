//! Database queries for evaluation logs.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Identifies the exact durable attempt that produced a log entry.
///
/// The composite foreign key validates all three fields. Callers must obtain
/// this identity from the claimed attempt, never from a publication-time latest
/// lookup that could relabel an old worker's diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct EvalLogOwner {
    /// Commit evaluated by the producer.
    pub commit_id: i32,
    /// Immutable attempt UUID.
    pub evaluation_attempt_id: Uuid,
    /// Positive attempt number belonging to the UUID and commit.
    pub attempt_number: i32,
}

/// Resolves the claimed expected attempt while serializing with lifecycle writes.
///
/// The caller retains the transaction until any registry binding is complete.
/// Lock order is database commit, attempt, then in-memory registry. No registry
/// lock may be held when this function is called.
///
/// # Errors
/// Returns SQL errors or `RowNotFound` for a missing, inactive, or superseded
/// claim. A queued successor never grants publication authority to its parent.
pub(crate) async fn claimed_owner_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    commit_id: i32,
    expected_attempt: i32,
) -> Result<EvalLogOwner, sqlx::Error> {
    sqlx::query(
        "SELECT id FROM commits WHERE id = $1
           AND evaluation_attempt_count = $2 AND evaluation_status = 'in_progress'
           AND NOT COALESCE(cancellation_requested, false) FOR UPDATE",
    )
    .bind(commit_id)
    .bind(expected_attempt)
    .fetch_one(&mut **tx)
    .await?;
    sqlx::query_as(
        "SELECT commit_id, id AS evaluation_attempt_id, attempt_number
         FROM evaluation_attempts a
         WHERE commit_id = $1 AND attempt_number = $2 AND status = 'in_progress'
           AND NOT EXISTS (SELECT 1 FROM evaluation_attempts newer
               WHERE newer.commit_id = a.commit_id
                 AND newer.attempt_number > a.attempt_number)
         FOR SHARE",
    )
    .bind(commit_id)
    .bind(expected_attempt)
    .fetch_one(&mut **tx)
    .await
}

// CONCURRENCY: Lifecycle writers lock the commit before creating successors.
// Hold that same lock until insertion commits, so a stale batch cannot pass a
// check in one snapshot and insert after a retry has acquired ownership.
async fn lock_log_owner(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    owner: EvalLogOwner,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT id FROM commits WHERE id = $1 FOR UPDATE")
        .bind(owner.commit_id)
        .fetch_one(&mut **tx)
        .await?;
    sqlx::query(
        "SELECT id FROM evaluation_attempts a
         WHERE id = $1 AND commit_id = $2 AND attempt_number = $3
           AND status <> 'queued'
           AND NOT EXISTS (SELECT 1 FROM evaluation_attempts newer
               WHERE newer.commit_id = a.commit_id
                 AND newer.attempt_number > a.attempt_number)
         FOR SHARE",
    )
    .bind(owner.evaluation_attempt_id)
    .bind(owner.commit_id)
    .bind(owner.attempt_number)
    .fetch_one(&mut **tx)
    .await?;
    Ok(())
}

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

/// Inserts one entry for the exact current producer owner.
///
/// Terminal diagnostics remain writable until a successor exists. No write
/// clears older history or infers ownership for unattributed legacy rows.
///
/// # Errors
/// Returns `RowNotFound` for stale/foreign/queued ownership, or SQL errors.
///
/// # Examples
/// ```no_run
/// # async fn append(pool: &sqlx::PgPool,
/// #     owner: crystal_forge::queries::eval_logs::EvalLogOwner)
/// #     -> Result<(), sqlx::Error> {
/// crystal_forge::queries::eval_logs::insert_eval_log(
///     pool, owner, 1, Some("info"), "Evaluation started",
/// ).await?;
/// # Ok(()) }
/// ```
pub async fn insert_eval_log(
    pool: &PgPool,
    owner: EvalLogOwner,
    sequence: i32,
    level: Option<&str>,
    message: &str,
) -> Result<Uuid, sqlx::Error> {
    let mut tx = pool.begin().await?;
    lock_log_owner(&mut tx, owner).await?;
    let rec = sqlx::query(
        r#"
        INSERT INTO eval_logs (commit_id, log_sequence, log_level, log_message,
                               evaluation_attempt_id, attempt_number)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id
        "#,
    )
    .bind(owner.commit_id)
    .bind(sequence)
    .bind(level)
    .bind(message)
    .bind(owner.evaluation_attempt_id)
    .bind(owner.attempt_number)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    rec.try_get("id")
}

/// Inserts a batch atomically under exact current attempt ownership.
///
/// Duplicate sequences within the same attempt are ignored. A stale batch is
/// rejected as a whole; it cannot replace or append to a successor's history.
/// Empty batches return zero without database access.
///
/// # Errors
/// Returns `RowNotFound` for stale/foreign/queued ownership, or SQL errors.
///
/// # Examples
/// ```no_run
/// # async fn flush(pool: &sqlx::PgPool,
/// #     owner: crystal_forge::queries::eval_logs::EvalLogOwner)
/// #     -> Result<(), sqlx::Error> {
/// let lines = vec![(1, Some("debug".into()), "Evaluator output".into())];
/// crystal_forge::queries::eval_logs::insert_eval_logs_batch(
///     pool, owner, &lines,
/// ).await?;
/// # Ok(()) }
/// ```
pub async fn insert_eval_logs_batch(
    pool: &PgPool,
    owner: EvalLogOwner,
    logs: &[(i32, Option<String>, String)], // (sequence, level, message)
) -> Result<u64, sqlx::Error> {
    if logs.is_empty() {
        return Ok(0);
    }
    let mut tx = pool.begin().await?;
    lock_log_owner(&mut tx, owner).await?;

    let mut sequences = Vec::with_capacity(logs.len());
    let mut levels: Vec<Option<&str>> = Vec::with_capacity(logs.len());
    let mut messages = Vec::with_capacity(logs.len());

    for (seq, lvl, msg) in logs {
        sequences.push(*seq);
        levels.push(lvl.as_deref());
        messages.push(msg.as_str());
    }

    let result = sqlx::query(
        r#"
        INSERT INTO eval_logs (commit_id, log_sequence, log_level, log_message,
                               evaluation_attempt_id, attempt_number)
        SELECT $1, logs.*, $5, $6
        FROM UNNEST($2::int[], $3::text[], $4::text[]) AS logs
        ON CONFLICT (evaluation_attempt_id, log_sequence)
            WHERE evaluation_attempt_id IS NOT NULL DO NOTHING
        "#,
    )
    .bind(owner.commit_id)
    .bind(&sequences)
    .bind(&levels as &[Option<&str>])
    .bind(&messages as &[&str])
    .bind(owner.evaluation_attempt_id)
    .bind(owner.attempt_number)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(result.rows_affected())
}

/// Fetches raw storage across all attempts, including unattributed legacy rows.
///
/// This administrative storage helper does not authorize evidence and MUST NOT
/// serve a latest-attempt HTTP response. Use the authorized transaction reader
/// for that boundary. Sequences are unique only within an owner.
///
/// # Errors
/// Returns database read errors.
pub async fn fetch_eval_logs_by_commit(
    pool: &PgPool,
    commit_id: i32,
) -> Result<Vec<EvalLogRow>, sqlx::Error> {
    sqlx::query_as::<_, EvalLogRow>(
        r#"
        SELECT id, commit_id, log_timestamp, log_sequence, log_level, log_message
        FROM eval_logs
        WHERE commit_id = $1
        ORDER BY log_sequence ASC
        "#,
    )
    .bind(commit_id)
    .fetch_all(pool)
    .await
}

/// Reads exact-owner logs through the caller's authorized read transaction.
///
/// The caller MUST authorize whole-flake access before calling this function.
/// Keeping the same repeatable-read snapshot prevents membership revocation or
/// system moves from separating authorization and evidence. Unattributed rows
/// are retained in storage but excluded, including for administrators.
///
/// # Errors
/// Returns database read errors without retrying in another snapshot.
pub(crate) async fn fetch_eval_logs_by_commit_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    owner: EvalLogOwner,
) -> Result<Vec<EvalLogRow>, sqlx::Error> {
    sqlx::query_as::<_, EvalLogRow>(
        "SELECT id, commit_id, log_timestamp, log_sequence, log_level, log_message
         FROM eval_logs WHERE commit_id = $1 AND evaluation_attempt_id = $2
           AND attempt_number = $3 ORDER BY log_sequence ASC",
    )
    .bind(owner.commit_id)
    .bind(owner.evaluation_attempt_id)
    .bind(owner.attempt_number)
    .fetch_all(&mut **tx)
    .await
}

/// Delete all evaluation logs for a specific commit.
pub async fn delete_eval_logs_by_commit(pool: &PgPool, commit_id: i32) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        DELETE FROM eval_logs WHERE commit_id = $1
        "#,
    )
    .bind(commit_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

/// Delete evaluation logs older than a given date (for retention policy).
pub async fn delete_eval_logs_before(
    pool: &PgPool,
    before: DateTime<Utc>,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        DELETE FROM eval_logs WHERE log_timestamp < $1
        "#,
    )
    .bind(before)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

/// Count total evaluation log entries across all commits.
pub async fn count_eval_logs(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let rec = sqlx::query(
        r#"
        SELECT COUNT(*)::bigint as count FROM eval_logs
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(rec.try_get::<Option<i64>, _>("count")?.unwrap_or(0))
}
