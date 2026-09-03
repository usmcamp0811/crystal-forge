use crate::config::CrystalForgeConfig;
use crate::queries::cve_scans::{
    CreateCveScanOutcome, acknowledge_revoked_cve_scan_execution, acquire_execution_lock,
    create_cve_scan, get_active_scan_for_derivation, heartbeat_cve_scan_execution,
    mark_cve_scan_failed_by_id_for_execution, mark_cve_scan_failed_for_execution,
    release_execution_lock_or_close, save_scan_results_for_execution,
};
use crate::queries::derivations::get_derivation_by_id;
use crate::vulnix::vulnix_runner::VulnixRunner;
use axum::async_trait;
use sqlx::PgPool;
use tracing::error;
use uuid::Uuid;

#[async_trait]
trait ImmediateCveScanRunner: Send + Sync + 'static {
    async fn scan_derivation(
        &self,
        pool: &PgPool,
        derivation_id: i32,
        vulnix_version: Option<String>,
    ) -> anyhow::Result<crate::vulnix::vulnix_parser::VulnixScanOutput>;
}

#[async_trait]
impl ImmediateCveScanRunner for VulnixRunner {
    async fn scan_derivation(
        &self,
        pool: &PgPool,
        derivation_id: i32,
        vulnix_version: Option<String>,
    ) -> anyhow::Result<crate::vulnix::vulnix_parser::VulnixScanOutput> {
        VulnixRunner::scan_derivation(self, pool, derivation_id, vulnix_version).await
    }
}

/// Domain errors for CVE scan operations.
///
/// Handlers match on this enum rather than inspecting error message strings,
/// so the API contract is stable even if log/display text changes.
#[derive(Debug)]
pub enum CveScanError {
    /// vulnix is not installed or not reachable on this node.
    VulnixUnavailable,
    /// Any other unexpected failure (DB error, spawn error, etc.).
    Internal(anyhow::Error),
}

impl std::fmt::Display for CveScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CveScanError::VulnixUnavailable => {
                write!(
                    f,
                    "vulnix is not available on this node; immediate scan cannot start"
                )
            }
            CveScanError::Internal(err) => write!(f, "{err:#}"),
        }
    }
}

impl From<anyhow::Error> for CveScanError {
    fn from(err: anyhow::Error) -> Self {
        CveScanError::Internal(err)
    }
}

pub async fn trigger_immediate_cve_scan(
    pool: PgPool,
    derivation_id: i32,
) -> Result<Uuid, CveScanError> {
    if let Some(existing_scan_id) = get_active_scan_for_derivation(&pool, derivation_id)
        .await
        .map_err(CveScanError::Internal)?
    {
        return Ok(existing_scan_id);
    }

    if !VulnixRunner::check_vulnix_available().await {
        return Err(CveScanError::VulnixUnavailable);
    }

    let vulnix_version = VulnixRunner::get_vulnix_version().await.ok();
    trigger_immediate_cve_scan_with_runner(
        pool,
        derivation_id,
        vulnix_version,
        || {
            let cfg = CrystalForgeConfig::load().unwrap_or_else(|_| CrystalForgeConfig::default());
            VulnixRunner::with_config(cfg.get_vulnix_config())
        },
        std::time::Duration::from_secs(30),
        None,
    )
    .await
}

/// Outcome of the immediate scan supervision loop, used to select the correct
/// post-loop cleanup: normal completion already persisted its own terminal
/// state, an ownership loss must be acknowledged as revoked, and a heartbeat
/// failure must be marked failed generically.
enum ImmediateScanOutcome {
    /// The scanner future resolved on its own. Terminal persistence (success
    /// or scanner-reported failure) already happened inside it.
    Completed(anyhow::Result<()>),
    /// A heartbeat observed that this execution's lease was revoked by stale
    /// recovery. The scanner future was already dropped/cancelled.
    Revoked,
    /// The heartbeat query itself failed or timed out. The scanner future
    /// was already dropped/cancelled.
    HeartbeatFailed(anyhow::Error),
}

async fn trigger_immediate_cve_scan_with_runner<R, F>(
    pool: PgPool,
    derivation_id: i32,
    vulnix_version: Option<String>,
    runner_factory: F,
    heartbeat_interval: std::time::Duration,
    revocation_ack_gate: Option<std::sync::Arc<tokio::sync::Semaphore>>,
) -> Result<Uuid, CveScanError>
where
    R: ImmediateCveScanRunner,
    F: FnOnce() -> R + Send + 'static,
{
    let scan_claim = create_cve_scan(&pool, derivation_id, "vulnix", vulnix_version.clone())
        .await
        .map_err(CveScanError::Internal)?;
    let claim = match scan_claim {
        CreateCveScanOutcome::Created(claim) => claim,
        CreateCveScanOutcome::Existing(scan_id) => return Ok(scan_id),
    };
    let scan_id = claim.scan_id;

    // Acquire a dedicated PostgreSQL session that holds the execution advisory
    // lock for the entire scanner lifetime. Worker-initiated scans already use
    // this fencing via `execute_scan`; an immediate scan is a full token-owned
    // execution too (token + heartbeat) and must carry the same live-lock
    // fencing so stale recovery can distinguish a paused live process from a
    // crashed one. The lock is released — or the connection is discarded if
    // release cannot be confirmed — before the spawned task below ends, so a
    // pooled connection can never return to general use while still owning it.
    let mut lock_conn = match pool.acquire().await {
        Ok(conn) => conn,
        Err(e) => {
            let err = anyhow::Error::new(e)
                .context("Failed to acquire connection for immediate scan execution lock");
            if let Err(mark_err) = mark_cve_scan_failed_by_id_for_execution(
                &pool,
                scan_id,
                derivation_id,
                &err.to_string(),
                claim.execution_id,
            )
            .await
            {
                error!(
                    "Failed to mark immediate CVE scan {scan_id} as failed after lock setup error: {mark_err:#}"
                );
            }
            return Err(CveScanError::Internal(err));
        }
    };
    if let Err(e) = acquire_execution_lock(&mut lock_conn, claim.execution_id).await {
        let err = e.context("Failed to acquire session-level execution lock for immediate scan");
        // The lock state after a failed acquisition attempt is uncertain;
        // never return this connection to the pool.
        if let Err(close_err) = lock_conn.close().await {
            error!(
                "Failed to close immediate CVE scan lock connection for {}: {close_err:#}",
                claim.execution_id
            );
        }
        if let Err(mark_err) = mark_cve_scan_failed_by_id_for_execution(
            &pool,
            scan_id,
            derivation_id,
            &err.to_string(),
            claim.execution_id,
        )
        .await
        {
            error!(
                "Failed to mark immediate CVE scan {scan_id} as failed after lock setup error: {mark_err:#}"
            );
        }
        return Err(CveScanError::Internal(err));
    }

    // CONCURRENCY: Recovery may revoke the claim before this path obtains its
    // session lock. Confirm ownership while the lock is held and before the
    // scanner future or runner is constructed.
    match heartbeat_cve_scan_execution(&pool, scan_id, claim.execution_id).await {
        Ok(true) => {}
        Ok(false) => {
            release_execution_lock_or_close(lock_conn, claim.execution_id).await;
            let _ =
                acknowledge_revoked_cve_scan_execution(&pool, scan_id, claim.execution_id).await;
            return Ok(scan_id);
        }
        Err(err) => {
            release_execution_lock_or_close(lock_conn, claim.execution_id).await;
            let handoff_error =
                err.context("Failed immediate CVE scan execution handoff heartbeat");
            if let Err(cleanup_err) = mark_cve_scan_failed_by_id_for_execution(
                &pool,
                scan_id,
                derivation_id,
                &handoff_error.to_string(),
                claim.execution_id,
            )
            .await
            {
                error!(
                    "Failed to mark immediate CVE scan {scan_id} as failed after execution handoff error: {cleanup_err:#}"
                );
            }
            return Err(CveScanError::Internal(handoff_error));
        }
    }

    let spawn_pool = pool.clone();
    tokio::spawn(async move {
        let execution = async {
            let derivation = get_derivation_by_id(&spawn_pool, derivation_id).await?;
            let runner = runner_factory();

            let started = std::time::Instant::now();

            match runner
                .scan_derivation(&spawn_pool, derivation_id, vulnix_version)
                .await
            {
                Ok(vulnix_entries) => {
                    let scan_duration_ms = Some(started.elapsed().as_millis() as i32);
                    if let Err(err) = save_scan_results_for_execution(
                        &spawn_pool,
                        scan_id,
                        &vulnix_entries,
                        scan_duration_ms,
                        claim.execution_id,
                    )
                    .await
                    {
                        return Err(err);
                    }
                }
                Err(err) => {
                    mark_cve_scan_failed_for_execution(
                        &spawn_pool,
                        scan_id,
                        &derivation,
                        &err.to_string(),
                        claim.execution_id,
                    )
                    .await?;
                }
            }

            Ok(())
        };

        let mut execution = Box::pin(execution);
        let start = tokio::time::Instant::now() + heartbeat_interval;
        let mut heartbeat = tokio::time::interval_at(start, heartbeat_interval);
        const HEARTBEAT_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
        let outcome = loop {
            tokio::select! {
                biased;
                result = &mut execution => break ImmediateScanOutcome::Completed(result),
                _ = heartbeat.tick() => {
                    match tokio::time::timeout(
                        HEARTBEAT_QUERY_TIMEOUT,
                        heartbeat_cve_scan_execution(
                            &spawn_pool,
                            scan_id,
                            claim.execution_id,
                        ),
                    )
                    .await
                    {
                        Ok(Ok(true)) => {}
                        Ok(Ok(false)) => {
                            // Cancel the scanner before this loop exits; the
                            // lock release and revocation acknowledgment both
                            // happen after the loop, once the future is gone.
                            drop(execution);
                            break ImmediateScanOutcome::Revoked;
                        }
                        Ok(Err(err)) => {
                            drop(execution);
                            break ImmediateScanOutcome::HeartbeatFailed(err.context(
                                "Failed to refresh immediate CVE scan execution heartbeat",
                            ));
                        }
                        Err(_) => {
                            drop(execution);
                            break ImmediateScanOutcome::HeartbeatFailed(anyhow::anyhow!(
                                "Timed out refreshing immediate CVE scan {scan_id} execution heartbeat"
                            ));
                        }
                    }
                }
            }
        };

        // CONCURRENCY: The scanner future is fully resolved, dropped, or
        // cancelled — never still running — before the lock is released, so a
        // replacement execution can never run concurrently with this one
        // while both would otherwise share the same derivation's active-scan
        // slot.
        release_execution_lock_or_close(lock_conn, claim.execution_id).await;

        match outcome {
            ImmediateScanOutcome::Completed(Ok(())) => {}
            ImmediateScanOutcome::Completed(Err(err)) => {
                if let Err(mark_err) = mark_cve_scan_failed_by_id_for_execution(
                    &spawn_pool,
                    scan_id,
                    derivation_id,
                    &err.to_string(),
                    claim.execution_id,
                )
                .await
                {
                    error!(
                        "Failed to mark immediate CVE scan {scan_id} as failed after setup error: {mark_err:#}"
                    );
                }
                error!("Immediate CVE scan task failed for derivation {derivation_id}: {err:#}");
            }
            ImmediateScanOutcome::Revoked => {
                // Production passes no gate. Tests use one to inspect the
                // revoked row after cancellation but before acknowledgment.
                if let Some(gate) = revocation_ack_gate {
                    let permit = gate
                        .acquire()
                        .await
                        .expect("immediate revocation acknowledgment gate closed");
                    permit.forget();
                }
                if !acknowledge_revoked_cve_scan_execution(&spawn_pool, scan_id, claim.execution_id)
                    .await
                    .unwrap_or(false)
                {
                    error!(
                        "CVE scan {scan_id} lost execution ownership and could not be acknowledged as revoked"
                    );
                }
            }
            ImmediateScanOutcome::HeartbeatFailed(err) => {
                if let Err(mark_err) = mark_cve_scan_failed_by_id_for_execution(
                    &spawn_pool,
                    scan_id,
                    derivation_id,
                    &err.to_string(),
                    claim.execution_id,
                )
                .await
                {
                    error!(
                        "Failed to mark immediate CVE scan {scan_id} as failed after heartbeat error: {mark_err:#}"
                    );
                }
                error!("Immediate CVE scan task failed for derivation {derivation_id}: {err:#}");
            }
        }
    });

    Ok(scan_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queries::cve_scans::{recover_stale_scans, save_scan_results_for_execution};
    use crate::queries::derivations::insert_derivation;
    use anyhow::Context;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::sync::Semaphore;

    #[derive(Clone)]
    struct BlockingRunner {
        calls: Arc<AtomicUsize>,
        active: Arc<AtomicUsize>,
        max_active: Arc<AtomicUsize>,
        cancelled: Arc<AtomicUsize>,
        release: Arc<Semaphore>,
    }

    struct ActiveCallGuard {
        active: Arc<AtomicUsize>,
        cancelled: Arc<AtomicUsize>,
        completed: bool,
    }

    impl Drop for ActiveCallGuard {
        fn drop(&mut self) {
            self.active.fetch_sub(1, Ordering::SeqCst);
            if !self.completed {
                self.cancelled.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    #[async_trait]
    impl ImmediateCveScanRunner for BlockingRunner {
        async fn scan_derivation(
            &self,
            _pool: &PgPool,
            _derivation_id: i32,
            _vulnix_version: Option<String>,
        ) -> anyhow::Result<crate::vulnix::vulnix_parser::VulnixScanOutput> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(active, Ordering::SeqCst);
            let mut guard = ActiveCallGuard {
                active: self.active.clone(),
                cancelled: self.cancelled.clone(),
                completed: false,
            };

            let permit = self
                .release
                .acquire()
                .await
                .context("blocking immediate runner semaphore closed")?;
            permit.forget();
            guard.completed = true;
            Ok(vec![])
        }
    }

    async fn wait_for_counter(counter: &AtomicUsize, expected: usize, description: &str) {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while counter.load(Ordering::SeqCst) < expected {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {description}"));
    }

    async fn wait_for_status(pool: &PgPool, scan_id: Uuid, expected: &str) {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let status: String =
                    sqlx::query_scalar("SELECT status FROM cve_scans WHERE id = $1")
                        .bind(scan_id)
                        .fetch_one(pool)
                        .await
                        .expect("immediate scan status should resolve");
                if status == expected {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for immediate scan status {expected}"));
    }

    #[tokio::test]
    async fn immediate_scan_renews_ownership_reuses_active_and_cancels_on_revocation() {
        let Ok(database_url) = std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL") else {
            return;
        };
        let pool = PgPool::connect(&database_url)
            .await
            .expect("dedicated CVE test database should be reachable");
        let recovery_pool = PgPool::connect(&database_url)
            .await
            .expect("independent stale-recovery pool should connect");
        let derivation = insert_derivation(
            &pool,
            None,
            &format!("task-325-immediate-lease-{}", Uuid::new_v4().simple()),
            "nixos",
        )
        .await
        .expect("immediate lease derivation should be inserted");
        let runner = BlockingRunner {
            calls: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicUsize::new(0)),
            max_active: Arc::new(AtomicUsize::new(0)),
            cancelled: Arc::new(AtomicUsize::new(0)),
            release: Arc::new(Semaphore::new(0)),
        };
        let heartbeat_interval = std::time::Duration::from_millis(50);
        let revocation_ack_gate = Arc::new(Semaphore::new(0));

        let first_runner = runner.clone();
        let first_scan_id = trigger_immediate_cve_scan_with_runner(
            pool.clone(),
            derivation.id,
            Some("test".to_string()),
            move || first_runner,
            heartbeat_interval,
            Some(revocation_ack_gate.clone()),
        )
        .await
        .expect("first immediate scan should start");
        wait_for_counter(&runner.calls, 1, "first immediate scanner invocation").await;

        let (execution_id, initial_heartbeat): (Uuid, chrono::DateTime<chrono::Utc>) =
            sqlx::query_as(
                r#"
                SELECT
                    (scan_metadata ->> 'execution_id')::uuid,
                    (scan_metadata ->> 'execution_heartbeat_at')::timestamptz
                FROM cve_scans
                WHERE id = $1 AND status = 'in_progress'
                "#,
            )
            .bind(first_scan_id)
            .fetch_one(&pool)
            .await
            .expect("immediate execution should store lease metadata");

        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let heartbeat: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
                    "SELECT (scan_metadata ->> 'execution_heartbeat_at')::timestamptz FROM cve_scans WHERE id = $1",
                )
                .bind(first_scan_id)
                .fetch_one(&pool)
                .await
                .expect("immediate heartbeat should resolve");
                if heartbeat > initial_heartbeat {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("immediate execution should renew its heartbeat");

        sqlx::query(
            r#"
            UPDATE cve_scans
            SET scan_metadata = scan_metadata || jsonb_build_object(
                'execution_started_at', NOW() - INTERVAL '2 hours',
                'execution_heartbeat_at', NOW() - INTERVAL '2 hours'
            )
            WHERE id = $1 AND scan_metadata ->> 'execution_id' = $2::uuid::text
            "#,
        )
        .bind(first_scan_id)
        .bind(execution_id)
        .execute(&pool)
        .await
        .expect("immediate execution should be artificially aged");

        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let heartbeat: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
                    "SELECT (scan_metadata ->> 'execution_heartbeat_at')::timestamptz FROM cve_scans WHERE id = $1",
                )
                .bind(first_scan_id)
                .fetch_one(&pool)
                .await
                .expect("refreshed immediate heartbeat should resolve");
                if heartbeat > chrono::Utc::now() - chrono::Duration::minutes(1) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("immediate execution should renew its artificially aged lease");
        assert_eq!(
            recover_stale_scans(&recovery_pool, std::time::Duration::from_secs(1800))
                .await
                .expect("healthy immediate recovery check should succeed"),
            0
        );

        let duplicate_scan_id = trigger_immediate_cve_scan(pool.clone(), derivation.id)
            .await
            .expect("duplicate immediate request should reuse the active scan");
        assert_eq!(duplicate_scan_id, first_scan_id);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(runner.calls.load(Ordering::SeqCst), 1);

        sqlx::query(
            r#"
            UPDATE cve_scans
            SET scan_metadata = scan_metadata || jsonb_build_object(
                'execution_revoked_at', NOW(),
                'stale_recovery_reason', 'test-immediate-revocation'
            )
            WHERE id = $1
              AND status = 'in_progress'
              AND scan_metadata ->> 'execution_id' = $2::uuid::text
            "#,
        )
        .bind(first_scan_id)
        .bind(execution_id)
        .execute(&recovery_pool)
        .await
        .expect("immediate execution should be revoked");
        wait_for_counter(&runner.cancelled, 1, "immediate scanner cancellation").await;
        assert_eq!(runner.active.load(Ordering::SeqCst), 0);
        let (status, revoked): (String, bool) = sqlx::query_as(
            "SELECT status, scan_metadata ? 'execution_revoked_at' FROM cve_scans WHERE id = $1",
        )
        .bind(first_scan_id)
        .fetch_one(&pool)
        .await
        .expect("cancelled immediate scan should remain active before acknowledgment");
        assert_eq!(status, "in_progress");
        assert!(revoked);
        let empty_results = vec![];
        assert!(
            save_scan_results_for_execution(
                &pool,
                first_scan_id,
                &empty_results,
                Some(1),
                execution_id,
            )
            .await
            .is_err(),
            "revoked immediate owner must not persist results"
        );
        revocation_ack_gate.add_permits(1);
        wait_for_status(&pool, first_scan_id, "failed").await;

        let replacement_runner = runner.clone();
        let replacement_scan_id = trigger_immediate_cve_scan_with_runner(
            pool.clone(),
            derivation.id,
            Some("test".to_string()),
            move || replacement_runner,
            heartbeat_interval,
            None,
        )
        .await
        .expect("replacement immediate scan should start");
        assert_ne!(replacement_scan_id, first_scan_id);
        wait_for_counter(&runner.calls, 2, "replacement immediate scanner invocation").await;
        assert_eq!(runner.max_active.load(Ordering::SeqCst), 1);
        runner.release.add_permits(1);
        wait_for_status(&pool, replacement_scan_id, "completed").await;

        sqlx::query("DELETE FROM cve_scans WHERE derivation_id = $1")
            .bind(derivation.id)
            .execute(&pool)
            .await
            .expect("immediate lease scans should be deleted");
        sqlx::query("DELETE FROM derivations WHERE id = $1")
            .bind(derivation.id)
            .execute(&pool)
            .await
            .expect("immediate lease derivation should be deleted");
    }

    /// Proves that an immediate scan holds the same live PostgreSQL execution
    /// advisory lock that worker-initiated scans hold, using the real lock
    /// rather than a mock: while the blocking runner is active, stale
    /// recovery must treat the scan as a live paused process and do nothing,
    /// exactly like [`super::super::builder::cve_worker`]'s worker path.
    /// Once the scanner completes and the supervision task exits, the lock
    /// must be observably released.
    #[tokio::test]
    async fn immediate_scan_holds_execution_lock_until_terminal_release() {
        use crate::queries::cve_scans::execution_lock_is_held;

        let Ok(database_url) = std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL") else {
            return;
        };
        let pool = PgPool::connect(&database_url)
            .await
            .expect("dedicated CVE test database should be reachable");
        let recovery_pool = PgPool::connect(&database_url)
            .await
            .expect("independent stale-recovery pool should connect");
        let derivation = insert_derivation(
            &pool,
            None,
            &format!("task-325-immediate-lock-{}", Uuid::new_v4().simple()),
            "nixos",
        )
        .await
        .expect("immediate lock derivation should be inserted");
        let runner = BlockingRunner {
            calls: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicUsize::new(0)),
            max_active: Arc::new(AtomicUsize::new(0)),
            cancelled: Arc::new(AtomicUsize::new(0)),
            release: Arc::new(Semaphore::new(0)),
        };

        // 1. Trigger an immediate scan with a blocking fake runner.
        let scan_id = trigger_immediate_cve_scan_with_runner(
            pool.clone(),
            derivation.id,
            Some("test".to_string()),
            {
                let runner = runner.clone();
                move || runner
            },
            std::time::Duration::from_secs(30),
            None,
        )
        .await
        .expect("immediate scan should start");

        // 2. Wait until the scanner has actually started.
        wait_for_counter(&runner.calls, 1, "immediate scanner invocation").await;

        // 3. Read the execution_id the claim stored.
        let execution_id: Uuid = sqlx::query_scalar(
            "SELECT (scan_metadata ->> 'execution_id')::uuid FROM cve_scans WHERE id = $1",
        )
        .bind(scan_id)
        .fetch_one(&pool)
        .await
        .expect("immediate execution id should resolve");

        // 4. The dedicated lock connection must hold a real advisory lock.
        assert!(
            execution_lock_is_held(&pool, execution_id)
                .await
                .expect("lock status should be queryable"),
            "an active immediate scan must hold its execution advisory lock"
        );

        // 5. Age the heartbeat past the stale threshold.
        sqlx::query(
            r#"
            UPDATE cve_scans
            SET scan_metadata = scan_metadata || jsonb_build_object(
                'execution_heartbeat_at', NOW() - INTERVAL '2 hours'
            )
            WHERE id = $1
            "#,
        )
        .bind(scan_id)
        .execute(&pool)
        .await
        .expect("immediate heartbeat should be aged");

        // 6/7. Stale recovery must treat this as a live paused process and
        // do nothing, because the lock is still held by the blocking runner.
        let recovered = recover_stale_scans(&recovery_pool, std::time::Duration::from_secs(1800))
            .await
            .expect("recovery attempt while the immediate lock is held should succeed");
        assert_eq!(
            recovered, 0,
            "recovery must not act on a live immediate scan whose lock is still held"
        );
        let (status, revoked): (String, bool) = sqlx::query_as(
            "SELECT status, scan_metadata ? 'execution_revoked_at' FROM cve_scans WHERE id = $1",
        )
        .bind(scan_id)
        .fetch_one(&pool)
        .await
        .expect("immediate scan state should be queryable");
        assert_eq!(
            status, "in_progress",
            "a live immediate scan must remain in_progress while its lock is held"
        );
        assert!(
            !revoked,
            "a live immediate scan must not be revoked while its lock is held"
        );

        // 8. Release the blocking fake runner so the scan completes.
        runner.release.add_permits(1);

        // 9. Wait for the terminal scan state.
        wait_for_status(&pool, scan_id, "completed").await;

        // 10. The lock must now be observably released.
        assert!(
            !execution_lock_is_held(&pool, execution_id)
                .await
                .expect("lock status should be queryable after completion"),
            "the execution advisory lock must be released once the immediate scan completes"
        );

        // 11. The fake runner must have no residual active invocations.
        assert_eq!(
            runner.active.load(Ordering::SeqCst),
            0,
            "no scanner invocation should remain active after completion"
        );

        sqlx::query("DELETE FROM cve_scans WHERE derivation_id = $1")
            .bind(derivation.id)
            .execute(&pool)
            .await
            .expect("immediate lock scans should be deleted");
        sqlx::query("DELETE FROM derivations WHERE id = $1")
            .bind(derivation.id)
            .execute(&pool)
            .await
            .expect("immediate lock derivation should be deleted");
    }
}
