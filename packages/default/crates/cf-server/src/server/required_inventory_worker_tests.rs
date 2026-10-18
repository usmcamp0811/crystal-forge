//! Exercises the production worker while inventory and finalization wait in SQL.
//!
//! The runner must supply a mock-mode Crystal Forge configuration. The worker
//! reloads that configuration independently of CFState; these tests never mutate
//! process-global configuration or replace the production inventory deadline.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tokio::sync::Mutex;
use tokio::time::{Duration, timeout};
use uuid::Uuid;

use super::{commit_evaluation_limiter, process_pending_commits};
use crate::api::models::CancelEvalOutcome;
use crate::config::{CrystalForgeConfig, ExecutionMode, ServerConfig};
use crate::handlers::agent_request::CFState;
use crate::queries::commits::{
    EvalCancellationOutcome, cancel_commit_evaluation,
    finalize_requested_commit_evaluation_cancellation,
};
use crate::queries::evaluation_snapshots::lock_snapshot_writer_tx;
use crate::queue::QueueNotifier;
use crate::server::jobs::BackgroundJobRegistry;

// CONCURRENCY: This module is the only test caller of the production worker and
// its process-global limiter. Keep every case under this guard; future worker
// tests sharing that limiter must use the same guard rather than a local copy.
static WORKER_TEST_LOCK: Mutex<()> = Mutex::const_new(());

// The disposable database isolates this private gate from other SQLx tests.
// Unlike 440248, this gate is first acquired only inside the capture trigger.
const CAPTURE_GATE: i64 = 488_004_004;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    Cancel,
    Timeout,
    CancelFinalizerFailure,
}

async fn queued_commit(pool: &PgPool) -> i32 {
    let name = format!("inventory-worker-{}", Uuid::new_v4());
    let flake: i32 =
        sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id")
            .bind(&name)
            .bind(format!("file:///required-inventory-worker/{name}"))
            .fetch_one(pool)
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO systems (hostname, flake_id, system_configuration_name,
             public_key, derivation, is_active)
         VALUES ($1, $2, $1, 'test-only-key', '', TRUE)",
    )
    .bind(&name)
    .bind(flake)
    .execute(pool)
    .await
    .unwrap();
    // The commit insertion trigger creates the queued attempt. The worker must
    // claim it itself; this fixture does not imitate any worker state changes.
    sqlx::query_scalar(
        "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp,
             evaluation_status, cancellation_requested)
         VALUES ($1, $2, NOW(), 'pending', false) RETURNING id",
    )
    .bind(flake)
    .bind(Uuid::new_v4().simple().to_string())
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn install_barriers(pool: &PgPool, commit: i32, stop: Stop) {
    sqlx::raw_sql(&format!(
        "CREATE FUNCTION required_inventory_worker_capture_gate() RETURNS trigger
         LANGUAGE plpgsql AS $$ BEGIN
             IF NEW.commit_id = {commit} THEN
                 PERFORM pg_advisory_xact_lock({CAPTURE_GATE}::bigint);
             END IF;
             RETURN NEW;
         END; $$;
         CREATE TRIGGER required_inventory_worker_capture_gate
             BEFORE INSERT ON evaluation_policy_plans FOR EACH ROW
             EXECUTE FUNCTION required_inventory_worker_capture_gate();"
    ))
    .execute(pool)
    .await
    .unwrap();
    if stop == Stop::CancelFinalizerFailure {
        // Cancellation updates the attempt before the commit. Raising here
        // proves that both changes roll back when the production finalizer errs.
        sqlx::raw_sql(&format!(
            "CREATE FUNCTION required_inventory_worker_finalizer_failure() RETURNS trigger
             LANGUAGE plpgsql AS $$ BEGIN
                 IF NEW.id = {commit} AND NEW.evaluation_status = 'cancelled' THEN
                     RAISE EXCEPTION 'required inventory worker injected finalizer failure';
                 END IF;
                 RETURN NEW;
             END; $$;
             CREATE TRIGGER required_inventory_worker_finalizer_failure
                 BEFORE UPDATE ON commits FOR EACH ROW
                 EXECUTE FUNCTION required_inventory_worker_finalizer_failure();"
        ))
        .execute(pool)
        .await
        .unwrap();
    }
}

// A backend PID alone is insufficient: SQLx can reuse that connection for the
// finalizer after rollback. Include xact_start to distinguish transactions.
async fn wait_for_waiter(
    pool: &PgPool,
    blocker: i32,
    application: &str,
    previous: Option<(i32, DateTime<Utc>)>,
) -> (i32, DateTime<Utc>) {
    timeout(Duration::from_secs(5), async {
        loop {
            let waiters: Vec<(i32, DateTime<Utc>)> = sqlx::query_as(
                "SELECT pid, xact_start FROM pg_stat_activity
                 WHERE datname = current_database() AND application_name = $1
                   AND xact_start IS NOT NULL AND $2 = ANY(pg_blocking_pids(pid))",
            )
            .bind(application)
            .bind(blocker)
            .fetch_all(pool)
            .await
            .unwrap();
            if let Some(waiter) = waiters.into_iter().find(|waiter| Some(*waiter) != previous) {
                return waiter;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("actual worker transaction must reach the held PostgreSQL lock")
}

async fn assert_no_producer_work(pool: &PgPool, commit: i32) {
    let counts: (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
             (SELECT COUNT(*) FROM evaluation_attempt_inventory WHERE commit_id = $1),
             (SELECT COUNT(*) FROM evaluation_mock_outcomes WHERE commit_id = $1),
             (SELECT COUNT(*) FROM evaluation_native_outcomes WHERE commit_id = $1),
             (SELECT COUNT(*) FROM derivations WHERE commit_id = $1),
             (SELECT COUNT(*) FROM build_jobs j JOIN derivations d ON d.id = j.derivation_id
                 WHERE d.commit_id = $1),
             (SELECT COUNT(*) FROM eval_logs WHERE commit_id = $1
                 AND (log_message LIKE '⏳ %' OR log_message LIKE '🧪 MOCK MODE:%'))",
    )
    .bind(commit)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(counts, (0, 0, 0, 0, 0, 0));
}

async fn blocked_worker(pool: &PgPool, stop: Stop) {
    let commit = queued_commit(pool).await;
    install_barriers(pool, commit, stop).await;
    let application = format!("required-inventory-worker-{}", Uuid::new_v4());
    // One connection can remain occupied by the dropped inventory transaction
    // until PostgreSQL processes rollback. The other serves ownership checks and
    // then finalization. Acquiring both after completion proves pool reclamation.
    let worker_pool = PgPoolOptions::new()
        .max_connections(2)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(
            (*pool.connect_options())
                .clone()
                .application_name(&application),
        )
        .await
        .unwrap();
    let queue = Arc::new(QueueNotifier::new());
    let mut config = ServerConfig::default();
    config.execution_mode = ExecutionMode::Mock;
    let state = Arc::new(CFState::new(
        worker_pool.clone(),
        config,
        queue.clone(),
        BackgroundJobRegistry::new(),
    ));
    let mut capture_gate = pool.begin().await.unwrap();
    let capture_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *capture_gate)
        .await
        .unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(CAPTURE_GATE)
        .execute(&mut *capture_gate)
        .await
        .unwrap();
    let mut snapshot_gate = pool.begin().await.unwrap();
    let snapshot_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *snapshot_gate)
        .await
        .unwrap();
    let limiter = commit_evaluation_limiter();
    assert_eq!(limiter.available_permits(), 1);
    let worker = process_pending_commits(&worker_pool, &state, &queue);
    tokio::pin!(worker);
    let control = async {
        // Worker claim itself needs 440248. Wait until claim has finished and
        // capture is blocked on our private gate before taking the global lock.
        wait_for_waiter(pool, capture_pid, &application, None).await;
        lock_snapshot_writer_tx(&mut snapshot_gate).await.unwrap();
        capture_gate.rollback().await.unwrap();
        let inventory = wait_for_waiter(pool, snapshot_pid, &application, None).await;
        let captured: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM evaluation_policy_plans WHERE commit_id = $1)",
        )
        .bind(commit)
        .fetch_one(pool)
        .await
        .unwrap();
        assert!(
            captured,
            "capture must commit before the required inventory wait"
        );
        assert_eq!(limiter.available_permits(), 0);
        if stop != Stop::Timeout {
            assert_eq!(
                timeout(
                    Duration::from_secs(2),
                    cancel_commit_evaluation(pool, commit)
                )
                .await
                .expect("production cancellation request must not need 440248")
                .unwrap(),
                CancelEvalOutcome::CancellingInProgress,
            );
        }
        // These are the real process-wide worker permits, not a helper-local
        // semaphore. Timeout uses the unmodified production 30-second deadline.
        let permit = timeout(
            Duration::from_secs(if stop == Stop::Timeout { 35 } else { 5 }),
            limiter.clone().acquire_owned(),
        )
        .await
        .expect("evaluator must release its worker slot before blocked finalization")
        .unwrap();
        drop(permit);
        assert_eq!(limiter.available_permits(), 1);
        // Observe a different transaction, not merely the abandoned inventory
        // waiter. The production failure finalizer is now waiting on 440248.
        wait_for_waiter(pool, snapshot_pid, &application, Some(inventory)).await;
        assert_no_producer_work(pool, commit).await;
    };
    tokio::select! {
        biased;
        result = &mut worker => panic!("worker returned before blocked-finalizer proof: {result:?}"),
        () = control => {}
    }
    assert!(
        timeout(Duration::from_millis(100), &mut worker)
            .await
            .is_err(),
        "production worker must still be blocked with its evaluation slot free"
    );
    snapshot_gate.rollback().await.unwrap();
    let result = timeout(Duration::from_secs(10), &mut worker)
        .await
        .expect("worker must finish after releasing the snapshot writer");
    if stop == Stop::CancelFinalizerFailure {
        assert!(format!("{:#}", result.unwrap_err()).contains("injected finalizer failure"));
        let rolled_back: (String, String, bool) = sqlx::query_as(
            "SELECT c.evaluation_status, a.status, a.completed_at IS NULL
             FROM commits c JOIN evaluation_attempts a ON a.commit_id = c.id
             WHERE c.id = $1 AND a.attempt_number = c.evaluation_attempt_count",
        )
        .bind(commit)
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(
            rolled_back,
            ("cancelling".into(), "in_progress".into(), true)
        );
        sqlx::raw_sql(
            "DROP TRIGGER required_inventory_worker_finalizer_failure ON commits;
             DROP FUNCTION required_inventory_worker_finalizer_failure();",
        )
        .execute(pool)
        .await
        .unwrap();
        let attempt: i32 =
            sqlx::query_scalar("SELECT evaluation_attempt_count FROM commits WHERE id = $1")
                .bind(commit)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(
            finalize_requested_commit_evaluation_cancellation(pool, commit, attempt)
                .await
                .unwrap(),
            EvalCancellationOutcome::Cancelled,
        );
    } else {
        result.unwrap();
    }
    let terminal: (String, String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT c.evaluation_status, a.status, a.failure_class, a.error_message
         FROM commits c JOIN evaluation_attempts a ON a.commit_id = c.id
         WHERE c.id = $1 AND a.attempt_number = c.evaluation_attempt_count",
    )
    .bind(commit)
    .fetch_one(pool)
    .await
    .unwrap();
    if stop == Stop::Timeout {
        assert_eq!((&*terminal.0, &*terminal.1), ("failed", "failed"));
        assert_eq!(terminal.2.as_deref(), Some("transient"));
        assert!(
            terminal
                .3
                .unwrap()
                .contains("required inventory persistence timed out after 30000 ms")
        );
    } else {
        assert_eq!((&*terminal.0, &*terminal.1), ("cancelled", "cancelled"));
    }
    assert_eq!(limiter.available_permits(), 1);
    timeout(Duration::from_secs(5), async {
        let mut first = worker_pool.acquire().await.unwrap();
        let mut second = worker_pool.acquire().await.unwrap();
        sqlx::query("SELECT 1").execute(&mut *first).await.unwrap();
        sqlx::query("SELECT 1").execute(&mut *second).await.unwrap();
        // Both worker connections are returned from SQLx's rollback path. Take
        // the same advisory and row locks to detect any lingering transaction.
        let mut reclaimed = pool.begin().await.unwrap();
        lock_snapshot_writer_tx(&mut reclaimed).await.unwrap();
        sqlx::query("SELECT id FROM commits WHERE id = $1 FOR UPDATE NOWAIT")
            .bind(commit)
            .execute(&mut *reclaimed)
            .await
            .unwrap();
        sqlx::query("SELECT id FROM evaluation_attempts WHERE commit_id = $1 FOR UPDATE NOWAIT")
            .bind(commit)
            .execute(&mut *reclaimed)
            .await
            .unwrap();
        reclaimed.rollback().await.unwrap();
        assert_no_producer_work(pool, commit).await;
    })
    .await
    .expect("worker connections and all inventory/finalizer locks must be reclaimed");
    worker_pool.close().await;
    sqlx::raw_sql(
        "DROP TRIGGER required_inventory_worker_capture_gate ON evaluation_policy_plans;
         DROP FUNCTION required_inventory_worker_capture_gate();",
    )
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated PostgreSQL and runner-provided mock Crystal Forge config; real 30s deadline"]
async fn required_inventory_actual_worker_releases_permit_before_blocked_finalization(
    pool: PgPool,
) {
    let _serialized = WORKER_TEST_LOCK.lock().await;
    let config =
        CrystalForgeConfig::load().expect("runner must provide a mock worker configuration");
    assert!(
        config.get_server_config().execution_mode.is_mock(),
        "runner must configure server.execution_mode=mock; CFState alone does not select worker mode"
    );
    // Keep the timeout case terminal regardless of the sandbox's retry defaults.
    sqlx::query("UPDATE automatic_retry_policy SET max_evaluation_retries = 0 WHERE id = 1")
        .execute(&pool)
        .await
        .unwrap();
    for stop in [Stop::Cancel, Stop::Timeout, Stop::CancelFinalizerFailure] {
        timeout(Duration::from_secs(55), blocked_worker(&pool, stop))
            .await
            .unwrap_or_else(|_| {
                panic!("worker regression exceeded its diagnostic bound: {stop:?}")
            });
    }
}
