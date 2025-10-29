use crate::builder::remove_gc_root;
use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use tracing::{debug, warn};

#[derive(Debug, FromRow, Clone)]
pub struct CachePushJob {
    pub id: i32,
    pub derivation_id: i32,
    pub status: String,
    pub store_path: Option<String>,
    pub scheduled_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub attempts: i32,
    pub error_message: Option<String>,
    pub push_size_bytes: Option<i64>,
    pub push_duration_ms: Option<i32>,
    pub cache_destination: Option<String>,
}

/// Get derivations that need cache pushing (build-complete status)
/// Prioritizes dependencies of newest NixOS systems first, then the systems themselves
pub async fn get_derivations_needing_cache_push_for_dest(
    pool: &PgPool,
    destination: &str,
    limit: Option<i32>,
    max_attempts: i32,
) -> Result<Vec<crate::models::derivations::Derivation>> {
    use crate::queries::derivations::EvaluationStatus;

    let sql = r#"
        WITH job_status AS (
            SELECT 
                derivation_id,
                MAX(CASE WHEN status IN ('completed', 'in_progress') THEN 1 ELSE 0 END) as has_active,
                MAX(CASE WHEN status = 'failed' AND attempts < $4 THEN 1 ELSE 0 END) as has_retryable,
                COUNT(*) as job_count
            FROM cache_push_jobs
            WHERE cache_destination = $3
            GROUP BY derivation_id
        ),
        newest_nixos_systems AS (
            SELECT DISTINCT d.id as nixos_id, d.completed_at as nixos_completed_at, c.commit_timestamp as commit_timestamp
            FROM derivations d
            JOIN commits c on d.commit_id = c.id
            WHERE d.derivation_type = 'nixos'
                AND d.status_id = $2
            ORDER BY c.commit_timestamp DESC, nixos_completed_at DESC
            LIMIT 20
        ),
        prioritized_derivations AS (
            SELECT 
                d.id,
                MAX(nns.commit_timestamp) as newest_nixos_parent
            FROM derivations d
            LEFT JOIN derivation_dependencies dd ON dd.depends_on_id = d.id
            LEFT JOIN newest_nixos_systems nns ON nns.nixos_id = dd.derivation_id
            GROUP BY d.id
        )
        SELECT
            d.id, d.commit_id, d.derivation_type, d.derivation_name,
            d.derivation_path, d.derivation_target, d.scheduled_at,
            d.completed_at, d.started_at, d.attempt_count,
            d.evaluation_duration_ms, d.error_message, d.pname,
            d.version, d.status_id, d.build_elapsed_seconds,
            d.build_current_target, d.build_last_activity_seconds,
            d.build_last_heartbeat, d.cf_agent_enabled, d.store_path
        FROM derivations d
        LEFT JOIN job_status js ON js.derivation_id = d.id
        LEFT JOIN prioritized_derivations pd ON pd.id = d.id
        WHERE d.status_id = $2
            AND d.store_path IS NOT NULL
            AND (
                js.derivation_id IS NULL
                OR (js.has_active = 0 AND js.has_retryable = 1)
            )
        ORDER BY 
            CASE WHEN d.derivation_type = 'nixos' THEN 1 ELSE 0 END,
            pd.newest_nixos_parent DESC NULLS LAST,
            d.commit_id DESC,
            d.completed_at ASC NULLS LAST
        LIMIT $1
    "#;

    let derivations = sqlx::query_as(sql)
        .bind(limit.unwrap_or(10))
        .bind(EvaluationStatus::BuildComplete.as_id())
        .bind(destination)
        .bind(max_attempts)
        .fetch_all(pool)
        .await?;

    Ok(derivations)
}

/// Create a new cache push job
pub async fn create_cache_push_job(
    pool: &PgPool,
    derivation_id: i32,
    store_path: &str,
    cache_destination: Option<&str>,
) -> Result<i32> {
    // First, try to find an existing pending or in-progress job
    if let Some(existing_job_id) = sqlx::query_scalar!(
        r#"
        SELECT id FROM cache_push_jobs 
        WHERE derivation_id = $1 AND status IN ('pending', 'in_progress')
        "#,
        derivation_id
    )
    .fetch_optional(pool)
    .await?
    {
        debug!(
            "Found existing cache push job {} for derivation {}",
            existing_job_id, derivation_id
        );
        return Ok(existing_job_id);
    }

    // Check for failed jobs that can be retried (< 5 attempts)
    if let Some(failed_job_id) = sqlx::query_scalar!(
        r#"
        SELECT id FROM cache_push_jobs 
        WHERE derivation_id = $1 AND status = 'failed' AND attempts < 5
        ORDER BY scheduled_at DESC
        LIMIT 1
        "#,
        derivation_id
    )
    .fetch_optional(pool)
    .await?
    {
        // Reset the failed job to pending
        sqlx::query!(
            r#"
            UPDATE cache_push_jobs 
            SET status = 'pending', 
                store_path = $2,
                cache_destination = $3,
                scheduled_at = NOW()
            WHERE id = $1
            "#,
            failed_job_id,
            store_path,
            cache_destination
        )
        .execute(pool)
        .await?;

        debug!(
            "Reset failed cache push job {} to pending for derivation {}",
            failed_job_id, derivation_id
        );
        return Ok(failed_job_id);
    }

    // No existing job, create a new one
    let job_id = sqlx::query_scalar!(
        r#"
        INSERT INTO cache_push_jobs (
            derivation_id, store_path, cache_destination, status
        ) VALUES ($1, $2, $3, 'pending')
        RETURNING id
        "#,
        derivation_id,
        store_path,
        cache_destination
    )
    .fetch_one(pool)
    .await?;

    debug!(
        "Created cache push job {} for derivation {}",
        job_id, derivation_id
    );
    Ok(job_id)
}

/// Mark cache push job as in progress
pub async fn mark_cache_push_in_progress(pool: &PgPool, job_id: i32) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE cache_push_jobs 
        SET 
            status = 'in_progress', 
            started_at = NOW(), 
            attempts = attempts + 1,
            completed_at = NULL,
            error_message = NULL,
            retry_after = NULL
        WHERE id = $1
        "#,
        job_id
    )
    .execute(pool)
    .await?;

    debug!("Marked cache push job {} as in progress", job_id);
    Ok(())
}

/// Mark cache push job as completed
pub async fn mark_cache_push_completed(
    pool: &PgPool,
    job_id: i32,
    push_size_bytes: Option<i64>,
    push_duration_ms: Option<i32>,
) -> Result<()> {
    // Get derivation_id before updating
    let derivation_id = sqlx::query_scalar!(
        "SELECT derivation_id FROM cache_push_jobs WHERE id = $1",
        job_id
    )
    .fetch_one(pool)
    .await?;

    sqlx::query!(
        r#"
        UPDATE cache_push_jobs 
        SET 
            status = 'completed',
            completed_at = NOW(),
            push_size_bytes = $2,
            push_duration_ms = $3
        WHERE id = $1
        "#,
        job_id,
        push_size_bytes,
        push_duration_ms
    )
    .execute(pool)
    .await?;

    debug!("Marked cache push job {} as completed", job_id);

    // Remove GC root now that it's in cache
    if let Err(e) = remove_gc_root(derivation_id).await {
        warn!(
            "Failed to remove GC root for derivation {}: {}",
            derivation_id, e
        );
    }

    Ok(())
}

/// Mark cache push job as failed with exponential backoff
pub async fn mark_cache_push_failed(pool: &PgPool, job_id: i32, error_message: &str) -> Result<()> {
    // Get current attempt count to calculate retry delay
    let attempts =
        sqlx::query_scalar!("SELECT attempts FROM cache_push_jobs WHERE id = $1", job_id)
            .fetch_one(pool)
            .await?;

    // Exponential backoff: 2min, 4min, 8min, 16min, 32min
    // After 5 attempts, mark as permanently failed
    let max_attempts = 5;

    if attempts < max_attempts {
        // Calculate backoff: 2^attempts minutes
        let backoff_minutes = 2_i32.pow(attempts as u32);

        sqlx::query!(
            r#"
            UPDATE cache_push_jobs 
            SET 
                status = 'failed',
                completed_at = NOW(),
                error_message = $2,
                retry_after = NOW() + ($3 || ' minutes')::INTERVAL
            WHERE id = $1
            "#,
            job_id,
            error_message,
            backoff_minutes.to_string()
        )
        .execute(pool)
        .await?;

        debug!(
            "Marked cache push job {} as failed (attempt {}/{}), will retry after {} minutes: {}",
            job_id, attempts, max_attempts, backoff_minutes, error_message
        );
    } else {
        // Permanent failure - don't set retry_after
        sqlx::query!(
            r#"
            UPDATE cache_push_jobs 
            SET 
                status = 'permanently_failed',
                completed_at = NOW(),
                error_message = $2,
                retry_after = NULL
            WHERE id = $1
            "#,
            job_id,
            error_message
        )
        .execute(pool)
        .await?;

        warn!(
            "Marked cache push job {} as permanently failed after {} attempts: {}",
            job_id, attempts, error_message
        );
    }

    Ok(())
}

/// Update derivation status to cache-pushed
pub async fn mark_derivation_cache_pushed(pool: &PgPool, derivation_id: i32) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE derivations 
        SET status_id = (SELECT id FROM derivation_statuses WHERE name = 'cache-pushed')
        WHERE id = $1
        "#,
        derivation_id
    )
    .execute(pool)
    .await?;

    debug!("Marked derivation {} as cache-pushed", derivation_id);
    Ok(())
}

/// Get pending cache push jobs, including failed jobs ready for retry
pub async fn get_pending_cache_push_jobs(
    pool: &PgPool,
    limit: Option<i32>,
) -> Result<Vec<CachePushJob>> {
    let jobs = sqlx::query_as!(
        CachePushJob,
        r#"
        SELECT 
            id, derivation_id, status, store_path, scheduled_at, started_at, 
            completed_at, attempts, error_message, push_size_bytes, 
            push_duration_ms, cache_destination
        FROM cache_push_jobs
        WHERE 
            (status = 'pending')
            OR 
            (status = 'failed' AND retry_after IS NOT NULL AND retry_after <= NOW())
        ORDER BY 
            CASE 
                WHEN status = 'pending' THEN 0
                WHEN status = 'failed' THEN 1
            END,
            scheduled_at ASC
        LIMIT $1
        "#,
        limit.unwrap_or(10) as i64
    )
    .fetch_all(pool)
    .await?;

    debug!(
        "Found {} cache push jobs ready to process (pending + retryable)",
        jobs.len()
    );
    Ok(jobs)
}

pub async fn cleanup_stale_cache_push_jobs(pool: &PgPool, timeout_minutes: i32) -> Result<()> {
    // Only clean up jobs that are truly stuck in 'in_progress' state
    // Don't touch 'failed' jobs that are waiting for retry
    let result = sqlx::query(
        r#"
        UPDATE cache_push_jobs 
        SET 
            status = 'failed',
            error_message = 'Job timeout - stuck in progress',
            completed_at = NOW(),
            retry_after = CASE 
                WHEN attempts < 5 THEN NOW() + (POW(2, attempts) || ' minutes')::INTERVAL
                ELSE NULL
            END
        WHERE status = 'in_progress'
            AND started_at < NOW() - ($1 || ' minutes')::INTERVAL
        "#,
    )
    .bind(timeout_minutes)
    .execute(pool)
    .await?;

    if result.rows_affected() > 0 {
        warn!(
            "🧹 Cleaned up {} stale cache push jobs stuck in progress",
            result.rows_affected()
        );
    }

    Ok(())
}
