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
        *self as i32
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

pub async fn insert_evaluation_target(
    pool: &PgPool,
    commit: &Commit,
    target_name: &str,
    target_type: &str,
) -> Result<Derivation> {
    let derivation_type = match target_type {
        "nixos" => "nixos",
        "package" => "package",
        _ => "nixos", // default
    };

    let inserted = sqlx::query_as!(
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
        ON CONFLICT (derivation_path) DO NOTHING
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
        commit.id,
        derivation_type,
        target_name,
        EvaluationStatus::DryRunPending.as_id()
    )
    .fetch_one(pool)
    .await?;

    Ok(inserted)
}

/// Unified function to update derivation status with optional additional fields
pub async fn update_evaluation_target_status(
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
pub async fn mark_target_dry_run_in_progress(pool: &PgPool, target_id: i32) -> Result<Derivation> {
    update_evaluation_target_status(
        pool,
        target_id,
        EvaluationStatus::DryRunInProgress,
        None,
        None,
    )
    .await
}

pub async fn mark_target_dry_run_complete(
    pool: &PgPool,
    target_id: i32,
    derivation_path: &str,
) -> Result<Derivation> {
    update_evaluation_target_status(
        pool,
        target_id,
        EvaluationStatus::DryRunComplete,
        Some(derivation_path),
        None,
    )
    .await
}

pub async fn mark_target_build_in_progress(pool: &PgPool, target_id: i32) -> Result<Derivation> {
    update_evaluation_target_status(
        pool,
        target_id,
        EvaluationStatus::BuildInProgress,
        None,
        None,
    )
    .await
}

pub async fn mark_target_build_complete(pool: &PgPool, target_id: i32) -> Result<Derivation> {
    update_evaluation_target_status(pool, target_id, EvaluationStatus::BuildComplete, None, None)
        .await
}

pub async fn mark_target_failed(
    pool: &PgPool,
    target_id: i32,
    phase: &str, // "dry-run" or "build"
    error_message: &str,
) -> Result<Derivation> {
    let status = match phase {
        "dry-run" => EvaluationStatus::DryRunFailed,
        "build" => EvaluationStatus::BuildFailed,
        _ => return Err(anyhow::anyhow!("Invalid phase: {}", phase)),
    };

    update_evaluation_target_status(pool, target_id, status, None, Some(error_message)).await
}

// Keeping the original function but updating it to use the new status
pub async fn update_evaluation_target_path(
    pool: &PgPool,
    target: &Derivation,
    path: &str,
) -> Result<Derivation> {
    update_evaluation_target_status(
        pool,
        target.id,
        EvaluationStatus::DryRunComplete,
        Some(path),
        None,
    )
    .await
}

pub async fn get_target_by_id(pool: &PgPool, target_id: i32) -> Result<Derivation> {
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
pub async fn get_pending_dry_run_targets(pool: &PgPool) -> Result<Vec<Derivation>> {
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
pub async fn get_targets_ready_for_build(pool: &PgPool) -> Result<Vec<Derivation>> {
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
pub async fn increment_evaluation_target_attempt_count(
    pool: &PgPool,
    target: &Derivation,
    error: &anyhow::Error,
) -> Result<()> {
    error!("❌ Failed to process target: {}", error);
    sqlx::query!(
        r#"
        UPDATE derivations
        SET attempt_count = attempt_count + 1
        WHERE id = $1
        "#,
        target.id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn reset_non_terminal_targets(pool: &PgPool) -> Result<()> {
    let result = sqlx::query!(
        r#"
        UPDATE derivations 
        SET status_id = CASE 
            WHEN derivation_path IS NULL THEN $1
            WHEN derivation_path IS NOT NULL THEN $2
            ELSE $1
        END,
        scheduled_at = NOW()
        WHERE status_id NOT IN ($3, $4, $5, $6)
        "#,
        EvaluationStatus::DryRunPending.as_id(),  // $1
        EvaluationStatus::BuildPending.as_id(),   // $2
        EvaluationStatus::DryRunComplete.as_id(), // $3
        EvaluationStatus::DryRunFailed.as_id(),   // $4
        EvaluationStatus::BuildComplete.as_id(),  // $5
        EvaluationStatus::BuildFailed.as_id()     // $6
    )
    .execute(pool)
    .await?;

    let rows_affected = result.rows_affected();
    info!("💡 Reset {} non-terminal targets.", rows_affected);

    Ok(())
}
