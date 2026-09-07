//! Executes one already-claimed Config Inspector job.
//!
//! This module owns the bounded two-stage Nix orchestration for one execution.
//! It does not claim queued jobs, run a worker loop, expose an API, or alter
//! primary evaluation and deployment behavior.

use anyhow::{Context, Result, bail};
use sqlx::{PgPool, Postgres, Transaction};
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;

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
    ConfigInspectionExecutionClaim, complete_config_inspection_execution_failure,
    complete_config_inspection_execution_success_tx, load_config_inspection_execution_context,
    lock_config_inspection_execution_tx,
};
use crate::queries::cve_scans::{acquire_execution_lock, release_execution_lock_or_close};
use crate::queries::evaluation_snapshots::persist_config_artifact_v2_tx;
use crate::security::snapshot_redaction::redact_text;

const STAGE_DEADLINE: Duration = Duration::from_secs(5 * 60);
const STAGE_STDOUT_LIMIT: usize = 256 * 1024 * 1024;
const STAGE_STDERR_LIMIT: usize = 256 * 1024;
const NIX_EVAL_JOBS_PROGRAM: &str = "nix-eval-jobs";
const NIX_WORKERS: &str = "2";
const STAGE1_APPLY: &str = "derivation: if derivation.meta ? crystalForgeInspector then derivation.meta.crystalForgeInspector else derivation.meta.crystalForgeProvenance";
const STAGE2_APPLY: &str = "derivation: if derivation.meta ? crystalForgeDefinitionValues then derivation.meta.crystalForgeDefinitionValues else derivation.meta";

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
    let Some(context) = load_config_inspection_execution_context(pool, &claim).await? else {
        return Ok(ConfigInspectionExecutionOutcome::LostOwnership);
    };
    let flake_ref = build_flake_reference(&context.repo_url, &context.commit_hash);
    let target = InspectionTarget::new(&flake_ref, &claim.configuration_name);

    let mut lock_conn = pool
        .acquire()
        .await
        .context("acquire Config Inspector execution lock connection")?;
    if let Err(error) = acquire_execution_lock(&mut lock_conn, claim.execution_id).await {
        // The session state is uncertain after a failed lock acquisition. Do
        // not return the connection to the pool.
        let _ = lock_conn.close().await;
        return terminalize_failure(
            pool,
            &claim,
            error.context("acquire execution advisory lock"),
        )
        .await;
    }

    // CONCURRENCY: Recovery can win between the context query and lock
    // acquisition. The first heartbeat is the final gate before any Nix work.
    let outcome = execute_with_lock(
        pool,
        &claim,
        &target,
        context.flake_id,
        nix_eval_jobs_program,
    )
    .await;
    release_execution_lock_or_close(lock_conn, claim.execution_id).await;
    outcome
}

async fn execute_with_lock(
    pool: &PgPool,
    claim: &ConfigInspectionExecutionClaim,
    target: &InspectionTarget,
    flake_id: i32,
    nix_eval_jobs_program: &Path,
) -> Result<ConfigInspectionExecutionOutcome> {
    if !crate::queries::config_inspections::heartbeat_config_inspection_execution(
        pool,
        claim.job_id,
        claim.execution_id,
    )
    .await
    .context("initial Config Inspector ownership heartbeat")?
    {
        return Ok(ConfigInspectionExecutionOutcome::LostOwnership);
    }

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
    if !lock_config_inspection_execution_tx(&mut tx, claim).await? {
        tx.rollback().await.ok();
        return Ok(PersistOutcome::LostOwnership);
    }
    let snapshot_id = persist_config_artifact_v2_tx(
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
}
