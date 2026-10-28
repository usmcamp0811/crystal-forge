//! Gates native and mock producers on bounded exact-attempt inventory storage.
//!
//! This module owns no worker permit and performs no failure finalization. Its
//! caller must propagate failure through the attempt lifecycle. The worker
//! permit bounds actual evaluation through its child-cleanup boundary, not
//! blocked database finalization. Database rollback can finish after this future
//! returns; child-owned heavy-Nix admission locks still fence pending cleanup.

use std::future::Future;

use anyhow::{Context, Result};
use sqlx::PgPool;
use tokio::time::{Duration, Instant};

use super::{
    DISCOVERY_CANCEL_POLL, DISCOVERY_CANCEL_QUERY_BUDGET, EvaluationCancelled, EvaluationSuperseded,
};
use crate::queries::evaluation_inventory::{
    InventoryExecutionMode, REQUIRED_INVENTORY_WRITE_BUDGET, persist_inventory,
};

/// Persists required inventory before native admission or mock stage execution.
///
/// One 30-second deadline covers pool acquisition, all transaction locks,
/// insertion, comparison, and COMMIT acknowledgement. Ownership/cancellation
/// checks run concurrently, at most two seconds apart after each bounded 500 ms
/// check. Success requires another bounded 500 ms check after persistence, so
/// total foreground time is at most 30 seconds plus that final check budget
/// (subject to executor scheduling). No writer task is spawned.
///
/// Timeout drops the borrowed write future. SQLx queues transaction rollback;
/// return does not prove immediate connection/lock reclamation or row absence.
/// Transaction-local database lock/statement limits of at most 31 seconds allow
/// a blocked write to end and rollback to run while the external blocker stays
/// held. Each limit preserves a stricter existing nonzero database/role/session
/// timeout; disabled limits receive the 31-second ceiling.
/// Those limits apply per statement, not to the whole write. Connection reuse
/// still requires database response delivery and executor progress.
/// A missing COMMIT acknowledgement is failure even if PostgreSQL committed.
/// Callers MUST propagate errors and MUST NOT admit a producer on failure.
///
/// # Errors
/// Returns typed cancellation, supersession, and inventory conflict errors
/// without conversion to warning/success. Returns database or bounded-check
/// failures and an operation-specific diagnostic on the write deadline.
///
/// # Examples
/// For native discovery of `web-server`, persist that exact discovered and
/// selected name before attempting evaluator admission. If storage times out,
/// propagate the error without starting the evaluator. The same gate precedes
/// simulated stages for mock inventory.
pub(super) async fn persist_required_inventory(
    pool: &PgPool,
    commit_id: i32,
    expected_attempt: i32,
    discovered: &[String],
    selected: &[String],
    mode: InventoryExecutionMode,
) -> Result<()> {
    required_write(
        persist_inventory(
            pool,
            commit_id,
            expected_attempt,
            discovered,
            selected,
            mode,
        ),
        || check_owner(pool, commit_id, expected_attempt),
        REQUIRED_INVENTORY_WRITE_BUDGET,
    )
    .await
}

// Ownership takes precedence over cancellation, including a newer queued retry.
// This is a nonlocking observation: persist_inventory remains the authoritative
// transactional owner check. A later launch check is still required at spawn.
async fn check_owner(pool: &PgPool, commit_id: i32, expected_attempt: i32) -> Result<()> {
    let state: Option<(Option<String>, Option<i32>, Option<bool>, bool)> = sqlx::query_as(
        "SELECT c.evaluation_status, c.evaluation_attempt_count, c.cancellation_requested,
             EXISTS (SELECT 1 FROM evaluation_attempts a
                 WHERE a.commit_id = c.id AND a.attempt_number = $2
                   AND a.status = 'in_progress'
                   AND NOT EXISTS (SELECT 1 FROM evaluation_attempts newer
                       WHERE newer.commit_id = c.id AND newer.attempt_number > $2))
         FROM commits c WHERE c.id = $1",
    )
    .bind(commit_id)
    .bind(expected_attempt)
    .fetch_optional(pool)
    .await?;
    match state {
        Some((status, attempt, cancelled, true))
            if attempt == Some(expected_attempt)
                && matches!(status.as_deref(), Some("in_progress" | "cancelling")) =>
        {
            if cancelled == Some(true) || status.as_deref() == Some("cancelling") {
                Err(EvaluationCancelled.into())
            } else {
                Ok(())
            }
        }
        _ => Err(EvaluationSuperseded.into()),
    }
}

async fn bounded_check<Check, CheckFuture>(check: &mut Check) -> Result<()>
where
    Check: FnMut() -> CheckFuture,
    CheckFuture: Future<Output = Result<()>>,
{
    tokio::time::timeout(DISCOVERY_CANCEL_QUERY_BUDGET, check())
        .await
        .context("required inventory persistence ownership/cancellation check timed out")?
}

// CONCURRENCY: Both futures are borrowed. Returning from select drops the
// transaction future immediately; no cancelled waiter can later resume writing.
// Unlike optional discovery, check failures must stop the mandatory gate.
async fn required_write<Check, CheckFuture>(
    write: impl Future<Output = Result<()>>,
    mut check: Check,
    budget: Duration,
) -> Result<()>
where
    Check: FnMut() -> CheckFuture,
    CheckFuture: Future<Output = Result<()>>,
{
    let deadline = Instant::now() + budget;
    let write = tokio::time::timeout_at(deadline, write);
    tokio::pin!(write);
    let result = {
        let monitor = async {
            loop {
                if let Err(error) = bounded_check(&mut check).await {
                    return Err::<(), _>(error);
                }
                tokio::time::sleep(DISCOVERY_CANCEL_POLL).await;
            }
        };
        tokio::pin!(monitor);
        tokio::select! {
            biased;
            result = &mut monitor => result,
            result = &mut write => result.with_context(|| format!(
                "required inventory persistence timed out after {} ms (whole transaction including COMMIT acknowledgement)",
                budget.as_millis(),
            ))?,
        }
    };
    // Preserve the original typed persistence failure. No final check can turn
    // an error (including an uncertain COMMIT) into permission to start work.
    result?;
    bounded_check(&mut check).await
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::queries::evaluation_inventory::{
        InventoryConflict, REQUIRED_INVENTORY_CLEANUP_GRACE,
    };
    use crate::queries::evaluation_snapshots::{SNAPSHOT_WRITER_LOCK_KEY, lock_snapshot_writer_tx};

    #[tokio::test]
    async fn required_inventory_deadline_drops_writer_without_admission() {
        struct Dropped<'a>(&'a Cell<bool>);
        impl Drop for Dropped<'_> {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }
        for _mode in [InventoryExecutionMode::Native, InventoryExecutionMode::Mock] {
            let dropped = Cell::new(false);
            let guard = Dropped(&dropped);
            let admitted = Cell::new(false);
            let result: Result<()> = async {
                required_write(
                    async move {
                        let _guard = guard;
                        std::future::pending::<Result<()>>().await
                    },
                    || async { Ok(()) },
                    Duration::from_millis(10),
                )
                .await?;
                admitted.set(true);
                Ok(())
            }
            .await;
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("required inventory persistence timed out after 10 ms")
            );
            assert!(dropped.get(), "writer future must be dropped before return");
            assert!(!admitted.get());
        }
    }

    #[tokio::test]
    async fn required_inventory_preserves_failures_and_blocks_both_producer_gates() {
        // The counter models only the caller's `await?` gate, not worker permits.
        for _mode in [InventoryExecutionMode::Native, InventoryExecutionMode::Mock] {
            for error in [
                anyhow::Error::new(EvaluationCancelled),
                anyhow::Error::new(EvaluationSuperseded),
                anyhow::Error::new(InventoryConflict),
                anyhow::anyhow!("COMMIT acknowledgement lost"),
            ] {
                let cancelled = error.is::<EvaluationCancelled>();
                let superseded = error.is::<EvaluationSuperseded>();
                let conflict = error.is::<InventoryConflict>();
                let admitted = Cell::new(0);
                let result: Result<()> = async {
                    required_write(
                        async { Err(error) },
                        || async { Ok(()) },
                        Duration::from_secs(1),
                    )
                    .await?;
                    admitted.set(admitted.get() + 1);
                    Ok(())
                }
                .await;
                let error = result.unwrap_err();
                assert_eq!(error.is::<EvaluationCancelled>(), cancelled);
                assert_eq!(error.is::<EvaluationSuperseded>(), superseded);
                assert_eq!(error.is::<InventoryConflict>(), conflict);
                assert_eq!(admitted.get(), 0);
            }
        }
    }

    #[tokio::test]
    async fn required_inventory_final_check_fails_closed() {
        for _mode in [InventoryExecutionMode::Native, InventoryExecutionMode::Mock] {
            for final_failure in 0..3 {
                let checks = Cell::new(0);
                let admitted = Cell::new(0);
                let result: Result<()> = async {
                    required_write(
                        async { Ok(()) },
                        || {
                            let final_check = checks.replace(checks.get() + 1) > 0;
                            async move {
                                if !final_check {
                                    return Ok(());
                                }
                                match final_failure {
                                    0 => Err(EvaluationCancelled.into()),
                                    1 => Err(EvaluationSuperseded.into()),
                                    _ => std::future::pending().await,
                                }
                            }
                        },
                        Duration::from_secs(1),
                    )
                    .await?;
                    admitted.set(1);
                    Ok(())
                }
                .await;
                let error = result.unwrap_err();
                assert_eq!(error.is::<EvaluationCancelled>(), final_failure == 0);
                assert_eq!(error.is::<EvaluationSuperseded>(), final_failure == 1);
                if final_failure == 2 {
                    assert!(
                        error
                            .to_string()
                            .contains("ownership/cancellation check timed out")
                    );
                }
                assert_eq!(admitted.get(), 0);
            }
        }
    }

    async fn active_commit(pool: &PgPool) -> i32 {
        let flake: i32 =
            sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id")
                .bind(format!("required-inventory-{}", uuid::Uuid::new_v4()))
                .bind(format!("https://example.invalid/{}", uuid::Uuid::new_v4()))
                .fetch_one(pool)
                .await
                .unwrap();
        let commit: i32 = sqlx::query_scalar(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp,
                 evaluation_status, evaluation_attempt_count, cancellation_requested)
             VALUES ($1, $2, NOW(), 'in_progress', 7, false) RETURNING id",
        )
        .bind(flake)
        .bind(uuid::Uuid::new_v4().to_string())
        .fetch_one(pool)
        .await
        .unwrap();
        // The insertion trigger creates the queued attempt; claim that exact row.
        sqlx::query(
            "UPDATE evaluation_attempts SET attempt_number = 7, status = 'in_progress',
                 started_at = NOW() WHERE commit_id = $1",
        )
        .bind(commit)
        .execute(pool)
        .await
        .unwrap();
        commit
    }

    async fn wait_for_blocked_writer(pool: &PgPool, blocker_pid: i32) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let waiting: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM pg_stat_activity
                     WHERE datname = current_database()
                       AND $1 = ANY(pg_blocking_pids(pid)))",
                )
                .bind(blocker_pid)
                .fetch_one(pool)
                .await
                .unwrap();
                if waiting {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("writer must reach the held PostgreSQL lock");
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Stop {
        Cancel,
        Supersede,
        Timeout,
    }

    // Run with one writer connection so reacquisition proves that the dropped
    // SQLx transaction has finished rollback, not merely that another connection
    // is available. Monitoring uses the independent migration-backed test pool.
    async fn blocked_write(pool: &PgPool, lock: &str, stop: Stop, mode: InventoryExecutionMode) {
        let commit = active_commit(pool).await;
        let writer_pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect_with((*pool.connect_options()).clone())
            .await
            .unwrap();
        let mut blocker = pool.begin().await.unwrap();
        let blocker_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *blocker)
            .await
            .unwrap();
        match lock {
            "snapshot" => lock_snapshot_writer_tx(&mut blocker).await.unwrap(),
            "commit" => {
                sqlx::query("SELECT id FROM commits WHERE id = $1 FOR UPDATE")
                    .bind(commit)
                    .execute(&mut *blocker)
                    .await
                    .unwrap();
            }
            "attempt" => {
                sqlx::query("SELECT id FROM evaluation_attempts WHERE commit_id = $1 FOR UPDATE")
                    .bind(commit)
                    .execute(&mut *blocker)
                    .await
                    .unwrap();
            }
            _ => unreachable!(),
        }
        let discovered = vec![" exact ".to_owned(), "A".to_owned(), "a".to_owned()];
        let admitted = Cell::new(0);
        let operation = async {
            let write = persist_inventory(&writer_pool, commit, 7, &discovered, &discovered, mode);
            let result: Result<()> = async {
                // Use the same production gate with the real write. The
                // outer test bound is only a deadlock diagnostic.
                required_write(
                    write,
                    || check_owner(pool, commit, 7),
                    if stop == Stop::Timeout {
                        Duration::from_secs(3)
                    } else {
                        Duration::from_secs(10)
                    },
                )
                .await?;
                admitted.set(1);
                Ok(())
            }
            .await;
            result
        };
        let control = async {
            wait_for_blocked_writer(pool, blocker_pid).await;
            if stop == Stop::Cancel {
                sqlx::query("UPDATE commits SET cancellation_requested = true WHERE id = $1")
                    .bind(commit)
                    .execute(pool)
                    .await
                    .unwrap();
            } else if stop == Stop::Supersede {
                sqlx::query(
                    "UPDATE evaluation_attempts SET status = 'failed' WHERE commit_id = $1",
                )
                .bind(commit)
                .execute(pool)
                .await
                .unwrap();
                sqlx::query("INSERT INTO evaluation_attempts (commit_id, attempt_number, status) VALUES ($1, 8, 'queued')")
                    .bind(commit).execute(pool).await.unwrap();
            }
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(12), async {
            tokio::join!(operation, control)
        })
        .await
        .expect("blocked write must return within its bound");
        let error = result.unwrap_err();
        assert_eq!(error.is::<EvaluationCancelled>(), stop == Stop::Cancel);
        assert_eq!(error.is::<EvaluationSuperseded>(), stop == Stop::Supersede);
        if stop == Stop::Timeout {
            assert!(
                error
                    .to_string()
                    .contains("required inventory persistence timed out after 3000 ms")
            );
        }
        assert_eq!(admitted.get(), 0);
        if stop == Stop::Cancel {
            sqlx::query("UPDATE commits SET cancellation_requested = false WHERE id = $1")
                .bind(commit)
                .execute(pool)
                .await
                .unwrap();
        }
        blocker.rollback().await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            let mut reclaimed = writer_pool.begin().await.unwrap();
            lock_snapshot_writer_tx(&mut reclaimed).await.unwrap();
            sqlx::query("SELECT id FROM commits WHERE id = $1 FOR UPDATE")
                .bind(commit).execute(&mut *reclaimed).await.unwrap();
            sqlx::query("SELECT id FROM evaluation_attempts WHERE commit_id = $1 FOR UPDATE")
                .bind(commit).execute(&mut *reclaimed).await.unwrap();
            let absent: bool = sqlx::query_scalar(
                "SELECT NOT EXISTS (SELECT 1 FROM evaluation_attempt_inventory WHERE commit_id = $1)",
            ).bind(commit).fetch_one(&mut *reclaimed).await.unwrap();
            assert!(absent, "blocked pre-insert writes must not insert later");
            reclaimed.rollback().await.unwrap();
        }).await.expect("connection and transaction locks must eventually be reclaimed");
        writer_pool.close().await;
        sqlx::query("UPDATE commits SET evaluation_status = 'failed' WHERE id = $1")
            .bind(commit)
            .execute(pool)
            .await
            .unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn required_inventory_snapshot_wait_cancel_reclaims_connection(pool: PgPool) {
        for mode in [InventoryExecutionMode::Native, InventoryExecutionMode::Mock] {
            blocked_write(&pool, "snapshot", Stop::Cancel, mode).await;
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn required_inventory_snapshot_wait_superseded_reclaims_connection(pool: PgPool) {
        for mode in [InventoryExecutionMode::Native, InventoryExecutionMode::Mock] {
            blocked_write(&pool, "snapshot", Stop::Supersede, mode).await;
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn required_inventory_lock_wait_timeout_reclaims_connection(pool: PgPool) {
        for lock in ["snapshot", "commit", "attempt"] {
            for mode in [InventoryExecutionMode::Native, InventoryExecutionMode::Mock] {
                blocked_write(&pool, lock, Stop::Timeout, mode).await;
            }
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn required_inventory_preserves_stricter_database_timeouts(pool: PgPool) {
        for mode in [InventoryExecutionMode::Native, InventoryExecutionMode::Mock] {
            for (lock_timeout, statement_timeout, expected_code) in
                [("500ms", "0", "55P03"), ("0", "500ms", "57014")]
            {
                let commit = active_commit(&pool).await;
                // Configure the writer session before any transaction. This
                // models an existing database/role policy independently for
                // each setting, including a disabled counterpart.
                let writer_pool = sqlx::postgres::PgPoolOptions::new()
                    .max_connections(1)
                    .after_connect(move |connection, _| {
                        Box::pin(async move {
                            sqlx::query(
                                "SELECT set_config('lock_timeout', $1, false),
                                     set_config('statement_timeout', $2, false)",
                            )
                            .bind(lock_timeout)
                            .bind(statement_timeout)
                            .execute(connection)
                            .await?;
                            Ok(())
                        })
                    })
                    .connect_with((*pool.connect_options()).clone())
                    .await
                    .unwrap();
                let original: (String, String) = sqlx::query_as(
                    "SELECT current_setting('lock_timeout'), current_setting('statement_timeout')",
                )
                .fetch_one(&writer_pool)
                .await
                .unwrap();
                let mut blocker = pool.begin().await.unwrap();
                lock_snapshot_writer_tx(&mut blocker).await.unwrap();
                let blocker_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                    .fetch_one(&mut *blocker)
                    .await
                    .unwrap();
                let admitted = Cell::new(false);
                let names = vec![" exact ".to_owned(), "A".to_owned(), "a".to_owned()];
                // This bound includes failure, rollback, reuse, and assertions.
                // Four 100 ms database-timeout cases need no 30-second wait.
                tokio::time::timeout(Duration::from_secs(2), async {
                    let operation = async {
                        let result: Result<()> = async {
                            required_write(
                                persist_inventory(&writer_pool, commit, 7, &names, &names, mode),
                                || check_owner(&pool, commit, 7),
                                REQUIRED_INVENTORY_WRITE_BUDGET,
                            )
                            .await?;
                            admitted.set(true);
                            Ok(())
                        }
                        .await;
                        result
                    };
                    let (result, ()) = tokio::join!(
                        operation,
                        wait_for_blocked_writer(&pool, blocker_pid),
                    );
                    let error = result.unwrap_err();
                    let database_error = error
                        .downcast_ref::<sqlx::Error>()
                        .and_then(|error| error.as_database_error())
                        .expect("the existing short database timeout must abort the statement");
                    assert_eq!(database_error.code().as_deref(), Some(expected_code));
                    assert!(!admitted.get());

                    // The real statement has aborted; SQLx must process rollback
                    // and return the only connection while `blocker` stays held.
                    let mut reclaimed = writer_pool.acquire().await.unwrap();
                    let restored: (String, String) = sqlx::query_as(
                        "SELECT current_setting('lock_timeout'), current_setting('statement_timeout')",
                    )
                    .fetch_one(&mut *reclaimed)
                    .await
                    .unwrap();
                    assert_eq!(restored, original);
                    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1)")
                        .bind(SNAPSHOT_WRITER_LOCK_KEY)
                        .fetch_one(&mut *reclaimed)
                        .await
                        .unwrap();
                    assert!(!acquired, "external global writer must still hold its lock");
                    let absent: bool = sqlx::query_scalar(
                        "SELECT NOT EXISTS (SELECT 1 FROM evaluation_attempt_inventory WHERE commit_id = $1)",
                    )
                    .bind(commit)
                    .fetch_one(&mut *reclaimed)
                    .await
                    .unwrap();
                    assert!(absent, "database timeout must not publish inventory");
                })
                .await
                .expect("existing 500 ms database policy must fail and reclaim within two seconds");

                blocker.rollback().await.unwrap();
                tokio::time::timeout(Duration::from_secs(2), async {
                    let mut reclaimed = writer_pool.begin().await.unwrap();
                    lock_snapshot_writer_tx(&mut reclaimed).await.unwrap();
                    let absent: bool = sqlx::query_scalar(
                        "SELECT NOT EXISTS (SELECT 1 FROM evaluation_attempt_inventory WHERE commit_id = $1)",
                    )
                    .bind(commit)
                    .fetch_one(&mut *reclaimed)
                    .await
                    .unwrap();
                    assert!(absent, "no writer may insert after the blocker is released");
                    reclaimed.rollback().await.unwrap();
                })
                .await
                .expect("writer lock must be reclaimable after blocker release");
                writer_pool.close().await;
                sqlx::query("UPDATE commits SET evaluation_status = 'failed' WHERE id = $1")
                    .bind(commit)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL; waits for real 31-second database timeouts"]
    async fn required_inventory_reclaims_connection_while_snapshot_blocker_stays_held(
        pool: PgPool,
    ) {
        let database_budget = REQUIRED_INVENTORY_WRITE_BUDGET + REQUIRED_INVENTORY_CLEANUP_GRACE;
        for mode in [InventoryExecutionMode::Native, InventoryExecutionMode::Mock] {
            let commit = active_commit(&pool).await;
            // Acquisition must allow the real server-side statement timeout to
            // fire. The final grace allows response delivery and queued rollback;
            // it is test acknowledgement time, not additional write permission.
            let cleanup_bound = database_budget + REQUIRED_INVENTORY_CLEANUP_GRACE;
            let writer_pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(1)
                .acquire_timeout(cleanup_bound)
                .connect_with((*pool.connect_options()).clone())
                .await
                .unwrap();
            let (writer_pid, original_lock_timeout, original_statement_timeout): (
                i32,
                String,
                String,
            ) = sqlx::query_as(
                "SELECT pg_backend_pid(), current_setting('lock_timeout'),
                         current_setting('statement_timeout')",
            )
            .fetch_one(&writer_pool)
            .await
            .unwrap();
            let mut blocker = pool.begin().await.unwrap();
            lock_snapshot_writer_tx(&mut blocker).await.unwrap();
            let blocker_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                .fetch_one(&mut *blocker)
                .await
                .unwrap();
            let observed_at = Cell::new(None);
            let admitted = Cell::new(false);
            let names = vec![" exact ".to_owned(), "A".to_owned(), "a".to_owned()];
            let operation = async {
                let result: Result<()> = async {
                    required_write(
                        persist_inventory(&writer_pool, commit, 7, &names, &names, mode),
                        || check_owner(&pool, commit, 7),
                        if mode == InventoryExecutionMode::Native {
                            REQUIRED_INVENTORY_WRITE_BUDGET
                        } else {
                            Duration::from_secs(3)
                        },
                    )
                    .await?;
                    admitted.set(true);
                    Ok(())
                }
                .await;
                result
            };
            let control = async {
                // An observed PostgreSQL waiter is the barrier. No sleep is
                // used to infer that the write reached its advisory lock.
                wait_for_blocked_writer(&pool, blocker_pid).await;
                observed_at.set(Some(Instant::now()));
                if mode == InventoryExecutionMode::Native {
                    sqlx::query("UPDATE commits SET cancellation_requested = true WHERE id = $1")
                        .bind(commit)
                        .execute(&pool)
                        .await
                        .unwrap();
                }
            };
            let (result, ()) = tokio::time::timeout(Duration::from_secs(10), async {
                tokio::join!(operation, control)
            })
            .await
            .expect("client cancellation or short foreground timeout must return promptly");
            let error = result.unwrap_err();
            if mode == InventoryExecutionMode::Native {
                assert!(error.is::<EvaluationCancelled>());
            } else {
                assert!(
                    error
                        .to_string()
                        .contains("required inventory persistence timed out after 3000 ms")
                );
            }
            assert!(!admitted.get());
            let cleanup_deadline =
                observed_at.get().expect("writer waiter was observed") + cleanup_bound;
            tokio::time::timeout_at(cleanup_deadline, async {
                // Keep `blocker` alive throughout reclamation. With only one
                // writer connection, acquisition plus a successful query proves
                // that the abandoned statement no longer strands the pool.
                let mut reclaimed = writer_pool.acquire().await.unwrap();
                let (lock_timeout, statement_timeout): (String, String) = sqlx::query_as(
                    "SELECT current_setting('lock_timeout'), current_setting('statement_timeout')",
                )
                .fetch_one(&mut *reclaimed)
                .await
                .unwrap();
                assert_eq!(lock_timeout, original_lock_timeout);
                assert_eq!(statement_timeout, original_statement_timeout);
                let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1)")
                    .bind(SNAPSHOT_WRITER_LOCK_KEY)
                    .fetch_one(&mut *reclaimed)
                    .await
                    .unwrap();
                assert!(!acquired, "external global writer must still hold its lock");
                let absent: bool = sqlx::query_scalar(
                    "SELECT NOT EXISTS (SELECT 1 FROM evaluation_attempt_inventory WHERE commit_id = $1)",
                )
                .bind(commit)
                .fetch_one(&mut *reclaimed)
                .await
                .unwrap();
                assert!(absent, "timed-out pre-insert statement must not publish inventory");
                let waiting: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM pg_locks WHERE pid = $1 AND NOT granted)",
                )
                .bind(writer_pid)
                .fetch_one(&pool)
                .await
                .unwrap();
                assert!(!waiting, "original writer must no longer wait on the external blocker");
            })
            .await
            .expect("writer connection must be reusable before releasing the external blocker");

            // Only after the held-blocker proof may the external transaction
            // end. Verify rollback/lock reclamation and no late insertion again.
            blocker.rollback().await.unwrap();
            tokio::time::timeout(Duration::from_secs(5), async {
                let mut reclaimed = writer_pool.begin().await.unwrap();
                lock_snapshot_writer_tx(&mut reclaimed).await.unwrap();
                let absent: bool = sqlx::query_scalar(
                    "SELECT NOT EXISTS (SELECT 1 FROM evaluation_attempt_inventory WHERE commit_id = $1)",
                )
                .bind(commit)
                .fetch_one(&mut *reclaimed)
                .await
                .unwrap();
                assert!(absent, "no abandoned writer may insert after blocker release");
                reclaimed.rollback().await.unwrap();
            })
            .await
            .expect("writer advisory lock must be reclaimable after rollback");
            writer_pool.close().await;
            sqlx::query("UPDATE commits SET evaluation_status = 'failed' WHERE id = $1")
                .bind(commit)
                .execute(&pool)
                .await
                .unwrap();
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires isolated migration-backed PostgreSQL"]
    async fn required_inventory_exact_owner_success_and_conflict(pool: PgPool) {
        let commit = active_commit(&pool).await;
        let names = vec![" exact ".to_owned(), "A".to_owned(), "a".to_owned()];
        persist_required_inventory(
            &pool,
            commit,
            7,
            &names,
            &names,
            InventoryExecutionMode::Native,
        )
        .await
        .unwrap();
        let actual: (i32, Vec<String>, Vec<String>, String) = sqlx::query_as(
            "SELECT attempt_number, configuration_names, selected_configuration_names, execution_mode
             FROM evaluation_attempt_inventory WHERE commit_id = $1",
        ).bind(commit).fetch_one(&pool).await.unwrap();
        assert_eq!(actual, (7, names.clone(), names.clone(), "native".into()));
        let error = persist_required_inventory(
            &pool,
            commit,
            7,
            &names,
            &names,
            InventoryExecutionMode::Mock,
        )
        .await
        .unwrap_err();
        assert!(error.is::<InventoryConflict>());
        let error = persist_required_inventory(
            &pool,
            commit,
            6,
            &names,
            &names,
            InventoryExecutionMode::Native,
        )
        .await
        .unwrap_err();
        assert!(error.is::<EvaluationSuperseded>());
        sqlx::query("UPDATE commits SET evaluation_status = 'failed' WHERE id = $1")
            .bind(commit)
            .execute(&pool)
            .await
            .unwrap();
        let mock_commit = active_commit(&pool).await;
        persist_required_inventory(
            &pool,
            mock_commit,
            7,
            &names,
            &names,
            InventoryExecutionMode::Mock,
        )
        .await
        .unwrap();
        let actual: (i32, Vec<String>, Vec<String>, String) = sqlx::query_as(
            "SELECT attempt_number, configuration_names, selected_configuration_names, execution_mode
             FROM evaluation_attempt_inventory WHERE commit_id = $1",
        ).bind(mock_commit).fetch_one(&pool).await.unwrap();
        assert_eq!(actual, (7, names.clone(), names, "mock".into()));
    }
}
