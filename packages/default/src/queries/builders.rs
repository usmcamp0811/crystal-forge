//! Database queries for builder management.

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::builders::{
    BuildJob, Builder, BuilderEnvironmentAssignment, BuilderMetrics, BuilderStatus, BuilderSummary,
    BuilderWithEnvironments, CreateBuilderRequest, ReportMetricsRequest, UpdateBuilderRequest,
};
use crate::models::public_key::PublicKey;

/// Generate a cryptographically correct Ed25519 keypair
/// Returns (public_key_base64, private_key_base64)
///
/// SECURITY: Public key is derived from private key (not generated independently)
pub fn generate_ed25519_keypair() -> Result<(String, String)> {
    // Generate signing (private) key from secure random source
    let signing_key = SigningKey::generate(&mut OsRng);

    // CRITICAL: Derive verifying (public) key from signing key
    // This ensures cryptographic correspondence between private and public keys
    let verifying_key: VerifyingKey = signing_key.verifying_key();

    // Encode to base64 for storage/transport
    let public_key_base64 = BASE64.encode(verifying_key.as_bytes());
    let private_key_base64 = BASE64.encode(signing_key.to_bytes());

    Ok((public_key_base64, private_key_base64))
}

/// Create a new builder (returns builder and optionally generated private key)
///
/// If `public_key` is provided in request, it is validated and used.
/// If `public_key` is None, a proper Ed25519 keypair is generated server-side.
///
/// Returns: (Builder, Option<private_key_base64>)
/// - private_key is Some(...) only when generated server-side
/// - private_key is returned ONCE and never stored
pub async fn create_builder(
    pool: &PgPool,
    request: &CreateBuilderRequest,
) -> Result<(Builder, Option<String>)> {
    let (public_key_str, private_key_option) = match &request.public_key {
        Some(pk) => {
            // Client provided public key - validate it
            let public_key =
                PublicKey::from_base64(pk, &request.name).context("Invalid public key format")?;
            (public_key.to_base64(), None)
        }
        None => {
            // No public key provided - generate proper Ed25519 keypair server-side
            let (public_key_base64, private_key_base64) =
                generate_ed25519_keypair().context("Failed to generate Ed25519 keypair")?;
            (public_key_base64, Some(private_key_base64))
        }
    };

    let max_concurrent_jobs = request.max_concurrent_jobs.unwrap_or(1);

    let builder = sqlx::query_as::<_, Builder>(
        r#"
        INSERT INTO builders (name, public_key, max_cpu_cores, max_memory_mb, max_concurrent_jobs, status)
        VALUES ($1, $2, $3, $4, $5, 'inactive')
        RETURNING *
        "#
    )
    .bind(&request.name)
    .bind(public_key_str)
    .bind(request.max_cpu_cores)
    .bind(request.max_memory_mb)
    .bind(max_concurrent_jobs)
    .fetch_one(pool)
    .await
    .context("Failed to create builder")?;

    // Create environment assignments if provided
    if !request.environment_ids.is_empty() {
        for env_id in &request.environment_ids {
            assign_builder_to_environment(pool, &builder.id, env_id).await?;
        }
    }

    Ok((builder, private_key_option))
}

/// Get a builder by ID
pub async fn get_builder_by_id(pool: &PgPool, builder_id: &Uuid) -> Result<Option<Builder>> {
    let builder = sqlx::query_as::<_, Builder>("SELECT * FROM builders WHERE id = $1")
        .bind(builder_id)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch builder by ID")?;

    Ok(builder)
}

/// Get a builder with its environment assignments
pub async fn get_builder_with_environments(
    pool: &PgPool,
    builder_id: &Uuid,
) -> Result<Option<BuilderWithEnvironments>> {
    let builder = match get_builder_by_id(pool, builder_id).await? {
        Some(b) => b,
        None => return Ok(None),
    };

    let env_ids = get_builder_environment_ids(pool, builder_id).await?;

    Ok(Some(BuilderWithEnvironments {
        builder,
        assigned_environment_ids: env_ids,
    }))
}

/// List all builders with summary information
pub async fn list_builders(pool: &PgPool) -> Result<Vec<BuilderSummary>> {
    let builders = sqlx::query_as::<_, BuilderSummary>(
        r#"
        SELECT
            b.id,
            b.name,
            b.status,
            b.max_cpu_cores,
            b.max_memory_mb,
            b.max_concurrent_jobs,
            b.last_heartbeat_at,
            COALESCE(COUNT(DISTINCT bea.id), 0)::int as assigned_environment_count,
            COALESCE(COUNT(DISTINCT CASE WHEN bj.status = 'building' THEN bj.id END), 0)::int as active_jobs,
            COALESCE(COUNT(DISTINCT CASE WHEN bj.status = 'queued' AND bj.builder_id = b.id THEN bj.id END), 0)::int as queued_jobs
        FROM builders b
        LEFT JOIN builder_environment_assignments bea ON bea.builder_id = b.id
        LEFT JOIN build_jobs bj ON bj.builder_id = b.id AND bj.status IN ('queued', 'building')
        GROUP BY b.id, b.name, b.status, b.max_cpu_cores, b.max_memory_mb, b.max_concurrent_jobs, b.last_heartbeat_at
        ORDER BY b.created_at DESC
        "#
    )
    .fetch_all(pool)
    .await
    .context("Failed to list builders")?;

    Ok(builders)
}

/// Update a builder
pub async fn update_builder(
    pool: &PgPool,
    builder_id: &Uuid,
    request: &UpdateBuilderRequest,
) -> Result<Builder> {
    // Build dynamic update query based on what fields are provided
    let mut query = String::from("UPDATE builders SET updated_at = now()");
    let mut param_count = 1;

    if request.name.is_some() {
        param_count += 1;
        query.push_str(&format!(", name = ${}", param_count));
    }
    if request.status.is_some() {
        param_count += 1;
        query.push_str(&format!(", status = ${}", param_count));
    }
    if request.max_cpu_cores.is_some() {
        param_count += 1;
        query.push_str(&format!(", max_cpu_cores = ${}", param_count));
    }
    if request.max_memory_mb.is_some() {
        param_count += 1;
        query.push_str(&format!(", max_memory_mb = ${}", param_count));
    }
    if request.max_concurrent_jobs.is_some() {
        param_count += 1;
        query.push_str(&format!(", max_concurrent_jobs = ${}", param_count));
    }

    query.push_str(" WHERE id = $1 RETURNING id, name, public_key, status, max_cpu_cores, max_memory_mb, max_concurrent_jobs, last_heartbeat_at, created_at, updated_at");

    let mut query_builder = sqlx::query_as::<_, Builder>(&query).bind(builder_id);

    if let Some(ref name) = request.name {
        query_builder = query_builder.bind(name);
    }
    if let Some(ref status) = request.status {
        query_builder = query_builder.bind(status.to_string());
    }
    if let Some(cpu) = request.max_cpu_cores {
        query_builder = query_builder.bind(cpu);
    }
    if let Some(mem) = request.max_memory_mb {
        query_builder = query_builder.bind(mem);
    }
    if let Some(jobs) = request.max_concurrent_jobs {
        query_builder = query_builder.bind(jobs);
    }

    let builder = query_builder
        .fetch_one(pool)
        .await
        .context("Failed to update builder")?;

    Ok(builder)
}

/// Update builder public key
pub async fn update_builder_public_key(
    pool: &PgPool,
    builder_id: &Uuid,
    public_key_base64: &str,
    builder_name: &str,
) -> Result<Builder> {
    let public_key = PublicKey::from_base64(public_key_base64, builder_name)
        .context("Invalid public key format")?;

    let builder = sqlx::query_as::<_, Builder>(
        r#"
        UPDATE builders
        SET public_key = $2, updated_at = now()
        WHERE id = $1
        RETURNING
            id,
            name,
            public_key,
            status,
            max_cpu_cores,
            max_memory_mb,
            max_concurrent_jobs,
            last_heartbeat_at,
            created_at,
            updated_at
        "#,
    )
    .bind(builder_id)
    .bind(public_key.to_base64())
    .fetch_one(pool)
    .await
    .context("Failed to update builder public key")?;

    Ok(builder)
}

/// Deactivate a builder (soft delete)
pub async fn deactivate_builder(pool: &PgPool, builder_id: &Uuid) -> Result<Builder> {
    let builder = sqlx::query_as::<_, Builder>(
        r#"
        UPDATE builders
        SET status = 'inactive', updated_at = now()
        WHERE id = $1
        RETURNING
            id,
            name,
            public_key,
            status,
            max_cpu_cores,
            max_memory_mb,
            max_concurrent_jobs,
            last_heartbeat_at,
            created_at,
            updated_at
        "#,
    )
    .bind(builder_id)
    .fetch_one(pool)
    .await
    .context("Failed to deactivate builder")?;

    Ok(builder)
}

/// Permanently delete a builder (hard delete)
pub async fn delete_builder(pool: &PgPool, builder_id: &Uuid) -> Result<()> {
    let result = sqlx::query(
        r#"
        DELETE FROM builders
        WHERE id = $1
        "#,
    )
    .bind(builder_id)
    .execute(pool)
    .await
    .context("Failed to delete builder")?;

    if result.rows_affected() == 0 {
        bail!("Builder not found");
    }

    Ok(())
}

/// Update builder heartbeat timestamp
pub async fn update_builder_heartbeat(pool: &PgPool, builder_id: &Uuid) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE builders
        SET last_heartbeat_at = now(), status = 'active', updated_at = now()
        WHERE id = $1
        "#,
        builder_id
    )
    .execute(pool)
    .await
    .context("Failed to update builder heartbeat")?;

    Ok(())
}

/// Record builder metrics
pub async fn record_builder_metrics(
    pool: &PgPool,
    builder_id: &Uuid,
    metrics: &ReportMetricsRequest,
) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO builder_metrics (
            builder_id,
            cpu_usage_percent,
            memory_usage_mb,
            system_cpu_usage_percent,
            system_memory_total_mb,
            system_memory_used_mb
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
        builder_id,
        metrics.cpu_usage_percent,
        metrics.memory_usage_mb,
        metrics.system_cpu_usage_percent,
        metrics.system_memory_total_mb,
        metrics.system_memory_used_mb
    )
    .execute(pool)
    .await
    .context("Failed to record builder metrics")?;

    Ok(())
}

/// Get recent metrics for a builder
pub async fn get_builder_metrics(
    pool: &PgPool,
    builder_id: &Uuid,
    limit: i64,
) -> Result<Vec<BuilderMetrics>> {
    let metrics = sqlx::query_as!(
        BuilderMetrics,
        r#"
        SELECT
            id,
            builder_id,
            timestamp,
            cpu_usage_percent,
            memory_usage_mb,
            system_cpu_usage_percent,
            system_memory_total_mb,
            system_memory_used_mb
        FROM builder_metrics
        WHERE builder_id = $1
        ORDER BY timestamp DESC
        LIMIT $2
        "#,
        builder_id,
        limit
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch builder metrics")?;

    Ok(metrics)
}

/// Assign a builder to an environment
pub async fn assign_builder_to_environment(
    pool: &PgPool,
    builder_id: &Uuid,
    environment_id: &Uuid,
) -> Result<BuilderEnvironmentAssignment> {
    let assignment = sqlx::query_as!(
        BuilderEnvironmentAssignment,
        r#"
        INSERT INTO builder_environment_assignments (builder_id, environment_id)
        VALUES ($1, $2)
        ON CONFLICT (builder_id, environment_id) DO NOTHING
        RETURNING id, builder_id, environment_id, created_at
        "#,
        builder_id,
        environment_id
    )
    .fetch_one(pool)
    .await
    .context("Failed to assign builder to environment")?;

    Ok(assignment)
}

/// Remove a builder from an environment
pub async fn remove_builder_from_environment(
    pool: &PgPool,
    builder_id: &Uuid,
    environment_id: &Uuid,
) -> Result<()> {
    sqlx::query!(
        r#"
        DELETE FROM builder_environment_assignments
        WHERE builder_id = $1 AND environment_id = $2
        "#,
        builder_id,
        environment_id
    )
    .execute(pool)
    .await
    .context("Failed to remove builder from environment")?;

    Ok(())
}

/// Get all environment IDs assigned to a builder (returns empty vec for wildcard builders)
pub async fn get_builder_environment_ids(pool: &PgPool, builder_id: &Uuid) -> Result<Vec<Uuid>> {
    let env_ids = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT environment_id
        FROM builder_environment_assignments
        WHERE builder_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(builder_id)
    .fetch_all(pool)
    .await
    .context("Failed to fetch builder environment assignments")?;

    Ok(env_ids)
}

/// Update all environment assignments for a builder (replace existing)
pub async fn update_builder_environments(
    pool: &PgPool,
    builder_id: &Uuid,
    environment_ids: &[Uuid],
) -> Result<()> {
    // Start a transaction
    let mut tx = pool.begin().await?;

    // Remove all existing assignments
    sqlx::query!(
        r#"
        DELETE FROM builder_environment_assignments
        WHERE builder_id = $1
        "#,
        builder_id
    )
    .execute(&mut *tx)
    .await
    .context("Failed to clear existing environment assignments")?;

    // Add new assignments
    for env_id in environment_ids {
        sqlx::query!(
            r#"
            INSERT INTO builder_environment_assignments (builder_id, environment_id)
            VALUES ($1, $2)
            "#,
            builder_id,
            env_id
        )
        .execute(&mut *tx)
        .await
        .context("Failed to create environment assignment")?;
    }

    tx.commit().await?;

    Ok(())
}

/// Mark builders as offline if they haven't sent heartbeat within timeout
pub async fn mark_stale_builders_offline(pool: &PgPool, timeout_seconds: i64) -> Result<i64> {
    let result = sqlx::query!(
        r#"
        UPDATE builders
        SET status = 'offline', updated_at = now()
        WHERE status = 'active'
          AND last_heartbeat_at < now() - ($1 || ' seconds')::interval
        "#,
        timeout_seconds.to_string()
    )
    .execute(pool)
    .await
    .context("Failed to mark stale builders offline")?;

    Ok(result.rows_affected() as i64)
}

// =============================================================================
// BUILD JOB QUERIES (Work Queue Operations)
// =============================================================================

/// Get the number of active (building) jobs for a builder
pub async fn count_active_jobs_for_builder(pool: &PgPool, builder_id: &Uuid) -> Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM build_jobs
        WHERE builder_id = $1 AND status = 'building'
        "#,
    )
    .bind(builder_id)
    .fetch_one(pool)
    .await
    .context("Failed to count active jobs for builder")?;

    Ok(count)
}

/// Get the next queued job for a builder based on environment assignments
/// Returns None if no jobs available
/// If builder has no environment assignments, returns jobs from any environment (wildcard)
pub async fn get_next_queued_job(
    pool: &PgPool,
    environment_ids: &[Uuid],
) -> Result<Option<BuildJob>> {
    let job = if environment_ids.is_empty() {
        // Wildcard: builder can pick up jobs from any environment
        sqlx::query_as::<_, BuildJob>(
            r#"
            SELECT *
            FROM build_jobs
            WHERE status = 'queued'
            ORDER BY
                priority_weight DESC,
                (
                    SELECT c.commit_timestamp
                    FROM derivations d
                    LEFT JOIN commits c ON c.id = d.commit_id
                    WHERE d.id = build_jobs.derivation_id
                ) DESC NULLS LAST,
                created_at ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .fetch_optional(pool)
        .await
        .context("Failed to fetch next queued job (wildcard)")?
    } else {
        // Filtered: only jobs matching builder's environment assignments
        sqlx::query_as::<_, BuildJob>(
            r#"
            SELECT *
            FROM build_jobs
            WHERE status = 'queued'
              AND (environment_id = ANY($1) OR environment_id IS NULL)
            ORDER BY
                priority_weight DESC,
                (
                    SELECT c.commit_timestamp
                    FROM derivations d
                    LEFT JOIN commits c ON c.id = d.commit_id
                    WHERE d.id = build_jobs.derivation_id
                ) DESC NULLS LAST,
                created_at ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .bind(environment_ids)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch next queued job (filtered)")?
    };

    Ok(job)
}

/// Atomically claim the next job for a builder (race-free concurrency enforcement)
///
/// This function ensures concurrency limits are enforced correctly by:
/// 1. Starting a transaction
/// 2. Counting active jobs WITH row-level lock
/// 3. Checking against max_concurrent_jobs limit
/// 4. Claiming next available job (if under limit)
/// 5. Committing transaction (making count+claim atomic)
///
/// This prevents race conditions where multiple concurrent claim attempts
/// could exceed the builder's max_concurrent_jobs limit.
///
/// TASK-147: Make builder concurrency limit enforcement race-free
pub async fn claim_next_job_atomic(
    pool: &PgPool,
    builder_id: &Uuid,
    max_concurrent_jobs: i32,
    environment_ids: &[Uuid],
) -> Result<Option<BuildJob>> {
    // Start transaction for atomic count + claim
    let mut tx = pool.begin().await.context("Failed to begin transaction")?;

    // 1. Count active jobs for this builder
    // Note: We don't use FOR UPDATE here because it doesn't work with COUNT(*).
    // The atomicity is ensured by the transaction and FOR UPDATE SKIP LOCKED on the job claim.
    let active_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM build_jobs
        WHERE builder_id = $1 AND status = 'building'
        "#,
    )
    .bind(builder_id)
    .fetch_one(&mut *tx)
    .await
    .context("Failed to count active jobs in transaction")?;

    // 2. Check limit BEFORE querying for next job
    if active_count >= max_concurrent_jobs as i64 {
        // Builder at capacity - rollback and return None
        tx.rollback()
            .await
            .context("Failed to rollback transaction")?;
        return Ok(None);
    }

    // 3. Claim next available job with FOR UPDATE SKIP LOCKED
    // This atomically finds and locks the next job in priority order
    let job = if environment_ids.is_empty() {
        // Wildcard: builder can claim jobs from any environment
        sqlx::query_as::<_, BuildJob>(
            r#"
            UPDATE build_jobs
            SET builder_id = $1,
                status = 'building',
                started_at = NOW(),
                updated_at = NOW()
            WHERE id = (
                SELECT id
                FROM build_jobs
                WHERE status = 'queued'
                ORDER BY
                    priority_weight DESC,
                    (
                        SELECT c.commit_timestamp
                        FROM derivations d
                        LEFT JOIN commits c ON c.id = d.commit_id
                        WHERE d.id = build_jobs.derivation_id
                    ) DESC NULLS LAST,
                    created_at ASC
                LIMIT 1
                FOR UPDATE SKIP LOCKED
            )
            RETURNING *
            "#,
        )
        .bind(builder_id)
        .fetch_optional(&mut *tx)
        .await
        .context("Failed to claim job (wildcard) in transaction")?
    } else {
        // Filtered: only jobs matching builder's environment assignments
        sqlx::query_as::<_, BuildJob>(
            r#"
            UPDATE build_jobs
            SET builder_id = $1,
                status = 'building',
                started_at = NOW(),
                updated_at = NOW()
            WHERE id = (
                SELECT id
                FROM build_jobs
                WHERE status = 'queued'
                  AND (environment_id = ANY($2) OR environment_id IS NULL)
                ORDER BY
                    priority_weight DESC,
                    (
                        SELECT c.commit_timestamp
                        FROM derivations d
                        LEFT JOIN commits c ON c.id = d.commit_id
                        WHERE d.id = build_jobs.derivation_id
                    ) DESC NULLS LAST,
                    created_at ASC
                LIMIT 1
                FOR UPDATE SKIP LOCKED
            )
            RETURNING *
            "#,
        )
        .bind(builder_id)
        .bind(environment_ids)
        .fetch_optional(&mut *tx)
        .await
        .context("Failed to claim job (filtered) in transaction")?
    };

    // 4. Commit transaction (makes count check + job assignment atomic)
    tx.commit().await.context("Failed to commit transaction")?;

    Ok(job)
}

/// Assign a job to a builder and mark it as building
pub async fn assign_job_to_builder(
    pool: &PgPool,
    job_id: &Uuid,
    builder_id: &Uuid,
) -> Result<BuildJob> {
    let job = sqlx::query_as::<_, BuildJob>(
        r#"
        UPDATE build_jobs
        SET builder_id = $2,
            status = 'building',
            started_at = now(),
            updated_at = now()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(job_id)
    .bind(builder_id)
    .fetch_one(pool)
    .await
    .context("Failed to assign job to builder")?;

    Ok(job)
}

/// Mark a job as successfully completed
pub async fn mark_job_complete(pool: &PgPool, job_id: &Uuid) -> Result<BuildJob> {
    let job = sqlx::query_as::<_, BuildJob>(
        r#"
        UPDATE build_jobs
        SET status = 'success',
            completed_at = now(),
            updated_at = now()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(job_id)
    .fetch_one(pool)
    .await
    .context("Failed to mark job as complete")?;

    Ok(job)
}

/// Append logs to a job
pub async fn append_job_logs(pool: &PgPool, job_id: &Uuid, new_logs: &str) -> Result<()> {
    append_job_logs_with_limits(pool, job_id, new_logs, 10 * 1024 * 1024).await
}

/// Append logs to a job with safety limits.
///
/// Enforces:
/// - job must be in queued/building status
/// - total log bytes must not exceed max_total_log_bytes
pub async fn append_job_logs_with_limits(
    pool: &PgPool,
    job_id: &Uuid,
    new_logs: &str,
    max_total_log_bytes: usize,
) -> Result<()> {
    let updated = sqlx::query_scalar::<_, Uuid>(
        r#"
        UPDATE build_jobs
        SET logs = COALESCE(logs, '') || $2,
            updated_at = now()
        WHERE id = $1
          AND status IN ('queued', 'building')
          AND OCTET_LENGTH(COALESCE(logs, '')) + OCTET_LENGTH($2) <= $3
        RETURNING id
        "#,
    )
    .bind(job_id)
    .bind(new_logs)
    .bind(max_total_log_bytes as i64)
    .fetch_optional(pool)
    .await
    .context("Failed to append job logs with limits")?;

    if updated.is_some() {
        return Ok(());
    }

    // Diagnose why update failed (status/limit/not-found) for precise error handling.
    let diagnostics = sqlx::query_as::<_, (String, Option<i64>)>(
        r#"
        SELECT status, OCTET_LENGTH(COALESCE(logs, ''))
        FROM build_jobs
        WHERE id = $1
        "#,
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .context("Failed to diagnose append log failure")?;

    match diagnostics {
        None => bail!("job_not_found"),
        Some((status, current_len_opt)) => {
            if status != "queued" && status != "building" {
                bail!("invalid_job_status:{status}");
            }

            let current_len = current_len_opt.unwrap_or(0) as usize;
            if current_len.saturating_add(new_logs.len()) > max_total_log_bytes {
                bail!("log_size_limit_exceeded");
            }

            bail!("append_log_failed_unknown");
        }
    }
}

/// Clear old logs for completed/failed jobs according to retention policy.
/// Returns number of rows updated for (success_logs_cleared, failed_logs_cleared).
pub async fn cleanup_expired_build_logs(
    pool: &PgPool,
    success_retention_days: i32,
    failed_retention_days: i32,
) -> Result<(u64, u64)> {
    let success_result = sqlx::query(
        r#"
        UPDATE build_jobs
        SET logs = NULL,
            updated_at = now()
        WHERE status = 'success'
          AND completed_at IS NOT NULL
          AND completed_at < now() - ($1::text || ' days')::interval
          AND logs IS NOT NULL
        "#,
    )
    .bind(success_retention_days.to_string())
    .execute(pool)
    .await
    .context("Failed to clean up successful build logs")?;

    let failed_result = sqlx::query(
        r#"
        UPDATE build_jobs
        SET logs = NULL,
            updated_at = now()
        WHERE status = 'failed'
          AND completed_at IS NOT NULL
          AND completed_at < now() - ($1::text || ' days')::interval
          AND logs IS NOT NULL
        "#,
    )
    .bind(failed_retention_days.to_string())
    .execute(pool)
    .await
    .context("Failed to clean up failed build logs")?;

    Ok((
        success_result.rows_affected(),
        failed_result.rows_affected(),
    ))
}

/// Get a build job by ID
pub async fn get_build_job_by_id(pool: &PgPool, job_id: &Uuid) -> Result<Option<BuildJob>> {
    let job = sqlx::query_as::<_, BuildJob>(
        r#"
        SELECT *
        FROM build_jobs
        WHERE id = $1
        "#,
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .context("Failed to fetch build job by ID")?;

    Ok(job)
}

/// Increase priority of a queued build job so it runs next.
pub async fn prioritize_build_job(pool: &PgPool, job_id: &Uuid) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE build_jobs
        SET
            priority_weight = (
                SELECT COALESCE(MAX(priority_weight), 1.0) + 1.0
                FROM build_jobs
                WHERE status = 'queued'
            ),
            updated_at = now()
        WHERE id = $1
          AND status = 'queued'
        "#,
    )
    .bind(job_id)
    .execute(pool)
    .await
    .context("Failed to prioritize build job")?;

    if result.rows_affected() == 0 {
        bail!("Queued build job not found");
    }

    Ok(())
}

/// Mark a job as failed with retry logic
/// If retry_count < max_retries, re-queue the job with incremented retry_count
/// Otherwise, mark as permanently failed
pub async fn mark_job_failed_with_retry(
    pool: &PgPool,
    job_id: &Uuid,
    error_message: Option<&str>,
) -> Result<BuildJob> {
    // First, get the current job state
    let job = get_build_job_by_id(pool, job_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Job not found"))?;

    if job.retry_count < job.max_retries {
        // Re-queue the job with incremented retry count
        // Slightly reduce priority weight on retry (newer commits stay higher priority)
        let new_priority = job.priority_weight * 0.95;

        let updated_job = sqlx::query_as::<_, BuildJob>(
            r#"
            UPDATE build_jobs
            SET status = 'queued',
                retry_count = retry_count + 1,
                priority_weight = $2,
                builder_id = NULL,
                started_at = NULL,
                updated_at = now()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(job_id)
        .bind(new_priority)
        .fetch_one(pool)
        .await
        .context("Failed to re-queue job for retry")?;

        Ok(updated_job)
    } else {
        // Permanently failed - exceeded max retries
        let failed_job = sqlx::query_as::<_, BuildJob>(
            r#"
            UPDATE build_jobs
            SET status = 'failed',
                completed_at = now(),
                updated_at = now()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(job_id)
        .fetch_one(pool)
        .await
        .context("Failed to mark job as permanently failed")?;

        Ok(failed_job)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::db::test_pool;
    use base64::Engine;

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_create_and_get_builder() {
        let pool = test_pool().await;

        // Generate a test keypair
        let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let verifying_key = signing_key.verifying_key();
        let public_key_base64 =
            base64::engine::general_purpose::STANDARD.encode(verifying_key.to_bytes());

        let request = CreateBuilderRequest {
            name: "test-builder".to_string(),
            public_key: Some(public_key_base64),
            max_cpu_cores: Some(4),
            max_memory_mb: Some(8192),
            max_concurrent_jobs: Some(2),
            environment_ids: vec![],
        };

        let (builder, _private_key) = create_builder(&pool, &request)
            .await
            .expect("Failed to create builder");

        assert_eq!(builder.name, "test-builder");
        assert_eq!(builder.max_cpu_cores, Some(4));
        assert_eq!(builder.max_concurrent_jobs, 2);
        assert_eq!(builder.status, BuilderStatus::Inactive);

        // Get builder back
        let fetched = get_builder_by_id(&pool, &builder.id)
            .await
            .expect("Failed to fetch builder")
            .expect("Builder not found");

        assert_eq!(fetched.id, builder.id);
        assert_eq!(fetched.name, builder.name);
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_builder_heartbeat() {
        let pool = test_pool().await;

        let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let verifying_key = signing_key.verifying_key();
        let public_key_base64 =
            base64::engine::general_purpose::STANDARD.encode(verifying_key.to_bytes());

        let request = CreateBuilderRequest {
            name: "heartbeat-test".to_string(),
            public_key: Some(public_key_base64),
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: None,
            environment_ids: vec![],
        };

        let (builder, _private_key) = create_builder(&pool, &request)
            .await
            .expect("Failed to create builder");

        assert_eq!(builder.status, BuilderStatus::Inactive);
        assert!(builder.last_heartbeat_at.is_none());

        // Update heartbeat
        update_builder_heartbeat(&pool, &builder.id)
            .await
            .expect("Failed to update heartbeat");

        // Fetch and verify
        let updated = get_builder_by_id(&pool, &builder.id)
            .await
            .expect("Failed to fetch builder")
            .expect("Builder not found");

        assert_eq!(updated.status, BuilderStatus::Active);
        assert!(updated.last_heartbeat_at.is_some());
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_create_builder_invalid_public_key_base64() {
        let pool = test_pool().await;

        // Invalid base64 string
        let request = CreateBuilderRequest {
            name: "invalid-key-builder".to_string(),
            public_key: Some("not-valid-base64!!!".to_string()),
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: None,
            environment_ids: vec![],
        };

        let result = create_builder(&pool, &request).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to decode base64")
        );
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_create_builder_invalid_public_key_length() {
        let pool = test_pool().await;

        // Valid base64 but wrong length (16 bytes instead of 32)
        let wrong_length_key = base64::engine::general_purpose::STANDARD.encode(vec![0u8; 16]);

        let request = CreateBuilderRequest {
            name: "wrong-length-builder".to_string(),
            public_key: Some(wrong_length_key),
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: None,
            environment_ids: vec![],
        };

        let result = create_builder(&pool, &request).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("must be exactly 32 bytes")
        );
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_create_builder_empty_public_key() {
        let pool = test_pool().await;

        let request = CreateBuilderRequest {
            name: "empty-key-builder".to_string(),
            public_key: Some("".to_string()),
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: None,
            environment_ids: vec![],
        };

        let result = create_builder(&pool, &request).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Public key cannot be empty")
        );
    }
}
