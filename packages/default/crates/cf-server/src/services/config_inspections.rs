//! Executes one already-claimed Config Inspector job.
//!
//! This module owns the bounded two-stage Nix orchestration for one execution.
//! It does not claim queued jobs, run a worker loop, expose an API, or alter
//! primary evaluation and deployment behavior.

use anyhow::{Context, Result, bail};
use chrono::Duration as ChronoDuration;
use sqlx::{PgConnection, PgPool, Postgres, Transaction};
use std::path::Path;
use std::time::Duration;
use std::{future::Future, pin::Pin};
use tokio::process::Command;
use tokio::time::MissedTickBehavior;
use tracing::{debug, info, warn};

use crate::derivations::utils::build_flake_reference;
use crate::flake::credentials::FlakeCredentialEnv;
use crate::models::config_inspector::{
    ConfigInspectorResult, InspectionTarget, assemble_config_inspection,
    build_definition_values_expression, build_inspector_expression,
    reconcile_definition_values_output, reconcile_inspector_output,
};
use crate::models::config_snapshot_artifact::config_artifact_v2_from_assembled;
use crate::models::evaluate_with_policies::{
    BoundedProcessOutput, HEAVY_NIX_ADVISORY_LOCK, heavy_nix_limiter, run_nix_command_bounded,
};
use crate::queries::config_inspections::{
    ConfigInspectionExecutionClaim, ConfigInspectionRecoverySummary,
    claim_next_config_inspection_job, complete_config_inspection_execution_failure,
    complete_config_inspection_execution_success_tx, load_config_inspection_execution_context,
    lock_config_inspection_execution_tx, recover_stale_config_inspection_jobs,
};
use crate::queries::cve_scans::{acquire_execution_lock, release_execution_lock_or_close};
use crate::queries::evaluation_snapshots::{
    lock_snapshot_writer_tx, persist_config_artifact_v2_deferred_tx,
};
use crate::security::snapshot_redaction::redact_text;

const STAGE_DEADLINE: Duration = Duration::from_secs(5 * 60);
const STAGE_STDOUT_LIMIT: usize = 256 * 1024 * 1024;
const STAGE_STDERR_LIMIT: usize = 256 * 1024;
const NIX_EVAL_JOBS_PROGRAM: &str = "nix-eval-jobs";
const NIX_WORKERS: &str = "2";
const STAGE1_APPLY: &str = "derivation: if derivation.meta ? crystalForgeInspector then derivation.meta.crystalForgeInspector else derivation.meta.crystalForgeProvenance";
const STAGE2_APPLY: &str = "derivation: if derivation.meta ? crystalForgeDefinitionValues then derivation.meta.crystalForgeDefinitionValues else derivation.meta";
const CONFIG_INSPECTION_QUEUE_POLL_INTERVAL: Duration = Duration::from_secs(5);
const CONFIG_INSPECTION_STALE_THRESHOLD: ChronoDuration = ChronoDuration::minutes(10);

/// Returns whether the dedicated Config Inspector worker may run.
///
/// Mock execution mode must not open the worker database path or perform
/// stale recovery. Primary mock evaluation remains independent of this worker.
pub fn should_run_config_inspection_worker(execution_mode_is_mock: bool) -> bool {
    !execution_mode_is_mock
}

/// Reports the durable result of one claimed Config Inspector execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ConfigInspectionExecutionOutcome {
    /// The exact execution persisted a V2 snapshot and reached `succeeded`.
    Succeeded { snapshot_id: uuid::Uuid },
    /// The exact execution was durably terminalized as failed.
    Failed,
    /// A lifecycle transition replaced or removed the claimed execution.
    LostOwnership,
}

/// Executes one already-claimed Config Inspector job.
///
/// The claim is revalidated before any subprocess starts. The execution
/// advisory lock remains held through both bounded Nix stages, semantic
/// assembly, V2 persistence, and terminalization. No queued job is claimed by
/// this function.
///
/// # Errors
///
/// Returns an error when the exact context cannot be loaded, the database
/// cannot maintain ownership or persistence, or the Nix execution setup fails.
pub(crate) async fn execute_claimed_config_inspection(
    pool: &PgPool,
    claim: ConfigInspectionExecutionClaim,
) -> Result<ConfigInspectionExecutionOutcome> {
    execute_claimed_config_inspection_with_program(pool, claim, Path::new(NIX_EVAL_JOBS_PROGRAM))
        .await
}

/// Runs the durable Config Inspector queue serially until the process stops.
///
/// Recovery and claiming remain owned by the query/lifecycle layer. This loop
/// awaits one exact executor call before it claims another job, so a worker
/// process never creates per-job Tokio tasks or an in-memory work queue.
pub async fn run_config_inspection_queue(pool: PgPool) {
    info!("Starting serial Config Inspector queue worker");
    recover_config_inspection_jobs(&pool).await;

    let mut ticker = tokio::time::interval(CONFIG_INSPECTION_QUEUE_POLL_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;
        run_config_inspection_worker_cycle(&pool).await;
    }
}

async fn run_config_inspection_worker_cycle(pool: &PgPool) {
    recover_config_inspection_jobs(pool).await;
    process_one_config_inspection_job(
        pool,
        |pool| Box::pin(claim_next_config_inspection_job(pool)),
        |pool, claim| Box::pin(execute_claimed_config_inspection(pool, claim)),
    )
    .await;
}

async fn recover_config_inspection_jobs(pool: &PgPool) {
    match recover_stale_config_inspection_jobs(pool, CONFIG_INSPECTION_STALE_THRESHOLD).await {
        Ok(summary) => log_recovery_summary(summary),
        Err(error) => warn!(%error, "config_inspection_stale_recovery_failed"),
    }
}

fn log_recovery_summary(summary: ConfigInspectionRecoverySummary) {
    if summary.inspected > 0 || summary.locked > 0 || summary.requeued > 0 || summary.failed > 0 {
        info!(
            inspected = summary.inspected,
            locked = summary.locked,
            requeued = summary.requeued,
            failed = summary.failed,
            "config_inspection_stale_recovery"
        );
    }
}

type ClaimFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<ConfigInspectionExecutionClaim>>> + Send + 'a>>;
type ExecuteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ConfigInspectionExecutionOutcome>> + Send + 'a>>;

async fn process_one_config_inspection_job<Claim, Execute>(
    pool: &PgPool,
    claim_next: Claim,
    execute: Execute,
) where
    Claim: for<'a> FnOnce(&'a PgPool) -> ClaimFuture<'a>,
    Execute: for<'a> FnOnce(&'a PgPool, ConfigInspectionExecutionClaim) -> ExecuteFuture<'a>,
{
    let claim = match claim_next(pool).await {
        Ok(Some(claim)) => claim,
        Ok(None) => return,
        Err(error) => {
            warn!(%error, "config_inspection_claim_failed");
            return;
        }
    };

    let claim_details = (
        claim.job_id,
        claim.commit_id,
        claim.configuration_name.clone(),
        claim.execution_id,
        claim.attempts,
    );
    match execute(pool, claim).await {
        Ok(ConfigInspectionExecutionOutcome::Succeeded { snapshot_id }) => {
            info!(
                job_id = %claim_details.0,
                commit_id = claim_details.1,
                configuration_name = %claim_details.2,
                execution_id = %claim_details.3,
                snapshot_id = %snapshot_id,
                attempts = claim_details.4,
                "config_inspection_succeeded"
            );
        }
        Ok(ConfigInspectionExecutionOutcome::Failed) => {
            warn!(
                job_id = %claim_details.0,
                commit_id = claim_details.1,
                configuration_name = %claim_details.2,
                execution_id = %claim_details.3,
                attempts = claim_details.4,
                "config_inspection_failed"
            );
        }
        Ok(ConfigInspectionExecutionOutcome::LostOwnership) => {
            debug!(
                job_id = %claim_details.0,
                commit_id = claim_details.1,
                configuration_name = %claim_details.2,
                execution_id = %claim_details.3,
                attempts = claim_details.4,
                "config_inspection_lost_ownership"
            );
        }
        Err(error) => {
            warn!(
                job_id = %claim_details.0,
                commit_id = claim_details.1,
                configuration_name = %claim_details.2,
                execution_id = %claim_details.3,
                attempts = claim_details.4,
                %error,
                "config_inspection_executor_error"
            );
        }
    }
}

/// Executes one claimed job with an injected `nix-eval-jobs` program.
///
/// The injected program is a private test seam. Production callers MUST use
/// [`execute_claimed_config_inspection`], which selects the repository's
/// `nix-eval-jobs` executable.
///
/// # Errors
///
/// Returns an error when the database, subprocess, reconciliation, or
/// persistence path fails before the execution can be terminalized.
pub(crate) async fn execute_claimed_config_inspection_with_program(
    pool: &PgPool,
    claim: ConfigInspectionExecutionClaim,
    nix_eval_jobs_program: &Path,
) -> Result<ConfigInspectionExecutionOutcome> {
    execute_claimed_config_inspection_with_lock_acquirer(
        pool,
        claim,
        nix_eval_jobs_program,
        acquire_execution_lock_for_executor,
    )
    .await
}

type ExecutionLockAcquireFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
type ExecutionLockAcquirer =
    for<'a> fn(&'a mut PgConnection, uuid::Uuid) -> ExecutionLockAcquireFuture<'a>;

fn acquire_execution_lock_for_executor<'a>(
    conn: &'a mut PgConnection,
    execution_id: uuid::Uuid,
) -> ExecutionLockAcquireFuture<'a> {
    Box::pin(acquire_execution_lock(conn, execution_id))
}

async fn execute_claimed_config_inspection_with_lock_acquirer(
    pool: &PgPool,
    claim: ConfigInspectionExecutionClaim,
    nix_eval_jobs_program: &Path,
    acquire_lock: ExecutionLockAcquirer,
) -> Result<ConfigInspectionExecutionOutcome> {
    let Some(context) = load_config_inspection_execution_context(pool, &claim).await? else {
        return Ok(ConfigInspectionExecutionOutcome::LostOwnership);
    };
    let flake_ref = build_flake_reference(&context.repo_url, &context.commit_hash);
    let target = InspectionTarget::new(&flake_ref, &claim.configuration_name);

    let mut lock_conn = pool
        .acquire()
        .await
        .context("acquire Config Inspector execution lock connection")?;
    if let Err(error) = acquire_lock(&mut lock_conn, claim.execution_id).await {
        // The session state is uncertain after a failed lock acquisition. Do
        // not return the connection to the pool.
        let _ = lock_conn.close().await;
        return Err(error.context("acquire execution advisory lock"));
    }

    // CONCURRENCY: The advisory lock alone does not prove that this claim is
    // still current. Confirm the exact token before loading credentials or
    // allowing any owned failure terminalization.
    let outcome = match crate::queries::config_inspections::heartbeat_config_inspection_execution(
        pool,
        claim.job_id,
        claim.execution_id,
    )
    .await
    {
        Ok(true) => {
            execute_with_lock(
                pool,
                &claim,
                &target,
                context.flake_id,
                nix_eval_jobs_program,
            )
            .await
        }
        Ok(false) => Ok(ConfigInspectionExecutionOutcome::LostOwnership),
        Err(error) => Err(error.context("initial Config Inspector ownership heartbeat")),
    };
    release_execution_lock_or_close(lock_conn, claim.execution_id).await;
    outcome
}

#[cfg(test)]
pub(crate) async fn execute_claimed_config_inspection_with_lock_failure_for_test(
    pool: &PgPool,
    claim: ConfigInspectionExecutionClaim,
    nix_eval_jobs_program: &Path,
) -> Result<ConfigInspectionExecutionOutcome> {
    fn fail_lock<'a>(
        _conn: &'a mut PgConnection,
        _execution_id: uuid::Uuid,
    ) -> ExecutionLockAcquireFuture<'a> {
        Box::pin(async { Err(anyhow::anyhow!("injected execution lock failure")) })
    }

    execute_claimed_config_inspection_with_lock_acquirer(
        pool,
        claim,
        nix_eval_jobs_program,
        fail_lock,
    )
    .await
}

async fn execute_with_lock(
    pool: &PgPool,
    claim: &ConfigInspectionExecutionClaim,
    target: &InspectionTarget,
    flake_id: i32,
    nix_eval_jobs_program: &Path,
) -> Result<ConfigInspectionExecutionOutcome> {
    let credentials = match FlakeCredentialEnv::load(pool, flake_id).await {
        Ok(credentials) => credentials,
        Err(error) => {
            return terminalize_failure(pool, claim, error.context("load flake credentials")).await;
        }
    };

    let semantic = match execute_nix_stages(
        pool,
        claim,
        target,
        credentials.as_ref(),
        nix_eval_jobs_program,
    )
    .await
    {
        Ok(Some(semantic)) => semantic,
        Ok(None) => return Ok(ConfigInspectionExecutionOutcome::LostOwnership),
        Err(error) => return terminalize_failure(pool, claim, error).await,
    };

    let artifact = match config_artifact_v2_from_assembled(semantic) {
        Ok(artifact) => artifact,
        Err(error) => {
            return terminalize_failure(
                pool,
                claim,
                error.context("convert Config Inspector artifact"),
            )
            .await;
        }
    };

    match persist_artifact_and_complete(pool, claim, artifact).await {
        Ok(PersistOutcome::Succeeded { snapshot_id }) => {
            Ok(ConfigInspectionExecutionOutcome::Succeeded { snapshot_id })
        }
        Ok(PersistOutcome::LostOwnership) => Ok(ConfigInspectionExecutionOutcome::LostOwnership),
        Err(error) => terminalize_failure(pool, claim, error).await,
    }
}

async fn execute_nix_stages(
    pool: &PgPool,
    claim: &ConfigInspectionExecutionClaim,
    target: &InspectionTarget,
    credentials: Option<&FlakeCredentialEnv>,
    nix_eval_jobs_program: &Path,
) -> Result<Option<crate::models::config_inspector::AssembledConfigInspection>> {
    // CONCURRENCY: Acquire the cross-process advisory lock before the
    // in-process semaphore, matching primary evaluation and hardening. Hold
    // both across Stage 1 and Stage 2, then release them before persistence.
    let mut heavy_lock = pool.begin().await.context("begin heavy Nix lock")?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(HEAVY_NIX_ADVISORY_LOCK)
        .execute(&mut *heavy_lock)
        .await
        .context("acquire heavy Nix advisory lock")?;
    let heavy_permit = heavy_nix_limiter()
        .acquire_owned()
        .await
        .context("heavy Nix limiter was closed")?;

    let stage1_output = run_stage(
        nix_eval_jobs_program,
        &build_inspector_expression(target),
        STAGE1_APPLY,
        credentials,
        "Config Inspector Stage 1",
    )
    .await?;
    let stage1 = reconcile_stage1(stage1_output, target, claim)?;

    if !crate::queries::config_inspections::heartbeat_config_inspection_execution(
        pool,
        claim.job_id,
        claim.execution_id,
    )
    .await
    .context("between-stage Config Inspector ownership heartbeat")?
    {
        drop(heavy_permit);
        heavy_lock.rollback().await.ok();
        return Ok(None);
    }

    let stage2_output = run_stage(
        nix_eval_jobs_program,
        &build_definition_values_expression(target),
        STAGE2_APPLY,
        credentials,
        "Config Inspector Stage 2",
    )
    .await?;
    let stage2 = reconcile_definition_values_output(stage2_output.stdout.bytes.as_slice(), &stage1)
        .context("reconcile Config Inspector Stage 2")?;
    let assembled = assemble_config_inspection(stage1, stage2)
        .context("assemble Config Inspector semantic result")?;

    if !crate::queries::config_inspections::heartbeat_config_inspection_execution(
        pool,
        claim.job_id,
        claim.execution_id,
    )
    .await
    .context("final Config Inspector ownership heartbeat")?
    {
        drop(heavy_permit);
        heavy_lock.rollback().await.ok();
        return Ok(None);
    }

    drop(heavy_permit);
    heavy_lock
        .commit()
        .await
        .context("release heavy Nix advisory lock")?;
    Ok(Some(assembled))
}

fn reconcile_stage1(
    output: BoundedProcessOutput,
    target: &InspectionTarget,
    claim: &ConfigInspectionExecutionClaim,
) -> Result<ConfigInspectorResult> {
    let stage1 = reconcile_inspector_output(output.stdout.bytes.as_slice(), target)
        .context("reconcile Config Inspector Stage 1")?;
    if stage1.carrier_drv_path != claim.carrier_drv_path {
        bail!("Config Inspector Stage 1 carrier does not match the claimed target");
    }
    Ok(stage1)
}

async fn run_stage(
    program: &Path,
    expression: &str,
    apply: &str,
    credentials: Option<&FlakeCredentialEnv>,
    process_name: &str,
) -> Result<BoundedProcessOutput> {
    let mut command = build_stage_command(program, expression, apply, credentials);
    let output = run_nix_command_bounded(
        &mut command,
        process_name,
        STAGE_DEADLINE,
        STAGE_STDOUT_LIMIT,
        STAGE_STDERR_LIMIT,
    )
    .await?;
    if output.stdout.is_truncated() {
        bail!("{process_name} stdout exceeded the bounded retention limit");
    }
    if !output.status.success() {
        let diagnostic = output.stderr.diagnostic_excerpt(4096);
        bail!("{process_name} exited unsuccessfully: {diagnostic}");
    }
    Ok(output)
}

fn build_stage_command(
    program: &Path,
    expression: &str,
    apply: &str,
    credentials: Option<&FlakeCredentialEnv>,
) -> Command {
    let mut command = Command::new(program);
    command.args([
        "--expr",
        expression,
        "--impure",
        "--meta",
        "--apply",
        apply,
        "--option",
        "experimental-features",
        "nix-command flakes",
        "--workers",
        NIX_WORKERS,
    ]);
    if let Some(credentials) = credentials {
        credentials.apply_to_nix_command(&mut command);
    }
    command
}

enum PersistOutcome {
    Succeeded { snapshot_id: uuid::Uuid },
    LostOwnership,
}

async fn persist_artifact_and_complete(
    pool: &PgPool,
    claim: &ConfigInspectionExecutionClaim,
    artifact: crate::models::config_snapshot_artifact::ConfigInspectionArtifactV2,
) -> Result<PersistOutcome> {
    let mut tx: Transaction<'_, Postgres> = pool
        .begin()
        .await
        .context("begin Config Inspector persistence")?;
    // INVARIANT: The snapshot-writer advisory lock is the first lock in every
    // snapshot mutation transaction. The execution row lock follows it.
    lock_snapshot_writer_tx(&mut tx)
        .await
        .context("acquire Config Inspector snapshot writer lock")?;
    if !lock_config_inspection_execution_tx(&mut tx, claim).await? {
        tx.rollback().await.ok();
        return Ok(PersistOutcome::LostOwnership);
    }
    let snapshot_id = persist_config_artifact_v2_deferred_tx(
        &mut tx,
        claim.commit_id,
        &claim.configuration_name,
        artifact,
    )
    .await
    .context("persist Config Inspector V2 artifact")?;
    if !complete_config_inspection_execution_success_tx(&mut tx, claim).await? {
        tx.rollback().await.ok();
        return Ok(PersistOutcome::LostOwnership);
    }
    tx.commit()
        .await
        .context("commit Config Inspector V2 execution")?;
    Ok(PersistOutcome::Succeeded { snapshot_id })
}

async fn terminalize_failure(
    pool: &PgPool,
    claim: &ConfigInspectionExecutionClaim,
    error: anyhow::Error,
) -> Result<ConfigInspectionExecutionOutcome> {
    let diagnostic = redact_text(&format!("{error:#}"));
    match complete_config_inspection_execution_failure(
        pool,
        claim.job_id,
        claim.execution_id,
        &diagnostic,
    )
    .await
    {
        Ok(true) => Ok(ConfigInspectionExecutionOutcome::Failed),
        Ok(false) => Ok(ConfigInspectionExecutionOutcome::LostOwnership),
        Err(error) => Err(error.context("terminalize failed Config Inspector execution")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex as StdMutex};
    use tokio::sync::{Mutex, Notify};
    use uuid::Uuid;

    fn test_pool() -> PgPool {
        PgPool::connect_lazy("postgres://worker-test.invalid/config_inspections")
            .expect("test pool should be constructible without connecting")
    }

    fn test_claim(attempts: i32) -> ConfigInspectionExecutionClaim {
        ConfigInspectionExecutionClaim {
            job_id: Uuid::new_v4(),
            commit_id: attempts,
            derivation_id: attempts,
            configuration_name: format!("worker-{attempts}"),
            carrier_drv_path: format!("/nix/store/{attempts}-worker.drv"),
            execution_id: Uuid::new_v4(),
            attempts,
        }
    }

    #[tokio::test]
    async fn worker_cycle_empty_queue_does_not_execute() {
        let executor_calls = Arc::new(AtomicUsize::new(0));
        let calls = Arc::clone(&executor_calls);
        process_one_config_inspection_job(
            &test_pool(),
            |_| Box::pin(async { Ok(None) }),
            move |_, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(ConfigInspectionExecutionOutcome::Failed) })
            },
        )
        .await;

        assert_eq!(executor_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn worker_cycle_passes_exact_claim_to_executor_once() {
        let expected = test_claim(1);
        let observed = Arc::new(Mutex::new(None));
        let observed_by_executor = Arc::clone(&observed);
        let claim = expected.clone();
        process_one_config_inspection_job(
            &test_pool(),
            move |_| Box::pin(async move { Ok(Some(claim)) }),
            move |_, claim| {
                let observed = Arc::clone(&observed_by_executor);
                Box::pin(async move {
                    *observed.lock().await = Some(claim);
                    Ok(ConfigInspectionExecutionOutcome::Failed)
                })
            },
        )
        .await;

        assert_eq!(*observed.lock().await, Some(expected));
    }

    #[tokio::test]
    async fn worker_cycles_execute_two_jobs_serially() {
        let queue = Arc::new(StdMutex::new(VecDeque::from([
            test_claim(1),
            test_claim(2),
        ])));
        let executor_calls = Arc::new(AtomicUsize::new(0));
        let first_started = Arc::new(Notify::new());
        let release_first = Arc::new(Notify::new());
        let first_pool = Arc::new(test_pool());

        let first_queue = Arc::clone(&queue);
        let first_calls = Arc::clone(&executor_calls);
        let first_started_for_executor = Arc::clone(&first_started);
        let release_first_for_executor = Arc::clone(&release_first);
        let first_pool_for_task = Arc::clone(&first_pool);
        let first_cycle = tokio::spawn(async move {
            process_one_config_inspection_job(
                &first_pool_for_task,
                move |_| {
                    let claim = first_queue
                        .lock()
                        .expect("queue mutex should not be poisoned")
                        .pop_front();
                    Box::pin(async move { Ok(claim) })
                },
                move |_, _| {
                    let calls = Arc::clone(&first_calls);
                    let started = Arc::clone(&first_started_for_executor);
                    let release = Arc::clone(&release_first_for_executor);
                    Box::pin(async move {
                        assert_eq!(calls.fetch_add(1, Ordering::SeqCst), 0);
                        started.notify_one();
                        release.notified().await;
                        Ok(ConfigInspectionExecutionOutcome::Succeeded {
                            snapshot_id: Uuid::new_v4(),
                        })
                    })
                },
            )
            .await;
        });

        first_started.notified().await;
        assert_eq!(executor_calls.load(Ordering::SeqCst), 1);
        release_first.notify_one();
        first_cycle.await.expect("first cycle should complete");

        let second_queue = Arc::clone(&queue);
        let second_calls = Arc::clone(&executor_calls);
        process_one_config_inspection_job(
            &test_pool(),
            move |_| {
                let claim = second_queue
                    .lock()
                    .expect("queue mutex should not be poisoned")
                    .pop_front();
                Box::pin(async move { Ok(claim) })
            },
            move |_, _| {
                second_calls.fetch_add(1, Ordering::SeqCst);
                Box::pin(async {
                    Ok(ConfigInspectionExecutionOutcome::Succeeded {
                        snapshot_id: Uuid::new_v4(),
                    })
                })
            },
        )
        .await;

        assert_eq!(executor_calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn worker_executor_error_does_not_retry_or_terminalize() {
        let executor_calls = Arc::new(AtomicUsize::new(0));
        let calls = Arc::clone(&executor_calls);
        process_one_config_inspection_job(
            &test_pool(),
            |_| Box::pin(async { Ok(Some(test_claim(1))) }),
            move |_, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Err(anyhow::anyhow!("injected worker error")) })
            },
        )
        .await;

        assert_eq!(executor_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn worker_lost_ownership_does_not_retry_or_mutate() {
        let executor_calls = Arc::new(AtomicUsize::new(0));
        let calls = Arc::clone(&executor_calls);
        process_one_config_inspection_job(
            &test_pool(),
            |_| Box::pin(async { Ok(Some(test_claim(1))) }),
            move |_, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(ConfigInspectionExecutionOutcome::LostOwnership) })
            },
        )
        .await;

        assert_eq!(executor_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn worker_failed_outcome_does_not_double_terminalize() {
        let executor_calls = Arc::new(AtomicUsize::new(0));
        let calls = Arc::clone(&executor_calls);
        process_one_config_inspection_job(
            &test_pool(),
            |_| Box::pin(async { Ok(Some(test_claim(1))) }),
            move |_, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(ConfigInspectionExecutionOutcome::Failed) })
            },
        )
        .await;

        assert_eq!(executor_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn worker_mode_gate_disables_mock_execution() {
        assert!(should_run_config_inspection_worker(false));
        assert!(!should_run_config_inspection_worker(true));
    }

    #[test]
    fn worker_loop_has_no_per_job_fanout() {
        let source = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/services/config_inspections.rs"
        ));
        let worker = source
            .split_once("pub async fn run_config_inspection_queue")
            .and_then(|(_, body)| body.split_once("async fn recover_config_inspection_jobs"))
            .map(|(body, _)| body)
            .expect("worker loop should remain present");
        assert!(!worker.contains("tokio::spawn"));
        assert!(!worker.contains("JoinSet"));
        assert!(!worker.contains("FuturesUnordered"));
        assert!(!worker.contains("buffer_unordered"));
    }

    #[test]
    fn stage_commands_are_bounded_and_targeted() {
        let target = InspectionTarget::new("git+https://example.test/repo?rev=abc", "host");
        let stage1 = build_inspector_expression(&target);
        let stage2 = build_definition_values_expression(&target);
        let command = build_stage_command(Path::new("nix-eval-jobs"), &stage1, STAGE1_APPLY, None);
        let args: Vec<String> = command
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(stage1.contains("configurationName"));
        assert!(stage2.contains("configurationName"));
        assert!(args.windows(2).any(|pair| pair == ["--impure", "--meta"]));
        assert!(args.windows(2).any(|pair| pair == ["--workers", "2"]));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--option", "experimental-features"])
        );
        assert!(args.contains(&STAGE1_APPLY.to_string()));
        assert_eq!(NIX_WORKERS, "2");
        assert_eq!(STAGE_STDOUT_LIMIT, 256 * 1024 * 1024);
        assert_eq!(STAGE_STDERR_LIMIT, 256 * 1024);
        assert_eq!(STAGE_DEADLINE, Duration::from_secs(300));
    }

    #[test]
    fn final_persistence_lock_order_is_structurally_guarded() {
        let source = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/services/config_inspections.rs"
        ));
        let persistence = source
            .split_once("async fn persist_artifact_and_complete")
            .and_then(|(_, body)| body.split_once("async fn terminalize_failure"))
            .map(|(body, _)| body)
            .expect("final persistence helper should remain present");
        let snapshot_lock = persistence
            .find("lock_snapshot_writer_tx(&mut tx)")
            .expect("snapshot writer lock must remain in final persistence");
        let execution_lock = persistence
            .find("lock_config_inspection_execution_tx(&mut tx, claim)")
            .expect("execution row lock must remain in final persistence");
        assert!(snapshot_lock < execution_lock);
        assert!(persistence.contains("persist_config_artifact_v2_deferred_tx"));
        assert!(!persistence.contains("persist_config_artifact_v2_tx"));
    }
}
