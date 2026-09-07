//! Durable scheduling for targeted Config Inspector enrichment.
//!
//! This module owns only the database job substrate. It does not evaluate Nix,
//! inspect flakes, create snapshots, or advance either snapshot selector.

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Duration, Utc};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::models::evaluate_with_policies::SuccessfulSystemResult;

const MAX_CONFIG_INSPECTION_ATTEMPTS: i32 = 3;
const MAX_CONFIG_INSPECTION_ERROR_CHARS: usize = 4096;
const STALE_RECOVERY_BATCH_SIZE: i64 = 32;
const MAX_ATTEMPTS_ERROR: &str = "Config inspection execution expired after maximum retry attempts";

/// Describes the lifecycle state of one durable Config Inspector job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfigInspectionJobStatus {
    /// The target is waiting for a future worker.
    Queued,
    /// A future worker has claimed the target.
    Running,
    /// The target completed successfully.
    Succeeded,
    /// The target completed with an error.
    Failed,
}

impl ConfigInspectionJobStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            other => bail!("unknown Config Inspector job status {other:?}"),
        }
    }
}

/// Represents a persisted Config Inspector target and its lifecycle audit data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfigInspectionJob {
    /// Stable job identity.
    pub id: Uuid,
    /// Commit containing the exact configuration target.
    pub commit_id: i32,
    /// NixOS carrier derivation targeted by the job.
    pub derivation_id: i32,
    /// Exact NixOS configuration name.
    pub configuration_name: String,
    /// Exact carrier `.drv` path certified by finalization.
    pub carrier_drv_path: String,
    /// Durable lifecycle state.
    pub status: ConfigInspectionJobStatus,
    /// Number of worker attempts recorded for this job.
    pub attempts: i32,
    /// Session-scoped token that owns the current or terminal execution.
    pub execution_id: Option<Uuid>,
    /// Last heartbeat recorded for the current or terminal execution.
    pub execution_heartbeat_at: Option<DateTime<Utc>>,
    /// Redacted terminal failure, when the job failed.
    pub error: Option<String>,
    /// Time at which the job became eligible for a worker.
    pub scheduled_at: DateTime<Utc>,
    /// Time at which a worker started the job.
    pub started_at: Option<DateTime<Utc>>,
    /// Time at which the job reached a terminal state.
    pub completed_at: Option<DateTime<Utc>>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last lifecycle update timestamp.
    pub updated_at: DateTime<Utc>,
}

/// Describes the exact execution lease granted by a durable job claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfigInspectionExecutionClaim {
    /// Durable job identity.
    pub job_id: Uuid,
    /// Commit containing the exact configuration target.
    pub commit_id: i32,
    /// NixOS carrier derivation targeted by the job.
    pub derivation_id: i32,
    /// Exact NixOS configuration name.
    pub configuration_name: String,
    /// Exact carrier `.drv` path.
    pub carrier_drv_path: String,
    /// Unique token fencing this execution from later owners.
    pub execution_id: Uuid,
    /// Number of claims made, including this claim.
    pub attempts: i32,
}

/// Resolves the immutable flake lineage required by one claimed execution.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub(crate) struct ConfigInspectionExecutionContext {
    /// Owning flake identity used for credential lookup.
    pub flake_id: i32,
    /// Repository reference stored by the owning flake.
    pub repo_url: String,
    /// Full immutable commit revision selected by the claimed derivation.
    pub commit_hash: String,
}

/// Summarizes one bounded stale-execution recovery pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ConfigInspectionRecoverySummary {
    /// Number of stale candidates inspected.
    pub inspected: usize,
    /// Number of candidates protected by a live execution advisory lock.
    pub locked: usize,
    /// Number of stale executions requeued.
    pub requeued: usize,
    /// Number of stale executions terminalized as failed.
    pub failed: usize,
}

/// Summarizes one set-based enqueue operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ConfigInspectionEnqueueSummary {
    /// Number of exact successful targets supplied by the caller.
    pub requested_targets: usize,
    /// Number of new queued rows inserted by this call.
    pub inserted_jobs: usize,
}

/// Returns whether automatic Config Inspector scheduling is enabled for a mode.
pub(crate) fn should_schedule_config_inspections(execution_mode_is_mock: bool) -> bool {
    !execution_mode_is_mock
}

/// Enqueues exact successful NixOS systems for later Config Inspector work.
///
/// The function resolves every supplied `(configuration_name, carrier_drv_path)`
/// against the specified commit and locks the matching derivation rows for the
/// complete validation and insert transaction. A ready V2 artifact for the
/// same carrier suppresses new work. Active rows are deduplicated by the
/// database partial unique index; terminal history does not suppress a later
/// attempt.
///
/// This function performs database work only. It does not spawn processes,
/// evaluate Nix, access Git, mutate snapshots, or advance selectors.
///
/// # Errors
///
/// Returns an error when a supplied target does not exactly match a NixOS
/// derivation for `commit_id`, or when database access fails. If validation
/// fails, the transaction inserts no inspection jobs.
pub(crate) async fn enqueue_config_inspection_jobs_for_successful_systems(
    pool: &PgPool,
    commit_id: i32,
    successful_systems: &[SuccessfulSystemResult],
) -> Result<ConfigInspectionEnqueueSummary> {
    let mut targets = BTreeMap::new();
    for successful in successful_systems {
        if let Some(existing) = targets.get(&successful.system_name)
            && existing != &successful.drv_path
        {
            bail!(
                "multiple successful derivations supplied for configuration {:?}",
                successful.system_name
            );
        }
        targets.insert(successful.system_name.clone(), successful.drv_path.clone());
    }

    if targets.is_empty() {
        return Ok(ConfigInspectionEnqueueSummary {
            requested_targets: 0,
            inserted_jobs: 0,
        });
    }

    let configuration_names: Vec<String> = targets.keys().cloned().collect();
    let carrier_drv_paths: Vec<String> = targets.values().cloned().collect();

    let mut tx = pool
        .begin()
        .await
        .context("begin Config Inspector enqueue")?;
    let resolved: Vec<(i32, String, String)> = sqlx::query_as(
        r#"
        WITH supplied AS (
            SELECT *
            FROM unnest($1::text[], $2::text[])
                AS target(configuration_name, carrier_drv_path)
        )
        SELECT derivation.id, derivation.derivation_name, derivation.derivation_path
        FROM supplied
        JOIN derivations derivation
          ON derivation.commit_id = $3
         AND derivation.derivation_type = 'nixos'
         AND derivation.derivation_name = supplied.configuration_name
         AND derivation.derivation_path = supplied.carrier_drv_path
        ORDER BY derivation.derivation_name
        FOR SHARE OF derivation
        "#,
    )
    .bind(&configuration_names)
    .bind(&carrier_drv_paths)
    .bind(commit_id)
    .fetch_all(&mut *tx)
    .await
    .context("resolve and lock successful Config Inspector targets")?;
    if resolved.len() != targets.len() {
        bail!(
            "successful Config Inspector targets do not exactly match NixOS derivations for commit {commit_id}"
        );
    }

    let derivation_ids: Vec<i32> = resolved.iter().map(|(id, _, _)| *id).collect();
    let resolved_configuration_names: Vec<&str> =
        resolved.iter().map(|(_, name, _)| name.as_str()).collect();
    let resolved_carrier_drv_paths: Vec<&str> =
        resolved.iter().map(|(_, _, path)| path.as_str()).collect();

    let inserted: Vec<Uuid> = sqlx::query_scalar(
        r#"
        WITH supplied AS (
            SELECT *
            FROM unnest($1::integer[], $2::text[], $3::text[])
                AS target(derivation_id, configuration_name, carrier_drv_path)
        ),
        ready AS (
            SELECT supplied.configuration_name
            FROM supplied
            JOIN config_snapshot_selections selection
              ON selection.commit_id = $4
             AND selection.configuration_name = supplied.configuration_name
            JOIN evaluation_snapshots snapshot
              ON snapshot.id = selection.current_snapshot_id
             AND snapshot.commit_id = $4
             AND snapshot.configuration_name = supplied.configuration_name
            WHERE snapshot.schema_version = 2
              AND snapshot.integrity_version = 2
              AND snapshot.lifecycle = 'available'
              AND snapshot.comparison_ready = TRUE
              AND snapshot.carrier_drv_path = supplied.carrier_drv_path
        )
        INSERT INTO config_inspection_jobs (
            commit_id, derivation_id, configuration_name, carrier_drv_path,
            status
        )
        SELECT $4, supplied.derivation_id, supplied.configuration_name,
               supplied.carrier_drv_path, 'queued'
        FROM supplied
        WHERE NOT EXISTS (
                  SELECT 1
                  FROM ready
                  WHERE ready.configuration_name = supplied.configuration_name
              )
        ON CONFLICT (commit_id, configuration_name)
            WHERE status IN ('queued', 'running')
        DO NOTHING
        RETURNING id
        "#,
    )
    .bind(&derivation_ids)
    .bind(&resolved_configuration_names)
    .bind(&resolved_carrier_drv_paths)
    .bind(commit_id)
    .fetch_all(&mut *tx)
    .await
    .context("enqueue Config Inspector jobs")?;

    tx.commit()
        .await
        .context("commit Config Inspector enqueue")?;

    Ok(ConfigInspectionEnqueueSummary {
        requested_targets: resolved.len(),
        inserted_jobs: inserted.len(),
    })
}

/// Loads a durable job by ID for the future worker and reconciliation paths.
pub(crate) async fn get_config_inspection_job(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<ConfigInspectionJob>> {
    let row = sqlx::query(
        r#"
        SELECT id, commit_id, derivation_id, configuration_name,
               carrier_drv_path, status, attempts, execution_id,
               execution_heartbeat_at, error, scheduled_at, started_at,
               completed_at, created_at, updated_at
        FROM config_inspection_jobs
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("load Config Inspector job")?;
    row.map(|row| {
        Ok(ConfigInspectionJob {
            id: row.try_get("id")?,
            commit_id: row.try_get("commit_id")?,
            derivation_id: row.try_get("derivation_id")?,
            configuration_name: row.try_get("configuration_name")?,
            carrier_drv_path: row.try_get("carrier_drv_path")?,
            status: ConfigInspectionJobStatus::parse(row.try_get::<String, _>("status")?.as_str())?,
            attempts: row.try_get("attempts")?,
            execution_id: row.try_get("execution_id")?,
            execution_heartbeat_at: row.try_get("execution_heartbeat_at")?,
            error: row.try_get("error")?,
            scheduled_at: row.try_get("scheduled_at")?,
            started_at: row.try_get("started_at")?,
            completed_at: row.try_get("completed_at")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    })
    .transpose()
}

/// Resolves a claim only when its complete target and execution identity still
/// match the current database row and derivation lineage.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot load the exact execution context.
pub(crate) async fn load_config_inspection_execution_context(
    pool: &PgPool,
    claim: &ConfigInspectionExecutionClaim,
) -> Result<Option<ConfigInspectionExecutionContext>> {
    sqlx::query_as(
        r#"
        SELECT flake.id AS flake_id,
               flake.repo_url,
               commit.git_commit_hash AS commit_hash
        FROM config_inspection_jobs job
        JOIN derivations derivation
          ON derivation.id = job.derivation_id
         AND derivation.commit_id = job.commit_id
         AND derivation.derivation_type = 'nixos'
         AND derivation.derivation_name = job.configuration_name
         AND derivation.derivation_path = job.carrier_drv_path
        JOIN commits commit ON commit.id = job.commit_id
        JOIN flakes flake ON flake.id = commit.flake_id
        WHERE job.id = $1
          AND job.status = 'running'
          AND job.execution_id = $2
          AND job.commit_id = $3
          AND job.derivation_id = $4
          AND job.configuration_name = $5
          AND job.carrier_drv_path = $6
        "#,
    )
    .bind(claim.job_id)
    .bind(claim.execution_id)
    .bind(claim.commit_id)
    .bind(claim.derivation_id)
    .bind(&claim.configuration_name)
    .bind(&claim.carrier_drv_path)
    .fetch_optional(pool)
    .await
    .context("load exact Config Inspector execution context")
}

/// Locks and revalidates the exact execution row before artifact persistence.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot acquire the row lock or inspect the
/// execution row.
pub(crate) async fn lock_config_inspection_execution_tx(
    tx: &mut Transaction<'_, Postgres>,
    claim: &ConfigInspectionExecutionClaim,
) -> Result<bool> {
    let row = sqlx::query(
        r#"
        SELECT id
        FROM config_inspection_jobs
        WHERE id = $1
          AND status = 'running'
          AND execution_id = $2
          AND commit_id = $3
          AND derivation_id = $4
          AND configuration_name = $5
          AND carrier_drv_path = $6
        FOR UPDATE
        "#,
    )
    .bind(claim.job_id)
    .bind(claim.execution_id)
    .bind(claim.commit_id)
    .bind(claim.derivation_id)
    .bind(&claim.configuration_name)
    .bind(&claim.carrier_drv_path)
    .fetch_optional(&mut **tx)
    .await
    .context("lock exact Config Inspector execution")?;
    Ok(row.is_some())
}

/// Marks the exact locked execution successful inside its persistence transaction.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot update the execution row.
pub(crate) async fn complete_config_inspection_execution_success_tx(
    tx: &mut Transaction<'_, Postgres>,
    claim: &ConfigInspectionExecutionClaim,
) -> Result<bool> {
    let affected = sqlx::query(
        r#"
        UPDATE config_inspection_jobs
        SET status = 'succeeded',
            completed_at = now(),
            error = NULL,
            updated_at = now()
        WHERE id = $1
          AND status = 'running'
          AND execution_id = $2
          AND commit_id = $3
          AND derivation_id = $4
          AND configuration_name = $5
          AND carrier_drv_path = $6
        "#,
    )
    .bind(claim.job_id)
    .bind(claim.execution_id)
    .bind(claim.commit_id)
    .bind(claim.derivation_id)
    .bind(&claim.configuration_name)
    .bind(&claim.carrier_drv_path)
    .execute(&mut **tx)
    .await
    .context("complete Config Inspector execution in persistence transaction")?
    .rows_affected();
    Ok(affected == 1)
}

/// Claims the oldest queued Config Inspector job without globally serializing claims.
///
/// PostgreSQL locks the selected candidate with `FOR UPDATE SKIP LOCKED` inside
/// the same statement that changes it to `running`. Concurrent claimers therefore
/// either skip the locked row or claim a different queued row.
pub(crate) async fn claim_next_config_inspection_job(
    pool: &PgPool,
) -> Result<Option<ConfigInspectionExecutionClaim>> {
    let claim = sqlx::query_as::<_, (Uuid, i32, i32, String, String, Uuid, i32)>(
        r#"
        WITH candidate AS (
            SELECT id
            FROM config_inspection_jobs
            WHERE status = 'queued'
            ORDER BY scheduled_at ASC, created_at ASC, id ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE config_inspection_jobs job
        SET status = 'running',
            attempts = job.attempts + 1,
            started_at = now(),
            completed_at = NULL,
            error = NULL,
            execution_id = gen_random_uuid(),
            execution_heartbeat_at = now(),
            updated_at = now()
        FROM candidate
        WHERE job.id = candidate.id
        RETURNING job.id, job.commit_id, job.derivation_id,
                  job.configuration_name, job.carrier_drv_path,
                  job.execution_id, job.attempts
        "#,
    )
    .fetch_optional(pool)
    .await
    .context("claim Config Inspector job")?;

    claim
        .map(
            |(
                job_id,
                commit_id,
                derivation_id,
                configuration_name,
                carrier_drv_path,
                execution_id,
                attempts,
            )| {
                Ok(ConfigInspectionExecutionClaim {
                    job_id,
                    commit_id,
                    derivation_id,
                    configuration_name,
                    carrier_drv_path,
                    execution_id,
                    attempts,
                })
            },
        )
        .transpose()
}

/// Refreshes a running execution heartbeat only for the exact owner token.
pub(crate) async fn heartbeat_config_inspection_execution(
    pool: &PgPool,
    job_id: Uuid,
    execution_id: Uuid,
) -> Result<bool> {
    let affected = sqlx::query(
        "UPDATE config_inspection_jobs SET execution_heartbeat_at = now(), updated_at = now() WHERE id = $1 AND status = 'running' AND execution_id = $2",
    )
    .bind(job_id)
    .bind(execution_id)
    .execute(pool)
    .await
    .context("heartbeat Config Inspector execution")?
    .rows_affected();
    Ok(affected == 1)
}

/// Completes a running execution successfully for the exact owner token.
pub(crate) async fn complete_config_inspection_execution_success(
    pool: &PgPool,
    job_id: Uuid,
    execution_id: Uuid,
) -> Result<bool> {
    let affected = sqlx::query(
        "UPDATE config_inspection_jobs SET status = 'succeeded', completed_at = now(), error = NULL, updated_at = now() WHERE id = $1 AND status = 'running' AND execution_id = $2",
    )
    .bind(job_id)
    .bind(execution_id)
    .execute(pool)
    .await
    .context("complete Config Inspector execution successfully")?
    .rows_affected();
    Ok(affected == 1)
}

/// Redacts and bounds a diagnostic before it crosses the persistence boundary.
fn bounded_config_inspection_error(error: &str) -> String {
    let redacted = crate::security::snapshot_redaction::redact_text(error);
    let bounded: String = redacted
        .chars()
        .take(MAX_CONFIG_INSPECTION_ERROR_CHARS)
        .collect();
    if bounded.trim().is_empty() {
        "Config inspection execution failed".to_string()
    } else {
        bounded
    }
}

/// Completes a running execution with a bounded redacted diagnostic.
pub(crate) async fn complete_config_inspection_execution_failure(
    pool: &PgPool,
    job_id: Uuid,
    execution_id: Uuid,
    error: &str,
) -> Result<bool> {
    let safe_error = bounded_config_inspection_error(error);
    let affected = sqlx::query(
        "UPDATE config_inspection_jobs SET status = 'failed', completed_at = now(), error = $3, updated_at = now() WHERE id = $1 AND status = 'running' AND execution_id = $2",
    )
    .bind(job_id)
    .bind(execution_id)
    .bind(safe_error)
    .execute(pool)
    .await
    .context("complete Config Inspector execution with failure")?
    .rows_affected();
    Ok(affected == 1)
}

/// Requeues or terminalizes one exact stale execution using a heartbeat CAS.
async fn recover_stale_config_inspection_execution(
    pool: &PgPool,
    job_id: Uuid,
    execution_id: Uuid,
    stale_before: DateTime<Utc>,
) -> Result<Option<ConfigInspectionJobStatus>> {
    let status = sqlx::query_scalar::<_, String>(
        r#"
        UPDATE config_inspection_jobs
        SET status = CASE
                         WHEN attempts < $4 THEN 'queued'
                         ELSE 'failed'
                     END,
            scheduled_at = CASE WHEN attempts < $4 THEN now() ELSE scheduled_at END,
            started_at = CASE WHEN attempts < $4 THEN NULL ELSE started_at END,
            completed_at = CASE WHEN attempts < $4 THEN NULL ELSE now() END,
            error = CASE
                        WHEN attempts < $4 THEN NULL
                        ELSE $5
                    END,
            execution_id = CASE WHEN attempts < $4 THEN NULL ELSE execution_id END,
            execution_heartbeat_at = CASE
                                         WHEN attempts < $4 THEN NULL
                                         ELSE execution_heartbeat_at
                                     END,
            updated_at = now()
        WHERE id = $1
          AND status = 'running'
          AND execution_id = $2
          AND execution_heartbeat_at < $3
        RETURNING status
        "#,
    )
    .bind(job_id)
    .bind(execution_id)
    .bind(stale_before)
    .bind(MAX_CONFIG_INSPECTION_ATTEMPTS)
    .bind(MAX_ATTEMPTS_ERROR)
    .fetch_optional(pool)
    .await
    .context("recover stale Config Inspector execution")?;

    status
        .map(|status| ConfigInspectionJobStatus::parse(&status))
        .transpose()
}

/// Recovers at most 32 stale Config Inspector executions.
///
/// A session-level execution advisory lock is authoritative over heartbeat age.
/// Recovery therefore never revokes a stale-looking execution while its owner
/// still holds the lock on a live PostgreSQL session.
pub(crate) async fn recover_stale_config_inspection_jobs(
    pool: &PgPool,
    stale_threshold: Duration,
) -> Result<ConfigInspectionRecoverySummary> {
    let stale_before = Utc::now() - stale_threshold;
    let candidates: Vec<(Uuid, Uuid, i32)> = sqlx::query_as(
        r#"
        SELECT id, execution_id, attempts
            FROM config_inspection_jobs
            WHERE status = 'running'
          AND execution_heartbeat_at < $1
        ORDER BY execution_heartbeat_at ASC, id ASC
        LIMIT $2
        "#,
    )
    .bind(stale_before)
    .bind(STALE_RECOVERY_BATCH_SIZE)
    .fetch_all(pool)
    .await
    .context("list stale Config Inspector executions")?;

    let mut summary = ConfigInspectionRecoverySummary {
        inspected: candidates.len(),
        ..Default::default()
    };
    for (job_id, execution_id, _attempts) in candidates {
        if crate::queries::cve_scans::execution_lock_is_held(pool, execution_id).await? {
            summary.locked += 1;
            continue;
        }
        match recover_stale_config_inspection_execution(pool, job_id, execution_id, stale_before)
            .await?
        {
            Some(ConfigInspectionJobStatus::Queued) => summary.requeued += 1,
            Some(ConfigInspectionJobStatus::Failed) => summary.failed += 1,
            Some(_) | None => {}
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sqlx::PgPool;

    async fn fixture(pool: &PgPool, name: &str) -> (i32, i32, String) {
        let suffix = Uuid::new_v4().simple().to_string();
        let flake_id: i32 = sqlx::query_scalar(
            "INSERT INTO flakes (name, repo_url, branch) VALUES ($1, $2, 'main') RETURNING id",
        )
        .bind(format!("config-inspection-{suffix}"))
        .bind(format!(
            "https://example.test/config-inspection-{suffix}.git"
        ))
        .fetch_one(pool)
        .await
        .expect("inspection fixture flake should persist");
        let commit_id: i32 = sqlx::query_scalar(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) VALUES ($1, $2, now()) RETURNING id",
        )
        .bind(flake_id)
        .bind(format!("{suffix:0>40}"))
        .fetch_one(pool)
        .await
        .expect("inspection fixture commit should persist");
        let drv_path = format!("/nix/store/{suffix}-{name}.drv");
        let derivation_id: i32 = sqlx::query_scalar(
            "INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_path, status_id, attempt_count) VALUES ($1, 'nixos', $2, $3, (SELECT id FROM derivation_statuses ORDER BY id LIMIT 1), 0) RETURNING id",
        )
        .bind(commit_id)
        .bind(name)
        .bind(&drv_path)
        .fetch_one(pool)
        .await
        .expect("inspection fixture derivation should persist");
        (commit_id, derivation_id, drv_path)
    }

    fn successful_system(
        _derivation_id: i32,
        name: &str,
        drv_path: &str,
    ) -> SuccessfulSystemResult {
        SuccessfulSystemResult {
            system_name: name.to_string(),
            derivation_target: format!("test://nixosConfigurations.{name}"),
            drv_path: drv_path.to_string(),
            expected_store_path: None,
            cf_agent_enabled: Some(true),
            build_eligible: true,
        }
    }

    async fn job_count(pool: &PgPool, commit_id: i32) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM config_inspection_jobs WHERE commit_id = $1")
            .bind(commit_id)
            .fetch_one(pool)
            .await
            .expect("inspection job count should load")
    }

    async fn insert_queued_job(
        pool: &PgPool,
        commit_id: i32,
        derivation_id: i32,
        configuration_name: &str,
        carrier_drv_path: &str,
        scheduled_at: DateTime<Utc>,
    ) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO config_inspection_jobs (commit_id, derivation_id, configuration_name, carrier_drv_path, status, scheduled_at) VALUES ($1, $2, $3, $4, 'queued', $5) RETURNING id",
        )
        .bind(commit_id)
        .bind(derivation_id)
        .bind(configuration_name)
        .bind(carrier_drv_path)
        .bind(scheduled_at)
        .fetch_one(pool)
        .await
        .expect("queued Config Inspector job should persist")
    }

    async fn make_claimable_job(pool: &PgPool, name: &str) -> (i32, Uuid) {
        let (commit_id, derivation_id, drv_path) = fixture(pool, name).await;
        let job_id =
            insert_queued_job(pool, commit_id, derivation_id, name, &drv_path, Utc::now()).await;
        (commit_id, job_id)
    }

    async fn set_stale(pool: &PgPool, job_id: Uuid, attempts: i32) {
        sqlx::query(
            "UPDATE config_inspection_jobs SET attempts = $2, execution_heartbeat_at = now() - interval '1 hour', updated_at = now() WHERE id = $1",
        )
        .bind(job_id)
        .bind(attempts)
        .execute(pool)
        .await
        .expect("Config Inspector execution should become stale");
    }

    async fn add_snapshot_selector(
        pool: &PgPool,
        commit_id: i32,
        configuration_name: &str,
        snapshot_id: Uuid,
    ) {
        sqlx::query(
            "INSERT INTO config_snapshot_selections (commit_id, configuration_name, current_snapshot_id) VALUES ($1, $2, $3)",
        )
        .bind(commit_id)
        .bind(configuration_name)
        .bind(snapshot_id)
        .execute(pool)
               .await
        .expect("snapshot selector should persist");
    }

    async fn insert_v2_snapshot(
        pool: &PgPool,
        commit_id: i32,
        configuration_name: &str,
        carrier_drv_path: &str,
        comparison_ready: bool,
    ) {
        let snapshot_id = Uuid::new_v4();
        let digest = vec![7_u8; 32];
        let payload = json!({
            "metadata": {
                "state": "available",
                "option_type": "string",
                "loc": [],
                "declared_type": "str",
                "declarations": [],
                "declaration_positions": [],
                "highest_prio": 100,
                "is_defined": true,
                "surviving_definition_sources": [{
                    "source_path": "modules/0.nix",
                    "priority": 100,
                    "source_input": null,
                    "source_revision": null
                }]
            },
            "effective_value": {"kind": "scalar", "value": "safe"},
            "provenance": {
                "state": "available",
                "definitions": [{
                    "ordinal": 0,
                    "source_path": "modules/0.nix",
                    "source_input": null,
                    "source_revision": null,
                    "module_key": null,
                    "priority": 100,
                    "status": "active_surviving",
                    "surviving_merge_order": 0,
                    "value": if comparison_ready {
                        json!({"kind": "scalar", "value": "safe"})
                    } else {
                        json!(null)
                    }
                }],
                "override_state": false
            }
        });
        let provenance_state = if comparison_ready {
            json!({
                "state": "available",
                "adapter_version": 1,
                "target_lib_version": null,
                "target_module_system_path": null,
                "provenance_digest": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "definition_value_enrichment": {
                    "state": "available",
                    "adapter_version": 1,
                    "provenance_digest": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                }
            })
        } else {
            json!({
                "state": "available",
                "adapter_version": 1,
                "target_lib_version": null,
                "target_module_system_path": null,
                "provenance_digest": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "definition_value_enrichment": {
                    "state": "unavailable",
                    "reason_code": "not_evaluated",
                    "diagnostic": null
                }
            })
        };
        sqlx::query(
            "INSERT INTO evaluation_option_contents (digest, schema_version, payload, search_text) VALUES ($1, 2, $2, 'test')",
        )
        .bind(&digest)
        .bind(payload)
        .execute(pool)
        .await
        .expect("V2 option content should persist");
        sqlx::query(
            "INSERT INTO evaluation_snapshots (id, commit_id, configuration_name, schema_version, lifecycle, option_count, module_count, content_bytes, target_key, source_out_path, carrier_drv_path, provenance_state, comparison_ready) VALUES ($1, $2, $3, 2, 'available', 1, 1, 1, $4, $5, $6, $7, $8)",
        )
        .bind(snapshot_id)
        .bind(commit_id)
        .bind(configuration_name)
        .bind("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        .bind("/nix/store/config-out")
        .bind(carrier_drv_path)
        .bind(provenance_state)
        .bind(comparison_ready)
        .execute(pool)
        .await
        .expect("V2 snapshot should persist");
        sqlx::query(
            "INSERT INTO evaluation_snapshot_options (snapshot_id, option_path, content_digest, is_overridden, option_key, path_components) VALUES ($1, 'services.test.value', $2, false, $3, ARRAY['services', 'test', 'value'])",
        )
        .bind(snapshot_id)
        .bind(&digest)
        .bind("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc")
        .execute(pool)
        .await
        .expect("V2 option reference should persist");
        sqlx::query("UPDATE evaluation_snapshots SET integrity_version = 2 WHERE id = $1")
            .bind(snapshot_id)
            .execute(pool)
            .await
            .expect("V2 snapshot should certify");
        sqlx::query(
            "INSERT INTO config_snapshot_selections (commit_id, configuration_name, current_snapshot_id) VALUES ($1, $2, $3)",
        )
        .bind(commit_id)
        .bind(configuration_name)
        .bind(snapshot_id)
        .execute(pool)
        .await
        .expect("V2 selector should persist");
    }

    #[test]
    fn execution_mode_gate_only_allows_real_mode() {
        assert!(should_schedule_config_inspections(false));
        assert!(!should_schedule_config_inspections(true));
    }

    #[test]
    fn status_values_match_migration_contract() {
        for status in [
            ConfigInspectionJobStatus::Queued,
            ConfigInspectionJobStatus::Running,
            ConfigInspectionJobStatus::Succeeded,
            ConfigInspectionJobStatus::Failed,
        ] {
            assert_eq!(
                ConfigInspectionJobStatus::parse(status.as_str()).unwrap(),
                status
            );
        }
    }

    #[test]
    fn migration_0253_documents_pre_worker_running_row_upgrade() {
        let migration =
            include_str!("../../migrations/0253_config_inspection_execution_ownership.sql");
        assert!(migration.contains("WHERE status = 'running'"));
        assert!(migration.contains("status = 'queued'"));
        assert!(migration.contains("started_at = NULL"));
        assert!(migration.contains("completed_at = NULL"));
        assert!(migration.contains("error = NULL"));
        assert!(migration.contains("execution_id uuid"));
        assert!(migration.contains("execution_heartbeat_at timestamptz"));
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn claim_is_ordered_and_records_execution_ownership(pool: PgPool) {
        let (commit_id, first_id, first_drv) = fixture(&pool, "first").await;
        let (_, second_id, second_drv) = fixture(&pool, "second").await;
        sqlx::query("UPDATE derivations SET commit_id = $1 WHERE id = $2")
            .bind(commit_id)
            .bind(second_id)
            .execute(&pool)
            .await
            .expect("second derivation should move to the fixture commit");
        let now = Utc::now();
        let first_job = insert_queued_job(
            &pool,
            commit_id,
            first_id,
            "first",
            &first_drv,
            now - Duration::minutes(2),
        )
        .await;
        let second_job = insert_queued_job(
            &pool,
            commit_id,
            second_id,
            "second",
            &second_drv,
            now - Duration::minutes(1),
        )
        .await;

        let claim = claim_next_config_inspection_job(&pool)
            .await
            .expect("oldest job should claim")
            .expect("a queued job should exist");
        assert_eq!(claim.job_id, first_job);
        assert_eq!(claim.attempts, 1);
        assert_ne!(claim.execution_id, Uuid::nil());
        let row = get_config_inspection_job(&pool, first_job)
            .await
            .expect("claimed job should load")
            .expect("claimed job should exist");
        assert_eq!(row.status, ConfigInspectionJobStatus::Running);
        assert!(row.started_at.is_some());
        assert!(row.execution_heartbeat_at.is_some());
        assert_eq!(row.execution_id, Some(claim.execution_id));
        assert_eq!(
            get_config_inspection_job(&pool, second_job)
                .await
                .expect("second job should load")
                .expect("second job should exist")
                .status,
            ConfigInspectionJobStatus::Queued
        );
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn concurrent_claimers_claim_one_or_two_distinct_jobs(pool: PgPool) {
        let (_, one) = make_claimable_job(&pool, "one").await;
        let (_, two) = make_claimable_job(&pool, "two").await;
        let (left, right) = tokio::join!(
            claim_next_config_inspection_job(&pool),
            claim_next_config_inspection_job(&pool)
        );
        let left = left
            .expect("first concurrent claim should succeed")
            .expect("first job should claim");
        let right = right
            .expect("second concurrent claim should succeed")
            .expect("second job should claim");
        assert_ne!(left.job_id, right.job_id);
        assert_eq!(
            [one, two]
                .into_iter()
                .filter(|id| *id == left.job_id || *id == right.job_id)
                .count(),
            2
        );

        let (_, only) = make_claimable_job(&pool, "only").await;
        let (left, right) = tokio::join!(
            claim_next_config_inspection_job(&pool),
            claim_next_config_inspection_job(&pool)
        );
        let claims = [
            left.expect("single-row first claim should succeed"),
            right.expect("single-row second claim should succeed"),
        ];
        assert_eq!(claims.iter().filter_map(|claim| claim.as_ref()).count(), 1);
        assert!(claims.iter().flatten().all(|claim| claim.job_id == only));
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn execution_owner_fencing_and_redacted_failure_are_enforced(pool: PgPool) {
        let (_, job_id) = make_claimable_job(&pool, "owner").await;
        let claim = claim_next_config_inspection_job(&pool)
            .await
            .expect("job should claim")
            .expect("claim should exist");
        let wrong = Uuid::new_v4();
        assert!(
            !heartbeat_config_inspection_execution(&pool, job_id, wrong)
                .await
                .unwrap()
        );
        assert!(
            heartbeat_config_inspection_execution(&pool, job_id, claim.execution_id)
                .await
                .unwrap()
        );
        assert!(
            !complete_config_inspection_execution_success(&pool, job_id, wrong)
                .await
                .unwrap()
        );
        assert!(
            !complete_config_inspection_execution_failure(
                &pool,
                job_id,
                wrong,
                "Authorization: Bearer secret-value"
            )
            .await
            .unwrap()
        );
        let diagnostic = format!("Authorization: Bearer secret-value {}", "x".repeat(5000));
        assert!(
            complete_config_inspection_execution_failure(
                &pool,
                job_id,
                claim.execution_id,
                &diagnostic
            )
            .await
            .unwrap()
        );
        let row = get_config_inspection_job(&pool, job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.status, ConfigInspectionJobStatus::Failed);
        let error = row.error.unwrap();
        assert!(error.chars().count() <= MAX_CONFIG_INSPECTION_ERROR_CHARS);
        assert!(!error.contains("secret-value"));
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn stale_recovery_respects_live_locks_requeues_and_rotates_tokens(pool: PgPool) {
        let (_, job_id) = make_claimable_job(&pool, "recover").await;
        let first = claim_next_config_inspection_job(&pool)
            .await
            .unwrap()
            .unwrap();
        set_stale(&pool, job_id, first.attempts).await;
        let mut lock_conn = pool.acquire().await.unwrap();
        crate::queries::cve_scans::acquire_execution_lock(&mut lock_conn, first.execution_id)
            .await
            .unwrap();
        let locked = recover_stale_config_inspection_jobs(&pool, Duration::minutes(5))
            .await
            .unwrap();
        assert_eq!(locked.locked, 1);
        assert_eq!(locked.requeued, 0);
        assert_eq!(
            get_config_inspection_job(&pool, job_id)
                .await
                .unwrap()
                .unwrap()
                .status,
            ConfigInspectionJobStatus::Running
        );
        assert!(
            crate::queries::cve_scans::release_execution_lock(&mut lock_conn, first.execution_id)
                .await
                .unwrap()
        );
        let recovered = recover_stale_config_inspection_jobs(&pool, Duration::minutes(5))
            .await
            .unwrap();
        assert_eq!(recovered.requeued, 1);
        let queued = get_config_inspection_job(&pool, job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(queued.status, ConfigInspectionJobStatus::Queued);
        assert_eq!(queued.attempts, 1);
        assert!(queued.execution_id.is_none());
        assert!(queued.execution_heartbeat_at.is_none());
        assert!(queued.started_at.is_none());

        let second = claim_next_config_inspection_job(&pool)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(second.attempts, 2);
        assert_ne!(second.execution_id, first.execution_id);
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn stale_recovery_fences_successors_late_heartbeats_and_max_attempts(pool: PgPool) {
        let (_, job_id) = make_claimable_job(&pool, "fence").await;
        let first = claim_next_config_inspection_job(&pool)
            .await
            .unwrap()
            .unwrap();
        set_stale(&pool, job_id, first.attempts).await;
        let cutoff = Utc::now() - Duration::minutes(5);
        let recovered = recover_stale_config_inspection_execution(
            &pool,
            first.job_id,
            first.execution_id,
            cutoff,
        )
        .await
        .unwrap();
        assert_eq!(recovered, Some(ConfigInspectionJobStatus::Queued));
        let successor = claim_next_config_inspection_job(&pool)
            .await
            .unwrap()
            .unwrap();
        assert_ne!(successor.execution_id, first.execution_id);
        assert_eq!(
            recover_stale_config_inspection_execution(
                &pool,
                first.job_id,
                first.execution_id,
                cutoff
            )
            .await
            .unwrap(),
            None
        );
        assert_eq!(
            get_config_inspection_job(&pool, job_id)
                .await
                .unwrap()
                .unwrap()
                .execution_id,
            Some(successor.execution_id)
        );

        assert!(
            heartbeat_config_inspection_execution(&pool, job_id, successor.execution_id)
                .await
                .unwrap()
        );
        assert_eq!(
            recover_stale_config_inspection_execution(
                &pool,
                successor.job_id,
                successor.execution_id,
                cutoff
            )
            .await
            .unwrap(),
            None
        );

        set_stale(&pool, job_id, MAX_CONFIG_INSPECTION_ATTEMPTS).await;
        assert_eq!(
            recover_stale_config_inspection_jobs(&pool, Duration::minutes(5))
                .await
                .unwrap()
                .failed,
            1
        );
        let failed = get_config_inspection_job(&pool, job_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(failed.status, ConfigInspectionJobStatus::Failed);
        assert_eq!(failed.error.as_deref(), Some(MAX_ATTEMPTS_ERROR));
        assert_eq!(failed.attempts, MAX_CONFIG_INSPECTION_ATTEMPTS);
        assert_eq!(failed.execution_id, Some(successor.execution_id));
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn stale_recovery_is_bounded_and_does_not_touch_selectors(pool: PgPool) {
        let mut job_ids = Vec::new();
        for index in 0..33 {
            let (_, job_id) = make_claimable_job(&pool, &format!("bounded-{index}")).await;
            job_ids.push(job_id);
        }
        for job_id in &job_ids {
            let claim = claim_next_config_inspection_job(&pool)
                .await
                .unwrap()
                .unwrap();
            set_stale(&pool, *job_id, claim.attempts).await;
        }
        let before_primary: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM evaluation_snapshot_selections")
                .fetch_one(&pool)
                .await
                .unwrap();
        let before_config: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM config_snapshot_selections")
                .fetch_one(&pool)
                .await
                .unwrap();
        let summary = recover_stale_config_inspection_jobs(&pool, Duration::minutes(5))
            .await
            .unwrap();
        assert_eq!(summary.inspected, 32);
        assert_eq!(summary.requeued, 32);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM evaluation_snapshot_selections")
                .fetch_one(&pool)
                .await
                .unwrap(),
            before_primary
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM config_snapshot_selections")
                .fetch_one(&pool)
                .await
                .unwrap(),
            before_config
        );
        assert!(
            get_config_inspection_job(&pool, job_ids[0])
                .await
                .unwrap()
                .is_some()
        );
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn enqueue_uses_exact_targets_and_is_idempotent(pool: PgPool) {
        let (commit_id, derivation_id, drv_path) = fixture(&pool, "host").await;
        let target = successful_system(derivation_id, "host", &drv_path);

        let first = enqueue_config_inspection_jobs_for_successful_systems(
            &pool,
            commit_id,
            &[target.clone()],
        )
        .await
        .expect("first inspection enqueue should succeed");
        assert_eq!(first.requested_targets, 1);
        assert_eq!(first.inserted_jobs, 1);

        let second =
            enqueue_config_inspection_jobs_for_successful_systems(&pool, commit_id, &[target])
                .await
                .expect("duplicate inspection enqueue should succeed");
        assert_eq!(second.requested_targets, 1);
        assert_eq!(second.inserted_jobs, 0);
        assert_eq!(job_count(&pool, commit_id).await, 1);

        let claim = claim_next_config_inspection_job(&pool)
            .await
            .expect("inspection job should claim");
        assert!(claim.is_some());
        let third = enqueue_config_inspection_jobs_for_successful_systems(
            &pool,
            commit_id,
            &[successful_system(derivation_id, "host", &drv_path)],
        )
        .await
        .expect("running duplicate inspection enqueue should succeed");
        assert_eq!(third.inserted_jobs, 0);
        assert_eq!(job_count(&pool, commit_id).await, 1);

        let row = sqlx::query(
            "SELECT derivation_id, configuration_name, carrier_drv_path, status FROM config_inspection_jobs WHERE commit_id = $1",
        )
        .bind(commit_id)
        .fetch_one(&pool)
        .await
        .expect("inspection job should load");
        assert_eq!(row.get::<i32, _>("derivation_id"), derivation_id);
        assert_eq!(row.get::<String, _>("configuration_name"), "host");
        assert_eq!(row.get::<String, _>("carrier_drv_path"), drv_path);
        assert_eq!(row.get::<String, _>("status"), "running");
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn enqueue_batches_multiple_configs_and_rejects_mismatches(pool: PgPool) {
        let (commit_id, first_id, first_drv) = fixture(&pool, "first").await;
        let (_, second_id, second_drv) = fixture(&pool, "second").await;
        sqlx::query("UPDATE derivations SET commit_id = $1 WHERE id = $2")
            .bind(commit_id)
            .bind(second_id)
            .execute(&pool)
            .await
            .expect("second derivation should move to the fixture commit");

        let summary = enqueue_config_inspection_jobs_for_successful_systems(
            &pool,
            commit_id,
            &[
                successful_system(first_id, "first", &first_drv),
                successful_system(second_id, "second", &second_drv),
            ],
        )
        .await
        .expect("batched inspection enqueue should succeed");
        assert_eq!(summary.inserted_jobs, 2);
        assert_eq!(job_count(&pool, commit_id).await, 2);

        let wrong = successful_system(first_id, "wrong-name", &first_drv);
        assert!(
            enqueue_config_inspection_jobs_for_successful_systems(&pool, commit_id, &[wrong])
                .await
                .is_err()
        );
        assert_eq!(job_count(&pool, commit_id).await, 2);
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn enqueue_suppresses_only_ready_same_carrier_v2(pool: PgPool) {
        let (commit_id, derivation_id, drv_path) = fixture(&pool, "host").await;
        let target = successful_system(derivation_id, "host", &drv_path);
        insert_v2_snapshot(&pool, commit_id, "host", &drv_path, true).await;

        let summary =
            enqueue_config_inspection_jobs_for_successful_systems(&pool, commit_id, &[target])
                .await
                .expect("ready V2 artifact should suppress work");
        assert_eq!(summary.inserted_jobs, 0);
        assert_eq!(job_count(&pool, commit_id).await, 0);
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn enqueue_allows_incomplete_or_different_v2_and_terminal_retries(pool: PgPool) {
        let (commit_id, derivation_id, drv_path) = fixture(&pool, "host").await;
        let target = successful_system(derivation_id, "host", &drv_path);
        insert_v2_snapshot(&pool, commit_id, "host", &drv_path, false).await;

        let first = enqueue_config_inspection_jobs_for_successful_systems(
            &pool,
            commit_id,
            &[target.clone()],
        )
        .await
        .expect("incomplete V2 should enqueue");
        assert_eq!(first.inserted_jobs, 1);
        sqlx::query(
            "UPDATE config_inspection_jobs SET status = 'failed', started_at = now(), completed_at = now(), error = 'test failure', updated_at = now() WHERE commit_id = $1",
        )
        .bind(commit_id)
        .execute(&pool)
        .await
        .expect("terminal test job should update");
        let retry =
            enqueue_config_inspection_jobs_for_successful_systems(&pool, commit_id, &[target])
                .await
                .expect("terminal history should allow retry");
        assert_eq!(retry.inserted_jobs, 1);
        assert_eq!(job_count(&pool, commit_id).await, 2);
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn enqueue_matrix_does_not_accept_non_satisfying_snapshot_states(pool: PgPool) {
        let cases = [
            ("v1", false, "v1"),
            ("different-carrier", true, "different-carrier"),
            ("unavailable", true, "unavailable"),
            ("uncertified", true, "uncertified"),
        ];
        for (case_name, insert_v2, configuration_name) in cases {
            let (commit_id, derivation_id, drv_path) = fixture(&pool, case_name).await;
            let snapshot_id = Uuid::new_v4();
            if case_name == "v1" {
                sqlx::query(
                    "INSERT INTO evaluation_snapshots (id, commit_id, configuration_name, schema_version, lifecycle) VALUES ($1, $2, $3, 1, 'available')",
                )
                .bind(snapshot_id)
                .bind(commit_id)
                .bind(configuration_name)
                .execute(&pool)
                .await
                .expect("V1 snapshot should persist");
                sqlx::query(
                    "INSERT INTO evaluation_snapshot_selections (commit_id, configuration_name, current_snapshot_id) VALUES ($1, $2, $3)",
                )
                .bind(commit_id)
                .bind(configuration_name)
                .bind(snapshot_id)
                .execute(&pool)
                .await
                .expect("V1 evaluation selector should persist");
            } else if case_name == "different-carrier" {
                insert_v2_snapshot(
                    &pool,
                    commit_id,
                    configuration_name,
                    &format!("{drv_path}-other"),
                    true,
                )
                .await;
                let target = successful_system(derivation_id, configuration_name, &drv_path);
                let summary = enqueue_config_inspection_jobs_for_successful_systems(
                    &pool,
                    commit_id,
                    &[target],
                )
                .await
                .expect("different-carrier V2 should enqueue");
                assert_eq!(summary.inserted_jobs, 1);
                continue;
            } else if insert_v2 {
                sqlx::query(
                    "INSERT INTO evaluation_snapshots (id, commit_id, configuration_name, schema_version, lifecycle, carrier_drv_path, comparison_ready) VALUES ($1, $2, $3, 2, $4, $5, $6)",
                )
                .bind(snapshot_id)
                .bind(commit_id)
                .bind(configuration_name)
                .bind(if case_name == "unavailable" { "unavailable" } else { "available" })
                .bind(&drv_path)
                .bind(case_name != "unavailable")
                .execute(&pool)
                .await
                .expect("non-certified V2 snapshot should persist");
            }
            if case_name != "v1" {
                add_snapshot_selector(&pool, commit_id, configuration_name, snapshot_id).await;
            }
            let summary = enqueue_config_inspection_jobs_for_successful_systems(
                &pool,
                commit_id,
                &[successful_system(
                    derivation_id,
                    configuration_name,
                    &drv_path,
                )],
            )
            .await
            .expect("non-satisfying snapshot should enqueue");
            assert_eq!(summary.inserted_jobs, 1, "case {case_name}");
        }
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn enqueue_rejects_each_finalized_target_mismatch_atomically(pool: PgPool) {
        let (commit_id, derivation_id, drv_path) = fixture(&pool, "host").await;
        let (_, other_commit_derivation_id, other_drv_path) = fixture(&pool, "other-commit").await;
        let package_id: i32 = sqlx::query_scalar(
            "INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_path, status_id, attempt_count) VALUES ($1, 'package', 'package', $2, (SELECT id FROM derivation_statuses ORDER BY id LIMIT 1), 0) RETURNING id",
        )
        .bind(commit_id)
        .bind(format!("{drv_path}-package"))
        .fetch_one(&pool)
        .await
        .expect("package derivation should persist");
        let cases = [
            successful_system(other_commit_derivation_id, "other-commit", &other_drv_path),
            successful_system(derivation_id, "wrong-name", &drv_path),
            successful_system(derivation_id, "host", &format!("{drv_path}-wrong")),
            successful_system(package_id, "package", &format!("{drv_path}-package")),
        ];
        for target in cases {
            let result =
                enqueue_config_inspection_jobs_for_successful_systems(&pool, commit_id, &[target])
                    .await;
            assert!(result.is_err());
            assert_eq!(job_count(&pool, commit_id).await, 0);
        }
        let valid = successful_system(derivation_id, "host", &drv_path);
        let invalid = successful_system(derivation_id, "wrong-name", &drv_path);
        assert!(
            enqueue_config_inspection_jobs_for_successful_systems(
                &pool,
                commit_id,
                &[valid, invalid]
            )
            .await
            .is_err()
        );
        assert_eq!(job_count(&pool, commit_id).await, 0);
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn config_inspection_jobs_reject_target_mutation_and_bad_lifecycle(pool: PgPool) {
        let (commit_id, derivation_id, drv_path) = fixture(&pool, "host").await;
        let target = successful_system(derivation_id, "host", &drv_path);
        enqueue_config_inspection_jobs_for_successful_systems(&pool, commit_id, &[target])
            .await
            .expect("inspection job should enqueue");
        let mutation = sqlx::query(
            "UPDATE config_inspection_jobs SET carrier_drv_path = '/nix/store/other.drv' WHERE commit_id = $1",
        )
        .bind(commit_id)
        .execute(&pool)
        .await;
        assert!(mutation.is_err());
        let lifecycle = sqlx::query(
            "UPDATE config_inspection_jobs SET status = 'succeeded', completed_at = now() WHERE commit_id = $1",
        )
        .bind(commit_id)
        .execute(&pool)
        .await;
        assert!(lifecycle.is_err());
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn concurrent_enqueue_calls_share_one_active_row(pool: PgPool) {
        let (commit_id, derivation_id, drv_path) = fixture(&pool, "host").await;
        let target = successful_system(derivation_id, "host", &drv_path);
        let (left, right) = tokio::join!(
            enqueue_config_inspection_jobs_for_successful_systems(
                &pool,
                commit_id,
                std::slice::from_ref(&target)
            ),
            enqueue_config_inspection_jobs_for_successful_systems(
                &pool,
                commit_id,
                std::slice::from_ref(&target)
            ),
        );
        assert!(left.is_ok());
        assert!(right.is_ok());
        assert_eq!(job_count(&pool, commit_id).await, 1);
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn executor_rejects_forged_context_before_starting_a_process(pool: PgPool) {
        let (_, job_id) = make_claimable_job(&pool, "executor-context").await;
        let claim = claim_next_config_inspection_job(&pool)
            .await
            .expect("job should claim")
            .expect("claim should exist");
        let mut forged = claim.clone();
        forged.commit_id += 1;

        let outcome =
            crate::services::config_inspections::execute_claimed_config_inspection_with_program(
                &pool,
                forged,
                std::path::Path::new("/definitely/missing/nix-eval-jobs"),
            )
            .await
            .expect("forged context should be rejected as lost ownership");
        assert_eq!(
            outcome,
            crate::services::config_inspections::ConfigInspectionExecutionOutcome::LostOwnership
        );
        assert_eq!(
            get_config_inspection_job(&pool, job_id)
                .await
                .expect("claimed job should load")
                .expect("claimed job should exist")
                .status,
            ConfigInspectionJobStatus::Running
        );
    }

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL database with CREATEDB"]
    async fn executor_persists_stage2_unavailable_v2_atomically(pool: PgPool) {
        let (commit_id, derivation_id, carrier_drv_path) = fixture(&pool, "executor-success").await;
        let job_id = insert_queued_job(
            &pool,
            commit_id,
            derivation_id,
            "executor-success",
            &carrier_drv_path,
            Utc::now(),
        )
        .await;
        let claim = claim_next_config_inspection_job(&pool)
            .await
            .expect("job should claim")
            .expect("claim should exist");
        let (repo_url, commit_hash): (String, String) = sqlx::query_as(
            "SELECT flakes.repo_url, commits.git_commit_hash FROM commits JOIN flakes ON flakes.id = commits.flake_id WHERE commits.id = $1",
        )
        .bind(commit_id)
        .fetch_one(&pool)
        .await
        .expect("fixture commit lineage should load");
        let flake_ref = crate::derivations::utils::build_flake_reference(&repo_url, &commit_hash);
        let target =
            crate::models::config_inspector::InspectionTarget::new(&flake_ref, "executor-success");
        let stage1_lines = [
            json!({
                "attr": "__crystalForgeConfigIndex",
                "attrPath": ["__crystalForgeConfigIndex"],
                "drvPath": carrier_drv_path.clone(),
                "extraValue": {
                    "kind": "index",
                    "targetKey": target.target_key,
                    "sourceOutPath": "/nix/store/config-inspection-source",
                    "options": [],
                    "origins": []
                }
            }),
            json!({
                "attr": "__crystalForgeProvenance",
                "attrPath": ["__crystalForgeProvenance"],
                "drvPath": carrier_drv_path,
                "extraValue": {
                    "adapterVersion": 1,
                    "supported": false,
                    "reasonCode": "capability_self_test_failed"
                }
            }),
        ]
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
        let stage1_shell = stage1_lines
            .iter()
            .map(|line| format!("'{}'", line.replace('\'', "'\\''")))
            .collect::<Vec<_>>()
            .join(" ");
        let tempdir = tempfile::tempdir().expect("executor test tempdir should create");
        let program = tempdir.path().join("fake-nix-eval-jobs");
        std::fs::write(
            &program,
            format!(
                "#!/bin/sh\ncase \"$*\" in\n  *crystalForgeInspector*) printf '%s\\n' {} ;;\n  *) : ;;\nesac\n",
                stage1_shell
            ),
        )
        .expect("fake executor should write");
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&program)
            .expect("fake executor should stat")
            .permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&program, permissions)
            .expect("fake executor should be executable");

        let before_primary: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM evaluation_snapshot_selections")
                .fetch_one(&pool)
                .await
                .expect("primary selector count should load");
        let before_job = get_config_inspection_job(&pool, job_id)
            .await
            .expect("claimed job should load")
            .expect("claimed job should exist");
        let outcome =
            crate::services::config_inspections::execute_claimed_config_inspection_with_program(
                &pool, claim, &program,
            )
            .await
            .expect("executor should complete semantic unavailable result");
        let snapshot_id = match outcome {
            crate::services::config_inspections::ConfigInspectionExecutionOutcome::Succeeded {
                snapshot_id,
            } => snapshot_id,
            other => panic!("expected successful snapshot execution, got {other:?}"),
        };
        let after_job = get_config_inspection_job(&pool, job_id)
            .await
            .expect("completed job should load")
            .expect("completed job should exist");
        assert_eq!(after_job.status, ConfigInspectionJobStatus::Succeeded);
        assert_eq!(after_job.attempts, before_job.attempts);
        assert_eq!(
            sqlx::query_scalar::<_, Uuid>(
                "SELECT current_snapshot_id FROM config_snapshot_selections WHERE commit_id = $1 AND configuration_name = $2",
            )
            .bind(commit_id)
            .bind("executor-success")
            .fetch_one(&pool)
            .await
            .expect("V2 selector should advance"),
            snapshot_id
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM evaluation_snapshot_selections")
                .fetch_one(&pool)
                .await
                .expect("primary selector count should reload"),
            before_primary
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT lifecycle FROM evaluation_snapshots WHERE id = $1"
            )
            .bind(snapshot_id)
            .fetch_one(&pool)
            .await
            .expect("V2 snapshot should load"),
            "available"
        );
        assert!(
            !sqlx::query_scalar::<_, bool>(
                "SELECT comparison_ready FROM evaluation_snapshots WHERE id = $1"
            )
            .bind(snapshot_id)
            .fetch_one(&pool)
            .await
            .expect("V2 comparison readiness should load")
        );
    }
}
