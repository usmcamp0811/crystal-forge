use crate::models::commits::Commit;
use crate::models::config::BuildConfig; // Add this line
use crate::models::derivations::{Derivation, DerivationType, parse_derivation_path};
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use sqlx::PgPool;
use sqlx::{Executor, Postgres};
use tracing::{debug, error, info, warn};

// Status IDs from the derivation_statuses table
// These should match the IDs you inserted in your migration
#[derive(Debug, Clone)]
pub enum EvaluationStatus {
    Pending = 1,
    Queued = 2,
    DryRunPending = 3,
    DryRunInProgress = 4,
    DryRunComplete = 5,
    DryRunFailed = 6,
    BuildPending = 7,
    BuildInProgress = 8,
    InProgress = 9,
    BuildComplete = 10,
    Complete = 11,
    BuildFailed = 12,
    Failed = 13,
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
                | EvaluationStatus::Complete
                | EvaluationStatus::Failed
        )
    }

    pub fn is_in_progress(&self) -> bool {
        matches!(
            self,
            EvaluationStatus::DryRunInProgress
                | EvaluationStatus::BuildInProgress
                | EvaluationStatus::InProgress
        )
    }
}

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
                WHEN derivations.status_id IN (5, 6, 10, 12) THEN derivations.status_id  -- Keep terminal states
                ELSE EXCLUDED.status_id  -- Reset non-terminal states to pending
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
        EvaluationStatus::DryRunPending.as_id()
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
            scheduled_at
        )
        VALUES ($1, $2, $3, $4, $5, 0, NOW())
        ON CONFLICT (COALESCE(commit_id, -1), derivation_name, derivation_type)
        DO UPDATE SET
            -- keep terminal states; otherwise reset to pending
            status_id = CASE
                WHEN derivations.status_id IN (5, 6, 10, 12, 11, 13) THEN derivations.status_id
                ELSE EXCLUDED.status_id
            END,
            -- keep/refresh target if provided
            derivation_target = COALESCE(EXCLUDED.derivation_target, derivations.derivation_target),
            -- nudge the scheduler only for non-terminal rows
            scheduled_at = CASE
                WHEN derivations.status_id IN (5, 6, 10, 12, 11, 13) THEN derivations.scheduled_at
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
        commit_id,
        derivation_type,
        derivation_name,
        derivation_target,
        EvaluationStatus::DryRunPending.as_id(),
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
        EvaluationStatus::Complete.as_id() // Packages found during scanning are already "complete"
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
        WHERE status_id = $1
        AND attempt_count < 5
        ORDER BY scheduled_at DESC
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
        WITH all_nixos_commits AS (
            SELECT 
                d.id as nixos_deriv_id,
                d.derivation_name as hostname,
                c.commit_timestamp,
                c.id as commit_id
            FROM derivations d
            INNER JOIN commits c ON d.commit_id = c.id
            WHERE d.derivation_type = 'nixos'
              AND d.status_id IN ($1, $2)
        )
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
        LEFT JOIN derivation_dependencies dd ON dd.depends_on_id = d.id
        LEFT JOIN all_nixos_commits n ON dd.derivation_id = n.nixos_deriv_id
        LEFT JOIN all_nixos_commits anc ON d.id = anc.nixos_deriv_id
        WHERE d.status_id IN ($1, $2)
          AND (
              (d.derivation_type = 'package' AND n.hostname IS NOT NULL)
              OR
              (d.derivation_type = 'nixos' AND anc.hostname IS NOT NULL)
          )
        ORDER BY 
            CASE 
                WHEN d.derivation_type = 'package' THEN COALESCE(n.hostname, 'zzz_orphan')
                WHEN d.derivation_type = 'nixos' THEN anc.hostname
            END ASC,
            CASE 
                WHEN d.derivation_type = 'package' THEN n.commit_timestamp
                WHEN d.derivation_type = 'nixos' THEN anc.commit_timestamp
            END DESC NULLS LAST,
            CASE 
                WHEN d.derivation_type = 'package' THEN 0
                WHEN d.derivation_type = 'nixos' THEN 1
            END ASC,
            d.id ASC
        "#,
        EvaluationStatus::DryRunComplete.as_id(),
        EvaluationStatus::BuildPending.as_id()
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
pub async fn handle_derivation_failure(
    pool: &PgPool,
    derivation: &Derivation,
    phase: &str, // "dry-run" or "build"
    error: &anyhow::Error,
) -> Result<Derivation> {
    // First increment the attempt count
    let new_attempt_count = derivation.attempt_count + 1;

    // Determine if this should be terminal based on attempt count
    let (status, is_terminal_failure) = match phase {
        "dry-run" => {
            if new_attempt_count >= 5 {
                (EvaluationStatus::DryRunFailed, true)
            } else {
                // For <5 attempts, we could use a different status like "DryRunFailed"
                // but the reset logic will pick it up and retry it
                (EvaluationStatus::DryRunFailed, false)
            }
        }
        "build" => {
            if new_attempt_count >= 5 {
                (EvaluationStatus::BuildFailed, true)
            } else {
                (EvaluationStatus::BuildFailed, false)
            }
        }
        _ => return Err(anyhow::anyhow!("Invalid phase: {}", phase)),
    };

    // Update with both attempt count and status in a single transaction
    let updated = if is_terminal_failure {
        sqlx::query_as!(
            Derivation,
            r#"
            UPDATE derivations SET 
                status_id = $1,
                attempt_count = $2,
                completed_at = NOW(),
                evaluation_duration_ms = EXTRACT(EPOCH FROM (NOW() - started_at)) * 1000,
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
            status.as_id(),
            new_attempt_count,
            error.to_string(),
            derivation.id
        )
        .fetch_one(pool)
        .await?
    } else {
        sqlx::query_as!(
            Derivation,
            r#"
            UPDATE derivations SET 
                status_id = $1,
                attempt_count = $2,
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
            status.as_id(),
            new_attempt_count,
            error.to_string(),
            derivation.id
        )
        .fetch_one(pool)
        .await?
    };

    if is_terminal_failure {
        error!(
            "❌ Derivation {} terminally failed after {} attempts: {}",
            updated.derivation_name, new_attempt_count, error
        );
    } else {
        warn!(
            "⚠️ Derivation {} failed (attempt {}): {}",
            updated.derivation_name, new_attempt_count, error
        );
    }

    Ok(updated)
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
        EvaluationStatus::DryRunFailed.as_id() // $1 = 6
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
        EvaluationStatus::BuildFailed.as_id() // $1 = 12
    )
    .execute(pool)
    .await?;

    // Then, reset derivations that should be retried (attempts < 5)
    // Reset ALL non-terminal derivations with < 5 attempts to pending states
    let reset_dry_run_result = sqlx::query!(
        r#"
        UPDATE derivations 
        SET status_id = $1, scheduled_at = NOW()
        WHERE derivation_path IS NULL 
        AND attempt_count < 5
        AND status_id NOT IN ($2, $3) -- Only exclude success states that should never be reset
        "#,
        EvaluationStatus::DryRunPending.as_id(),  // $1 = 3
        EvaluationStatus::DryRunComplete.as_id(), // $2 = 5 (success - never reset)
        EvaluationStatus::BuildComplete.as_id()   // $3 = 10 (success - never reset)
    )
    .execute(pool)
    .await?;

    let reset_build_result = sqlx::query!(
        r#"
        UPDATE derivations 
        SET status_id = $1, scheduled_at = NOW()
        WHERE derivation_path IS NOT NULL 
        AND attempt_count < 5
        AND status_id NOT IN ($2, $3) -- Only exclude success states that should never be reset
        "#,
        EvaluationStatus::BuildPending.as_id(),   // $1 = 7
        EvaluationStatus::DryRunComplete.as_id(), // $2 = 5 (success - never reset)
        EvaluationStatus::BuildComplete.as_id()   // $3 = 10 (success - never reset)
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

pub async fn mark_target_build_complete(
    pool: &PgPool,
    target_id: i32,
    store_path: &str,
) -> Result<Derivation> {
    mark_derivation_build_complete(pool, target_id, store_path).await
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

    info!(
        "🔍 Analyzing {} derivation paths for package information",
        derivation_paths.len()
    );

    for &drv_path in derivation_paths {
        // Parse derivation path to extract package information
        if let Some(package_info) = parse_derivation_path(drv_path) {
            // Skip the main NixOS system derivation - it should already exist as the parent
            if drv_path.contains("nixos-system-") {
                debug!("⏭️ Skipping NixOS system derivation: {}", drv_path);
                continue;
            }

            // Use the parsed package name as the derivation_name, fallback to derivation path
            let derivation_name = package_info
                .pname
                .as_ref()
                .map(|name| {
                    // Include version in the name if available for better identification
                    match &package_info.version {
                        Some(version) => format!("{}-{}", name, version),
                        None => name.clone(),
                    }
                })
                .unwrap_or_else(|| {
                    // Extract a reasonable name from the derivation path if parsing fails
                    drv_path
                        .split('/')
                        .last()
                        .and_then(|s| s.strip_suffix(".drv"))
                        .and_then(|s| s.split_once('-').map(|(_, name)| name))
                        .unwrap_or(drv_path)
                        .to_string()
                });

            // Insert package derivation - using DO NOTHING to skip duplicates
            let package_derivation = sqlx::query_as!(
                Derivation,
                r#"
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
                None::<i32>,     // commit_id is NULL for discovered packages
                "package",       // derivation_type = "package"
                derivation_name, // derivation_name (uses parsed package name)
                drv_path,        // derivation_path (the actual .drv path)
                package_info.pname.as_deref(), // pname
                package_info.version.as_deref(), // version
                EvaluationStatus::DryRunComplete.as_id()
            )
            .fetch_one(pool)
            .await;

            match package_derivation {
                Ok(pkg_deriv) => {
                    // CORRECT dependency relationship: The NixOS system (parent) depends on packages
                    let dependency_result = sqlx::query!(
                        r#"
                        INSERT INTO derivation_dependencies (derivation_id, depends_on_id)
                        VALUES ($1, $2)
                        ON CONFLICT (derivation_id, depends_on_id) DO NOTHING
                        "#,
                        parent_derivation_id, // The NixOS system derivation
                        pkg_deriv.id          // depends on this package
                    )
                    .execute(pool)
                    .await;

                    if let Err(e) = dependency_result {
                        warn!(
                            "⚠️ Failed to insert dependency relationship for {}: {}",
                            drv_path, e
                        );
                    }
                }
                Err(e) => {
                    warn!("⚠️ Failed to insert package derivation {}: {}", drv_path, e);
                }
            }
        }
    }

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

pub async fn get_latest_successful_derivation_for_flake(
    pool: &PgPool,
    flake_id: i32,
) -> Result<Option<Derivation>> {
    let derivation = sqlx::query_as::<_, Derivation>(
        r#"
        SELECT 
            d.id,
            d.commit_id,
            d.derivation_type,
            d.derivation_name,
            d.derivation_path,
            d.scheduled_at,
            d.completed_at,
            d.started_at,
            d.attempt_count,
            d.evaluation_duration_ms,
            d.error_message,
            d.pname,
            d.version,
            d.status_id,
            d.derivation_target,
            d.build_elapsed_seconds,
            d.build_current_target,
            d.build_last_activity_seconds,
            d.build_last_heartbeat,
            d.cf_agent_enabled,
            d.store_path
        FROM derivations d
        INNER JOIN commits c ON d.commit_id = c.id
        INNER JOIN derivation_statuses ds ON d.status_id = ds.id
        WHERE c.flake_id = $1
        AND ds.name = 'cache-pushed'
        AND ds.is_success = true
        AND d.derivation_type = 'nixos'
        ORDER BY d.completed_at DESC
        LIMIT 1
        "#,
    )
    .bind(flake_id)
    .fetch_optional(pool)
    .await?;

    Ok(derivation)
}

/// Discover and queue all transitive dependencies for NixOS systems
pub async fn discover_and_queue_all_transitive_dependencies(
    pool: &PgPool,
    build_config: &BuildConfig,
) -> Result<()> {
    // Get all NixOS systems that have completed dry-run but haven't had dependencies discovered
    let nixos_systems = sqlx::query_as!(
        Derivation,
        r#"
        SELECT
            id, commit_id, derivation_type as "derivation_type: DerivationType",
            derivation_name, derivation_path, derivation_target, scheduled_at,
            completed_at, started_at, attempt_count, evaluation_duration_ms,
            error_message, pname, version, status_id, build_elapsed_seconds,
            build_current_target, build_last_activity_seconds, build_last_heartbeat,
            cf_agent_enabled, store_path
        FROM derivations 
        WHERE derivation_type = 'nixos' 
        AND status_id = $1
        AND NOT EXISTS (
            SELECT 1 FROM derivation_dependencies dd WHERE dd.derivation_id = derivations.id
        )
        "#,
        EvaluationStatus::DryRunComplete.as_id()
    )
    .fetch_all(pool)
    .await?;

    for nixos_system in nixos_systems {
        info!(
            "Discovering dependencies for NixOS system: {}",
            nixos_system.derivation_name
        );

        if let Err(e) =
            discover_all_transitive_dependencies_for_system(pool, &nixos_system, build_config).await
        {
            error!(
                "Failed to discover dependencies for {}: {}",
                nixos_system.derivation_name, e
            );
        }
    }

    Ok(())
}

/// Discover all transitive dependencies for a single NixOS system using nix-store --query
async fn discover_all_transitive_dependencies_for_system(
    pool: &PgPool,
    nixos_system: &Derivation,
    build_config: &BuildConfig,
) -> Result<()> {
    let drv_path = nixos_system
        .derivation_path
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("NixOS system missing derivation_path"))?;

    // Use nix-store --query --requisites to get ALL transitive dependencies
    let mut cmd = tokio::process::Command::new("nix-store");
    cmd.args(["--query", "--requisites", drv_path]);
    build_config.apply_to_command(&mut cmd);

    let output = cmd.output().await?;
    if !output.status.success() {
        bail!(
            "nix-store query failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let all_deps: Vec<&str> = stdout_str
        .lines()
        .filter(|line| line.ends_with(".drv"))
        .filter(|line| *line != drv_path) // Exclude self
        .collect();

    info!(
        "Found {} transitive dependencies for {}",
        all_deps.len(),
        nixos_system.derivation_name
    );

    // Insert all dependencies as individual derivations
    discover_and_insert_packages(pool, nixos_system.id, &all_deps).await?;

    Ok(())
}

/// Get next buildable derivations for a specific NixOS system
pub async fn get_next_buildable_for_nixos_system(
    pool: &PgPool,
    nixos_system_name: &str,
) -> Result<Vec<Derivation>> {
    let rows = sqlx::query_as!(
        Derivation,
        r#"
        WITH RECURSIVE system_dependencies AS (
            -- Start with the specific NixOS system
            SELECT 
                d.id,
                0 as depth
            FROM derivations d
            WHERE d.derivation_name = $1
            AND d.derivation_type = 'nixos'
            AND d.status_id = $2  -- dry-run-complete
            
            UNION ALL
            
            -- Find all transitive dependencies
            SELECT 
                dep.id,
                sd.depth + 1
            FROM system_dependencies sd
            JOIN derivation_dependencies dd ON sd.id = dd.derivation_id
            JOIN derivations dep ON dd.depends_on_id = dep.id
            WHERE sd.depth < 10  -- Prevent infinite recursion
        ),
        ready_to_build AS (
            SELECT 
                d.id,
                d.commit_id,
                d.derivation_type,
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
                d.store_path,
                sd.depth
            FROM system_dependencies sd
            JOIN derivations d ON sd.id = d.id
            WHERE d.status_id IN ($2, $3)  -- dry-run-complete or build-pending
            -- Only include derivations where ALL dependencies are already built
            AND NOT EXISTS (
                SELECT 1 
                FROM derivation_dependencies dd_check
                JOIN derivations dep_check ON dd_check.depends_on_id = dep_check.id
                WHERE dd_check.derivation_id = d.id
                AND dep_check.status_id NOT IN ($4, $5)  -- not build-complete or cache-pushed
            )
        )
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
        FROM ready_to_build
        ORDER BY 
            depth DESC,          -- Build deepest dependencies first
            derivation_type,     -- Packages before NixOS systems  
            scheduled_at ASC     -- Oldest first
        "#,
        nixos_system_name,
        EvaluationStatus::DryRunComplete.as_id(), // $2
        EvaluationStatus::BuildPending.as_id(),   // $3
        EvaluationStatus::BuildComplete.as_id(),  // $4
        14_i32                                    // cache-pushed status             // $5
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Check if a derivation has all its dependencies built
pub async fn has_all_dependencies_built(pool: &PgPool, derivation_id: i32) -> Result<bool> {
    let result = sqlx::query!(
        r#"
        SELECT COUNT(*) as unbuilt_count
        FROM derivation_dependencies dd
        JOIN derivations dep ON dd.depends_on_id = dep.id
        WHERE dd.derivation_id = $1
        AND dep.status_id NOT IN ($2, $3)  -- not build-complete or cache-pushed
        "#,
        derivation_id,
        EvaluationStatus::BuildComplete.as_id(),
        14_i32 // cache-pushed status
    )
    .fetch_one(pool)
    .await?;

    Ok(result.unbuilt_count == Some(0))
}

/// Get all NixOS systems that are ready to start their dependency builds
pub async fn get_nixos_systems_ready_for_dependency_builds(pool: &PgPool) -> Result<Vec<String>> {
    let systems = sqlx::query!(
        r#"
        SELECT DISTINCT d.derivation_name
        FROM derivations d
        WHERE d.derivation_type = 'nixos'
        AND d.status_id = $1  -- dry-run-complete
        ORDER BY d.derivation_name
        "#,
        EvaluationStatus::DryRunComplete.as_id()
    )
    .fetch_all(pool)
    .await?;

    Ok(systems.into_iter().map(|s| s.derivation_name).collect())
}

/// Mark that dependency builds have started for a NixOS system
pub async fn mark_nixos_dependency_builds_started(
    pool: &PgPool,
    nixos_system_name: &str,
) -> Result<()> {
    // You might want to add a field to track this state, or use a separate table
    // For now, we can use the existing status system
    sqlx::query!(
        r#"
        UPDATE derivations 
        SET status_id = $1
        WHERE derivation_name = $2 
        AND derivation_type = 'nixos'
        AND status_id = $3
        "#,
        EvaluationStatus::BuildPending.as_id(), // Move to build-pending while deps build
        nixos_system_name,
        EvaluationStatus::DryRunComplete.as_id()
    )
    .execute(pool)
    .await?;

    Ok(())
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

/// Atomically claim the next derivation to build
pub async fn claim_next_derivation(pool: &PgPool) -> Result<Option<Derivation>> {
    // First, find the next item ID without locking
    let next_id: Option<i32> = sqlx::query_scalar!(
        r#"
        WITH all_nixos_commits AS (
            SELECT 
                d.id as nixos_deriv_id,
                d.derivation_name as hostname,
                c.commit_timestamp
            FROM derivations d
            INNER JOIN commits c ON d.commit_id = c.id
            WHERE d.derivation_type = 'nixos'
              AND d.status_id IN ($1, $2)
        )
        SELECT d.id
        FROM derivations d
        LEFT JOIN derivation_dependencies dd ON dd.depends_on_id = d.id
        LEFT JOIN all_nixos_commits n ON dd.derivation_id = n.nixos_deriv_id
        LEFT JOIN all_nixos_commits anc ON d.id = anc.nixos_deriv_id
        WHERE d.status_id IN ($1, $2)
          AND (
              (d.derivation_type = 'package' AND n.hostname IS NOT NULL)
              OR
              (d.derivation_type = 'nixos' AND anc.hostname IS NOT NULL)
          )
        ORDER BY 
            CASE 
                WHEN d.derivation_type = 'package' THEN COALESCE(n.hostname, 'zzz_orphan')
                WHEN d.derivation_type = 'nixos' THEN anc.hostname
            END ASC,
            CASE 
                WHEN d.derivation_type = 'package' THEN n.commit_timestamp
                WHEN d.derivation_type = 'nixos' THEN anc.commit_timestamp
            END DESC NULLS LAST,
            CASE 
                WHEN d.derivation_type = 'package' THEN 0
                WHEN d.derivation_type = 'nixos' THEN 1
            END ASC,
            d.id ASC
        LIMIT 1
        "#,
        EvaluationStatus::DryRunComplete.as_id(),
        EvaluationStatus::BuildPending.as_id()
    )
    .fetch_optional(pool)
    .await?;

    let Some(id) = next_id else {
        return Ok(None);
    };

    // Now atomically claim it with FOR UPDATE SKIP LOCKED
    let derivation = sqlx::query_as!(
        Derivation,
        r#"
        UPDATE derivations
        SET status_id = $2, started_at = NOW()
        WHERE id = (
            SELECT id FROM derivations 
            WHERE id = $1 
            AND status_id IN ($3, $4)
            FOR UPDATE SKIP LOCKED
        )
        RETURNING
            id, commit_id, derivation_type as "derivation_type: DerivationType",
            derivation_name, derivation_path, derivation_target, scheduled_at,
            completed_at, started_at, attempt_count, evaluation_duration_ms,
            error_message, pname, version, status_id, build_elapsed_seconds,
            build_current_target, build_last_activity_seconds, build_last_heartbeat,
            cf_agent_enabled, store_path
        "#,
        id,
        EvaluationStatus::BuildInProgress.as_id(),
        EvaluationStatus::DryRunComplete.as_id(),
        EvaluationStatus::BuildPending.as_id()
    )
    .fetch_optional(pool)
    .await?;

    Ok(derivation)
}
