//! Creates fresh recovery attempts without replaying captured public plans.

use anyhow::{Result, ensure};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

const INTERRUPTION_REASON: &str = "Server interruption: active evaluation stopped before completion; queued a fresh evaluation attempt";

/// Identifies the commit and exact active owner observed at recovery entry.
#[derive(sqlx::FromRow)]
pub(super) struct InterruptedSource {
    commit_id: i32,
    git_commit_hash: String,
    attempt_number: i32,
    attempt_id: Option<Uuid>,
}

/// Snapshots sources before waiting for writer locks.
///
/// A concurrent startup must validate this source UUID, never reinterpret a
/// newly started child as the same interruption. Queued work is excluded even
/// when a legacy commit status disagrees with its active attempt.
///
/// # Errors
/// Returns PostgreSQL errors when the entry snapshot cannot be read.
pub(super) async fn interrupted_sources(pool: &PgPool) -> Result<Vec<InterruptedSource>> {
    Ok(sqlx::query_as(
        "SELECT c.id AS commit_id, c.git_commit_hash,
                COALESCE(c.evaluation_attempt_count, 0) AS attempt_number, a.id AS attempt_id
         FROM commits c LEFT JOIN evaluation_attempts a
           ON a.commit_id = c.id AND a.attempt_number = c.evaluation_attempt_count
             AND a.status = 'in_progress'
          WHERE c.evaluation_status IN ('in_progress', 'cancelling')
            AND NOT EXISTS (SELECT 1 FROM evaluation_attempts queued
                WHERE queued.commit_id = c.id AND queued.status = 'queued')
         ORDER BY c.id",
    )
    .fetch_all(pool)
    .await?)
}

/// Validates exact sources and returns recovered and cancelled commit counts.
///
/// CONCURRENCY: Keeps the writer -> queue -> commit -> attempt lock order.
/// The caller MUST roll back on error and commit the whole batch on success.
/// The active-attempt index is an additional invariant guard. Source evidence
/// and failure retry budgets remain unchanged.
///
/// # Errors
/// Returns PostgreSQL errors, exhausted attempt-number errors, or an error if
/// insertion does not create exactly one child. No partial transition may commit.
pub(super) async fn recover_sources_tx(
    tx: &mut Transaction<'_, Postgres>,
    sources: &[InterruptedSource],
) -> Result<(usize, usize)> {
    crate::queries::evaluation_snapshots::lock_snapshot_writer_tx(tx).await?;
    super::lock_eval_queue_order_tx(tx).await?;
    let mut recovered = 0;
    let mut cancelled = 0;
    for source in sources {
        let state: Option<(String, bool)> = sqlx::query_as(
            "SELECT evaluation_status, COALESCE(cancellation_requested, FALSE)
             FROM commits WHERE id = $1 AND git_commit_hash = $2
               AND COALESCE(evaluation_attempt_count, 0) = $3
               AND evaluation_status IN ('in_progress', 'cancelling')
               AND NOT EXISTS (SELECT 1 FROM evaluation_attempts queued
                   WHERE queued.commit_id = commits.id AND queued.status = 'queued')
             FOR UPDATE",
        )
        .bind(source.commit_id)
        .bind(&source.git_commit_hash)
        .bind(source.attempt_number)
        .fetch_optional(&mut **tx)
        .await?;
        let Some((status, cancellation_requested)) = state else {
            continue;
        };
        let attempt: Option<(Uuid, Option<Uuid>, i32)> = sqlx::query_as(
            "SELECT a.id, a.root_attempt_id, a.automatic_retry_count
             FROM evaluation_attempts a
             WHERE a.id = $1 AND a.commit_id = $2 AND a.attempt_number = $3
               AND a.status = 'in_progress'
               AND NOT EXISTS (SELECT 1 FROM evaluation_attempts newer
                   WHERE newer.commit_id = a.commit_id
                     AND newer.attempt_number > a.attempt_number)
             FOR UPDATE OF a",
        )
        .bind(source.attempt_id)
        .bind(source.commit_id)
        .bind(source.attempt_number)
        .fetch_optional(&mut **tx)
        .await?;
        if status == "cancelling" || cancellation_requested {
            // Preserve startup's cancellation finalization, including legacy
            // cancelling commits without an active attempt. Never queue a child.
            if let Some((id, _, _)) = attempt {
                sqlx::query(
                    "UPDATE evaluation_attempts SET status = 'cancelled',
                         completed_at = COALESCE(completed_at, NOW()), updated_at = NOW()
                     WHERE id = $1",
                )
                .bind(id)
                .execute(&mut **tx)
                .await?;
            }
            sqlx::query(
                "UPDATE commits SET evaluation_status = 'cancelled',
                     evaluation_completed_at = COALESCE(evaluation_completed_at, NOW()),
                     cancellation_requested = FALSE WHERE id = $1",
            )
            .bind(source.commit_id)
            .execute(&mut **tx)
            .await?;
            cancelled += 1;
            continue;
        }
        let Some((id, root, retry_count)) = attempt else {
            continue;
        };
        let next_number = source
            .attempt_number
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("evaluation attempt number exhausted"))?;
        // COMPATIBILITY: 0190 permits 'unknown', not a new interruption class.
        // This is not a confirmed Nix failure. Do not run the general failure
        // finalizer: it rewrites checks and may consume an automatic retry.
        sqlx::query(
            "UPDATE evaluation_attempts SET status = 'failed',
                 completed_at = COALESCE(completed_at, NOW()), error_message = $2,
                 failure_class = 'unknown', updated_at = NOW() WHERE id = $1",
        )
        .bind(id)
        .bind(INTERRUPTION_REASON)
        .execute(&mut **tx)
        .await?;
        // INVARIANT: Recovery preserves the failure budget and existing root.
        // No automatic_retry_source_id is assigned: this is not a failure retry.
        // No evidence is copied; normal claim/loader/discovery own the new UUID.
        let inserted = sqlx::query(
            "INSERT INTO evaluation_attempts (commit_id, parent_attempt_id,
                 root_attempt_id, attempt_number, automatic_retry_count)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(source.commit_id)
        .bind(id)
        .bind(root.unwrap_or(id))
        .bind(next_number)
        .bind(retry_count)
        .execute(&mut **tx)
        .await?;
        ensure!(inserted.rows_affected() == 1, "missing recovery successor");
        // Retain the historical start until the next canonical claim replaces
        // it. Queue position is unchanged, matching prior startup recovery.
        sqlx::query(
            "UPDATE commits SET evaluation_status = 'pending',
                 evaluation_completed_at = NULL, evaluation_error_message = NULL
             WHERE id = $1",
        )
        .bind(source.commit_id)
        .execute(&mut **tx)
        .await?;
        recovered += 1;
    }
    Ok((recovered, cancelled))
}
