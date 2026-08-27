use crate::config::CrystalForgeConfig;
use crate::queries::cve_scans::{
    CreateCveScanOutcome, acknowledge_revoked_cve_scan_execution, create_cve_scan,
    get_active_scan_for_derivation, heartbeat_cve_scan_execution,
    mark_cve_scan_failed_by_id_for_execution, mark_cve_scan_failed_for_execution,
    save_scan_results_for_execution,
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
        let result = loop {
            tokio::select! {
                biased;
                result = &mut execution => break result,
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
                            drop(execution);
                            // Production passes no gate. Tests use one to inspect
                            // the revoked row after cancellation but before acknowledgment.
                            if let Some(gate) = revocation_ack_gate {
                                let permit = gate
                                    .acquire()
                                    .await
                                    .expect("immediate revocation acknowledgment gate closed");
                                permit.forget();
                            }
                            if acknowledge_revoked_cve_scan_execution(
                                &spawn_pool,
                                scan_id,
                                claim.execution_id,
                            )
                            .await
                            .unwrap_or(false)
                            {
                                return;
                            }
                            break Err(anyhow::anyhow!(
                                "CVE scan {scan_id} lost execution ownership"
                            ));
                        }
                        Ok(Err(err)) => {
                            drop(execution);
                            break Err(err.context(
                                "Failed to refresh immediate CVE scan execution heartbeat",
                            ));
                        }
                        Err(_) => {
                            drop(execution);
                            break Err(anyhow::anyhow!(
                                "Timed out refreshing immediate CVE scan {scan_id} execution heartbeat"
                            ));
                        }
                    }
                }
            }
        };

        if let Err(err) = result {
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
}
