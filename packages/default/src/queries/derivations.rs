use crate::models::commits::Commit;
// Add this line
use crate::models::derivations::build_agent_target;
use crate::models::derivations::{Derivation, DerivationType, parse_derivation_path};
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
) -> Result<crate::models::derivations::Derivation> {
    let commit_id = commit.map(|c| c.id);

    let derivation = sqlx::query_as!(
        crate::models::derivations::Derivation,
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
        VALUES ($1, $2, $3, $4, $5, 0, NOW(), $10)
        ON CONFLICT (COALESCE(commit_id, -1), derivation_name, derivation_type)
        DO UPDATE SET
            -- keep terminal states; otherwise reset
            status_id = CASE
                WHEN derivations.status_id IN ($6, $7, $8, $9) THEN derivations.status_id
                ELSE EXCLUDED.status_id
            END,
            -- keep/refresh target if provided
            derivation_target = COALESCE(EXCLUDED.derivation_target, derivations.derivation_target),
            -- nudge the scheduler only for non-terminal rows
            scheduled_at = CASE
                WHEN derivations.status_id IN ($6, $7, $8, $9) THEN derivations.scheduled_at
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
        // $6..$9  (terminal statuses)
        EvaluationStatus::DryRunComplete.as_id(),
        EvaluationStatus::DryRunFailed.as_id(),
        EvaluationStatus::BuildComplete.as_id(),
        EvaluationStatus::BuildFailed.as_id(),
        cf_agent_enabled
    )
    .fetch_one(pool)
    .await?;

    Ok(derivation)
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
            HostLatestTarget {
                hostname: hostname,
                derivation_id: r.derivation_id,
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
