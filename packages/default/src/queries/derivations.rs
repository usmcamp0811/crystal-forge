use crate::models::commits::Commit;
use crate::models::derivations::{Derivation, DerivationType};
use anyhow::Result;
use sqlx::PgPool;
use tracing::{error, info};

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
            status_id = derivations.status_id
        RETURNING 
            id,
            commit_id,
            derivation_type as "derivation_type: DerivationType",
            derivation_name,
            derivation_path,
            scheduled_at,
            completed_at,
            started_at,
            attempt_count,
            evaluation_duration_ms,
            error_message,
            parent_derivation_id,
            pname,
            version,
            status_id
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
            scheduled_at,
            completed_at,
            started_at,
            attempt_count,
            evaluation_duration_ms,
            error_message,
            parent_derivation_id,
            pname,
            version,
            status_id
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
) -> Result<Derivation> {
    let status_id = status.as_id();

    // Build the query dynamically based on what fields need updating
    let mut query_parts = vec!["UPDATE derivations SET status_id = $1".to_string()];
    let mut param_count = 1;

    // Always update timestamps based on status
    if status.is_in_progress() {
        query_parts.push("started_at = NOW()".to_string());
    } else if status.is_terminal() {
        query_parts.push("completed_at = NOW()".to_string());
        query_parts.push(
            "evaluation_duration_ms = EXTRACT(EPOCH FROM (NOW() - started_at)) * 1000".to_string(),
        );
    }

    // Handle derivation path updates
    if derivation_path.is_some() {
        param_count += 1;
        query_parts.push(format!("derivation_path = ${}", param_count));
    }

    // Handle error message updates
    if error_message.is_some() {
        param_count += 1;
        query_parts.push(format!("error_message = ${}", param_count));
    }

    param_count += 1;
    let where_clause = format!("WHERE id = ${}", param_count);

    let returning_clause = r#"
    RETURNING
        id,
        commit_id,
        derivation_type as "derivation_type: DerivationType",
        derivation_name,
        derivation_path,
        scheduled_at,
        completed_at,
        started_at,
        attempt_count,
        evaluation_duration_ms,
        error_message,
        parent_derivation_id,
        pname,
        version,
        status_id
    "#;

    let full_query = format!(
        "{} {} {}",
        query_parts.join(", "),
        where_clause,
        returning_clause
    );

    // Execute query with appropriate parameters
    let updated = match (derivation_path, error_message) {
        (Some(path), Some(err)) => {
            sqlx::query_as(&full_query)
                .bind(status_id)
                .bind(path)
                .bind(err)
                .bind(target_id)
                .fetch_one(pool)
                .await?
        }
        (Some(path), None) => {
            sqlx::query_as(&full_query)
                .bind(status_id)
                .bind(path)
                .bind(target_id)
                .fetch_one(pool)
                .await?
        }
        (None, Some(err)) => {
            sqlx::query_as(&full_query)
                .bind(status_id)
                .bind(err)
                .bind(target_id)
                .fetch_one(pool)
                .await?
        }
        (None, None) => {
            sqlx::query_as(&full_query)
                .bind(status_id)
                .bind(target_id)
                .fetch_one(pool)
                .await?
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
    )
    .await
}

pub async fn mark_derivation_build_complete(
    pool: &PgPool,
    derivation_id: i32,
) -> Result<Derivation> {
    update_derivation_status(
        pool,
        derivation_id,
        EvaluationStatus::BuildComplete,
        None,
        None,
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

    update_derivation_status(pool, derivation_id, status, None, Some(error_message)).await
}

// Keeping the original function but updating it to use the new status
pub async fn update_derivation_path(
    pool: &PgPool,
    target: &Derivation,
    path: &str,
) -> Result<Derivation> {
    update_derivation_status(
        pool,
        target.id,
        EvaluationStatus::DryRunComplete,
        Some(path),
        None,
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
            scheduled_at,
            completed_at,
            started_at,
            attempt_count,
            evaluation_duration_ms,
            error_message,
            parent_derivation_id,
            pname,
            version,
            status_id
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
            scheduled_at,
            completed_at,
            started_at,
            attempt_count,
            evaluation_duration_ms,
            error_message,
            parent_derivation_id,
            pname,
            version,
            status_id
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
        SELECT
            id,
            commit_id,
            derivation_type as "derivation_type: DerivationType",
            derivation_name,
            derivation_path,
            scheduled_at,
            completed_at,
            started_at,
            attempt_count,
            evaluation_duration_ms,
            error_message,
            parent_derivation_id,
            pname,
            version,
            status_id
        FROM derivations
        WHERE status_id = $1
        ORDER BY completed_at ASC
        "#,
        EvaluationStatus::DryRunComplete.as_id()
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

pub async fn reset_non_terminal_derivations(pool: &PgPool) -> Result<()> {
    // Split into separate, simpler queries to avoid parameter type issues

    // Reset derivations without paths to dry-run-pending
    let result1 = sqlx::query!(
        r#"
        UPDATE derivations 
        SET status_id = $1, scheduled_at = NOW()
        WHERE derivation_path IS NULL 
        AND status_id NOT IN ($2, $3, $4, $5)
        "#,
        EvaluationStatus::DryRunPending.as_id(),
        EvaluationStatus::DryRunComplete.as_id(),
        EvaluationStatus::DryRunFailed.as_id(),
        EvaluationStatus::BuildComplete.as_id(),
        EvaluationStatus::BuildFailed.as_id()
    )
    .execute(pool)
    .await?;

    // Reset derivations with paths to build-pending
    let result2 = sqlx::query!(
        r#"
        UPDATE derivations 
        SET status_id = $1, scheduled_at = NOW()
        WHERE derivation_path IS NOT NULL 
        AND status_id NOT IN ($2, $3, $4, $5)
        "#,
        EvaluationStatus::BuildPending.as_id(),
        EvaluationStatus::DryRunComplete.as_id(),
        EvaluationStatus::DryRunFailed.as_id(),
        EvaluationStatus::BuildComplete.as_id(),
        EvaluationStatus::BuildFailed.as_id()
    )
    .execute(pool)
    .await?;

    let total_rows_affected = result1.rows_affected() + result2.rows_affected();
    info!("💡 Reset {} non-terminal derivations.", total_rows_affected);

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

pub async fn mark_target_build_complete(pool: &PgPool, target_id: i32) -> Result<Derivation> {
    mark_derivation_build_complete(pool, target_id).await
}

pub async fn mark_target_failed(
    pool: &PgPool,
    target_id: i32,
    phase: &str,
    error_message: &str,
) -> Result<Derivation> {
    mark_derivation_failed(pool, target_id, phase, error_message).await
}
