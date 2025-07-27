use crate::models::commits::Commit;
use crate::models::evaluation_targets::{EvaluationTarget, TargetType};
use anyhow::Result;
use sqlx::PgPool;
use tracing::error;

// Enum to represent the different phases and statuses
#[derive(Debug, Clone)]
pub enum EvaluationStatus {
    DryRunPending,
    DryRunInProgress,
    DryRunComplete,
    DryRunFailed,
    BuildPending,
    BuildInProgress,
    BuildComplete,
    BuildFailed,
}

impl EvaluationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            EvaluationStatus::DryRunPending => "dry-run-pending",
            EvaluationStatus::DryRunInProgress => "dry-run-inprogress",
            EvaluationStatus::DryRunComplete => "dry-run-complete",
            EvaluationStatus::DryRunFailed => "dry-run-failed",
            EvaluationStatus::BuildPending => "build-pending",
            EvaluationStatus::BuildInProgress => "build-inprogress",
            EvaluationStatus::BuildComplete => "build-complete",
            EvaluationStatus::BuildFailed => "build-failed",
        }
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

pub async fn insert_evaluation_target(
    pool: &PgPool,
    commit: &Commit,
    target_name: &str,
    target_type: &str,
) -> Result<EvaluationTarget> {
    let inserted = sqlx::query_as!(
        EvaluationTarget,
        r#"
    INSERT INTO evaluation_targets (commit_id, target_type, target_name, status)
    VALUES ($1, $2, $3, $4)
    ON CONFLICT (commit_id, target_type, target_name)
    DO UPDATE SET commit_id = EXCLUDED.commit_id
    RETURNING id, commit_id, target_type, target_name, derivation_path, scheduled_at, completed_at, status
    "#,
        commit.id,
        target_type,
        target_name,
        EvaluationStatus::DryRunPending.as_str()
    )
    .fetch_one(pool)
    .await?;

    Ok(inserted)
}

/// Unified function to update evaluation target status with optional additional fields
pub async fn update_evaluation_target_status(
    pool: &PgPool,
    target_id: i32,
    status: EvaluationStatus,
    derivation_path: Option<&str>,
    error_message: Option<&str>,
) -> Result<EvaluationTarget> {
    let status_str = status.as_str();

    // Build the query dynamically based on what fields need updating
    let mut query_parts = vec!["UPDATE evaluation_targets SET status = $1".to_string()];
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
        target_type as "target_type: TargetType",
        target_name,
        derivation_path,
        scheduled_at,
        completed_at,
        status
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
                .bind(status_str)
                .bind(path)
                .bind(err)
                .bind(target_id)
                .fetch_one(pool)
                .await?
        }
        (Some(path), None) => {
            sqlx::query_as(&full_query)
                .bind(status_str)
                .bind(path)
                .bind(target_id)
                .fetch_one(pool)
                .await?
        }
        (None, Some(err)) => {
            sqlx::query_as(&full_query)
                .bind(status_str)
                .bind(err)
                .bind(target_id)
                .fetch_one(pool)
                .await?
        }
        (None, None) => {
            sqlx::query_as(&full_query)
                .bind(status_str)
                .bind(target_id)
                .fetch_one(pool)
                .await?
        }
    };

    Ok(updated)
}

// Simplified convenience functions that use the unified update function
pub async fn mark_target_dry_run_in_progress(
    pool: &PgPool,
    target_id: i32,
) -> Result<EvaluationTarget> {
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
) -> Result<EvaluationTarget> {
    update_evaluation_target_status(
        pool,
        target_id,
        EvaluationStatus::DryRunComplete,
        Some(derivation_path),
        None,
    )
    .await
}

pub async fn mark_target_build_in_progress(
    pool: &PgPool,
    target_id: i32,
) -> Result<EvaluationTarget> {
    update_evaluation_target_status(
        pool,
        target_id,
        EvaluationStatus::BuildInProgress,
        None,
        None,
    )
    .await
}

pub async fn mark_target_build_complete(pool: &PgPool, target_id: i32) -> Result<EvaluationTarget> {
    update_evaluation_target_status(pool, target_id, EvaluationStatus::BuildComplete, None, None)
        .await
}

pub async fn mark_target_failed(
    pool: &PgPool,
    target_id: i32,
    phase: &str, // "dry-run" or "build"
    error_message: &str,
) -> Result<EvaluationTarget> {
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
    target: &EvaluationTarget,
    path: &str,
) -> Result<EvaluationTarget> {
    update_evaluation_target_status(
        pool,
        target.id,
        EvaluationStatus::DryRunComplete,
        Some(path),
        None,
    )
    .await
}

pub async fn get_target_by_id(pool: &PgPool, target_id: i32) -> Result<EvaluationTarget> {
    let target = sqlx::query_as!(
        EvaluationTarget,
        r#"
        SELECT
            id,
            commit_id,
            target_type,
            target_name,
            derivation_path,
            scheduled_at,
            completed_at,
            status
        FROM evaluation_targets
        WHERE id = $1
        "#,
        target_id
    )
    .fetch_one(pool)
    .await?;

    Ok(target)
}

// Updated to get targets ready for dry-run
pub async fn get_pending_dry_run_targets(pool: &PgPool) -> Result<Vec<EvaluationTarget>> {
    let rows = sqlx::query_as!(
        EvaluationTarget,
        r#"
        SELECT
            id,
            commit_id,
            target_type,
            target_name,
            derivation_path,
            scheduled_at,
            completed_at,
            status
        FROM evaluation_targets
        WHERE status = $1
        AND attempt_count < 5
        ORDER BY scheduled_at DESC
        "#,
        EvaluationStatus::DryRunPending.as_str()
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

// New function to get targets ready for building
pub async fn get_targets_ready_for_build(pool: &PgPool) -> Result<Vec<EvaluationTarget>> {
    let rows = sqlx::query_as!(
        EvaluationTarget,
        r#"
        SELECT
            id,
            commit_id,
            target_type as "target_type: TargetType",
            target_name,
            derivation_path,
            scheduled_at,
            completed_at,
            status
        FROM evaluation_targets
        WHERE status = $1
        ORDER BY completed_at ASC
        "#,
        EvaluationStatus::DryRunComplete.as_str()
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

pub async fn update_scheduled_at(pool: &PgPool) -> Result<()> {
    sqlx::query!(
        r#"
    UPDATE evaluation_targets
    SET scheduled_at = NOW() 
    WHERE status NOT IN ($1, $2, $3, $4)
    "#,
        EvaluationStatus::DryRunComplete.as_str(),
        EvaluationStatus::DryRunFailed.as_str(),
        EvaluationStatus::BuildComplete.as_str(),
        EvaluationStatus::BuildFailed.as_str()
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Increment the number of attempts for failed operations
pub async fn increment_evaluation_target_attempt_count(
    pool: &PgPool,
    target: &EvaluationTarget,
    error: &anyhow::Error,
) -> Result<()> {
    error!("❌ Failed to process target: {}", error);
    sqlx::query!(
        r#"
    UPDATE evaluation_targets
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
    sqlx::query!(
        r#"
        UPDATE evaluation_targets 
        SET status = CASE 
            WHEN derivation_path IS NULL THEN $1
            ELSE $2
        END
        WHERE status NOT IN ($3, $4, $5, $6)
        "#,
        EvaluationStatus::DryRunPending.as_str(),
        EvaluationStatus::BuildPending.as_str(),
        EvaluationStatus::DryRunComplete.as_str(),
        EvaluationStatus::DryRunFailed.as_str(),
        EvaluationStatus::BuildComplete.as_str(),
        EvaluationStatus::BuildFailed.as_str()
    )
    .execute(pool)
    .await?;
    Ok(())
}
