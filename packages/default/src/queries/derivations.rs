use crate::models::commits::Commit;
use anyhow::anyhow;
use crate::models::derivations::{Derivation, DerivationType, parse_derivation_path};
use anyhow::Result;
use sqlx::PgPool;
use tracing::{debug, error, info};

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
        VALUES ($1, $2, $3, $4, $5, $6, NOW())
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
            status_id
        "#,
        commit_id,
        derivation_type,
        derivation_name,
        derivation_target,
        1, // assuming status_id 1 is "pending"
        0  // initial attempt_count
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

    // Use a simpler approach with separate queries for different cases
    let updated = match (derivation_path, error_message) {
        (Some(path), Some(err)) => {
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
                        status_id
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
                        status_id
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
        (Some(path), None) => {
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
                        status_id
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
                        status_id
                    "#,
                    status_id,
                    path,
                    target_id
                )
                .fetch_one(pool)
                .await?
            }
        }
        (None, Some(err)) => {
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
                        status_id
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
                        status_id
                    "#,
                    status_id,
                    err,
                    target_id
                )
                .fetch_one(pool)
                .await?
            }
        }
        (None, None) => {
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
                        status_id
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
                        status_id
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
                        status_id
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
            derivation_target,
            scheduled_at,
            completed_at,
            started_at,
            attempt_count,
            evaluation_duration_ms,
            error_message,
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
            derivation_target,
            scheduled_at,
            completed_at,
            started_at,
            attempt_count,
            evaluation_duration_ms,
            error_message,
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
        WITH nixos_system_groups AS (
            -- Get NixOS systems and their associated packages
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
                -- Find the related NixOS system for packages, or use self for NixOS systems
                CASE 
                    WHEN d.derivation_type = 'package' THEN 
                        COALESCE(
                            (SELECT MIN(nixos_dep.id) 
                             FROM derivation_dependencies dd 
                             JOIN derivations nixos_dep ON dd.derivation_id = nixos_dep.id 
                             WHERE dd.depends_on_id = d.id 
                               AND nixos_dep.derivation_type = 'nixos'
                             LIMIT 1),
                            999999 -- Orphaned packages get highest group number
                        )
                    ELSE d.id -- NixOS systems group by themselves
                END as nixos_group_id,
                -- Priority: packages first (0), then NixOS systems (1)
                CASE 
                    WHEN d.derivation_type = 'package' THEN 0
                    WHEN d.derivation_type = 'nixos' THEN 1
                    ELSE 2
                END as type_priority
            FROM derivations d
            WHERE d.status_id in ( $1, $2 )
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
            status_id
        FROM nixos_system_groups
        ORDER BY 
            nixos_group_id,          -- Group related packages and systems together
            type_priority,           -- Within each group: packages first, then NixOS
            completed_at ASC         -- Within same type: oldest first
        "#,
        EvaluationStatus::DryRunComplete.as_id()
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

pub async fn reset_non_terminal_derivations(pool: &PgPool) -> Result<()> {
    // Reset derivations without paths to dry-run-pending
    // Include failed derivations that haven't exceeded attempt limit
    let result1 = sqlx::query!(
        r#"
        UPDATE derivations 
        SET status_id = $1, scheduled_at = NOW()
        WHERE derivation_path IS NULL 
        AND (
            status_id NOT IN ($2, $3, $4) 
            OR (status_id = $5 AND attempt_count < 5)  -- Include dry-run-failed with < 5 attempts
        )
        "#,
        EvaluationStatus::DryRunPending.as_id(),  // $1 = 3
        EvaluationStatus::DryRunComplete.as_id(), // $2 = 5 (always terminal)
        EvaluationStatus::BuildComplete.as_id(),  // $3 = 10 (always terminal)
        EvaluationStatus::BuildFailed.as_id(),    // $4 = 12 (always terminal)
        EvaluationStatus::DryRunFailed.as_id()    // $5 = 6 (only terminal if attempts >= 5)
    )
    .execute(pool)
    .await?;

    // Reset derivations with paths to build-pending
    // Include failed derivations that haven't exceeded attempt limit
    let result2 = sqlx::query!(
        r#"
        UPDATE derivations 
        SET status_id = $1, scheduled_at = NOW()
        WHERE derivation_path IS NOT NULL 
        AND (
            status_id NOT IN ($2, $3, $4)
            OR (status_id = $4 AND attempt_count < 5)  -- Include build-failed with < 5 attempts
        )
        "#,
        EvaluationStatus::BuildPending.as_id(),   // $1 = 7
        EvaluationStatus::DryRunComplete.as_id(), // $2 = 5 (always terminal)
        EvaluationStatus::BuildComplete.as_id(),  // $3 = 10 (always terminal)
        EvaluationStatus::BuildFailed.as_id()     // $4 = 12 (check this for retry condition)
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

            // Insert package derivation
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
                    version = EXCLUDED.version,
                    status_id = CASE 
                        WHEN derivations.status_id IN (5, 6, 10, 12) THEN derivations.status_id  -- Keep terminal states
                        ELSE EXCLUDED.status_id  -- Update to new status if not terminal
                    END,
                    scheduled_at = CASE 
                        WHEN derivations.status_id IN (5, 6, 10, 12) THEN derivations.scheduled_at  -- Keep terminal timestamps
                        ELSE NOW()  -- Update timestamp for non-terminal
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
                    status_id
                "#,
                None::<i32>,                           // commit_id is NULL for discovered packages
                "package",                             // derivation_type = "package"
                derivation_name,                       // derivation_name (uses parsed package name)
                drv_path,                             // derivation_path (the actual .drv path)
                package_info.pname.as_deref(),       // pname
                package_info.version.as_deref(),     // version
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
