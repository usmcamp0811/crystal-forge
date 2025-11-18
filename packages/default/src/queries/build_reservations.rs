use crate::derivations::Derivation;
use crate::queries::derivations::EvaluationStatus;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use tracing::{debug, info, warn};

/// Represents an active build reservation
#[derive(Debug, FromRow)]
pub struct BuildReservation {
    pub id: i32,
    pub worker_id: String,
    pub derivation_id: i32,
    pub nixos_derivation_id: Option<i32>,
    pub reserved_at: DateTime<Utc>,
    pub heartbeat_at: DateTime<Utc>,
}

/// Represents a row from view_buildable_derivations
#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct BuildableDerivation {
    pub id: i32,
    pub derivation_name: String,
    pub derivation_type: String,
    pub derivation_path: Option<String>,
    pub pname: Option<String>,
    pub version: Option<String>,
    pub status_id: i32,
    pub nixos_id: Option<i32>,
    pub nixos_commit_ts: Option<DateTime<Utc>>,
    pub total_packages: Option<i64>,
    pub completed_packages: Option<i64>,
    pub cached_packages: Option<i64>,
    pub active_workers: Option<i64>,
    pub build_type: String,
    pub queue_position: Option<i64>,
}

/// Represents a row from view_build_queue_status
#[derive(Debug, FromRow)]
pub struct QueueStatus {
    pub nixos_id: i32,
    pub system_name: String,
    pub commit_timestamp: DateTime<Utc>,
    pub git_commit_hash: String,
    pub total_packages: i64,
    pub completed_packages: i64,
    pub building_packages: i64,
    pub pending_packages: i64,
    pub cached_packages: i64,
    pub active_workers: i64,
    pub worker_ids: Option<Vec<String>>,
    pub earliest_reservation: Option<DateTime<Utc>>,
    pub latest_heartbeat: Option<DateTime<Utc>>,
    pub status: String,
    pub cache_status: Option<String>,
    pub has_stale_workers: bool,
}

/// Create a new build reservation
pub async fn create_reservation(
    pool: &PgPool,
    worker_id: &str,
    derivation_id: i32,
    nixos_derivation_id: Option<i32>,
) -> Result<i32> {
    let reservation_id = sqlx::query_scalar!(
        r#"
        INSERT INTO build_reservations (worker_id, derivation_id, nixos_derivation_id)
        VALUES ($1, $2, $3)
        RETURNING id
        "#,
        worker_id,
        derivation_id,
        nixos_derivation_id
    )
    .fetch_one(pool)
    .await?;

    debug!(
        "Created reservation {} for worker {} on derivation {}",
        reservation_id, worker_id, derivation_id
    );

    Ok(reservation_id)
}

/// Delete a build reservation
pub async fn delete_reservation<'e, E>(
    executor: E,
    worker_id: &str,
    derivation_id: i32,
) -> Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query!(
        r#"
        DELETE FROM build_reservations
        WHERE worker_id = $1 AND derivation_id = $2
        "#,
        worker_id,
        derivation_id
    )
    .execute(executor)
    .await?;

    debug!(
        "Deleted reservation for worker {} on derivation {}",
        worker_id, derivation_id
    );

    Ok(())
}

/// Update heartbeat for a worker's reservations
pub async fn update_heartbeat(pool: &PgPool, worker_id: &str) -> Result<u64> {
    let result = sqlx::query!(
        r#"
        UPDATE build_reservations
        SET heartbeat_at = NOW()
        WHERE worker_id = $1
        "#,
        worker_id
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

/// Clean up stale reservations (heartbeat older than threshold)
pub async fn cleanup_stale_reservations(
    pool: &PgPool,
    stale_threshold_seconds: i64,
) -> Result<Vec<i32>> {
    let reclaimed = sqlx::query!(
        r#"
        DELETE FROM build_reservations
        WHERE heartbeat_at < NOW() - make_interval(secs => $1)
        RETURNING derivation_id
        "#,
        stale_threshold_seconds as f64
    )
    .fetch_all(pool)
    .await?;

    let derivation_ids: Vec<i32> = reclaimed
        .into_iter()
        .map(|r| r.derivation_id) // Remove filter_map, it's not Option
        .collect();

    if !derivation_ids.is_empty() {
        warn!(
            "Reclaimed {} stale reservations: {:?}",
            derivation_ids.len(),
            derivation_ids
        );

        // Reset derivations back to dry-run-complete (not Scheduled)
        for derivation_id in &derivation_ids {
            let _ = sqlx::query!(
                r#"
                UPDATE derivations
                SET status_id = $1, started_at = NULL
                WHERE id = $2
                "#,
                EvaluationStatus::DryRunComplete.as_id(), // Use DryRunComplete
                derivation_id
            )
            .execute(pool)
            .await;
        }
    }

    Ok(derivation_ids)
}

/// Get the next buildable derivation from the queue
pub async fn get_next_buildable_derivation(
    pool: &PgPool,
    wait_for_cache_push: bool,
) -> Result<Option<BuildableDerivation>> {
    let row = sqlx::query_as!(
        BuildableDerivation,
        r#"
        SELECT 
            id as "id!",
            derivation_name as "derivation_name!",
            derivation_type as "derivation_type!",
            derivation_path,
            pname,
            version,
            status_id as "status_id!",
            nixos_id,
            nixos_commit_ts,
            total_packages,
            completed_packages,
            cached_packages,
            active_workers,
            build_type as "build_type!",
            queue_position
        FROM view_buildable_derivations
        WHERE 
            CASE 
                WHEN build_type = 'system' AND $1 = true THEN 
                    cached_packages = total_packages
                ELSE true
            END
        ORDER BY queue_position
        LIMIT 1
        "#,
        wait_for_cache_push
    )
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// Claim the next derivation that needs building
///
/// FIXED: Only claims derivations with status = 5 (DryRunComplete)
/// This ensures we only build derivations that are ready, not ones that are:
/// - Still being evaluated (DryRunPending, DryRunInProgress)
/// - Already building (BuildInProgress)
/// - Failed (DryRunFailed, BuildFailed)
pub async fn claim_next_derivation(pool: &PgPool, worker_id: &str) -> Result<Option<Derivation>> {
    let mut tx = pool.begin().await?;

    // 1) Use the view query directly within the transaction to get correct ordering
    let buildable = sqlx::query_as!(
        BuildableDerivation,
        r#"
        SELECT 
            id as "id!",
            derivation_name as "derivation_name!",
            derivation_type as "derivation_type!",
            derivation_path,
            pname,
            version,
            status_id as "status_id!",
            nixos_id,
            nixos_commit_ts,
            total_packages,
            completed_packages,
            cached_packages,
            active_workers,
            build_type as "build_type!",
            queue_position
        FROM view_buildable_derivations
        ORDER BY queue_position
        LIMIT 1
        "#
    )
    .fetch_optional(&mut *tx)
    .await?;

    let Some(buildable) = buildable else {
        tx.rollback().await?;
        return Ok(None);
    };

    // 2) Atomically try to reserve it
    let reserved = sqlx::query!(
        r#"
        INSERT INTO build_reservations (worker_id, derivation_id, nixos_derivation_id)
        VALUES ($1, $2, $3)
        ON CONFLICT (derivation_id) DO NOTHING
        RETURNING derivation_id
        "#,
        worker_id,
        buildable.id,
        buildable.nixos_id,
    )
    .fetch_optional(&mut *tx)
    .await?;

    if reserved.is_none() {
        tx.rollback().await?;
        return Ok(None);
    }

    // 3) Update status to BuildInProgress
    let updated = sqlx::query!(
        r#"
        UPDATE derivations
        SET 
            status_id = $1,
            started_at = NOW(),
            attempt_count = COALESCE(attempt_count, 0) + 1
        WHERE id = $2
          AND status_id IN ($3, $4)
        "#,
        EvaluationStatus::BuildInProgress.as_id(),
        buildable.id,
        EvaluationStatus::DryRunComplete.as_id(),
        EvaluationStatus::BuildPending.as_id(),
    )
    .execute(&mut *tx)
    .await?;

    if updated.rows_affected() != 1 {
        sqlx::query!(
            r#"DELETE FROM build_reservations WHERE derivation_id = $1"#,
            buildable.id
        )
        .execute(&mut *tx)
        .await?;
        tx.rollback().await?;
        return Ok(None);
    }

    // 4) Fetch the full Derivation record
    let derivation = sqlx::query_as!(
        Derivation,
        r#"
        SELECT 
            id, commit_id,
            derivation_type as "derivation_type: crate::derivations::DerivationType",
            derivation_name, derivation_path, derivation_target,
            scheduled_at, completed_at, started_at, attempt_count,
            evaluation_duration_ms, error_message, pname, version, status_id,
            build_elapsed_seconds, build_current_target, build_last_activity_seconds,
            build_last_heartbeat, cf_agent_enabled, store_path
        FROM derivations
        WHERE id = $1
        "#,
        buildable.id
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    info!(
        "Worker {} claimed derivation {} ({}) - attempt {}/5",
        worker_id, derivation.id, derivation.derivation_name, derivation.attempt_count
    );

    Ok(Some(derivation))
}

/// Get all systems in the queue with their progress
pub async fn get_queue_status(pool: &PgPool) -> Result<Vec<QueueStatus>> {
    let rows = sqlx::query_as!(
        QueueStatus,
        r#"
        SELECT 
            nixos_id as "nixos_id!",
            system_name as "system_name!",
            commit_timestamp as "commit_timestamp!",
            git_commit_hash as "git_commit_hash!",
            total_packages as "total_packages!",
            completed_packages as "completed_packages!",
            building_packages as "building_packages!",
            pending_packages as "pending_packages!",
            cached_packages as "cached_packages!",
            active_workers as "active_workers!",
            worker_ids,
            earliest_reservation,
            latest_heartbeat,
            status as "status!",
            cache_status,
            has_stale_workers as "has_stale_workers!"
        FROM view_build_queue_status
        ORDER BY commit_timestamp DESC
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Get queue status for a specific system
pub async fn get_queue_status_for_system(
    pool: &PgPool,
    nixos_id: i32,
) -> Result<Option<QueueStatus>> {
    let row = sqlx::query_as!(
        QueueStatus,
        r#"
        SELECT 
            nixos_id as "nixos_id!",
            system_name as "system_name!",
            commit_timestamp as "commit_timestamp!",
            git_commit_hash as "git_commit_hash!",
            total_packages as "total_packages!",
            completed_packages as "completed_packages!",
            building_packages as "building_packages!",
            pending_packages as "pending_packages!",
            cached_packages as "cached_packages!",
            active_workers as "active_workers!",
            worker_ids,
            earliest_reservation,
            latest_heartbeat,
            status as "status!",
            cache_status,
            has_stale_workers as "has_stale_workers!"
        FROM view_build_queue_status
        WHERE nixos_id = $1
        "#,
        nixos_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// Check if a worker has any active reservations
pub async fn get_worker_reservations(
    pool: &PgPool,
    worker_id: &str,
) -> Result<Vec<BuildReservation>> {
    let reservations = sqlx::query_as!(
        BuildReservation,
        r#"
        SELECT id, worker_id, derivation_id, nixos_derivation_id, reserved_at, heartbeat_at
        FROM build_reservations
        WHERE worker_id = $1
        "#,
        worker_id
    )
    .fetch_all(pool)
    .await?;

    Ok(reservations)
}

/// Get total number of active workers across all systems
pub async fn get_active_worker_count(pool: &PgPool) -> Result<i64> {
    let count = sqlx::query_scalar!(
        r#"
        SELECT COUNT(DISTINCT worker_id) as "count!"
        FROM build_reservations
        "#
    )
    .fetch_one(pool)
    .await?;

    Ok(count)
}

/// Get queue depth (total buildable derivations)
pub async fn get_queue_depth(pool: &PgPool, wait_for_cache_push: bool) -> Result<i64> {
    let count = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*) as "count!"
        FROM view_buildable_derivations
        WHERE 
            CASE 
                WHEN build_type = 'system' AND $1 = true THEN 
                    cached_packages = total_packages
                ELSE true
            END
        "#,
        wait_for_cache_push
    )
    .fetch_one(pool)
    .await?;

    Ok(count)
}
