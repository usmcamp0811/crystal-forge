use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use tracing::{debug, error};

#[derive(Debug, FromRow)]
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
pub async fn get_derivations_needing_cache_push(
    pool: &PgPool,
    limit: Option<i32>,
) -> Result<Vec<crate::models::derivations::Derivation>> {
    let sql = r#"
        SELECT 
            d.id,
            d.commit_id,
            d.derivation_type AS "derivation_type: crate::models::derivations::DerivationType",
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
            d.build_last_heartbeat
        FROM derivations d
        WHERE d.status_id = (SELECT id FROM derivation_statuses WHERE name = 'build-complete')
          AND d.derivation_path IS NOT NULL
          -- no active job already done or running
          AND NOT EXISTS (
            SELECT 1
            FROM cache_push_jobs j
            WHERE j.derivation_id = d.id
              AND j.status IN ('completed','in_progress')
          )
          -- either no job yet, or only failed ones with attempts left
          AND (
            NOT EXISTS (SELECT 1 FROM cache_push_jobs j2 WHERE j2.derivation_id = d.id)
            OR EXISTS (
              SELECT 1 FROM cache_push_jobs j3
              WHERE j3.derivation_id = d.id
                AND j3.status = 'failed'
                AND j3.attempts < 5
            )
          )
        ORDER BY d.completed_at ASC NULLS LAST
        LIMIT $1
    "#;

    let derivations = sqlx::query_as(sql)
        .bind(limit.unwrap_or(10)) // i32 is fine
        .fetch_all(pool)
        .await?;

    debug!("Found {} derivations needing cache push", derivations.len());
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
        SET status = 'in_progress', started_at = NOW(), attempts = attempts + 1
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
    Ok(())
}

/// Mark cache push job as failed
pub async fn mark_cache_push_failed(pool: &PgPool, job_id: i32, error_message: &str) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE cache_push_jobs 
        SET 
            status = 'failed',
            completed_at = NOW(),
            error_message = $2
        WHERE id = $1
        "#,
        job_id,
        error_message
    )
    .execute(pool)
    .await?;

    debug!(
        "Marked cache push job {} as failed: {}",
        job_id, error_message
    );
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

/// Get pending cache push jobs
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
        WHERE status = 'pending'
        ORDER BY scheduled_at ASC
        LIMIT $1
        "#,
        limit.unwrap_or(10) as i64 // Cast to i64
    )
    .fetch_all(pool)
    .await?;

    debug!("Found {} pending cache push jobs", jobs.len());
    Ok(jobs)
}
