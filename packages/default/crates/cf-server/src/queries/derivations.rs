use crate::models::commits::Commit;
// Add this line
use crate::derivations::{Derivation, DerivationType, build_agent_target, parse_derivation_path};
use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use sqlx::PgPool;
use sqlx::{Executor, Postgres};
use tracing::{debug, error, info, warn};

// Status IDs from the derivation_statuses table
// These should match the IDs you inserted in your migration
#[derive(Debug, Clone)]
pub enum EvaluationStatus {
    DryRunPending = 3,
    DryRunInProgress = 4,
    DryRunComplete = 5,
    DryRunFailed = 6,
    BuildPending = 7,
    BuildInProgress = 8,
    BuildComplete = 10,
    BuildFailed = 12,
}

impl EvaluationStatus {
    pub fn as_id(&self) -> i32 {
        self.clone() as i32
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            EvaluationStatus::DryRunComplete
                | EvaluationStatus::DryRunFailed
                | EvaluationStatus::BuildComplete
                | EvaluationStatus::BuildFailed
        )
    }

    pub fn is_in_progress(&self) -> bool {
        matches!(
            self,
            EvaluationStatus::DryRunInProgress | EvaluationStatus::BuildInProgress
        )
    }
}

/// Outcome of recording a synthetic evaluation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyntheticFailureWrite {
    /// Inserted a new derivation row with status DryRunFailed.
    Inserted { derivation_id: i32 },
    /// Updated an existing derivation from a non-terminal evaluation state
    /// (DryRunPending, DryRunInProgress) to DryRunFailed.
    UpdatedPendingEvaluation { derivation_id: i32 },
    /// An existing derivation was preserved because it is in a terminal or build
    /// state (DryRunComplete, DryRunFailed, BuildPending, BuildInProgress,
    /// BuildComplete, BuildFailed).
    PreservedExisting { derivation_id: i32, status_id: i32 },
}

/// Atomically record a synthetic evaluation failure for a derivation.
///
/// Uses a transaction with `SELECT ... FOR UPDATE` so the status check and
/// potential status transition are mutually atomic with respect to concurrent
/// build worker transitions.
///
/// Permitted state transitions:
/// - No existing row → insert directly as DryRunFailed (6)
/// - Dry-run evaluation states (3, 4, 5, 6) → DryRunFailed (6)
/// - All other statuses → preserved unchanged
///
/// When transitioning from 3 or 4, stale evaluation fields are explicitly
/// normalized (derivation_path, store_path, etc. are cleared) and the error
/// message is set.
///
/// This function also locks the row via `FOR UPDATE` and performs the
/// transition inside the same transaction, so a concurrent transition
/// (e.g. from 4 → 7 by a build worker) cannot race between the check and
/// the update. If the row was already modified concurrently, the `FOR UPDATE`
/// wait ensures we see the latest committed state before deciding.
/// Before commit, the function also acquires stable POA&M finding locks for
/// systems that currently deploy the derivation. A create or link action cannot
/// commit from an observation that this write supersedes.
pub async fn record_synthetic_eval_failure(
    pool: &PgPool,
    commit_id: Option<i32>,
    derivation_name: &str,
    derivation_type: &str,
    derivation_target: Option<&str>,
    error_message: &str,
) -> Result<SyntheticFailureWrite> {
    let mut tx = pool.begin().await?;

    // Atomically try to insert a new row as DryRunFailed. If the row already
    // exists (concurrent insert won), ON CONFLICT DO NOTHING returns no row
    // and we fall through to the existing-row path below. This avoids the
    // TOCTOU race between SELECT ... FOR UPDATE (which sees no row) and the
    // subsequent INSERT (which conflicts with a concurrent insert).
    let insert_result = sqlx::query_as::<_, (i32, i32)>(
        r#"
        INSERT INTO derivations (
            commit_id,
            derivation_type,
            derivation_name,
            derivation_target,
            status_id,
            attempt_count,
            error_message,
            policy_requirements_met,
            completed_at
        )
        VALUES ($1, $2, $3, $4, $5, 0, $6, FALSE, NOW())
        ON CONFLICT (COALESCE(commit_id, -1), derivation_name, derivation_type)
        DO NOTHING
        RETURNING id, status_id
        "#,
    )
    .bind(commit_id)
    .bind(derivation_type)
    .bind(derivation_name)
    .bind(derivation_target)
    .bind(EvaluationStatus::DryRunFailed.as_id())
    .bind(error_message)
    .fetch_optional(&mut *tx)
    .await?;

    let result = match insert_result {
        Some((id, _status_id)) => {
            // We successfully inserted the row. Since ON CONFLICT DO NOTHING
            // suppresses the conflict error, RETURNING only emits a row when
            // an actual insert happened.
            SyntheticFailureWrite::Inserted { derivation_id: id }
        }
        None => {
            // Row already exists (inserted concurrently by another transaction).
            // Lock it with FOR UPDATE (waits for the concurrent inserter to
            // commit) then check whether we need to transition the status.
            let existing = sqlx::query_as::<_, (i32, i32)>(
                r#"
                SELECT id, status_id
                FROM derivations
                WHERE COALESCE(commit_id, -1) = COALESCE($1, -1)
                  AND derivation_name = $2
                  AND derivation_type = $3
                FOR UPDATE
                "#,
            )
            .bind(commit_id)
            .bind(derivation_name)
            .bind(derivation_type)
            .fetch_optional(&mut *tx)
            .await?;

            match existing {
                Some((id, status_id)) => {
                    crate::services::composite_enforcement::lock_poam_findings_for_derivation_tx(
                        &mut tx, id,
                    )
                    .await?;
                    sqlx::query(
                        "DELETE FROM composite_policy_assessments WHERE derivation_id = $1",
                    )
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                    match status_id {
                        3 | 4 | 5 | 6 => {
                            // Dry-run evaluation states → DryRunFailed.
                            // Also normalize stale evaluation fields (clear them).
                            sqlx::query(
                                r#"
                            UPDATE derivations
                            SET status_id = $1,
                                error_message = $2,
                                completed_at = NOW(),
                                derivation_path = NULL,
                                store_path = NULL,
                                expected_store_path = NULL,
                                cf_agent_enabled = NULL,
                                policy_requirements_met = FALSE,
                                policy_results = '{}'::jsonb
                            WHERE id = $3
                                "#,
                            )
                            .bind(EvaluationStatus::DryRunFailed.as_id())
                            .bind(error_message)
                            .bind(id)
                            .execute(&mut *tx)
                            .await?;

                            SyntheticFailureWrite::UpdatedPendingEvaluation { derivation_id: id }
                        }
                        _ => {
                            sqlx::query(
                                "UPDATE derivations SET policy_requirements_met = FALSE, policy_results = '{}'::jsonb WHERE id = $1",
                            )
                            .bind(id)
                            .execute(&mut *tx)
                            .await?;
                            // Preserve build state while invalidating the failed
                            // reevaluation's deployment authorization state.
                            SyntheticFailureWrite::PreservedExisting {
                                derivation_id: id,
                                status_id,
                            }
                        }
                    }
                }
                None => {
                    // This should not happen — the CONFLICT proved someone else
                    // inserted, so the row must exist. Defensively bail.
                    anyhow::bail!(
                        "Concurrent race: row disappeared after INSERT ON CONFLICT DO NOTHING \
                         for {}",
                        derivation_name
                    );
                }
            }
        }
    };

    // CONCURRENCY: Publish refreshed policy results through the same stable
    // finding-key boundary used by POA&M create and link actions. The helper
    // also covers legacy policies that have no composite assessment rows.
    crate::services::composite_enforcement::lock_poam_findings_for_derivation_tx(
        &mut tx,
        match &result {
            SyntheticFailureWrite::Inserted { derivation_id }
            | SyntheticFailureWrite::UpdatedPendingEvaluation { derivation_id }
            | SyntheticFailureWrite::PreservedExisting { derivation_id, .. } => *derivation_id,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(result)
}

/// Inserts or updates a derivation entry, assigning the correct status via enum IDs.
pub async fn insert_derivation(
    pool: &PgPool,
    commit: Option<&Commit>,
    target_name: &str,
    target_type: &str,
) -> Result<Derivation> {
    let derivation_type = match target_type {
        "nixos" => "nixos",
        "package" => "package",
        _ => "nixos", // default
    };

    let commit_id = commit.map(|c| c.id);

    let derivation = sqlx::query_as!(
        Derivation,
        r#"
        INSERT INTO derivations (
            commit_id, 
            derivation_type, 
            derivation_name, 
            status_id,
            attempt_count
        )
        VALUES ($1, $2, $3, $4, 0)
        ON CONFLICT (COALESCE(commit_id, -1), derivation_name, derivation_type) 
        DO UPDATE SET
            status_id = CASE 
                WHEN derivations.status_id IN ($5, $6) THEN derivations.status_id  -- Keep terminal states
                ELSE EXCLUDED.status_id  -- Reset non-terminal states to pending-ish
            END
        RETURNING 
            id,
            commit_id,
            derivation_type as "derivation_type: DerivationType",
            derivation_name,
            derivation_path,
            derivation_target,
            scheduled_at,
            completed_at,
            started_at,
            attempt_count,
            evaluation_duration_ms,
            error_message,
            pname,
            version,
            status_id,
            build_elapsed_seconds,
            build_current_target,
            build_last_activity_seconds,
            build_last_heartbeat,
            cf_agent_enabled,
            store_path
        "#,
        commit_id,
        derivation_type,
        target_name,
        EvaluationStatus::DryRunPending.as_id(),
        EvaluationStatus::DryRunComplete.as_id(),
        EvaluationStatus::BuildComplete.as_id(),
    )
    .fetch_one(pool)
    .await?;

    Ok(derivation)
}

pub async fn insert_derivation_with_target(
    pool: &PgPool,
    commit: Option<&crate::models::commits::Commit>,
    derivation_name: &str,
    derivation_type: &str,
    derivation_target: Option<&str>,
    cf_agent_enabled: Option<bool>,
) -> Result<crate::derivations::Derivation> {
    let commit_id = commit.map(|c| c.id);

    let derivation = sqlx::query_as!(
        crate::derivations::Derivation,
        r#"
        INSERT INTO derivations (
            commit_id,
            derivation_type,
            derivation_name,
            derivation_target,
            status_id,
            attempt_count,
            scheduled_at,
            cf_agent_enabled
        )
        VALUES ($1, $2, $3, $4, $5, 0, NOW(), $12)
        ON CONFLICT (COALESCE(commit_id, -1), derivation_name, derivation_type)
        DO UPDATE SET
            -- keep terminal AND active-build states; otherwise reset
            status_id = CASE
                WHEN derivations.status_id IN ($6, $7, $8, $9, $10, $11) THEN derivations.status_id
                ELSE EXCLUDED.status_id
            END,
            -- keep/refresh target if provided
            derivation_target = COALESCE(EXCLUDED.derivation_target, derivations.derivation_target),
            -- nudge the scheduler only for preserved-state rows
            scheduled_at = CASE
                WHEN derivations.status_id IN ($6, $7, $8, $9, $10, $11) THEN derivations.scheduled_at
                ELSE NOW()
            END
        RETURNING
            id,
            commit_id,
            derivation_type as "derivation_type: DerivationType",
            derivation_name,
            derivation_path,
            derivation_target,
            scheduled_at,
            completed_at,
            started_at,
            attempt_count,
            evaluation_duration_ms,
            error_message,
            pname,
            version,
            status_id,
            build_elapsed_seconds,
            build_current_target,
            build_last_activity_seconds,
            build_last_heartbeat,
            cf_agent_enabled,
            store_path
        "#,
        // $1..$5
        commit_id,
        derivation_type,
        derivation_name,
        derivation_target,
        EvaluationStatus::DryRunPending.as_id(),
        // $6..$9  (statuses to preserve — terminal OR active build)
        EvaluationStatus::DryRunComplete.as_id(),
        EvaluationStatus::DryRunFailed.as_id(),
        EvaluationStatus::BuildPending.as_id(),
        EvaluationStatus::BuildInProgress.as_id(),
        EvaluationStatus::BuildComplete.as_id(),
        EvaluationStatus::BuildFailed.as_id(),
        cf_agent_enabled
    )
    .fetch_one(pool)
    .await?;

    Ok(derivation)
}

/// Outcome of atomically recording a successful evaluation result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuccessfulEvalWrite {
    /// A new derivation row was inserted directly as DryRunComplete.
    Inserted { derivation_id: i32 },
    /// An existing non-build-state row was transitioned to DryRunComplete.
    /// Callers should enqueue a build job.
    UpdatedEvaluationState { derivation_id: i32 },
    /// An existing row was in a build-active state (BuildPending, BuildInProgress,
    /// BuildComplete, BuildFailed) and was preserved unchanged.
    /// Callers must NOT enqueue a duplicate build job.
    PreservedBuildState { derivation_id: i32, status_id: i32 },
}

/// Atomically record a successful evaluation result for a derivation.
///
/// Uses a transaction with `SELECT ... FOR UPDATE` to ensure the status check
/// and any transition are atomic with respect to concurrent build-worker
/// transitions — the same pattern as `record_synthetic_eval_failure`.
///
/// Permitted state transitions:
/// - No row → insert directly as DryRunComplete (5)
/// - DryRunPending (3), DryRunInProgress (4), DryRunFailed (6),
///   DryRunComplete (5) → DryRunComplete (5), with all fields updated
///   (clears stale error_message, sets cf_agent_enabled, path, etc.)
/// - BuildPending (7), BuildInProgress (8), BuildComplete (10),
///   BuildFailed (12) → preserved unchanged (do NOT overwrite active/done builds)
///
/// Returns the derivation id and outcome so the caller can decide whether
/// to enqueue a build job (only for Inserted and UpdatedEvaluationState).
/// Before commit, the function also acquires stable POA&M finding locks for
/// systems that currently deploy the derivation. A create or link action cannot
/// commit from an observation that this write supersedes.
pub async fn record_successful_eval_result(
    pool: &PgPool,
    commit_id: Option<i32>,
    derivation_name: &str,
    derivation_type: &str,
    derivation_target: Option<&str>,
    derivation_path: &str,
    expected_store_path: Option<&str>,
    cf_agent_enabled: Option<bool>,
    policy_requirements_met: bool,
    policy_results: &serde_json::Value,
) -> Result<SuccessfulEvalWrite> {
    let mut tx = pool.begin().await?;

    // Attempt an atomic insert as DryRunComplete. ON CONFLICT DO NOTHING means
    // an existing row is left unchanged and no row is returned, letting us fall
    // through to the SELECT FOR UPDATE path.
    let insert_result = sqlx::query_as::<_, (i32,)>(
        r#"
        INSERT INTO derivations (
            commit_id,
            derivation_type,
            derivation_name,
            derivation_target,
            status_id,
            attempt_count,
            derivation_path,
            expected_store_path,
            cf_agent_enabled,
            policy_requirements_met,
            policy_results,
            error_message,
            completed_at,
            scheduled_at
        )
        VALUES ($1, $2, $3, $4, $5, 0, $6, $7, $8, $9, $10, NULL, NOW(), NOW())
        ON CONFLICT (COALESCE(commit_id, -1), derivation_name, derivation_type)
        DO NOTHING
        RETURNING id
        "#,
    )
    .bind(commit_id)
    .bind(derivation_type)
    .bind(derivation_name)
    .bind(derivation_target)
    .bind(EvaluationStatus::DryRunComplete.as_id())
    .bind(derivation_path)
    .bind(expected_store_path)
    .bind(cf_agent_enabled)
    .bind(policy_requirements_met)
    .bind(policy_results)
    .fetch_optional(&mut *tx)
    .await?;

    let result = match insert_result {
        Some((id,)) => SuccessfulEvalWrite::Inserted { derivation_id: id },
        None => {
            // Row already exists. Lock it and apply the state matrix.
            let existing = sqlx::query_as::<_, (i32, i32)>(
                r#"
                SELECT id, status_id
                FROM derivations
                WHERE COALESCE(commit_id, -1) = COALESCE($1, -1)
                  AND derivation_name = $2
                  AND derivation_type = $3
                FOR UPDATE
                "#,
            )
            .bind(commit_id)
            .bind(derivation_name)
            .bind(derivation_type)
            .fetch_optional(&mut *tx)
            .await?;

            match existing {
                Some((id, status_id)) => {
                    crate::services::composite_enforcement::lock_poam_findings_for_derivation_tx(
                        &mut tx, id,
                    )
                    .await?;
                    sqlx::query(
                        "DELETE FROM composite_policy_assessments WHERE derivation_id = $1",
                    )
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                    match status_id {
                        // Build-active or build-terminal: preserve the build
                        // status but still refresh policy metadata so that
                        // re-evaluation after a policy change produces
                        // up-to-date matrix results.
                        7 | 8 | 10 | 12 => {
                            sqlx::query(
                                r#"
                            UPDATE derivations
                            SET cf_agent_enabled = $1,
                                policy_requirements_met = $2,
                                policy_results = $3
                            WHERE id = $4
                            "#,
                            )
                            .bind(cf_agent_enabled)
                            .bind(policy_requirements_met)
                            .bind(policy_results)
                            .bind(id)
                            .execute(&mut *tx)
                            .await?;
                            SuccessfulEvalWrite::PreservedBuildState {
                                derivation_id: id,
                                status_id,
                            }
                        }
                        // DryRunPending (3), DryRunInProgress (4), DryRunFailed (6),
                        // DryRunComplete (5): update to DryRunComplete with fresh data,
                        // clearing any stale error.
                        _ => {
                            sqlx::query(
                            r#"
                            UPDATE derivations
                            SET status_id = $1,
                                derivation_path = $2,
                                derivation_target = COALESCE($3, derivation_target),
                                expected_store_path = $4,
                                cf_agent_enabled = $5,
                                policy_requirements_met = $6,
                                policy_results = $7,
                                error_message = NULL,
                                completed_at = NOW(),
                                evaluation_duration_ms =
                                    EXTRACT(EPOCH FROM (NOW() - COALESCE(started_at, scheduled_at))) * 1000
                            WHERE id = $8
                            "#,
                        )
                        .bind(EvaluationStatus::DryRunComplete.as_id())
                        .bind(derivation_path)
                        .bind(derivation_target)
                        .bind(expected_store_path)
                        .bind(cf_agent_enabled)
                        .bind(policy_requirements_met)
                        .bind(policy_results)
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;

                            SuccessfulEvalWrite::UpdatedEvaluationState { derivation_id: id }
                        }
                    }
                }
                None => {
                    // Row disappeared between our INSERT and the SELECT — should not
                    // happen in practice, but bail rather than silently dropping data.
                    anyhow::bail!(
                        "Concurrent race: row disappeared after INSERT ON CONFLICT DO NOTHING \
                         for {}",
                        derivation_name
                    );
                }
            }
        }
    };

    // CONCURRENCY: Publish refreshed policy results through the same stable
    // finding-key boundary used by POA&M create and link actions. The helper
    // also covers legacy policies that have no composite assessment rows.
    crate::services::composite_enforcement::lock_poam_findings_for_derivation_tx(
        &mut tx,
        match &result {
            SuccessfulEvalWrite::Inserted { derivation_id }
            | SuccessfulEvalWrite::UpdatedEvaluationState { derivation_id }
            | SuccessfulEvalWrite::PreservedBuildState { derivation_id, .. } => *derivation_id,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(result)
}

/// Transaction-aware variant of `record_successful_eval_result`.
///
/// Operates through a caller-owned transaction so multiple systems can be
/// written atomically; if any write fails the caller can roll back all of them.
///
/// State matrix, POA&M finding locking, and behavior are identical to
/// [`record_successful_eval_result`]. The caller holds the finding locks until
/// it commits or rolls back the transaction.
pub async fn record_successful_eval_result_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    commit_id: Option<i32>,
    derivation_name: &str,
    derivation_type: &str,
    derivation_target: Option<&str>,
    derivation_path: &str,
    expected_store_path: Option<&str>,
    cf_agent_enabled: Option<bool>,
    policy_requirements_met: bool,
    policy_results: &serde_json::Value,
) -> Result<SuccessfulEvalWrite> {
    let insert_result = sqlx::query_as::<_, (i32,)>(
        r#"
        INSERT INTO derivations (
            commit_id,
            derivation_type,
            derivation_name,
            derivation_target,
            status_id,
            attempt_count,
            derivation_path,
            expected_store_path,
            cf_agent_enabled,
            policy_requirements_met,
            policy_results,
            error_message,
            completed_at,
            scheduled_at
        )
        VALUES ($1, $2, $3, $4, $5, 0, $6, $7, $8, $9, $10, NULL, NOW(), NOW())
        ON CONFLICT (COALESCE(commit_id, -1), derivation_name, derivation_type)
        DO NOTHING
        RETURNING id
        "#,
    )
    .bind(commit_id)
    .bind(derivation_type)
    .bind(derivation_name)
    .bind(derivation_target)
    .bind(EvaluationStatus::DryRunComplete.as_id())
    .bind(derivation_path)
    .bind(expected_store_path)
    .bind(cf_agent_enabled)
    .bind(policy_requirements_met)
    .bind(policy_results)
    .fetch_optional(&mut **tx)
    .await?;

    let result = match insert_result {
        Some((id,)) => Ok::<_, anyhow::Error>(SuccessfulEvalWrite::Inserted { derivation_id: id }),
        None => {
            let existing = sqlx::query_as::<_, (i32, i32)>(
                r#"
                SELECT id, status_id
                FROM derivations
                WHERE COALESCE(commit_id, -1) = COALESCE($1, -1)
                  AND derivation_name = $2
                  AND derivation_type = $3
                FOR UPDATE
                "#,
            )
            .bind(commit_id)
            .bind(derivation_name)
            .bind(derivation_type)
            .fetch_optional(&mut **tx)
            .await?;

            match existing {
                Some((id, status_id)) => {
                    crate::services::composite_enforcement::lock_poam_findings_for_derivation_tx(
                        tx, id,
                    )
                    .await?;
                    sqlx::query(
                        "DELETE FROM composite_policy_assessments WHERE derivation_id = $1",
                    )
                    .bind(id)
                    .execute(&mut **tx)
                    .await?;
                    match status_id {
                        // Build-active or build-terminal: preserve the build
                        // status but still refresh policy metadata so that
                        // re-evaluation after a policy change produces
                        // up-to-date matrix results. Without this, a build
                        // that was queued under old policy assignments would
                        // retain stale policy_results forever.
                        7 | 8 | 10 | 12 => {
                            sqlx::query(
                                r#"
                            UPDATE derivations
                            SET cf_agent_enabled = $1,
                                policy_requirements_met = $2,
                                policy_results = $3
                                -- Do NOT update completed_at, derivation_path,
                                -- expected_store_path, or store_path: the build
                                -- (or potential build) associated with this row
                                -- already has its own state and timestamps, and
                                -- overwriting completed_at would distort elapsed-
                                -- time reporting and history ordering.
                            WHERE id = $4
                            "#,
                            )
                            .bind(cf_agent_enabled)
                            .bind(policy_requirements_met)
                            .bind(policy_results)
                            .bind(id)
                            .execute(&mut **tx)
                            .await?;
                            Ok(SuccessfulEvalWrite::PreservedBuildState {
                                derivation_id: id,
                                status_id,
                            })
                        }
                        _ => {
                            sqlx::query(
                            r#"
                            UPDATE derivations
                            SET status_id = $1,
                                derivation_path = $2,
                                derivation_target = COALESCE($3, derivation_target),
                                expected_store_path = $4,
                                cf_agent_enabled = $5,
                                policy_requirements_met = $6,
                                policy_results = $7,
                                error_message = NULL,
                                completed_at = NOW(),
                                evaluation_duration_ms =
                                    EXTRACT(EPOCH FROM (NOW() - COALESCE(started_at, scheduled_at))) * 1000
                            WHERE id = $8
                            "#,
                        )
                        .bind(EvaluationStatus::DryRunComplete.as_id())
                        .bind(derivation_path)
                        .bind(derivation_target)
                        .bind(expected_store_path)
                        .bind(cf_agent_enabled)
                        .bind(policy_requirements_met)
                        .bind(policy_results)
                        .bind(id)
                        .execute(&mut **tx)
                        .await?;
                            Ok(SuccessfulEvalWrite::UpdatedEvaluationState { derivation_id: id })
                        }
                    }
                }
                None => anyhow::bail!(
                    "Concurrent race: row disappeared after INSERT ON CONFLICT DO NOTHING for {}",
                    derivation_name
                ),
            }
        }
    }?;
    crate::services::composite_enforcement::lock_poam_findings_for_derivation_tx(
        tx,
        match &result {
            SuccessfulEvalWrite::Inserted { derivation_id }
            | SuccessfulEvalWrite::UpdatedEvaluationState { derivation_id }
            | SuccessfulEvalWrite::PreservedBuildState { derivation_id, .. } => *derivation_id,
        },
    )
    .await?;
    Ok(result)
}

/// Transaction-aware variant of `record_synthetic_eval_failure`.
///
/// Operates through a caller-owned transaction for atomic multi-system writes.
/// The caller holds the POA&M finding locks until it commits or rolls back the
/// transaction.
pub async fn record_synthetic_eval_failure_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    commit_id: Option<i32>,
    derivation_name: &str,
    derivation_type: &str,
    derivation_target: Option<&str>,
    error_message: &str,
) -> Result<SyntheticFailureWrite> {
    let insert_result = sqlx::query_as::<_, (i32, i32)>(
        r#"
        INSERT INTO derivations (
            commit_id,
            derivation_type,
            derivation_name,
            derivation_target,
            status_id,
            attempt_count,
            error_message,
            policy_requirements_met,
            completed_at
        )
        VALUES ($1, $2, $3, $4, $5, 0, $6, FALSE, NOW())
        ON CONFLICT (COALESCE(commit_id, -1), derivation_name, derivation_type)
        DO NOTHING
        RETURNING id, status_id
        "#,
    )
    .bind(commit_id)
    .bind(derivation_type)
    .bind(derivation_name)
    .bind(derivation_target)
    .bind(EvaluationStatus::DryRunFailed.as_id())
    .bind(error_message)
    .fetch_optional(&mut **tx)
    .await?;

    let result = match insert_result {
        Some((id, _)) => {
            Ok::<_, anyhow::Error>(SyntheticFailureWrite::Inserted { derivation_id: id })
        }
        None => {
            let existing = sqlx::query_as::<_, (i32, i32)>(
                r#"
                SELECT id, status_id
                FROM derivations
                WHERE COALESCE(commit_id, -1) = COALESCE($1, -1)
                  AND derivation_name = $2
                  AND derivation_type = $3
                FOR UPDATE
                "#,
            )
            .bind(commit_id)
            .bind(derivation_name)
            .bind(derivation_type)
            .fetch_optional(&mut **tx)
            .await?;

            match existing {
                Some((id, status_id)) => {
                    crate::services::composite_enforcement::lock_poam_findings_for_derivation_tx(
                        tx, id,
                    )
                    .await?;
                    sqlx::query(
                        "DELETE FROM composite_policy_assessments WHERE derivation_id = $1",
                    )
                    .bind(id)
                    .execute(&mut **tx)
                    .await?;
                    match status_id {
                        3 | 4 | 5 | 6 => {
                            sqlx::query(
                                r#"
                            UPDATE derivations
                            SET status_id = $1,
                                error_message = $2,
                                completed_at = NOW(),
                                derivation_path = NULL,
                                store_path = NULL,
                                expected_store_path = NULL,
                                cf_agent_enabled = NULL,
                                policy_requirements_met = FALSE,
                                policy_results = '{}'::jsonb
                            WHERE id = $3
                                "#,
                            )
                            .bind(EvaluationStatus::DryRunFailed.as_id())
                            .bind(error_message)
                            .bind(id)
                            .execute(&mut **tx)
                            .await?;
                            Ok(SyntheticFailureWrite::UpdatedPendingEvaluation {
                                derivation_id: id,
                            })
                        }
                        _ => {
                            sqlx::query(
                                "UPDATE derivations SET policy_requirements_met = FALSE, policy_results = '{}'::jsonb WHERE id = $1",
                            )
                            .bind(id)
                            .execute(&mut **tx)
                            .await?;
                            Ok(SyntheticFailureWrite::PreservedExisting {
                                derivation_id: id,
                                status_id,
                            })
                        }
                    }
                }
                None => anyhow::bail!(
                    "Concurrent race: row disappeared after INSERT ON CONFLICT DO NOTHING for {}",
                    derivation_name
                ),
            }
        }
    }?;
    crate::services::composite_enforcement::lock_poam_findings_for_derivation_tx(
        tx,
        match &result {
            SyntheticFailureWrite::Inserted { derivation_id }
            | SyntheticFailureWrite::UpdatedPendingEvaluation { derivation_id }
            | SyntheticFailureWrite::PreservedExisting { derivation_id, .. } => *derivation_id,
        },
    )
    .await?;
    Ok(result)
}

// Convenience function for the common case with a commit
pub async fn insert_derivation_for_commit(
    pool: &PgPool,
    commit: &Commit,
    target_name: &str,
    target_type: &str,
) -> Result<Derivation> {
    insert_derivation(pool, Some(commit), target_name, target_type).await
}

// Convenience function for packages without a specific commit
pub async fn insert_package_derivation(
    pool: &PgPool,
    package_name: &str,
    pname: Option<&str>,
    version: Option<&str>,
) -> Result<Derivation> {
    let inserted = sqlx::query_as!(
        Derivation,
        r#"
        INSERT INTO derivations (
            commit_id,
            derivation_type, 
            derivation_name,
            pname,
            version,
            status_id,
            attempt_count
        )
        VALUES ($1, $2, $3, $4, $5, $6, 0)
        ON CONFLICT (derivation_path) DO UPDATE SET
            derivation_name = EXCLUDED.derivation_name,
            pname = EXCLUDED.pname,
            version = EXCLUDED.version
        RETURNING 
            id,
            commit_id,
            derivation_type as "derivation_type: DerivationType",
            derivation_name,
            derivation_path,
            derivation_target,
            scheduled_at,
            completed_at,
            started_at,
            attempt_count,
            evaluation_duration_ms,
            error_message,
            pname,
            version,
            status_id,
            build_elapsed_seconds,
            build_current_target,
            build_last_activity_seconds,
            build_last_heartbeat,
            cf_agent_enabled,
            store_path
        "#,
        None::<i32>, // commit_id is NULL for standalone packages
        "package",
        package_name,
        pname,
        version,
        // Previously: EvaluationStatus::Complete
        // Use DryRunComplete to reflect “discovered/ready after dry-run”
        EvaluationStatus::DryRunComplete.as_id()
    )
    .fetch_one(pool)
    .await?;

    Ok(inserted)
}

/// Unified function to update derivation status with optional additional fields
pub async fn update_derivation_status(
    pool: &PgPool,
    target_id: i32,
    status: EvaluationStatus,
    derivation_path: Option<&str>,
    error_message: Option<&str>,
    store_path: Option<&str>,
) -> Result<Derivation> {
    let status_id = status.as_id();

    // Match in the SAME order as function args: (derivation_path, error_message, store_path)
    let updated = match (derivation_path, error_message, store_path) {
        // path + error + store
        (Some(path), Some(err), Some(nix_store)) => {
            if status.is_terminal() {
                sqlx::query_as!(
                    Derivation,
                    r#"
                    UPDATE derivations SET 
                        status_id = $1,
                        completed_at = NOW(),
                        evaluation_duration_ms = EXTRACT(EPOCH FROM (NOW() - started_at)) * 1000,
                        derivation_path = $2,
                        error_message = $3,
                        store_path = $5
                    WHERE id = $4
                    RETURNING
                        id,
                        commit_id,
                        derivation_type as "derivation_type: DerivationType",
                        derivation_name,
                        derivation_path,
                        derivation_target,
                        scheduled_at,
                        completed_at,
                        started_at,
                        attempt_count,
                        evaluation_duration_ms,
                        error_message,
                        pname,
                        version,
                        status_id,
                        build_elapsed_seconds,
                        build_current_target,
                        build_last_activity_seconds,
                        build_last_heartbeat,
                        cf_agent_enabled,
                        store_path
                    "#,
                    status_id,
                    path,
                    err,
                    target_id,
                    nix_store
                )
                .fetch_one(pool)
                .await?
            } else {
                sqlx::query_as!(
                    Derivation,
                    r#"
                    UPDATE derivations SET 
                        status_id = $1,
                        started_at = NOW(),
                        derivation_path = $2,
                        error_message = $3,
                        store_path = $5
                    WHERE id = $4
                    RETURNING
                        id,
                        commit_id,
                        derivation_type as "derivation_type: DerivationType",
                        derivation_name,
                        derivation_path,
                        derivation_target,
                        scheduled_at,
                        completed_at,
                        started_at,
                        attempt_count,
                        evaluation_duration_ms,
                        error_message,
                        pname,
                        version,
                        status_id,
                        build_elapsed_seconds,
                        build_current_target,
                        build_last_activity_seconds,
                        build_last_heartbeat,
                        cf_agent_enabled,
                        store_path
                    "#,
                    status_id,
                    path,
                    err,
                    target_id,
                    nix_store
                )
                .fetch_one(pool)
                .await?
            }
        }

        // path + store (no error)
        (Some(path), None, Some(nix_store)) => {
            if status.is_terminal() {
                sqlx::query_as!(
                    Derivation,
                    r#"
                    UPDATE derivations SET 
                        status_id = $1,
                        completed_at = NOW(),
                        evaluation_duration_ms = EXTRACT(EPOCH FROM (NOW() - started_at)) * 1000,
                        derivation_path = $2, 
                        store_path = $4
                    WHERE id = $3
                    RETURNING
                        id,
                        commit_id,
                        derivation_type as "derivation_type: DerivationType",
                        derivation_name,
                        derivation_path,
                        derivation_target,
                        scheduled_at,
                        completed_at,
                        started_at,
                        attempt_count,
                        evaluation_duration_ms,
                        error_message,
                        pname,
                        version,
                        status_id,
                        build_elapsed_seconds,
                        build_current_target,
                        build_last_activity_seconds,
                        build_last_heartbeat,
                        cf_agent_enabled,
                        store_path
                    "#,
                    status_id,
                    path,
                    target_id,
                    nix_store
                )
                .fetch_one(pool)
                .await?
            } else {
                sqlx::query_as!(
                    Derivation,
                    r#"
                    UPDATE derivations SET 
                        status_id = $1,
                        started_at = NOW(),
                        derivation_path = $2,
                        store_path = $4
                    WHERE id = $3
                    RETURNING
                        id,
                        commit_id,
                        derivation_type as "derivation_type: DerivationType",
                        derivation_name,
                        derivation_path,
                        derivation_target,
                        scheduled_at,
                        completed_at,
                        started_at,
                        attempt_count,
                        evaluation_duration_ms,
                        error_message,
                        pname,
                        version,
                        status_id,
                        build_elapsed_seconds,
                        build_current_target,
                        build_last_activity_seconds,
                        build_last_heartbeat,
                        cf_agent_enabled,
                        store_path
                    "#,
                    status_id,
                    path,
                    target_id,
                    nix_store
                )
                .fetch_one(pool)
                .await?
            }
        }

        // path only (no error, no store)
        (Some(path), None, None) => {
            if status.is_terminal() {
                sqlx::query_as!(
                    Derivation,
                    r#"
                    UPDATE derivations SET 
                        status_id = $1,
                        completed_at = NOW(),
                        evaluation_duration_ms = EXTRACT(EPOCH FROM (NOW() - started_at)) * 1000,
                        derivation_path = $2
                    WHERE id = $3
                    RETURNING
                        id,
                        commit_id,
                        derivation_type as "derivation_type: DerivationType",
                        derivation_name,
                        derivation_path,
                        derivation_target,
                        scheduled_at,
                        completed_at,
                        started_at,
                        attempt_count,
                        evaluation_duration_ms,
                        error_message,
                        pname,
                        version,
                        status_id,
                        build_elapsed_seconds,
                        build_current_target,
                        build_last_activity_seconds,
                        build_last_heartbeat,
                        cf_agent_enabled,
                        store_path
                    "#,
                    status_id,
                    path,
                    target_id
                )
                .fetch_one(pool)
                .await?
            } else {
                sqlx::query_as!(
                    Derivation,
                    r#"
                    UPDATE derivations SET 
                        status_id = $1,
                        started_at = NOW(),
                        derivation_path = $2
                    WHERE id = $3
                    RETURNING
                        id,
                        commit_id,
                        derivation_type as "derivation_type: DerivationType",
                        derivation_name,
                        derivation_path,
                        derivation_target,
                        scheduled_at,
                        completed_at,
                        started_at,
                        attempt_count,
                        evaluation_duration_ms,
                        error_message,
                        pname,
                        version,
                        status_id,
                        build_elapsed_seconds,
                        build_current_target,
                        build_last_activity_seconds,
                        build_last_heartbeat,
                        cf_agent_enabled,
                        store_path
                    "#,
                    status_id,
                    path,
                    target_id
                )
                .fetch_one(pool)
                .await?
            }
        }

        // path + error (no store)
        (Some(path), Some(err), None) => {
            if status.is_terminal() {
                sqlx::query_as!(
                    Derivation,
                    r#"
                    UPDATE derivations SET 
                        status_id = $1,
                        completed_at = NOW(),
                        evaluation_duration_ms = EXTRACT(EPOCH FROM (NOW() - started_at)) * 1000,
                        derivation_path = $2,
                        error_message = $3
                    WHERE id = $4
                    RETURNING
                        id,
                        commit_id,
                        derivation_type as "derivation_type: DerivationType",
                        derivation_name,
                        derivation_path,
                        derivation_target,
                        scheduled_at,
                        completed_at,
                        started_at,
                        attempt_count,
                        evaluation_duration_ms,
                        error_message,
                        pname,
                        version,
                        status_id,
                        build_elapsed_seconds,
                        build_current_target,
                        build_last_activity_seconds,
                        build_last_heartbeat,
                        cf_agent_enabled,
                        store_path
                    "#,
                    status_id,
                    path,
                    err,
                    target_id
                )
                .fetch_one(pool)
                .await?
            } else {
                sqlx::query_as!(
                    Derivation,
                    r#"
                    UPDATE derivations SET 
                        status_id = $1,
                        started_at = NOW(),
                        derivation_path = $2,
                        error_message = $3
                    WHERE id = $4
                    RETURNING
                        id,
                        commit_id,
                        derivation_type as "derivation_type: DerivationType",
                        derivation_name,
                        derivation_path,
                        derivation_target,
                        scheduled_at,
                        completed_at,
                        started_at,
                        attempt_count,
                        evaluation_duration_ms,
                        error_message,
                        pname,
                        version,
                        status_id,
                        build_elapsed_seconds,
                        build_current_target,
                        build_last_activity_seconds,
                        build_last_heartbeat,
                        cf_agent_enabled,
                        store_path
                    "#,
                    status_id,
                    path,
                    err,
                    target_id
                )
                .fetch_one(pool)
                .await?
            }
        }

        // error + store (no path)
        (None, Some(err), Some(nix_store)) => {
            if status.is_terminal() {
                sqlx::query_as!(
                    Derivation,
                    r#"
                    UPDATE derivations SET 
                        status_id = $1,
                        completed_at = NOW(),
                        evaluation_duration_ms = EXTRACT(EPOCH FROM (NOW() - started_at)) * 1000,
                        error_message = $2,
                        store_path = $4
                    WHERE id = $3
                    RETURNING
                        id,
                        commit_id,
                        derivation_type as "derivation_type: DerivationType",
                        derivation_name,
                        derivation_path,
                        derivation_target,
                        scheduled_at,
                        completed_at,
                        started_at,
                        attempt_count,
                        evaluation_duration_ms,
                        error_message,
                        pname,
                        version,
                        status_id,
                        build_elapsed_seconds,
                        build_current_target,
                        build_last_activity_seconds,
                        build_last_heartbeat,
                        cf_agent_enabled,
                        store_path
                    "#,
                    status_id,
                    err,
                    target_id,
                    nix_store
                )
                .fetch_one(pool)
                .await?
            } else {
                sqlx::query_as!(
                    Derivation,
                    r#"
                    UPDATE derivations SET 
                        status_id = $1,
                        started_at = NOW(),
                        error_message = $2,
                        store_path = $4
                    WHERE id = $3
                    RETURNING
                        id,
                        commit_id,
                        derivation_type as "derivation_type: DerivationType",
                        derivation_name,
                        derivation_path,
                        derivation_target,
                        scheduled_at,
                        completed_at,
                        started_at,
                        attempt_count,
                        evaluation_duration_ms,
                        error_message,
                        pname,
                        version,
                        status_id,
                        build_elapsed_seconds,
                        build_current_target,
                        build_last_activity_seconds,
                        build_last_heartbeat,
                        cf_agent_enabled,
                        store_path
                    "#,
                    status_id,
                    err,
                    target_id,
                    nix_store
                )
                .fetch_one(pool)
                .await?
            }
        }

        // error only (no path, no store)
        (None, Some(err), None) => {
            if status.is_terminal() {
                sqlx::query_as!(
                    Derivation,
                    r#"
                    UPDATE derivations SET 
                        status_id = $1,
                        completed_at = NOW(),
                        evaluation_duration_ms = EXTRACT(EPOCH FROM (NOW() - started_at)) * 1000,
                        error_message = $2
                    WHERE id = $3
                    RETURNING
                        id,
                        commit_id,
                        derivation_type as "derivation_type: DerivationType",
                        derivation_name,
                        derivation_path,
                        derivation_target,
                        scheduled_at,
                        completed_at,
                        started_at,
                        attempt_count,
                        evaluation_duration_ms,
                        error_message,
                        pname,
                        version,
                        status_id,
                        build_elapsed_seconds,
                        build_current_target,
                        build_last_activity_seconds,
                        build_last_heartbeat,
                        cf_agent_enabled,
                        store_path
                    "#,
                    status_id,
                    err,
                    target_id
                )
                .fetch_one(pool)
                .await?
            } else {
                sqlx::query_as!(
                    Derivation,
                    r#"
                    UPDATE derivations SET 
                        status_id = $1,
                        started_at = NOW(),
                        error_message = $2
                    WHERE id = $3
                    RETURNING
                        id,
                        commit_id,
                        derivation_type as "derivation_type: DerivationType",
                        derivation_name,
                        derivation_path,
                        derivation_target,
                        scheduled_at,
                        completed_at,
                        started_at,
                        attempt_count,
                        evaluation_duration_ms,
                        error_message,
                        pname,
                        version,
                        status_id,
                        build_elapsed_seconds,
                        build_current_target,
                        build_last_activity_seconds,
                        build_last_heartbeat,
                        cf_agent_enabled,
                        store_path
                    "#,
                    status_id,
                    err,
                    target_id
                )
                .fetch_one(pool)
                .await?
            }
        }

        // store only (no path, no error)
        (None, None, Some(nix_store)) => {
            if status.is_terminal() {
                sqlx::query_as!(
                    Derivation,
                    r#"
                    UPDATE derivations SET
                        status_id = $1,
                        completed_at = NOW(),
                        evaluation_duration_ms = EXTRACT(EPOCH FROM (NOW() - started_at)) * 1000,
                        store_path = $3
                    WHERE id = $2
                    RETURNING
                        id,
                        commit_id,
                        derivation_type as "derivation_type: DerivationType",
                        derivation_name,
                        derivation_path,
                        derivation_target,
                        scheduled_at,
                        completed_at,
                        started_at,
                        attempt_count,
                        evaluation_duration_ms,
                        error_message,
                        pname,
                        version,
                        status_id,
                        build_elapsed_seconds,
                        build_current_target,
                        build_last_activity_seconds,
                        build_last_heartbeat,
                        cf_agent_enabled,
                        store_path
                    "#,
                    status_id,
                    target_id,
                    nix_store
                )
                .fetch_one(pool)
                .await?
            } else if status.is_in_progress() {
                sqlx::query_as!(
                    Derivation,
                    r#"
                    UPDATE derivations SET
                        status_id = $1,
                        started_at = NOW(),
                        store_path = $3
                    WHERE id = $2
                    RETURNING
                        id,
                        commit_id,
                        derivation_type as "derivation_type: DerivationType",
                        derivation_name,
                        derivation_path,
                        derivation_target,
                        scheduled_at,
                        completed_at,
                        started_at,
                        attempt_count,
                        evaluation_duration_ms,
                        error_message,
                        pname,
                        version,
                        status_id,
                        build_elapsed_seconds,
                        build_current_target,
                        build_last_activity_seconds,
                        build_last_heartbeat,
                        cf_agent_enabled,
                        store_path
                    "#,
                    status_id,
                    target_id,
                    nix_store
                )
                .fetch_one(pool)
                .await?
            } else {
                sqlx::query_as!(
                    Derivation,
                    r#"
                    UPDATE derivations SET
                        status_id = $1,
                        store_path = $3
                    WHERE id = $2
                    RETURNING
                        id,
                        commit_id,
                        derivation_type as "derivation_type: DerivationType",
                        derivation_name,
                        derivation_path,
                        derivation_target,
                        scheduled_at,
                        completed_at,
                        started_at,
                        attempt_count,
                        evaluation_duration_ms,
                        error_message,
                        pname,
                        version,
                        status_id,
                        build_elapsed_seconds,
                        build_current_target,
                        build_last_activity_seconds,
                        build_last_heartbeat,
                        cf_agent_enabled,
                        store_path
                    "#,
                    status_id,
                    target_id,
                    nix_store
                )
                .fetch_one(pool)
                .await?
            }
        }

        // nothing special provided
        (None, None, None) => {
            if status.is_in_progress() {
                sqlx::query_as!(
                    Derivation,
                    r#"
                    UPDATE derivations SET 
                        status_id = $1,
                        started_at = NOW()
                    WHERE id = $2
                    RETURNING
                        id,
                        commit_id,
                        derivation_type as "derivation_type: DerivationType",
                        derivation_name,
                        derivation_path,
                        derivation_target,
                        scheduled_at,
                        completed_at,
                        started_at,
                        attempt_count,
                        evaluation_duration_ms,
                        error_message,
                        pname,
                        version,
                        status_id,
                        build_elapsed_seconds,
                        build_current_target,
                        build_last_activity_seconds,
                        build_last_heartbeat,
                        cf_agent_enabled,
                        store_path
                    "#,
                    status_id,
                    target_id
                )
                .fetch_one(pool)
                .await?
            } else if status.is_terminal() {
                sqlx::query_as!(
                    Derivation,
                    r#"
                    UPDATE derivations SET 
                        status_id = $1,
                        completed_at = NOW(),
                        evaluation_duration_ms = EXTRACT(EPOCH FROM (NOW() - started_at)) * 1000
                    WHERE id = $2
                    RETURNING
                        id,
                        commit_id,
                        derivation_type as "derivation_type: DerivationType",
                        derivation_name,
                        derivation_path,
                        derivation_target,
                        scheduled_at,
                        completed_at,
                        started_at,
                        attempt_count,
                        evaluation_duration_ms,
                        error_message,
                        pname,
                        version,
                        status_id,
                        build_elapsed_seconds,
                        build_current_target,
                        build_last_activity_seconds,
                        build_last_heartbeat,
                        cf_agent_enabled,
                        store_path
                    "#,
                    status_id,
                    target_id
                )
                .fetch_one(pool)
                .await?
            } else {
                sqlx::query_as!(
                    Derivation,
                    r#"
                    UPDATE derivations SET status_id = $1
                    WHERE id = $2
                    RETURNING
                        id,
                        commit_id,
                        derivation_type as "derivation_type: DerivationType",
                        derivation_name,
                        derivation_path,
                        derivation_target,
                        scheduled_at,
                        completed_at,
                        started_at,
                        attempt_count,
                        evaluation_duration_ms,
                        error_message,
                        pname,
                        version,
                        status_id,
                        build_elapsed_seconds,
                        build_current_target,
                        build_last_activity_seconds,
                        build_last_heartbeat,
                        cf_agent_enabled,
                        store_path
                    "#,
                    status_id,
                    target_id
                )
                .fetch_one(pool)
                .await?
            }
        }
    };

    Ok(updated)
}

// Simplified convenience functions that use the unified update function
pub async fn mark_derivation_dry_run_in_progress(
    pool: &PgPool,
    derivation_id: i32,
) -> Result<Derivation> {
    update_derivation_status(
        pool,
        derivation_id,
        EvaluationStatus::DryRunInProgress,
        None,
        None,
        None,
    )
    .await
}

pub async fn mark_derivation_dry_run_complete(
    pool: &PgPool,
    derivation_id: i32,
    derivation_path: &str,
) -> Result<Derivation> {
    update_derivation_status(
        pool,
        derivation_id,
        EvaluationStatus::DryRunComplete,
        Some(derivation_path),
        None,
        None,
    )
    .await
}

pub async fn set_expected_store_path(
    pool: &PgPool,
    derivation_id: i32,
    expected_store_path: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE derivations
        SET expected_store_path = $2
        WHERE id = $1
        "#,
    )
    .bind(derivation_id)
    .bind(expected_store_path)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn mark_derivation_build_in_progress(
    pool: &PgPool,
    derivation_id: i32,
) -> Result<Derivation> {
    update_derivation_status(
        pool,
        derivation_id,
        EvaluationStatus::BuildInProgress,
        None,
        None,
        None,
    )
    .await
}

pub async fn mark_derivation_build_complete(
    pool: &PgPool,
    derivation_id: i32,
    store_path: &str,
) -> Result<Derivation> {
    update_derivation_status(
        pool,
        derivation_id,
        EvaluationStatus::BuildComplete,
        None,
        None,
        Some(store_path),
    )
    .await
}

pub async fn mark_derivation_failed(
    pool: &PgPool,
    derivation_id: i32,
    phase: &str, // "dry-run" or "build"
    error_message: &str,
) -> Result<Derivation> {
    let status = match phase {
        "dry-run" => EvaluationStatus::DryRunFailed,
        "build" => EvaluationStatus::BuildFailed,
        _ => return Err(anyhow::anyhow!("Invalid phase: {}", phase)),
    };

    update_derivation_status(pool, derivation_id, status, None, Some(error_message), None).await
}

// Keeping the original function but updating it to use the new status
pub async fn update_derivation_path(
    pool: &PgPool,
    target: &Derivation,
    path: &str,
    store_path: &str,
) -> Result<Derivation> {
    update_derivation_status(
        pool,
        target.id,
        EvaluationStatus::DryRunComplete,
        Some(path),
        None,
        Some(store_path),
    )
    .await
}

pub async fn get_derivations_by_paths(pool: &PgPool, paths: &[&str]) -> Result<Vec<Derivation>> {
    // Convert &[&str] to Vec<String> for sqlx
    let paths_vec: Vec<String> = paths.iter().map(|s| s.to_string()).collect();

    sqlx::query_as!(
        Derivation,
        r#"
        SELECT 
            id, commit_id, 
            derivation_type as "derivation_type: DerivationType",
            derivation_name, derivation_path, derivation_target,
            scheduled_at, completed_at, started_at, attempt_count,
            evaluation_duration_ms, error_message, pname, version,
            status_id, build_elapsed_seconds, build_current_target,
            build_last_activity_seconds, build_last_heartbeat,
            cf_agent_enabled, store_path
        FROM derivations
        WHERE derivation_path = ANY($1)
        "#,
        &paths_vec
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch derivations by paths")
}

pub async fn get_derivation_by_id(pool: &PgPool, target_id: i32) -> Result<Derivation> {
    let target = sqlx::query_as!(
        Derivation,
        r#"
        SELECT
            id,
            commit_id,
            derivation_type as "derivation_type: DerivationType",
            derivation_name,
            derivation_path,
            derivation_target,
            scheduled_at,
            completed_at,
            started_at,
            attempt_count,
            evaluation_duration_ms,
            error_message,
            pname,
            version,
            status_id,
            build_elapsed_seconds,
            build_current_target,
            build_last_activity_seconds,
            build_last_heartbeat,
            cf_agent_enabled,
            store_path
        FROM derivations
        WHERE id = $1
        "#,
        target_id
    )
    .fetch_one(pool)
    .await?;

    Ok(target)
}

// Updated to get targets ready for dry-run
pub async fn get_pending_dry_run_derivations(pool: &PgPool) -> Result<Vec<Derivation>> {
    let rows = sqlx::query_as!(
        Derivation,
        r#"
        SELECT
            d.id,
            d.commit_id,
            d.derivation_type as "derivation_type: DerivationType",
            d.derivation_name,
            d.derivation_path,
            d.derivation_target,
            d.scheduled_at,
            d.completed_at,
            d.started_at,
            d.attempt_count,
            d.evaluation_duration_ms,
            d.error_message,
            d.pname,
            d.version,
            d.status_id,
            d.build_elapsed_seconds,
            d.build_current_target,
            d.build_last_activity_seconds,
            d.build_last_heartbeat,
            d.cf_agent_enabled,
            d.store_path
        FROM derivations d
        LEFT JOIN commits c ON d.commit_id = c.id
        WHERE d.status_id = $1
        AND d.attempt_count < 5
        ORDER BY c.commit_timestamp DESC NULLS LAST
        "#,
        EvaluationStatus::DryRunPending.as_id()
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

// New function to get targets ready for building
pub async fn get_derivations_ready_for_build(pool: &PgPool) -> Result<Vec<Derivation>> {
    let rows = sqlx::query_as!(
        Derivation,
        r#"
        SELECT
            d.id,
            d.commit_id,
            d.derivation_type as "derivation_type: DerivationType",
            d.derivation_name,
            d.derivation_path,
            d.derivation_target,
            d.scheduled_at,
            d.completed_at,
            d.started_at,
            d.attempt_count,
            d.evaluation_duration_ms,
            d.error_message,
            d.pname,
            d.version,
            d.status_id,
            d.build_elapsed_seconds,
            d.build_current_target,
            d.build_last_activity_seconds,
            d.build_last_heartbeat,
            d.cf_agent_enabled,
            d.store_path
        FROM derivations d
        INNER JOIN view_buildable_derivations vbd ON d.id = vbd.id
        ORDER BY vbd.queue_position
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn update_scheduled_at(pool: &PgPool) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE derivations
        SET scheduled_at = NOW() 
        WHERE status_id NOT IN ($1, $2, $3, $4)
        "#,
        EvaluationStatus::DryRunComplete.as_id(),
        EvaluationStatus::DryRunFailed.as_id(),
        EvaluationStatus::BuildComplete.as_id(),
        EvaluationStatus::BuildFailed.as_id()
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Increment the number of attempts for failed operations
pub async fn increment_derivation_attempt_count(
    pool: &PgPool,
    derivation: &Derivation,
    error: &anyhow::Error,
) -> Result<()> {
    error!("❌ Failed to process derivation: {}", error);
    sqlx::query!(
        r#"
        UPDATE derivations
        SET attempt_count = attempt_count + 1
        WHERE id = $1
        "#,
        derivation.id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Handle derivation failure with proper attempt count logic
pub async fn handle_derivation_failure<'e, E>(
    executor: E,
    derivation: &Derivation,
    phase: &str,
    error: &anyhow::Error,
) -> Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query!(
        r#"
        UPDATE derivations
        SET status_id = $1, 
            error_message = $2,
            attempt_count = attempt_count + 1,
            completed_at = NOW()
        WHERE id = $3
        "#,
        EvaluationStatus::BuildFailed.as_id(),
        format!("{}: {}", phase, error),
        derivation.id
    )
    .execute(executor)
    .await?;

    Ok(())
}

pub async fn reset_non_terminal_derivations(pool: &PgPool) -> Result<()> {
    // First, set derivations to terminal failed states if attempts >= 5
    let terminal_dry_run_result = sqlx::query!(
        r#"
        UPDATE derivations 
        SET status_id = $1
        WHERE derivation_path IS NULL 
        AND attempt_count >= 5
        AND status_id != $1  -- Only update if not already in terminal failed state
        "#,
        EvaluationStatus::DryRunFailed.as_id() // 6
    )
    .execute(pool)
    .await?;

    let terminal_build_result = sqlx::query!(
        r#"
        UPDATE derivations 
        SET status_id = $1
        WHERE derivation_path IS NOT NULL 
        AND attempt_count >= 5
        AND status_id != $1  -- Only update if not already in terminal failed state
        "#,
        EvaluationStatus::BuildFailed.as_id() // 12
    )
    .execute(pool)
    .await?;

    // Then, reset derivations that should be retried (attempts < 5)
    let reset_dry_run_result = sqlx::query!(
        r#"
        UPDATE derivations 
        SET status_id = $1, scheduled_at = NOW()
        WHERE derivation_path IS NULL 
        AND attempt_count < 5
        AND status_id NOT IN ($2, $3) -- success states that should never be reset
        "#,
        EvaluationStatus::DryRunPending.as_id(),  // 3
        EvaluationStatus::DryRunComplete.as_id(), // 5
        EvaluationStatus::BuildComplete.as_id()   // 10
    )
    .execute(pool)
    .await?;

    let reset_build_result = sqlx::query!(
        r#"
        UPDATE derivations 
        SET status_id = $1, scheduled_at = NOW()
        WHERE derivation_path IS NOT NULL 
        AND attempt_count < 5
        AND status_id NOT IN ($2, $3) -- success states that should never be reset
        "#,
        EvaluationStatus::BuildPending.as_id(),   // 7
        EvaluationStatus::DryRunComplete.as_id(), // 5
        EvaluationStatus::BuildComplete.as_id()   // 10
    )
    .execute(pool)
    .await?;

    let total_terminal =
        terminal_dry_run_result.rows_affected() + terminal_build_result.rows_affected();
    let total_reset = reset_dry_run_result.rows_affected() + reset_build_result.rows_affected();

    info!(
        "💡 Set {} derivations to terminal failed state (attempts >= 5)",
        total_terminal
    );
    info!(
        "💡 Reset {} derivations for retry (attempts < 5)",
        total_reset
    );
    info!(
        "💡 Total derivations processed: {}",
        total_terminal + total_reset
    );

    Ok(())
}

// Keeping the original function names for backward compatibility
pub async fn mark_target_dry_run_in_progress(pool: &PgPool, target_id: i32) -> Result<Derivation> {
    mark_derivation_dry_run_in_progress(pool, target_id).await
}

pub async fn mark_target_dry_run_complete(
    pool: &PgPool,
    target_id: i32,
    derivation_path: &str,
) -> Result<Derivation> {
    mark_derivation_dry_run_complete(pool, target_id, derivation_path).await
}

pub async fn mark_target_build_in_progress(pool: &PgPool, target_id: i32) -> Result<Derivation> {
    mark_derivation_build_in_progress(pool, target_id).await
}

pub async fn mark_target_build_complete<'e, E>(
    executor: E,
    derivation_id: i32,
    store_path: &str,
) -> Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query!(
        r#"
        UPDATE derivations
        SET status_id = $1, completed_at = NOW(), store_path = $2
        WHERE id = $3
        "#,
        EvaluationStatus::BuildComplete.as_id(),
        store_path,
        derivation_id
    )
    .execute(executor)
    .await?;

    Ok(())
}

pub async fn mark_target_failed(
    pool: &PgPool,
    target_id: i32,
    phase: &str,
    error_message: &str,
) -> Result<Derivation> {
    mark_derivation_failed(pool, target_id, phase, error_message).await
}

/// Marks a dependency build-plan calculation as active.
///
/// A new calculation clears any prior build count so clients cannot combine a
/// stale count with the `calculating` state.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot update the derivation row.
pub async fn mark_dependency_build_plan_calculating(
    pool: &PgPool,
    derivation_id: i32,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE derivations
           SET closure_total = NULL,
               closure_cached = NULL,
               dependency_build_count = NULL,
               dependency_build_plan_status = 'calculating'
         WHERE id = $1
        "#,
    )
    .bind(derivation_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Persists a completed dependency build plan.
///
/// `dependency_derivation_count` and `dependency_build_count` exclude the exact
/// top-level system derivation. The build count includes only derivations that
/// Nix reported in the dry-run build section under the effective build config.
///
/// # Errors
///
/// Returns an error when PostgreSQL rejects the counts or cannot update the row.
pub async fn complete_dependency_build_plan(
    pool: &PgPool,
    derivation_id: i32,
    dependency_derivation_count: i32,
    dependency_build_count: i32,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE derivations
           SET closure_total = $2,
               closure_cached = NULL,
               dependency_build_count = $3,
               dependency_build_plan_status = 'complete'
         WHERE id = $1
        "#,
    )
    .bind(derivation_id)
    .bind(dependency_derivation_count)
    .bind(dependency_build_count)
    .execute(pool)
    .await?;
    Ok(())
}

/// Marks dependency build-plan calculation as failed without inventing a count.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot update the derivation row.
pub async fn fail_dependency_build_plan(pool: &PgPool, derivation_id: i32) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE derivations
           SET closure_total = NULL,
               closure_cached = NULL,
               dependency_build_count = NULL,
               dependency_build_plan_status = 'failed'
         WHERE id = $1
        "#,
    )
    .bind(derivation_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Discover packages from derivation paths and insert them into the database
pub async fn discover_and_insert_packages(
    pool: &PgPool,
    parent_derivation_id: i32,
    derivation_paths: &[&str],
) -> Result<()> {
    use tracing::warn;

    if derivation_paths.is_empty() {
        return Ok(());
    }

    info!(
        "🔍 Analyzing {} derivation paths for package information",
        derivation_paths.len()
    );

    // NEW: Batch collect all valid packages first
    let mut packages_to_insert = Vec::new();

    for &drv_path in derivation_paths {
        if let Some(package_info) = parse_derivation_path(drv_path) {
            if drv_path.contains("nixos-system-") {
                debug!("⏭️ Skipping NixOS system derivation: {}", drv_path);
                continue;
            }

            let derivation_name = package_info
                .pname
                .as_ref()
                .map(|name| match &package_info.version {
                    Some(version) => format!("{}-{}", name, version),
                    None => name.clone(),
                })
                .unwrap_or_else(|| {
                    drv_path
                        .split('/')
                        .last()
                        .and_then(|s| s.strip_suffix(".drv"))
                        .and_then(|s| s.split_once('-').map(|(_, name)| name))
                        .unwrap_or(drv_path)
                        .to_string()
                });

            packages_to_insert.push((drv_path, derivation_name, package_info));
        }
    }

    if packages_to_insert.is_empty() {
        info!("No packages to insert");
        return Ok(());
    }

    // NEW: Batch insert all packages in a single transaction
    let mut tx = pool.begin().await?;

    for (drv_path, derivation_name, package_info) in packages_to_insert {
        let result = sqlx::query!(
            r#"
            WITH inserted AS (
                INSERT INTO derivations (
                    commit_id,
                    derivation_type, 
                    derivation_name, 
                    derivation_path, 
                    pname, 
                    version, 
                    status_id, 
                    attempt_count
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, 0)
                ON CONFLICT (derivation_path) DO UPDATE SET
                    derivation_name = EXCLUDED.derivation_name,
                    pname = EXCLUDED.pname,
                    version = EXCLUDED.version
                RETURNING id
            )
            INSERT INTO derivation_dependencies (derivation_id, depends_on_id)
            SELECT $8, id FROM inserted
            ON CONFLICT (derivation_id, depends_on_id) DO NOTHING
            "#,
            None::<i32>,
            "package",
            derivation_name,
            drv_path,
            package_info.pname.as_deref(),
            package_info.version.as_deref(),
            EvaluationStatus::DryRunComplete.as_id(),
            parent_derivation_id
        )
        .execute(&mut *tx)
        .await;

        if let Err(e) = result {
            warn!("⚠️ Failed to insert package {}: {}", drv_path, e);
        }
    }

    tx.commit().await?;
    info!("✅ Completed package discovery");
    Ok(())
}

pub async fn update_derivation_path_and_metadata(
    pool: &PgPool,
    derivation_id: i32,
    derivation_path: &str,
    pname: Option<&str>,
    version: Option<&str>,
) -> Result<()> {
    let pname = pname.ok_or_else(|| anyhow!("missing pname"))?;
    let version = version.ok_or_else(|| anyhow!("missing version"))?;
    let name = format!("{}-{}", pname, version);
    sqlx::query!(
        r#"
        UPDATE derivations 
        SET 
            derivation_name = $5,
            derivation_path = $1,
            pname = $2,
            version = $3
        WHERE id = $4
        "#,
        derivation_path,
        pname,
        version,
        derivation_id,
        name
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn update_derivation_build_status<'e, E>(
    executor: E,
    derivation_id: i32,
    elapsed_seconds: i32,
    current_build_target: Option<&str>,
    last_activity_seconds: i32,
) -> Result<()>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query!(
        r#"
        UPDATE derivations SET
            build_elapsed_seconds = $1,
            build_current_target = $2,
            build_last_activity_seconds = $3,
            build_last_heartbeat = NOW()
        WHERE id = $4
        "#,
        elapsed_seconds,
        current_build_target,
        last_activity_seconds,
        derivation_id
    )
    .execute(executor)
    .await?;

    Ok(())
}

pub async fn clear_derivation_build_status(pool: &PgPool, derivation_id: i32) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE derivations SET
            build_elapsed_seconds = NULL,
            build_current_target = NULL,
            build_last_activity_seconds = NULL,
            build_last_heartbeat = NULL
        WHERE id = $1
        "#,
        derivation_id
    )
    .execute(pool)
    .await?;

    Ok(())
}

#[derive(Debug, Clone)]
pub struct HostLatestTarget {
    pub hostname: String,
    pub derivation_id: i32,
    pub commit_hash: String,
    pub derivation_target: Option<String>,
    pub store_path: Option<String>,
    pub last_cache_completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

// src/db/queries.rs
pub async fn get_latest_deployable_targets_for_flake_hosts(
    pool: &PgPool,
    flake_id: i32,
    hostnames: &[String],
) -> Result<Vec<HostLatestTarget>> {
    if hostnames.is_empty() {
        return Ok(vec![]);
    }

    // NOTE: pass `hostnames` as a TEXT[] (Vec<String>) to $2
    let rows = sqlx::query!(
        r#"
        WITH latest_commit AS (
          SELECT id, flake_id, git_commit_hash
          FROM commits
          WHERE flake_id = $1
          ORDER BY commit_timestamp DESC
          LIMIT 1
        ),
        per_host AS (
          SELECT
            d.derivation_name AS hostname,
            d.id              AS derivation_id,
            d.derivation_target,
            d.store_path,
            f.repo_url        AS repo_url,
            lc.git_commit_hash AS commit_hash,
            MAX(cpj.completed_at) AS last_cache_completed_at,
            ROW_NUMBER() OVER (
              PARTITION BY d.derivation_name
              ORDER BY
                MAX(cpj.completed_at) DESC NULLS LAST,
                MAX(d.completed_at)   DESC NULLS LAST,
                MAX(d.id)             DESC
            ) AS rn
          FROM derivations d
          JOIN latest_commit lc
            ON d.commit_id = lc.id
          JOIN flakes f
            ON lc.flake_id = f.id
          JOIN cache_push_jobs cpj
            ON cpj.derivation_id = d.id
           AND cpj.status = 'completed'
          WHERE d.derivation_type = 'nixos'
            AND d.derivation_target IS NOT NULL
            AND d.derivation_name = ANY($2::text[])
            AND d.cf_agent_enabled IS TRUE
            AND d.policy_requirements_met IS TRUE
          GROUP BY
            d.derivation_name,
            d.id,
            d.derivation_target,
            d.store_path,
            f.repo_url,
            lc.git_commit_hash
        )
        SELECT
          hostname,
          derivation_id,
          derivation_target,
          store_path,
          last_cache_completed_at,
          repo_url,
          commit_hash
        FROM per_host
        WHERE rn = 1
        "#,
        flake_id,
        hostnames
    )
    .fetch_all(pool)
    .await?;

    let out = rows
        .into_iter()
        .map(|r| {
            let hostname = r.hostname.clone();
            let commit_hash = r.commit_hash.clone();
            HostLatestTarget {
                hostname: hostname,
                derivation_id: r.derivation_id,
                commit_hash,
                store_path: r.store_path,
                derivation_target: Some(build_agent_target(
                    &r.repo_url,
                    &r.commit_hash,
                    &r.hostname,
                )),
                last_cache_completed_at: r.last_cache_completed_at,
            }
        })
        .collect();

    Ok(out)
}

pub async fn mark_derivation_cache_pushed(pool: &PgPool, derivation_id: i32) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE derivations 
        SET status_id = $1
        WHERE id = $2
        "#,
        14_i32, // cache-pushed status
        derivation_id
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Update the cf_agent_enabled field for a derivation
pub async fn update_cf_agent_enabled(
    pool: &PgPool,
    derivation_id: i32,
    cf_agent_enabled: bool,
) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE derivations 
        SET cf_agent_enabled = $1
        WHERE id = $2
        "#,
        cf_agent_enabled,
        derivation_id
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Batch create cache push jobs for all built derivations missing jobs
pub async fn batch_queue_cache_jobs(pool: &PgPool, destination: &str) -> Result<usize> {
    let count = sqlx::query_scalar!(
        r#"
        INSERT INTO cache_push_jobs (derivation_id, store_path, cache_destination, status)
        SELECT 
            d.id,
            d.store_path,
            $1,
            'pending'
        FROM derivations d
        WHERE d.status_id = 10  -- build-complete
            AND d.store_path IS NOT NULL
            AND NOT EXISTS (
                SELECT 1 FROM cache_push_jobs cpj 
                WHERE cpj.derivation_id = d.id
                AND cpj.cache_destination = $1  -- CHECK SPECIFIC DESTINATION
            )
        RETURNING id
        "#,
        destination
    )
    .fetch_all(pool)
    .await?
    .len();

    if count > 0 {
        info!("📤 Batch queued {} cache push jobs", count);
    }

    Ok(count)
}

/// Reset a derivation back to dry-run-complete status when store path is missing
pub async fn reset_derivation_for_rebuild(pool: &PgPool, derivation_id: i32) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE derivations
        SET 
            status_id = 5,  -- dry-run-complete
            store_path = NULL,
            completed_at = NULL,
            error_message = 'Store path was garbage collected, needs rebuild'
        WHERE id = $1
        "#,
        derivation_id
    )
    .execute(pool)
    .await?;

    info!(
        "Reset derivation {} to dry-run-complete for rebuild",
        derivation_id
    );
    Ok(())
}

pub async fn batch_mark_derivations_complete(
    pool: &PgPool,
    deriv_ids: &[i32],
    store_paths: &[String],
) -> Result<()> {
    use anyhow::Context;

    sqlx::query!(
        r#"
        UPDATE derivations
        SET 
            status_id = $1,
            completed_at = NOW(),
            evaluation_duration_ms = EXTRACT(EPOCH FROM (NOW() - started_at)) * 1000,
            store_path = data.store_path
        FROM (
            SELECT UNNEST($2::int[]) as id, UNNEST($3::text[]) as store_path
        ) as data
        WHERE derivations.id = data.id
        "#,
        EvaluationStatus::DryRunComplete as i32,
        deriv_ids,
        store_paths
    )
    .execute(pool)
    .await
    .context("Failed to batch mark derivations as complete")?;

    Ok(())
}

/// Update the database with build progress information
pub async fn update_build_heartbeat(
    pool: &PgPool,
    derivation_id: i32,
    elapsed_seconds: i32,
    current_target: Option<&str>,
    last_activity_seconds: i32,
) -> Result<()> {
    sqlx::query!(
        r#"
            UPDATE derivations
            SET 
                build_elapsed_seconds = $1,
                build_current_target = $2,
                build_last_activity_seconds = $3,
                build_last_heartbeat = NOW()
            WHERE id = $4
            "#,
        elapsed_seconds,
        current_target,
        last_activity_seconds,
        derivation_id
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Queue derivations for building after successful dry run
///
/// This transitions derivations from DryRunComplete (5) to BuildPending (7)
/// so they can be claimed by builders.
pub async fn queue_derivations_for_build(pool: &PgPool, commit_id: i32) -> Result<usize> {
    let result = sqlx::query!(
        r#"
        UPDATE derivations
        SET 
            status_id = $1,  -- BuildPending (7)
            scheduled_at = COALESCE(scheduled_at, NOW())
        WHERE commit_id = $2
        AND status_id = $3  -- DryRunComplete (5)
        AND derivation_type = 'nixos'
        RETURNING id
        "#,
        EvaluationStatus::BuildPending.as_id(), // 7
        commit_id,
        EvaluationStatus::DryRunComplete.as_id(), // 5
    )
    .fetch_all(pool)
    .await?;

    let count = result.len();

    if count > 0 {
        info!(
            "Queued {} derivations for building (commit_id={})",
            count, commit_id
        );
    }

    Ok(count)
}

/// Queue all derivations that completed dry run
///
/// This is useful for batch operations or catching up after server restart
pub async fn queue_all_ready_derivations(pool: &PgPool) -> Result<usize> {
    let result = sqlx::query!(
        r#"
        UPDATE derivations
        SET 
            status_id = $1,  -- BuildPending (7)
            scheduled_at = COALESCE(scheduled_at, NOW())
        WHERE status_id = $2  -- DryRunComplete (5)
        AND derivation_type = 'nixos'
        RETURNING id
        "#,
        EvaluationStatus::BuildPending.as_id(),   // 7
        EvaluationStatus::DryRunComplete.as_id(), // 5
    )
    .fetch_all(pool)
    .await?;

    let count = result.len();

    if count > 0 {
        info!("Queued {} derivations for building", count);
    }

    Ok(count)
}

/// Auto-queue derivations with CF agent enabled
///
/// This can be called after evaluation to immediately queue systems
/// that have the CF agent enabled, skipping dry run.
pub async fn auto_queue_cf_agent_systems(pool: &PgPool, commit_id: i32) -> Result<usize> {
    let result = sqlx::query!(
        r#"
        UPDATE derivations
        SET 
            status_id = $1,  -- BuildPending (7)
            scheduled_at = COALESCE(scheduled_at, NOW())
        WHERE commit_id = $2
        AND cf_agent_enabled = true
        AND derivation_type = 'nixos'
        AND status_id IN ($3, $4)  -- DryRunPending or DryRunComplete
        RETURNING id
        "#,
        EvaluationStatus::BuildPending.as_id(), // 7
        commit_id,
        EvaluationStatus::DryRunPending.as_id(),  // 3
        EvaluationStatus::DryRunComplete.as_id(), // 5
    )
    .fetch_all(pool)
    .await?;

    let count = result.len();

    if count > 0 {
        info!(
            "Auto-queued {} CF agent systems for building (commit_id={})",
            count, commit_id
        );
    }

    Ok(count)
}

pub async fn cleanup_partial_derivations(pool: &PgPool) -> Result<()> {
    sqlx::query!(
        r#"
        DELETE FROM derivations
        WHERE derivation_path IS NULL
          AND status_id IN (3, 4)  -- Only delete pending/in-progress
        "#
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Reset all in-progress builds on startup
/// This ensures clean state when server restarts mid-build
pub async fn reset_stuck_builds(pool: &PgPool) -> Result<()> {
    let reset = sqlx::query!(
        r#"
        UPDATE derivations
        SET 
            status_id = 7,  -- build-pending
            started_at = NULL
        WHERE status_id = 8  -- build-inprogress
        RETURNING id, derivation_name
        "#
    )
    .fetch_all(pool)
    .await?;

    if !reset.is_empty() {
        warn!("🧹 Reset {} in-progress builds on startup", reset.len());
        for row in &reset {
            info!("  - Derivation {} ({})", row.id, row.derivation_name);
        }
    }

    Ok(())
}

/// Look up a derivation ID by its built store path.
/// Returns None if no derivation with that store path exists.
pub async fn get_derivation_id_by_store_path(
    pool: &PgPool,
    store_path: &str,
) -> Result<Option<i32>> {
    let id =
        sqlx::query_scalar::<_, i32>("SELECT id FROM derivations WHERE store_path = $1 LIMIT 1")
            .bind(store_path)
            .fetch_optional(pool)
            .await?;
    Ok(id)
}

// ── Derivation state-machine regression tests ────────────────────────────────
//
// Tests for record_successful_eval_result covering the build-state-preservation
// matrix (P1-2 fix).  Requires an isolated database:
//
//   DATABASE_URL=postgres://crystal_forge:password@localhost:3042/crystal_forge \
//     cargo test -p cf-server --lib queries::derivations::tests \
//     -- --ignored --test-threads=1

#[cfg(test)]
mod tests {
    use super::{
        SuccessfulEvalWrite, record_successful_eval_result, record_synthetic_eval_failure,
    };
    use sqlx::PgPool;

    fn test_database_url() -> String {
        std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .expect(
                "CRYSTAL_FORGE_TEST_DATABASE_URL or DATABASE_URL must be set for database tests",
            )
    }

    async fn test_pool() -> PgPool {
        PgPool::connect(&test_database_url())
            .await
            .expect("failed to connect to test database")
    }

    async fn insert_throwaway_flake(pool: &PgPool) -> i32 {
        sqlx::query_scalar::<_, i32>(
            "INSERT INTO flakes (name, repo_url, branch) VALUES ($1, $2, 'main') RETURNING id",
        )
        .bind(format!("drv-test-flake-{}", uuid::Uuid::new_v4().simple()))
        .bind(format!(
            "https://git.example/drv-test-{}.git",
            uuid::Uuid::new_v4().simple()
        ))
        .fetch_one(pool)
        .await
        .expect("failed to insert test flake")
    }

    async fn insert_throwaway_commit(pool: &PgPool, flake_id: i32) -> i32 {
        sqlx::query_scalar::<_, i32>(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) \
             VALUES ($1, $2, NOW()) RETURNING id",
        )
        .bind(flake_id)
        .bind(uuid::Uuid::new_v4().simple().to_string())
        .fetch_one(pool)
        .await
        .expect("failed to insert test commit")
    }

    // ── Test A: successful eval inserts a new DryRunComplete row ─────────────
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn successful_eval_inserts_new_row_as_dry_run_complete() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let name = format!("sys-insert-{}", uuid::Uuid::new_v4().simple());
        let policy_results = serde_json::json!({});

        let result = record_successful_eval_result(
            &pool,
            Some(commit_id),
            &name,
            "nixos",
            Some("/nix/store/fake.target"),
            "/nix/store/fake.drv",
            None,
            Some(true),
            true,
            &policy_results,
        )
        .await
        .expect("record_successful_eval_result should not error");

        assert!(
            matches!(result, SuccessfulEvalWrite::Inserted { .. }),
            "first write must be Inserted"
        );

        let status: i32 = sqlx::query_scalar("SELECT status_id FROM derivations WHERE id = $1")
            .bind(match result {
                SuccessfulEvalWrite::Inserted { derivation_id } => derivation_id,
                _ => unreachable!(),
            })
            .fetch_one(&pool)
            .await
            .expect("query should succeed");
        assert_eq!(status, 5, "new row must be DryRunComplete (5)");

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Test B: successful eval preserves BuildPending (7) ───────────────────
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn successful_eval_preserves_build_pending() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let name = format!("sys-bp-{}", uuid::Uuid::new_v4().simple());
        let policy_results = serde_json::json!({});

        // Insert row then manually set BuildPending (7).
        record_successful_eval_result(
            &pool,
            Some(commit_id),
            &name,
            "nixos",
            None,
            "/nix/store/fake.drv",
            None,
            Some(true),
            true,
            &policy_results,
        )
        .await
        .expect("insert should succeed");

        sqlx::query("UPDATE derivations SET status_id = 7 WHERE COALESCE(commit_id,-1) = $1 AND derivation_name = $2 AND derivation_type = 'nixos'")
            .bind(commit_id)
            .bind(&name)
            .execute(&pool)
            .await
            .expect("force to BuildPending");

        // Retry eval — must NOT overwrite BuildPending.
        let result = record_successful_eval_result(
            &pool,
            Some(commit_id),
            &name,
            "nixos",
            None,
            "/nix/store/updated.drv",
            None,
            Some(true),
            true,
            &policy_results,
        )
        .await
        .expect("retry should not error");

        assert!(
            matches!(
                result,
                SuccessfulEvalWrite::PreservedBuildState { status_id: 7, .. }
            ),
            "BuildPending must be preserved on retry; got {:?}",
            result
        );

        let status: i32 = sqlx::query_scalar(
            "SELECT status_id FROM derivations WHERE COALESCE(commit_id,-1) = $1 AND derivation_name = $2 AND derivation_type = 'nixos'",
        )
        .bind(commit_id)
        .bind(&name)
        .fetch_one(&pool)
        .await
        .expect("query should succeed");
        assert_eq!(status, 7, "status must remain BuildPending (7)");

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Test C: successful eval preserves BuildInProgress (8) ────────────────
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn successful_eval_preserves_build_in_progress() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let name = format!("sys-bip-{}", uuid::Uuid::new_v4().simple());
        let policy_results = serde_json::json!({});

        record_successful_eval_result(
            &pool,
            Some(commit_id),
            &name,
            "nixos",
            None,
            "/nix/store/fake.drv",
            None,
            Some(true),
            true,
            &policy_results,
        )
        .await
        .expect("insert should succeed");

        sqlx::query("UPDATE derivations SET status_id = 8 WHERE COALESCE(commit_id,-1) = $1 AND derivation_name = $2 AND derivation_type = 'nixos'")
            .bind(commit_id)
            .bind(&name)
            .execute(&pool)
            .await
            .expect("force to BuildInProgress");

        let result = record_successful_eval_result(
            &pool,
            Some(commit_id),
            &name,
            "nixos",
            None,
            "/nix/store/updated.drv",
            None,
            Some(true),
            true,
            &policy_results,
        )
        .await
        .expect("retry should not error");

        assert!(
            matches!(
                result,
                SuccessfulEvalWrite::PreservedBuildState { status_id: 8, .. }
            ),
            "BuildInProgress must be preserved; got {:?}",
            result
        );

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }

    // ── Test D: successful eval clears stale error after synthetic failure ────
    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn successful_eval_clears_stale_error_from_synthetic_failure() {
        let pool = test_pool().await;
        let flake_id = insert_throwaway_flake(&pool).await;
        let commit_id = insert_throwaway_commit(&pool, flake_id).await;
        let name = format!("sys-stale-err-{}", uuid::Uuid::new_v4().simple());
        let policy_results = serde_json::json!({});

        // First attempt: synthetic failure.
        record_synthetic_eval_failure(
            &pool,
            Some(commit_id),
            &name,
            "nixos",
            None,
            "nix-eval-jobs silently dropped this system",
        )
        .await
        .expect("synthetic failure should not error");

        // Verify it has an error_message and status 6.
        let (status, err): (i32, Option<String>) = sqlx::query_as(
            "SELECT status_id, error_message FROM derivations WHERE COALESCE(commit_id,-1) = $1 AND derivation_name = $2 AND derivation_type = 'nixos'",
        )
        .bind(commit_id)
        .bind(&name)
        .fetch_one(&pool)
        .await
        .expect("query should succeed");
        assert_eq!(status, 6, "initial status must be DryRunFailed (6)");
        assert!(err.is_some(), "synthetic failure must set error_message");

        // Second attempt: successful eval.
        let result = record_successful_eval_result(
            &pool,
            Some(commit_id),
            &name,
            "nixos",
            Some("/nix/store/target"),
            "/nix/store/recovered.drv",
            None,
            Some(true),
            true,
            &policy_results,
        )
        .await
        .expect("recovery should not error");

        assert!(
            matches!(result, SuccessfulEvalWrite::UpdatedEvaluationState { .. }),
            "recovery from DryRunFailed must be UpdatedEvaluationState; got {:?}",
            result
        );

        let (status, err, agent): (i32, Option<String>, Option<bool>) = sqlx::query_as(
            "SELECT status_id, error_message, cf_agent_enabled FROM derivations WHERE COALESCE(commit_id,-1) = $1 AND derivation_name = $2 AND derivation_type = 'nixos'",
        )
        .bind(commit_id)
        .bind(&name)
        .fetch_one(&pool)
        .await
        .expect("query should succeed");

        assert_eq!(status, 5, "recovered row must be DryRunComplete (5)");
        assert!(
            err.is_none(),
            "stale error_message must be cleared on recovery"
        );
        assert_eq!(agent, Some(true), "cf_agent_enabled must be persisted");

        let _ = sqlx::query("DELETE FROM flakes WHERE id = $1")
            .bind(flake_id)
            .execute(&pool)
            .await;
    }
}
