//! Database queries for builder management.

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::{DateTime, Utc};
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use sqlx::PgPool;
use tracing::info;
use uuid::Uuid;

use crate::models::builders::{
    BuildJob, BuildJobRow, Builder, BuilderEnvironmentAssignment, BuilderMetrics, BuilderSummary,
    BuilderWithEnvironments, CreateBuilderRequest, RemoteBuildExecutionStrategy,
    ReportMetricsRequest, UpdateBuilderRequest,
};
use crate::models::public_key::PublicKey;
use crate::models::retry_policy::{
    AutomaticRetryPolicy, RetryFailureClass, automatic_retry_budget_remaining,
    automatic_retry_eligible,
};
use crate::queries::attention;

const CLAIM_NEXT_JOB_SERVER_DERIVATION_WILDCARD_SQL: &str = r#"
    UPDATE build_jobs
    SET builder_id = $1,
        builder_session_id = $2,
        status = 'building',
        started_at = NOW(),
        updated_at = NOW()
    WHERE id = (
        SELECT build_jobs.id
        FROM build_jobs
        JOIN derivations d ON d.id = build_jobs.derivation_id
        WHERE build_jobs.status = 'queued'
            AND build_jobs.available_at <= NOW()
          AND d.cf_agent_enabled IS TRUE
          AND d.policy_requirements_met IS TRUE
        ORDER BY
            build_jobs.queue_position DESC NULLS LAST,
            build_jobs.priority_weight DESC,
            (
                SELECT c.commit_timestamp
                FROM commits c
                WHERE c.id = d.commit_id
            ) DESC NULLS LAST,
            build_jobs.created_at ASC
        LIMIT 1
        FOR UPDATE OF build_jobs SKIP LOCKED
    )
    RETURNING *
    "#;

const CLAIM_NEXT_JOB_SERVER_DERIVATION_FILTERED_SQL: &str = r#"
    UPDATE build_jobs
    SET builder_id = $1,
        builder_session_id = $3,
        status = 'building',
        started_at = NOW(),
        updated_at = NOW()
    WHERE id = (
        SELECT build_jobs.id
        FROM build_jobs
        JOIN derivations d ON d.id = build_jobs.derivation_id
        WHERE build_jobs.status = 'queued'
          AND build_jobs.available_at <= NOW()
          AND (build_jobs.environment_id = ANY($2) OR build_jobs.environment_id IS NULL)
          AND d.cf_agent_enabled IS TRUE
          AND d.policy_requirements_met IS TRUE
        ORDER BY
            build_jobs.queue_position DESC NULLS LAST,
            build_jobs.priority_weight DESC,
            (
                SELECT c.commit_timestamp
                FROM commits c
                WHERE c.id = d.commit_id
            ) DESC NULLS LAST,
            build_jobs.created_at ASC
        LIMIT 1
        FOR UPDATE OF build_jobs SKIP LOCKED
    )
    RETURNING *
    "#;

const CLAIM_NEXT_JOB_VERIFIED_SOURCE_WILDCARD_SQL: &str = r#"
    UPDATE build_jobs
    SET builder_id = $1,
        builder_session_id = $3,
        status = 'building',
        started_at = NOW(),
        updated_at = NOW()
    WHERE id = (
        SELECT build_jobs.id
        FROM build_jobs
        JOIN derivations d ON d.id = build_jobs.derivation_id
        LEFT JOIN commits c ON c.id = d.commit_id
        LEFT JOIN flakes f ON f.id = c.flake_id AND f.deleted_at IS NULL
        WHERE build_jobs.status = 'queued'
          AND build_jobs.available_at <= NOW()
          AND d.cf_agent_enabled IS TRUE
          AND d.policy_requirements_met IS TRUE
          AND (
              NOT $2
              OR (
                  d.commit_id IS NOT NULL
                  AND d.derivation_path IS NOT NULL
                  AND c.id IS NOT NULL
                  AND f.id IS NOT NULL
              )
          )
        ORDER BY
            build_jobs.queue_position DESC NULLS LAST,
            build_jobs.priority_weight DESC,
            c.commit_timestamp DESC NULLS LAST,
            build_jobs.created_at ASC
        LIMIT 1
        FOR UPDATE OF build_jobs SKIP LOCKED
    )
    RETURNING *
    "#;

const CLAIM_NEXT_JOB_VERIFIED_SOURCE_FILTERED_SQL: &str = r#"
    UPDATE build_jobs
    SET builder_id = $1,
        builder_session_id = $4,
        status = 'building',
        started_at = NOW(),
        updated_at = NOW()
    WHERE id = (
        SELECT build_jobs.id
        FROM build_jobs
        JOIN derivations d ON d.id = build_jobs.derivation_id
        LEFT JOIN commits c ON c.id = d.commit_id
        LEFT JOIN flakes f ON f.id = c.flake_id AND f.deleted_at IS NULL
        WHERE build_jobs.status = 'queued'
          AND build_jobs.available_at <= NOW()
          AND (build_jobs.environment_id = ANY($2) OR build_jobs.environment_id IS NULL)
          AND d.cf_agent_enabled IS TRUE
          AND d.policy_requirements_met IS TRUE
          AND (
              NOT $3
              OR (
                  d.commit_id IS NOT NULL
                  AND d.derivation_path IS NOT NULL
                  AND c.id IS NOT NULL
                  AND f.id IS NOT NULL
              )
          )
        ORDER BY
            build_jobs.queue_position DESC NULLS LAST,
            build_jobs.priority_weight DESC,
            c.commit_timestamp DESC NULLS LAST,
            build_jobs.created_at ASC
        LIMIT 1
        FOR UPDATE OF build_jobs SKIP LOCKED
    )
    RETURNING *
    "#;

/// Advisory lock used to serialize all queue priority_weight mutations.
const BUILD_QUEUE_PRIORITY_LOCK_ID: i64 = 0x4346_4251; // 'CFBQ'

async fn lock_build_queue_priority(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(BUILD_QUEUE_PRIORITY_LOCK_ID)
        .execute(&mut **tx)
        .await
        .context("Failed to acquire build queue priority lock")?;
    Ok(())
}

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
    let enabled = request.enabled.unwrap_or(true);

    let builder = sqlx::query_as::<_, Builder>(
        r#"
        INSERT INTO builders (name, host, arch, public_key, max_cpu_cores, max_memory_mb, max_concurrent_jobs, enabled, status, registered)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'inactive', true)
        RETURNING *
        "#
    )
    .bind(&request.name)
    .bind(&request.host)
    .bind(&request.arch)
    .bind(public_key_str)
    .bind(request.max_cpu_cores)
    .bind(request.max_memory_mb)
    .bind(max_concurrent_jobs)
    .bind(enabled)
    .fetch_one(pool)
    .await
    .context("Failed to create builder")?;

    // Create environment assignments if provided
    if !request.environment_ids.is_empty() {
        for env_id in &request.environment_ids {
            assign_builder_to_environment(pool, &builder.id, env_id).await?;
        }
    }

    Ok((builder.with_public_key_fingerprint(), private_key_option))
}

/// Get a builder by ID
pub async fn get_builder_by_id(pool: &PgPool, builder_id: &Uuid) -> Result<Option<Builder>> {
    let builder = sqlx::query_as::<_, Builder>("SELECT * FROM builders WHERE id = $1")
        .bind(builder_id)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch builder by ID")?;

    Ok(builder.map(Builder::with_public_key_fingerprint))
}

/// Get a builder by its registered public key.
pub async fn get_builder_by_public_key(
    pool: &PgPool,
    public_key: &PublicKey,
) -> Result<Option<Builder>> {
    let builder = sqlx::query_as::<_, Builder>("SELECT * FROM builders WHERE public_key = $1")
        .bind(public_key)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch builder by public key")?;

    Ok(builder.map(Builder::with_public_key_fingerprint))
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
            b.host,
            b.arch,
            b.status,
            b.max_cpu_cores,
            b.max_memory_mb,
            b.max_concurrent_jobs,
            b.enabled,
            b.last_heartbeat_at,
            COALESCE(COUNT(DISTINCT bea.id), 0)::int as assigned_environment_count,
            COALESCE(COUNT(DISTINCT CASE WHEN bj.status = 'building' AND bj.builder_id = b.id THEN bj.id END), 0)::int as active_jobs,
            -- Count queued jobs eligible for this builder (matching environments or no environment)
            COALESCE((
                SELECT COUNT(DISTINCT qj.id)
                FROM build_jobs qj
                WHERE qj.status = 'queued'
                  AND (
                    qj.environment_id IS NULL
                    OR qj.environment_id IN (SELECT environment_id FROM builder_environment_assignments WHERE builder_id = b.id)
                  )
            ), 0)::int as queued_jobs,
            -- JSON array of assigned environments with name and color (ordered by name)
            COALESCE(
                (
                    SELECT json_agg(json_build_object('name', e.name, 'color_hex', e.color_hex) ORDER BY e.name)
                    FROM builder_environment_assignments bea_inner
                    JOIN environments e ON e.id = bea_inner.environment_id
                    WHERE bea_inner.builder_id = b.id
                ),
                '[]'::json
            ) as assigned_environments,
            -- Fingerprint is computed from public_key to avoid sync issues
            encode(digest(decode(b.public_key, 'base64'), 'sha256'::text), 'hex') as public_key_fingerprint,
            b.registered,
            b.load_avg,
            -- Count completed jobs in last 24 hours
            COALESCE((
                SELECT COUNT(*)
                FROM build_jobs
                WHERE builder_id = b.id
                  AND status = 'success'
                  AND completed_at > now() - interval '24 hours'
            ), 0)::int as completed_24h,
            -- Count failed jobs in last 24 hours
            COALESCE((
                SELECT COUNT(*)
                FROM build_jobs
                WHERE builder_id = b.id
                  AND status = 'failed'
                  AND completed_at > now() - interval '24 hours'
            ), 0)::int as failed_24h
        FROM builders b
        LEFT JOIN builder_environment_assignments bea ON bea.builder_id = b.id
        LEFT JOIN build_jobs bj ON bj.builder_id = b.id AND bj.status = 'building'
        GROUP BY b.id, b.name, b.host, b.arch, b.status, b.max_cpu_cores, b.max_memory_mb, b.max_concurrent_jobs, b.enabled, b.last_heartbeat_at, b.registered, b.load_avg
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
    if request.host.is_some() {
        param_count += 1;
        query.push_str(&format!(", host = ${}", param_count));
    }
    if request.arch.is_some() {
        param_count += 1;
        query.push_str(&format!(", arch = ${}", param_count));
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
    if request.enabled.is_some() {
        param_count += 1;
        query.push_str(&format!(", enabled = ${}", param_count));
    }

    query.push_str(" WHERE id = $1 RETURNING *");

    let mut query_builder = sqlx::query_as::<_, Builder>(&query).bind(builder_id);

    if let Some(ref name) = request.name {
        query_builder = query_builder.bind(name);
    }
    if let Some(ref host) = request.host {
        query_builder = query_builder.bind(host);
    }
    if let Some(ref arch) = request.arch {
        query_builder = query_builder.bind(arch);
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
    if let Some(enabled) = request.enabled {
        query_builder = query_builder.bind(enabled);
    }

    let builder = query_builder
        .fetch_one(pool)
        .await
        .context("Failed to update builder")?;

    Ok(builder.with_public_key_fingerprint())
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
        RETURNING *
        "#,
    )
    .bind(builder_id)
    .bind(public_key.to_base64())
    .fetch_one(pool)
    .await
    .context("Failed to update builder public key")?;

    Ok(builder.with_public_key_fingerprint())
}

/// Deactivate a builder (soft delete)
pub async fn deactivate_builder(pool: &PgPool, builder_id: &Uuid) -> Result<Builder> {
    let builder = sqlx::query_as::<_, Builder>(
        r#"
        UPDATE builders
        SET status = 'inactive', updated_at = now()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(builder_id)
    .fetch_one(pool)
    .await
    .context("Failed to deactivate builder")?;

    Ok(builder.with_public_key_fingerprint())
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
pub async fn update_builder_heartbeat(
    pool: &PgPool,
    builder_id: &Uuid,
    builder_session_id: Option<&Uuid>,
) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE builders
        SET last_heartbeat_at = now(), status = 'active', updated_at = now()
        WHERE id = $1
          AND (
                (current_session_id IS NULL AND $2::uuid IS NULL)
                OR current_session_id = $2
          )
        "#,
    )
    .bind(builder_id)
    .bind(builder_session_id)
    .execute(pool)
    .await
    .context("Failed to update builder heartbeat")?;

    if result.rows_affected() == 0 {
        bail!("Builder heartbeat rejected due to session mismatch");
    }

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

/// Re-queue orphaned jobs stuck in `building` with no active builder.
///
/// A job is considered orphaned when it is `building` and:
/// - `builder_id` is NULL, or
/// - the referenced builder row is missing, or
/// - the referenced builder is not `active` (e.g. offline/inactive), or
/// - the referenced builder is disabled and should not accept work.
pub async fn requeue_orphaned_building_jobs(pool: &PgPool) -> Result<Vec<BuildJob>> {
    requeue_orphaned_building_jobs_with_reason(pool, "builder recovery").await
}

/// Re-queue orphaned jobs stuck in `building` and append an auditable reason to
/// each recovered job's logs.
pub async fn requeue_orphaned_building_jobs_with_reason(
    pool: &PgPool,
    reason: &str,
) -> Result<Vec<BuildJob>> {
    let recovered = sqlx::query_as::<_, BuildJobRow>(
        r#"
        UPDATE build_jobs bj
        SET status = 'queued',
            builder_id = NULL,
            builder_session_id = NULL,
            started_at = NULL,
            logs = RIGHT(
                COALESCE(logs, ''),
                -- RIGHT(text, n) counts characters, not bytes. Divide the
                -- remaining byte budget by 4 so retained UTF-8 text cannot
                -- exceed the log byte ceiling even with 4-byte code points.
                GREATEST(
                    0,
                    (10 * 1024 * 1024 - OCTET_LENGTH(E'\n\nRecovery: re-queued from building by ' || $1)) / 4
                )
            ) || E'\n\nRecovery: re-queued from building by ' || $1,
            updated_at = now()
        WHERE bj.status = 'building'
          AND (
                bj.builder_id IS NULL
                OR NOT EXISTS (
                    SELECT 1 FROM builders b WHERE b.id = bj.builder_id
                )
                OR EXISTS (
                    SELECT 1 FROM builders b
                    WHERE b.id = bj.builder_id
                      AND (b.status <> 'active' OR NOT b.enabled)
                )
          )
        RETURNING bj.*
        "#,
    )
    .bind(reason)
    .fetch_all(pool)
    .await
    .context("Failed to re-queue orphaned building jobs")?;

    Ok(recovered)
}

const REQUEUE_BUILDER_ASSIGNED_BUILDING_JOBS_SQL: &str = r#"
        UPDATE build_jobs bj
        SET status = 'queued',
            builder_id = NULL,
            builder_session_id = NULL,
            started_at = NULL,
            logs = RIGHT(
                COALESCE(logs, ''),
                GREATEST(
                    0,
                    (10 * 1024 * 1024 - OCTET_LENGTH(E'\n\nRecovery: re-queued from building by ' || $2)) / 4
                )
            ) || E'\n\nRecovery: re-queued from building by ' || $2,
            updated_at = now()
        WHERE bj.status = 'building'
          AND bj.builder_id = $1
          AND bj.builder_session_id IS DISTINCT FROM $3
        RETURNING bj.*
        "#;

/// Establish a builder process/session and recover only stale work from older sessions.
pub async fn establish_builder_session(
    pool: &PgPool,
    builder_id: &Uuid,
    builder_session_id: &Uuid,
    stale_timeout_secs: i64,
    reason: &str,
) -> Result<Vec<BuildJob>> {
    let mut tx = pool
        .begin()
        .await
        .context("Failed to begin builder session transaction")?;

    let current: Option<(Option<Uuid>, Option<DateTime<Utc>>, String)> = sqlx::query_as(
        r#"
        SELECT current_session_id, last_heartbeat_at, status
        FROM builders
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(builder_id)
    .fetch_optional(&mut *tx)
    .await
    .context("Failed to lock builder for session establishment")?;

    let Some((current_session_id, last_heartbeat_at, status)) = current else {
        bail!("Builder not found");
    };

    let cutoff = Utc::now() - chrono::Duration::seconds(stale_timeout_secs);
    let heartbeat_is_fresh = last_heartbeat_at.is_some_and(|heartbeat| heartbeat > cutoff);
    let different_active_session = current_session_id
        .map(|session_id| session_id != *builder_session_id)
        .unwrap_or(false)
        && heartbeat_is_fresh;
    let fresh_legacy_session =
        current_session_id.is_none() && status == "active" && heartbeat_is_fresh;

    if different_active_session || fresh_legacy_session {
        bail!(
            "active_builder_session_exists: builder {} has a fresh active session",
            builder_id
        );
    }

    sqlx::query(
        r#"
        UPDATE builders
        SET current_session_id = $2,
            current_session_started_at = CASE
                WHEN current_session_id IS DISTINCT FROM $2 THEN now()
                ELSE current_session_started_at
            END,
            last_heartbeat_at = now(),
            status = 'active',
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(builder_id)
    .bind(builder_session_id)
    .execute(&mut *tx)
    .await
    .context("Failed to update builder session")?;

    let recovered = sqlx::query_as::<_, BuildJobRow>(REQUEUE_BUILDER_ASSIGNED_BUILDING_JOBS_SQL)
        .bind(builder_id)
        .bind(reason)
        .bind(builder_session_id)
        .fetch_all(&mut *tx)
        .await
        .context("Failed to re-queue builder-assigned building jobs for stale session")?;

    tx.commit()
        .await
        .context("Failed to commit builder session transaction")?;

    Ok(recovered)
}

/// Re-queue jobs that were assigned to this builder in a previous process.
///
/// API builders keep active work only in-process. If the service restarts after
/// claiming a job, the replacement process resolves the same builder ID and
/// resumes heartbeats, so generic orphan recovery will not see the builder as
/// offline. Startup recovery must therefore clear stale `building` assignments
/// for this specific builder identity before the new process starts polling.
pub async fn requeue_builder_assigned_building_jobs_with_reason(
    pool: &PgPool,
    builder_id: &Uuid,
    reason: &str,
) -> Result<Vec<BuildJob>> {
    let recovered = sqlx::query_as::<_, BuildJobRow>(REQUEUE_BUILDER_ASSIGNED_BUILDING_JOBS_SQL)
        .bind(builder_id)
        .bind(reason)
        .bind(Option::<Uuid>::None)
        .fetch_all(pool)
        .await
        .context("Failed to re-queue builder-assigned building jobs")?;

    Ok(recovered)
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
    execution_strategy: RemoteBuildExecutionStrategy,
    builder_session_id: Option<&Uuid>,
) -> Result<Option<BuildJob>> {
    // Start transaction for atomic count + claim
    let mut tx = pool.begin().await.context("Failed to begin transaction")?;

    // Lock and validate the persistent builder row before counting or claiming.
    // This makes session takeover atomic with the claim path: once a newer
    // process establishes `current_session_id`, an older process cannot pass
    // authentication and then claim a job with its obsolete session.
    let current_session_id: Option<Uuid> = sqlx::query_scalar(
        r#"
        SELECT current_session_id
        FROM builders
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(builder_id)
    .fetch_optional(&mut *tx)
    .await
    .context("Failed to lock builder session for claim")?
    .ok_or_else(|| anyhow::anyhow!("builder_not_found"))?;

    match (current_session_id, builder_session_id) {
        // Legacy sessionless: both sides have no session → allowed
        (None, None) => {}
        // Session match: builder supplied a session that matches the current one
        (Some(current), Some(supplied)) if current == *supplied => {}
        // All other combinations are mismatches:
        //   (Some(_), None)     – current has a session but builder didn't supply one
        //   (None, Some(_))     – builder supplied a session but current has none
        //   (Some(a), Some(b))  – sessions don't match
        _ => {
            tx.rollback()
                .await
                .context("Failed to rollback superseded builder claim")?;
            bail!("builder_session_mismatch");
        }
    }

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
        match execution_strategy {
            RemoteBuildExecutionStrategy::ServerDerivation => {
                sqlx::query_as::<_, BuildJobRow>(CLAIM_NEXT_JOB_SERVER_DERIVATION_WILDCARD_SQL)
                    .bind(builder_id)
                    .bind(builder_session_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .context("Failed to claim job (wildcard server_derivation) in transaction")?
            }
            RemoteBuildExecutionStrategy::SourceReEvaluateVerified => {
                sqlx::query_as::<_, BuildJobRow>(CLAIM_NEXT_JOB_VERIFIED_SOURCE_WILDCARD_SQL)
                    .bind(builder_id)
                    .bind(true)
                    .bind(builder_session_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .context(
                        "Failed to claim job (wildcard source_re_evaluate_verified) in transaction",
                    )?
            }
        }
    } else {
        // Filtered: only jobs matching builder's environment assignments
        match execution_strategy {
            RemoteBuildExecutionStrategy::ServerDerivation => {
                sqlx::query_as::<_, BuildJobRow>(CLAIM_NEXT_JOB_SERVER_DERIVATION_FILTERED_SQL)
                    .bind(builder_id)
                    .bind(environment_ids)
                    .bind(builder_session_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .context("Failed to claim job (filtered server_derivation) in transaction")?
            }
            RemoteBuildExecutionStrategy::SourceReEvaluateVerified => {
                sqlx::query_as::<_, BuildJobRow>(CLAIM_NEXT_JOB_VERIFIED_SOURCE_FILTERED_SQL)
                    .bind(builder_id)
                    .bind(environment_ids)
                    .bind(true)
                    .bind(builder_session_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .context(
                        "Failed to claim job (filtered source_re_evaluate_verified) in transaction",
                    )?
            }
        }
    };

    // 4. Commit transaction (makes count check + job assignment atomic)
    tx.commit().await.context("Failed to commit transaction")?;

    Ok(job)
}

/// Assign a job to a builder and mark it as building.
///
/// Test-only helper: this directly transitions an arbitrary job to
/// 'building' by ID, bypassing the policy-gated claim queries
/// (`claim_next_job_atomic`, `get_next_job_for_builder`). It exists to
/// force test fixtures into a known 'building' state when exercising
/// unrelated behavior (e.g. orphaned-job recovery, stale-builder
/// handling). It must never be used as, or become, a production claim
/// path — hence `#[cfg(test)]` rather than `pub`.
#[cfg(test)]
pub(crate) async fn assign_job_to_builder(
    pool: &PgPool,
    job_id: &Uuid,
    builder_id: &Uuid,
) -> Result<BuildJob> {
    let job = sqlx::query_as::<_, BuildJobRow>(
        r#"
        UPDATE build_jobs
        SET builder_id = $2,
            builder_session_id = NULL,
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

/// Mark a job as successfully completed for a specific builder lease.
///
/// This transition is guarded by `(id, builder_id, status='building')` so a stale
/// builder cannot clobber a job that has already been re-queued or reclaimed.
pub async fn mark_job_complete(
    pool: &PgPool,
    job_id: &Uuid,
    builder_id: &Uuid,
    builder_session_id: Option<&Uuid>,
) -> Result<BuildJob> {
    let result = sqlx::query(
        r#"
        UPDATE build_jobs
        SET status = 'success',
            completed_at = now(),
            updated_at = now()
        WHERE id = $1
          AND builder_id = $2
          AND (builder_session_id IS NULL OR builder_session_id = $3)
          AND status = 'building'
        "#,
    )
    .bind(job_id)
    .bind(builder_id)
    .bind(builder_session_id)
    .execute(pool)
    .await
    .context("Failed to mark job as complete")?;

    if result.rows_affected() == 0 {
        bail!("Build job not found or no longer owned by this builder in building status");
    }

    let job = get_build_job_by_id(pool, job_id).await?.ok_or_else(|| {
        anyhow::anyhow!("Build job disappeared after successful complete transition")
    })?;

    Ok(job)
}

/// Atomically mark a build job and its derivation as complete in a single transaction.
///
/// This prevents the inconsistent state where `build_jobs` says `'success'` but
/// `derivations` still says `'build_in_progress'`. The derivation update and job
/// transition either both succeed or both roll back.
///
/// **Idempotent**: If the job is already `'success'` with matching builder+session,
/// this returns `Ok((job, false))` without modifying anything. Ownership is validated
/// before the idempotent check so a superseded builder cannot supply a different
/// `store_path` and queue a bogus cache push.
///
/// The returned boolean is `true` when this call performed a new transition
/// (`building → success`). The caller should only queue best-effort cache-push side
/// effects when `true`; idempotent retries reuse the originally persisted store path
/// and must not accept a newly supplied request path.
pub async fn complete_job_atomic(
    pool: &PgPool,
    job_id: &Uuid,
    builder_id: &Uuid,
    builder_session_id: Option<&Uuid>,
    store_path: Option<&str>,
) -> Result<(BuildJob, bool)> {
    let mut tx = pool
        .begin()
        .await
        .context("Failed to begin completion transaction")?;

    // Lock the job row and read all fields needed for ownership validation.
    let row = sqlx::query_as::<_, (i32, String, Option<Uuid>, Option<Uuid>)>(
        r#"
        SELECT derivation_id, status, builder_id, builder_session_id
        FROM build_jobs
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(job_id)
    .fetch_optional(&mut *tx)
    .await
    .context("Failed to lock job for completion")?
    .ok_or_else(|| anyhow::anyhow!("Build job not found"))?;

    let (derivation_id, status, job_builder_id, job_session_id) = row;

    // Validate builder ownership BEFORE any status check. This prevents a
    // superseded or unrelated builder from exploiting the idempotent path
    // to queue a cache push with a different store path.
    if job_builder_id != Some(*builder_id) {
        bail!("Build job not owned by this builder");
    }
    match (job_session_id, builder_session_id) {
        (None, None) => {}                  // legacy sessionless match
        (Some(j), Some(b)) if j == *b => {} // exact session match
        _ => bail!("Builder session mismatch"),
    }

    if status == "success" {
        // Idempotent: already completed by this exact builder+session.
        // Return false so the caller knows not to queue a new cache push.
        tx.commit()
            .await
            .context("Failed to commit idempotent completion transaction")?;
        let job = get_build_job_by_id(pool, job_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Build job disappeared on idempotent completion"))?;
        return Ok((job, false));
    }

    if status != "building" {
        bail!("Build job status is '{status}', expected 'building' or 'success'");
    }

    // Guarded transition: job must be 'building' and owned by builder+session.
    // Uses exact session matching (same semantics as claims and heartbeats).
    let updated = sqlx::query_as::<_, (i32,)>(
        r#"
        UPDATE build_jobs
        SET status = 'success',
            completed_at = now(),
            updated_at = now()
        WHERE id = $1
          AND builder_id = $2
          AND (
                (builder_session_id IS NULL AND $3::uuid IS NULL)
                OR builder_session_id = $3
          )
          AND status = 'building'
        RETURNING derivation_id
        "#,
    )
    .bind(job_id)
    .bind(builder_id)
    .bind(builder_session_id)
    .fetch_optional(&mut *tx)
    .await
    .context("Failed to mark job complete")?;

    updated.ok_or_else(|| {
        anyhow::anyhow!("Build job not owned by this builder or session mismatch")
    })?;

    // Update derivation in the same transaction.
    if let Some(store_path) = store_path {
        crate::queries::derivations::mark_target_build_complete(
            &mut *tx,
            derivation_id,
            store_path,
        )
        .await
        .context("Failed to mark derivation complete")?;
    }

    tx.commit()
        .await
        .context("Failed to commit completion transaction")?;

    let _ = attention::resolve(pool, "builds", "build_job", &job_id.to_string())
        .await
        .map_err(|e| tracing::error!("failed to resolve build attention occurrence: {e:#}"));

    let job = get_build_job_by_id(pool, job_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Build job disappeared after completion"))?;

    Ok((job, true))
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
    append_job_logs_with_limits_guarded(pool, job_id, None, None, new_logs, max_total_log_bytes)
        .await
}

pub async fn append_job_logs_with_limits_for_builder(
    pool: &PgPool,
    job_id: &Uuid,
    builder_id: &Uuid,
    builder_session_id: Option<&Uuid>,
    new_logs: &str,
    max_total_log_bytes: usize,
) -> Result<()> {
    append_job_logs_with_limits_guarded(
        pool,
        job_id,
        Some(builder_id),
        builder_session_id,
        new_logs,
        max_total_log_bytes,
    )
    .await
}

async fn append_job_logs_with_limits_guarded(
    pool: &PgPool,
    job_id: &Uuid,
    builder_id: Option<&Uuid>,
    builder_session_id: Option<&Uuid>,
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
          AND ($4::uuid IS NULL OR builder_id = $4)
          AND (builder_session_id IS NULL OR builder_session_id = $5)
          AND OCTET_LENGTH(COALESCE(logs, '')) + OCTET_LENGTH($2) <= $3
        RETURNING id
        "#,
    )
    .bind(job_id)
    .bind(new_logs)
    .bind(max_total_log_bytes as i64)
    .bind(builder_id)
    .bind(builder_session_id)
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
    let job = sqlx::query_as::<_, BuildJobRow>(
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
    let mut tx = pool
        .begin()
        .await
        .context("Failed to open prioritize transaction")?;

    lock_build_queue_priority(&mut tx).await?;

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
    .execute(&mut *tx)
    .await
    .context("Failed to prioritize build job")?;

    if result.rows_affected() == 0 {
        bail!("Queued build job not found");
    }

    tx.commit()
        .await
        .context("Failed to commit prioritize transaction")?;

    Ok(())
}

/// Move a queued build job one position earlier in the persisted queue order.
pub async fn move_build_job_up(pool: &PgPool, job_id: &Uuid) -> Result<()> {
    reorder_queued_build_job(pool, job_id, true).await
}

/// Move a queued build job one position later in the persisted queue order.
pub async fn move_build_job_down(pool: &PgPool, job_id: &Uuid) -> Result<()> {
    reorder_queued_build_job(pool, job_id, false).await
}

async fn reorder_queued_build_job(pool: &PgPool, job_id: &Uuid, move_up: bool) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("Failed to open queue reorder transaction")?;

    lock_build_queue_priority(&mut tx).await?;

    let mut ids: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT id
        FROM build_jobs
        WHERE status = 'queued'
        ORDER BY queue_position DESC NULLS LAST, priority_weight DESC, created_at ASC
        FOR UPDATE
        "#,
    )
    .fetch_all(&mut *tx)
    .await
    .context("Failed to load queued build order")?;

    let Some(idx) = ids.iter().position(|id| id == job_id) else {
        bail!("Queued build job not found");
    };

    if move_up {
        if idx == 0 {
            return Ok(());
        }
        ids.swap(idx, idx - 1);
    } else {
        if idx + 1 >= ids.len() {
            return Ok(());
        }
        ids.swap(idx, idx + 1);
    }

    let total = ids.len();
    for (index, id) in ids.iter().enumerate() {
        let position = (total - index) as i64;
        sqlx::query(
            r#"
            UPDATE build_jobs
            SET queue_position = $2,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(position)
        .execute(&mut *tx)
        .await
        .context("Failed updating reordered build positions")?;
    }

    tx.commit()
        .await
        .context("Failed to commit queue reorder")?;
    Ok(())
}

/// Reorder the entire build queue given an ordered list of job UUIDs.
/// All queued jobs must be present in the list exactly once.
pub async fn reorder_build_queue(pool: &PgPool, ordered_job_ids: &[Uuid]) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("Failed to begin bulk reorder transaction")?;

    lock_build_queue_priority(&mut tx).await?;

    // Get current queued jobs
    let current_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT id
        FROM build_jobs
        WHERE status = 'queued'
        ORDER BY queue_position DESC NULLS LAST, priority_weight DESC, created_at ASC
        FOR UPDATE
        "#,
    )
    .fetch_all(&mut *tx)
    .await
    .context("Failed to load queued build jobs")?;

    // Validate that ordered list matches current queue
    if ordered_job_ids.len() != current_ids.len() {
        bail!(
            "Reorder list length mismatch: got {}, expected {}",
            ordered_job_ids.len(),
            current_ids.len()
        );
    }

    let mut ordered_set = std::collections::HashSet::new();
    for id in ordered_job_ids {
        if !ordered_set.insert(id) {
            bail!("Duplicate job ID in reorder list: {}", id);
        }
        if !current_ids.contains(id) {
            bail!("Job ID not in queue: {}", id);
        }
    }

    // Apply new queue positions — first in list = front (highest position with DESC sort)
    let total = ordered_job_ids.len();
    for (index, id) in ordered_job_ids.iter().enumerate() {
        let position = (total - index) as i64;
        sqlx::query(
            r#"
            UPDATE build_jobs
            SET queue_position = $2,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(position)
        .execute(&mut *tx)
        .await
        .context("Failed to update queue position")?;
    }

    tx.commit().await.context("Failed to commit bulk reorder")?;
    Ok(())
}

#[derive(Debug)]
pub struct BuildFailureTransition {
    pub failed_job: BuildJob,
    pub retry_job: Option<BuildJob>,
}

/// Terminally fail one attempt and atomically schedule at most one automatic child.
pub async fn mark_job_failed_with_retry(
    pool: &PgPool,
    job_id: &Uuid,
    builder_id: &Uuid,
    builder_session_id: Option<&Uuid>,
    error_message: Option<&str>,
    failure_class: RetryFailureClass,
) -> Result<BuildFailureTransition> {
    let mut tx = pool
        .begin()
        .await
        .context("Failed to begin fail transaction")?;

    let job = sqlx::query_as::<_, BuildJobRow>(
        r#"
        SELECT *
        FROM build_jobs
        WHERE id = $1
          AND builder_id = $2
          AND (builder_session_id IS NULL OR builder_session_id = $3)
          AND status = 'building'
        FOR UPDATE
        "#,
    )
    .bind(job_id)
    .bind(builder_id)
    .bind(builder_session_id)
    .fetch_optional(&mut *tx)
    .await
    .context("Failed to load owned building job for fail transition")?
    .ok_or_else(|| {
        anyhow::anyhow!("Build job not found or no longer owned by this builder in building status")
    })?;

    let policy = sqlx::query_as::<_, AutomaticRetryPolicy>(
        "SELECT max_build_retries, max_evaluation_retries, backoff_seconds, transient_only FROM automatic_retry_policy WHERE id = 1",
    )
    .fetch_optional(&mut *tx)
    .await
    .context("Failed to read retry policy during build failure")?
    .unwrap_or_default();

    let failed_job = sqlx::query_as::<_, BuildJobRow>(
        r#"
        UPDATE build_jobs
        SET status = 'failed',
            completed_at = NOW(),
            logs = CASE
                WHEN $2::text IS NULL THEN logs
                ELSE COALESCE(logs, '') || E'\n\nError: ' || $2
            END,
            updated_at = NOW()
        WHERE id = $1 AND status = 'building'
        RETURNING *
        "#,
    )
    .bind(job_id)
    .bind(error_message)
    .fetch_one(&mut *tx)
    .await
    .context("Failed to mark build attempt failed")?;

    let retry_job =
        if automatic_retry_budget_remaining(job.retry_count, i32::from(policy.max_build_retries))
            && automatic_retry_eligible(policy.transient_only, failure_class)
        {
            sqlx::query_as::<_, BuildJobRow>(
                r#"
            WITH queue_base AS (
                SELECT COALESCE(MAX(queue_position), 0) + 1 AS next_pos
                FROM build_jobs
                WHERE status = 'queued' OR status = 'building'
            )
            INSERT INTO build_jobs (
                derivation_id, environment_id, status, retry_count, max_retries,
                priority_weight, queue_position, parent_job_id, root_job_id,
                automatic_retry_source_id, attempt_number, available_at
            )
            SELECT
                $1, $2, 'queued', $3, $4, $5, queue_base.next_pos, $6, $7, $6, $8,
                NOW() + make_interval(secs => $9)
            FROM queue_base
            ON CONFLICT (automatic_retry_source_id)
                WHERE automatic_retry_source_id IS NOT NULL DO NOTHING
            RETURNING *
            "#,
            )
            .bind(job.derivation_id)
            .bind(job.environment_id)
            .bind(job.retry_count + 1)
            .bind(i32::from(policy.max_build_retries))
            .bind(job.priority_weight * 0.95)
            .bind(job.id)
            .bind(job.root_job_id.unwrap_or(job.id))
            .bind(job.attempt_number + 1)
            .bind(policy.backoff_seconds)
            .fetch_optional(&mut *tx)
            .await
            .context("Failed to schedule automatic build retry")?
        } else {
            None
        };

    tx.commit()
        .await
        .context("Failed to commit build failure")?;

    if retry_job.is_none() {
        let opened_at = failed_job.completed_at.unwrap_or_else(Utc::now);
        let _ = attention::open_or_observe(
            pool,
            "builds",
            "build_job",
            &job_id.to_string(),
            &attention::build_occurrence_key(*job_id),
            opened_at,
            serde_json::json!({"job_id": job_id.to_string()}),
        )
        .await
        .map_err(|e| tracing::error!("failed to open build attention occurrence: {e:#}"));
    }

    Ok(BuildFailureTransition {
        failed_job,
        retry_job,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Cancel / requeue lifecycle
// ─────────────────────────────────────────────────────────────────────────────

/// Return the raw status string of a build job, or `None` if the job does not
/// exist.  Used by the builder to poll for external cancellation.
pub async fn get_build_job_status(pool: &PgPool, job_id: &Uuid) -> Result<Option<String>> {
    let row = sqlx::query_scalar::<_, String>(r#"SELECT status FROM build_jobs WHERE id = $1"#)
        .bind(job_id)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch build job status")?;
    Ok(row)
}

/// Cancel a build job.
///
/// * Queued jobs → immediately `cancelled` (with `completed_at = now()`).
/// * Building jobs → `cancelling` (builder detects this on next heartbeat and
///   calls `finalize_cancelled_job` once the nix process has stopped).
///
/// Returns the updated `BuildJob`, or an error if the transition is illegal.
pub async fn cancel_build_job(pool: &PgPool, job_id: &Uuid) -> Result<BuildJob> {
    let job = get_build_job_by_id(pool, job_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Build job not found"))?;

    let (new_status, set_completed_at) = match job.status.as_str() {
        "queued" => ("cancelled", true),
        "building" => ("cancelling", false),
        "cancelling" => bail!("Build job is already being cancelled"),
        "cancelled" => bail!("Build job is already cancelled"),
        "success" => bail!("Cannot cancel a completed build"),
        "failed" => bail!("Cannot cancel a failed build"),
        other => bail!("Cannot cancel build in status: {}", other),
    };

    let updated = sqlx::query_as::<_, BuildJobRow>(
        r#"
        UPDATE build_jobs
        SET status       = $2,
            completed_at = CASE WHEN $3 THEN now() ELSE completed_at END,
            updated_at   = now()
        WHERE id = $1
        RETURNING *
        "#,
    )
    .bind(job_id)
    .bind(new_status)
    .bind(set_completed_at)
    .fetch_one(pool)
    .await
    .context("Failed to cancel build job")?;

    Ok(updated)
}

/// Force-cancel a build job stuck in the `cancelling` state.
///
/// This is a manual recovery operation for builds that:
/// - Have been stuck in `cancelling` for an extended period
/// - Failed to complete graceful shutdown (builder crashed/disconnected)
/// - Need immediate termination without waiting for builder confirmation
///
/// Transitions:
/// * `cancelling` → `cancelled` (sets completed_at = now())
/// * Already `cancelled` → returns error (idempotent behavior not desired for force operations)
///
/// Returns the updated `BuildJob`, or an error if the transition is illegal.
pub async fn force_cancel_build_job(pool: &PgPool, job_id: &Uuid) -> Result<BuildJob> {
    // Atomic transition guard: only force-cancel while state is still
    // `cancelling`.
    let updated = sqlx::query_as::<_, BuildJobRow>(
        r#"
        UPDATE build_jobs
        SET status       = 'cancelled',
            completed_at = now(),
            updated_at   = now()
        WHERE id = $1
          AND status = 'cancelling'
        RETURNING *
        "#,
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .context("Failed to force-cancel build job")?;

    if let Some(updated) = updated {
        info!("Force-cancelled job {} → 'cancelled'", job_id);
        return Ok(updated);
    }

    let current_status = get_build_job_status(pool, job_id).await?;
    match current_status.as_deref() {
        None => bail!("Build job not found"),
        Some("queued") => bail!("Cannot force-cancel a queued job; use regular cancel instead"),
        Some("building") => {
            bail!("Cannot force-cancel a building job; use regular cancel to enter stopping state")
        }
        Some("cancelled") => bail!("Build job is already cancelled"),
        Some("success") => bail!("Cannot force-cancel a completed build"),
        Some("failed") => bail!("Cannot force-cancel a failed build"),
        Some(status) => bail!(
            "Build is no longer force-cancellable (current status: {})",
            status
        ),
    }
}

/// Transition a job from `cancelling` → `cancelled` with a `completed_at`
/// timestamp.  Called by the builder after it has killed the nix process and
/// flushed any final logs.
///
/// Idempotent: if the job is already `cancelled` the update matches 0 rows and
/// we return the existing row unchanged rather than an error.
pub async fn finalize_cancelled_job(
    pool: &PgPool,
    job_id: &Uuid,
    builder_id: &Uuid,
    builder_session_id: Option<&Uuid>,
) -> Result<BuildJob> {
    let result = sqlx::query(
        r#"
        UPDATE build_jobs
        SET status       = 'cancelled',
            completed_at = now(),
            updated_at   = now()
        WHERE id = $1
          AND builder_id = $2
          AND (builder_session_id IS NULL OR builder_session_id = $3)
          AND status IN ('cancelling', 'cancelled')
        "#,
    )
    .bind(job_id)
    .bind(builder_id)
    .bind(builder_session_id)
    .execute(pool)
    .await
    .context("Failed to finalize cancelled job")?;

    if result.rows_affected() == 0 {
        bail!("Build job not found or no longer owned by this builder in cancellable state");
    }

    let job = get_build_job_by_id(pool, job_id).await?.ok_or_else(|| {
        anyhow::anyhow!("Build job disappeared after successful finalize-cancelled transition")
    })?;

    Ok(job)
}

/// Create a new queued build attempt from a terminal job.
///
/// This preserves immutable build history by keeping the original row intact
/// and inserting a brand-new `build_jobs` row for the same derivation/context.
///
/// Requeue-eligible terminal statuses: `cancelled`, `failed`, `success`.
/// New attempts are appended to the queue tail by assigning a weight lower than
/// the current minimum queued weight.
pub async fn requeue_build_job_as_new_attempt(pool: &PgPool, job_id: &Uuid) -> Result<BuildJob> {
    let inserted = sqlx::query_as::<_, BuildJobRow>(
        r#"
        WITH source_job AS (
            SELECT derivation_id, environment_id, id,
                   COALESCE(root_job_id, id) AS root_job_id, attempt_number
            FROM build_jobs
            WHERE id = $1
              AND status IN ('cancelled', 'failed', 'success')
        ), queue_pos AS (
            SELECT COALESCE(MAX(queue_position), 0) + 1 AS next_pos
            FROM build_jobs
            WHERE status = 'queued' OR status = 'building'
        )
        INSERT INTO build_jobs (
            derivation_id,
            environment_id,
            status,
            retry_count,
            max_retries,
            priority_weight,
            queue_position,
            parent_job_id,
            root_job_id,
            attempt_number,
            available_at
        )
        SELECT
            s.derivation_id,
            s.environment_id,
            'queued',
            0,
            COALESCE((SELECT max_build_retries FROM automatic_retry_policy WHERE id = 1), 2),
            1.0,
            queue_pos.next_pos,
            s.id,
            s.root_job_id,
            s.attempt_number + 1,
            NOW()
        FROM source_job s
        CROSS JOIN queue_pos
        RETURNING *
        "#,
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .context("Failed to requeue build job as new attempt")?
    .ok_or_else(|| {
        anyhow::anyhow!(
            "Build job not found or not in a requeue-eligible status (cancelled/failed/success)"
        )
    })?;

    Ok(inserted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::builders::BuilderStatus;
    use crate::test_utils::db::test_pool;
    use base64::Engine;
    use chrono::{Duration, Utc};
    use sqlx::postgres::PgPoolOptions;

    #[test]
    fn claim_next_job_queries_lock_only_build_jobs_rows() {
        for sql in [
            CLAIM_NEXT_JOB_SERVER_DERIVATION_WILDCARD_SQL,
            CLAIM_NEXT_JOB_SERVER_DERIVATION_FILTERED_SQL,
            CLAIM_NEXT_JOB_VERIFIED_SOURCE_WILDCARD_SQL,
            CLAIM_NEXT_JOB_VERIFIED_SOURCE_FILTERED_SQL,
        ] {
            assert!(
                sql.contains("FOR UPDATE OF build_jobs SKIP LOCKED"),
                "claim SQL must lock only build_jobs so nullable outer joins remain legal: {sql}"
            );
        }

        for sql in [
            CLAIM_NEXT_JOB_SERVER_DERIVATION_WILDCARD_SQL,
            CLAIM_NEXT_JOB_SERVER_DERIVATION_FILTERED_SQL,
        ] {
            assert!(
                !sql.contains("LEFT JOIN flakes"),
                "server_derivation claim SQL must not execute verified-source nullable joins: {sql}"
            );
        }

        for sql in [
            CLAIM_NEXT_JOB_VERIFIED_SOURCE_WILDCARD_SQL,
            CLAIM_NEXT_JOB_VERIFIED_SOURCE_FILTERED_SQL,
        ] {
            assert!(
                sql.contains("LEFT JOIN commits") && sql.contains("LEFT JOIN flakes"),
                "verified-source claim SQL must cover metadata outer joins: {sql}"
            );
        }
    }

    #[test]
    fn claim_next_job_atomic_locks_builder_session_before_claiming() {
        let source = include_str!("builders.rs");

        assert!(
            source.contains("SELECT current_session_id")
                && source.contains("FROM builders")
                && source.contains("FOR UPDATE"),
            "claim path must lock the builder row before queue mutation"
        );
        assert!(
            source.contains("builder_session_mismatch"),
            "claim path must reject superseded builder sessions"
        );
    }

    #[test]
    fn builder_startup_recovery_targets_only_same_builder_building_jobs() {
        let sql = REQUEUE_BUILDER_ASSIGNED_BUILDING_JOBS_SQL;

        assert!(
            sql.contains("WHERE bj.status = 'building'"),
            "startup recovery must only requeue in-flight jobs: {sql}"
        );
        assert!(
            sql.contains("AND bj.builder_id = $1"),
            "startup recovery must only requeue jobs assigned to the resolving builder: {sql}"
        );
        assert!(
            sql.contains("builder_id = NULL"),
            "startup recovery must clear stale builder ownership: {sql}"
        );
        assert!(
            sql.contains("builder_session_id = NULL"),
            "startup recovery must clear stale builder session ownership: {sql}"
        );
        assert!(
            sql.contains("AND bj.builder_session_id IS DISTINCT FROM $3"),
            "startup recovery must not requeue jobs owned by the current session: {sql}"
        );
        assert!(
            sql.contains("status = 'queued'"),
            "startup recovery must put stale jobs back on the queue: {sql}"
        );
        assert!(
            sql.contains("Recovery: re-queued from building by"),
            "startup recovery must append an auditable recovery log: {sql}"
        );
    }

    async fn queue_test_pool() -> PgPool {
        let database_url = std::env::var("CRYSTAL_FORGE_TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1/cf_test".to_string());

        PgPoolOptions::new()
            .connect_lazy(&database_url)
            .expect("lazy queue test pool should construct")
    }

    async fn create_active_test_builder(pool: &PgPool, name: &str) -> Builder {
        let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let verifying_key = signing_key.verifying_key();
        let public_key_base64 =
            base64::engine::general_purpose::STANDARD.encode(verifying_key.to_bytes());
        let unique_name = format!("{}-{}", name, Uuid::new_v4());

        let request = CreateBuilderRequest {
            name: unique_name.clone(),
            host: Some(format!("{}.test.local", unique_name)),
            arch: "x86_64-linux".to_string(),
            public_key: Some(public_key_base64),
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: Some(4),
            enabled: Some(true),
            environment_ids: vec![],
        };

        let (builder, _private_key) = create_builder(pool, &request)
            .await
            .expect("Failed to create test builder");

        sqlx::query("UPDATE builders SET status = 'active' WHERE id = $1")
            .bind(builder.id)
            .execute(pool)
            .await
            .expect("Failed to activate test builder");

        get_builder_by_id(pool, &builder.id)
            .await
            .expect("Failed to fetch test builder")
            .expect("Test builder not found")
    }

    async fn create_queued_job(
        pool: &PgPool,
        repo_url: &str,
        flake_name: &str,
        commit_hash: &str,
        commit_timestamp: chrono::DateTime<chrono::Utc>,
        derivation_name: &str,
        priority_weight: f64,
        created_at: chrono::DateTime<chrono::Utc>,
    ) -> Uuid {
        crate::queries::flakes::insert_flake(pool, flake_name, repo_url, "main", "all_configs")
            .await
            .expect("Failed to insert flake");

        crate::queries::commits::insert_commit_with_metadata(
            pool,
            commit_hash,
            repo_url,
            commit_timestamp,
            Some("test commit"),
            Some("test"),
        )
        .await
        .expect("Failed to insert commit");

        let commit = crate::queries::commits::get_commit_by_hash(pool, commit_hash)
            .await
            .expect("Failed to fetch commit");

        let derivation = crate::queries::derivations::insert_derivation_with_target(
            pool,
            Some(&commit),
            derivation_name,
            "nixos",
            Some("test-host"),
            Some(true),
        )
        .await
        .expect("Failed to insert derivation");

        let queue_position: i64 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(MAX(queue_position), 0) + 1
            FROM build_jobs
            WHERE status = 'queued' OR status = 'building'
            "#,
        )
        .fetch_one(pool)
        .await
        .expect("Failed to compute queue position");

        let build_job_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO build_jobs (derivation_id, status, priority_weight, queue_position, created_at)
            VALUES ($1, 'queued', $2, $3, $4)
            RETURNING id
            "#,
        )
        .bind(derivation.id)
        .bind(priority_weight)
        .bind(queue_position)
        .bind(created_at)
        .fetch_one(pool)
        .await
        .expect("Failed to insert queued build job");

        // Mark derivation as policy-passing so the claim-query gates
        // (cf_agent_enabled IS TRUE, policy_requirements_met IS TRUE)
        // accept this job.
        sqlx::query(
            "UPDATE derivations SET cf_agent_enabled = TRUE, policy_requirements_met = TRUE WHERE id = $1",
        )
        .bind(derivation.id)
        .execute(pool)
        .await
        .expect("Failed to set policy fields on test derivation");

        // Return the build job id (not the derivation id).
        build_job_id
    }

    async fn queued_order(pool: &PgPool) -> Vec<Uuid> {
        sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT id
            FROM build_jobs
            WHERE status = 'queued'
            ORDER BY queue_position DESC NULLS LAST, priority_weight DESC, created_at ASC
            "#,
        )
        .fetch_all(pool)
        .await
        .expect("Failed to fetch queued order")
    }

    async fn set_build_job_status(pool: &PgPool, job_id: Uuid, status: &str) {
        sqlx::query(
            r#"
            UPDATE build_jobs
            SET status = $2,
                updated_at = now(),
                started_at = CASE WHEN $2 IN ('building', 'cancelling') THEN COALESCE(started_at, now()) ELSE started_at END,
                completed_at = CASE WHEN $2 IN ('success', 'failed', 'cancelled') THEN now() ELSE NULL END
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .bind(status)
        .execute(pool)
        .await
        .expect("Failed to update test job status");
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_requeue_creates_new_attempt_and_preserves_original_for_terminal_statuses() {
        let pool = queue_test_pool().await;
        let now = Utc::now();

        for (idx, terminal_status) in ["cancelled", "failed", "success"].iter().enumerate() {
            let job_id = create_queued_job(
                &pool,
                &format!("https://example.com/task-290-requeue-{}.git", idx),
                &format!("task-290-requeue-{}", idx),
                &format!("task290requeue{:024}", idx),
                now - Duration::minutes(5),
                &format!("drv-task-290-requeue-{}", idx),
                5.0,
                now - Duration::minutes(4),
            )
            .await;

            set_build_job_status(&pool, job_id, terminal_status).await;

            let original = get_build_job_by_id(&pool, &job_id)
                .await
                .expect("fetch original job")
                .expect("original job exists");
            assert_eq!(original.status, *terminal_status);

            let requeued = requeue_build_job_as_new_attempt(&pool, &job_id)
                .await
                .expect("requeue should create new attempt");

            assert_ne!(requeued.id, original.id, "new attempt must have new id");
            assert_eq!(requeued.status, "queued");
            assert_eq!(requeued.derivation_id, original.derivation_id);
            assert_eq!(requeued.environment_id, original.environment_id);
            assert_eq!(requeued.retry_count, 0);

            // Original attempt remains immutable.
            let original_after = get_build_job_by_id(&pool, &job_id)
                .await
                .expect("fetch original job after requeue")
                .expect("original job still exists");
            assert_eq!(original_after.status, *terminal_status);
        }
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_requeue_when_queue_is_empty_assigns_positive_priority_weight() {
        let pool = queue_test_pool().await;
        let now = Utc::now();

        let job_id = create_queued_job(
            &pool,
            "https://example.com/task-290-requeue-empty.git",
            "task-290-requeue-empty",
            "task290requeueempty000000000000",
            now - Duration::minutes(5),
            "drv-task-290-requeue-empty",
            1.0,
            now - Duration::minutes(4),
        )
        .await;

        // Make queue empty by taking source job out of queued state.
        set_build_job_status(&pool, job_id, "success").await;

        let requeued = requeue_build_job_as_new_attempt(&pool, &job_id)
            .await
            .expect("requeue should succeed on empty queue");

        assert_eq!(requeued.status, "queued");
        assert!(
            requeued.priority_weight > 0.0,
            "priority_weight must satisfy DB CHECK (priority_weight > 0)"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires test database creation privileges"]
    async fn automatic_retry_creates_one_delayed_linked_child_and_keeps_source_terminal(
        pool: PgPool,
    ) {
        let now = Utc::now();
        let job_id = create_queued_job(
            &pool,
            &format!("https://example.com/retry-{}.git", Uuid::new_v4()),
            &format!("retry-{}", Uuid::new_v4()),
            &Uuid::new_v4().simple().to_string(),
            now,
            "retry-system",
            5.0,
            now,
        )
        .await;
        let builder = create_active_test_builder(&pool, "automatic-retry-builder").await;

        sqlx::query(
            "UPDATE build_jobs SET status = 'building', builder_id = $2, started_at = NOW() WHERE id = $1",
        )
        .bind(job_id)
        .bind(builder.id)
        .execute(&pool)
        .await
        .expect("assign source attempt");
        sqlx::query(
            "UPDATE automatic_retry_policy SET max_build_retries = 2, backoff_seconds = 30, transient_only = TRUE WHERE id = 1",
        )
        .execute(&pool)
        .await
        .expect("set policy governing the observed failure");

        let transition = mark_job_failed_with_retry(
            &pool,
            &job_id,
            &builder.id,
            None,
            Some("temporary source timeout"),
            RetryFailureClass::Transient,
        )
        .await
        .expect("fail transition should schedule retry");

        let child = transition.retry_job.expect("retry child should exist");
        assert_eq!(transition.failed_job.status, "failed");
        assert!(transition.failed_job.completed_at.is_some());
        assert_eq!(child.parent_job_id, Some(job_id));
        assert_eq!(child.root_job_id, Some(job_id));
        assert_eq!(child.attempt_number, 2);
        assert_eq!(child.retry_count, 1);
        assert!(child.available_at >= now + Duration::seconds(29));

        let duplicate = mark_job_failed_with_retry(
            &pool,
            &job_id,
            &builder.id,
            None,
            Some("duplicate event"),
            RetryFailureClass::Transient,
        )
        .await;
        assert!(duplicate.is_err());
        let child_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM build_jobs WHERE automatic_retry_source_id = $1",
        )
        .bind(job_id)
        .fetch_one(&pool)
        .await
        .expect("count retry children");
        assert_eq!(child_count, 1);
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_dashboard_queue_matches_next_claim_order_for_queued_items() {
        let pool = queue_test_pool().await;
        let now = Utc::now();

        let _job_low_priority = create_queued_job(
            &pool,
            "https://example.com/order-a.git",
            "order-a",
            "a0000001",
            now - Duration::minutes(10),
            "order-a-system",
            5.0,
            now - Duration::minutes(5),
        )
        .await;

        let expected_first = create_queued_job(
            &pool,
            "https://example.com/order-b.git",
            "order-b",
            "b0000001",
            now,
            "order-b-system",
            10.0,
            now - Duration::minutes(3),
        )
        .await;

        let _same_priority_older_commit = create_queued_job(
            &pool,
            "https://example.com/order-c.git",
            "order-c",
            "c0000001",
            now - Duration::minutes(30),
            "order-c-system",
            10.0,
            now - Duration::minutes(1),
        )
        .await;

        let queue = crate::queries::dashboard::fetch_build_queue(&pool, 50)
            .await
            .expect("Failed to fetch dashboard queue");
        let first_in_queue = queue
            .items
            .iter()
            .find_map(|item| item.job_id)
            .expect("Expected queued jobs in dashboard queue");

        assert_eq!(first_in_queue, expected_first);

        let builder = create_active_test_builder(&pool, "order-match-builder").await;
        let claimed = claim_next_job_atomic(
            &pool,
            &builder.id,
            builder.max_concurrent_jobs,
            &[],
            RemoteBuildExecutionStrategy::ServerDerivation,
            None,
        )
        .await
        .expect("Failed to claim next job")
        .expect("Expected a queued job to be claimed");

        assert_eq!(claimed.id, first_in_queue);
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_prioritize_updates_dashboard_order_and_claim_order() {
        let pool = queue_test_pool().await;
        let now = Utc::now();

        let first = create_queued_job(
            &pool,
            "https://example.com/prio-a.git",
            "prio-a",
            "d0000001",
            now,
            "prio-a-system",
            10.0,
            now - Duration::minutes(5),
        )
        .await;

        let second = create_queued_job(
            &pool,
            "https://example.com/prio-b.git",
            "prio-b",
            "e0000001",
            now,
            "prio-b-system",
            10.0,
            now - Duration::minutes(1),
        )
        .await;

        let before = crate::queries::dashboard::fetch_build_queue(&pool, 50)
            .await
            .expect("Failed to fetch queue before prioritize");
        let first_before = before
            .items
            .iter()
            .find_map(|item| item.job_id)
            .expect("Expected queued jobs before prioritize");
        assert_eq!(first_before, first);

        prioritize_build_job(&pool, &second)
            .await
            .expect("Failed to prioritize second job");

        let after = crate::queries::dashboard::fetch_build_queue(&pool, 50)
            .await
            .expect("Failed to fetch queue after prioritize");
        let first_after = after
            .items
            .iter()
            .find_map(|item| item.job_id)
            .expect("Expected queued jobs after prioritize");
        assert_eq!(first_after, second);

        let builder = create_active_test_builder(&pool, "prioritize-order-builder").await;
        let claimed = claim_next_job_atomic(
            &pool,
            &builder.id,
            builder.max_concurrent_jobs,
            &[],
            RemoteBuildExecutionStrategy::ServerDerivation,
            None,
        )
        .await
        .expect("Failed to claim after prioritize")
        .expect("Expected a queued job after prioritize");
        assert_eq!(claimed.id, second);
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_move_up_and_down_persist_order_after_reload() {
        let pool = queue_test_pool().await;
        let now = Utc::now();

        let first = create_queued_job(
            &pool,
            "https://example.com/reorder-a.git",
            "reorder-a",
            "r0000001",
            now,
            "reorder-a-system",
            30.0,
            now - Duration::minutes(3),
        )
        .await;
        let second = create_queued_job(
            &pool,
            "https://example.com/reorder-b.git",
            "reorder-b",
            "r0000002",
            now,
            "reorder-b-system",
            20.0,
            now - Duration::minutes(2),
        )
        .await;
        let third = create_queued_job(
            &pool,
            "https://example.com/reorder-c.git",
            "reorder-c",
            "r0000003",
            now,
            "reorder-c-system",
            10.0,
            now - Duration::minutes(1),
        )
        .await;

        assert_eq!(queued_order(&pool).await, vec![first, second, third]);

        move_build_job_down(&pool, &first)
            .await
            .expect("move down should succeed");
        assert_eq!(queued_order(&pool).await, vec![second, first, third]);

        move_build_job_up(&pool, &third)
            .await
            .expect("move up should succeed");
        assert_eq!(queued_order(&pool).await, vec![second, third, first]);
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_move_up_down_first_last_are_noops() {
        let pool = queue_test_pool().await;
        let now = Utc::now();

        let first = create_queued_job(
            &pool,
            "https://example.com/noop-a.git",
            "noop-a",
            "n0000001",
            now,
            "noop-a-system",
            20.0,
            now - Duration::minutes(2),
        )
        .await;
        let second = create_queued_job(
            &pool,
            "https://example.com/noop-b.git",
            "noop-b",
            "n0000002",
            now,
            "noop-b-system",
            10.0,
            now - Duration::minutes(1),
        )
        .await;

        move_build_job_up(&pool, &first)
            .await
            .expect("move up first should no-op");
        move_build_job_down(&pool, &second)
            .await
            .expect("move down last should no-op");

        assert_eq!(queued_order(&pool).await, vec![first, second]);
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_move_rejects_unknown_or_non_queued_job() {
        let pool = queue_test_pool().await;
        let now = Utc::now();

        let queued = create_queued_job(
            &pool,
            "https://example.com/reject-a.git",
            "reject-a",
            "x0000001",
            now,
            "reject-a-system",
            10.0,
            now - Duration::minutes(1),
        )
        .await;
        set_build_job_status(&pool, queued, "building").await;

        let err = move_build_job_up(&pool, &queued)
            .await
            .expect_err("building job should be rejected");
        assert!(err.to_string().contains("Queued build job not found"));

        let unknown = Uuid::new_v4();
        let err = move_build_job_down(&pool, &unknown)
            .await
            .expect_err("unknown job should be rejected");
        assert!(err.to_string().contains("Queued build job not found"));
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_concurrent_claims_take_top_two_without_duplicates() {
        let pool = queue_test_pool().await;
        let now = Utc::now();

        let expected_first = create_queued_job(
            &pool,
            "https://example.com/conc-a.git",
            "conc-a",
            "f0000001",
            now,
            "conc-a-system",
            30.0,
            now - Duration::minutes(3),
        )
        .await;

        let expected_second = create_queued_job(
            &pool,
            "https://example.com/conc-b.git",
            "conc-b",
            "f0000002",
            now,
            "conc-b-system",
            20.0,
            now - Duration::minutes(2),
        )
        .await;

        let _remaining = create_queued_job(
            &pool,
            "https://example.com/conc-c.git",
            "conc-c",
            "f0000003",
            now,
            "conc-c-system",
            10.0,
            now - Duration::minutes(1),
        )
        .await;

        let builder_a = create_active_test_builder(&pool, "concurrent-builder-a").await;
        let builder_b = create_active_test_builder(&pool, "concurrent-builder-b").await;

        let (claimed_a, claimed_b) = tokio::join!(
            claim_next_job_atomic(
                &pool,
                &builder_a.id,
                builder_a.max_concurrent_jobs,
                &[],
                RemoteBuildExecutionStrategy::ServerDerivation,
                None,
            ),
            claim_next_job_atomic(
                &pool,
                &builder_b.id,
                builder_b.max_concurrent_jobs,
                &[],
                RemoteBuildExecutionStrategy::ServerDerivation,
                None,
            )
        );

        let claimed_a = claimed_a
            .expect("claim A failed")
            .expect("claim A expected a job");
        let claimed_b = claimed_b
            .expect("claim B failed")
            .expect("claim B expected a job");

        assert_ne!(
            claimed_a.id, claimed_b.id,
            "Concurrent claims must not duplicate jobs"
        );

        let claimed_ids: std::collections::HashSet<Uuid> =
            [claimed_a.id, claimed_b.id].into_iter().collect();
        let expected_ids: std::collections::HashSet<Uuid> =
            [expected_first, expected_second].into_iter().collect();

        assert_eq!(claimed_ids, expected_ids);
    }

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
            host: Some("test-builder.test.local".to_string()),
            arch: "x86_64-linux".to_string(),
            public_key: Some(public_key_base64),
            max_cpu_cores: Some(4),
            max_memory_mb: Some(8192),
            max_concurrent_jobs: Some(2),
            enabled: Some(true),
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
    async fn test_get_builder_by_public_key_resolves_registered_builder() {
        let pool = test_pool().await;

        let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let public_key_base64 = base64::engine::general_purpose::STANDARD
            .encode(signing_key.verifying_key().to_bytes());

        let request = CreateBuilderRequest {
            name: "public-key-lookup-builder".to_string(),
            host: Some("public-key-lookup-builder.test.local".to_string()),
            arch: "x86_64-linux".to_string(),
            public_key: Some(public_key_base64.clone()),
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: None,
            enabled: Some(true),
            environment_ids: vec![],
        };

        let (builder, _private_key) = create_builder(&pool, &request)
            .await
            .expect("Failed to create builder");

        let public_key = PublicKey::from_base64(&public_key_base64, "builder")
            .expect("generated public key should parse");
        let fetched = get_builder_by_public_key(&pool, &public_key)
            .await
            .expect("Failed to fetch builder by public key")
            .expect("Builder should resolve by registered public key");

        assert_eq!(fetched.id, builder.id);

        let unregistered_key = base64::engine::general_purpose::STANDARD.encode(
            ed25519_dalek::SigningKey::generate(&mut rand::thread_rng())
                .verifying_key()
                .to_bytes(),
        );
        let unregistered_key = PublicKey::from_base64(&unregistered_key, "builder")
            .expect("generated public key should parse");
        let missing = get_builder_by_public_key(&pool, &unregistered_key)
            .await
            .expect("Failed to query unregistered public key");

        assert!(missing.is_none());
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
            host: Some("heartbeat-test.test.local".to_string()),
            arch: "x86_64-linux".to_string(),
            public_key: Some(public_key_base64),
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: None,
            enabled: Some(true),
            environment_ids: vec![],
        };

        let (builder, _private_key) = create_builder(&pool, &request)
            .await
            .expect("Failed to create builder");

        assert_eq!(builder.status, BuilderStatus::Inactive);
        assert!(builder.last_heartbeat_at.is_none());

        // Update heartbeat
        update_builder_heartbeat(&pool, &builder.id, None)
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
            host: Some("invalid-key-builder.test.local".to_string()),
            arch: "x86_64-linux".to_string(),
            public_key: Some("not-valid-base64!!!".to_string()),
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: None,
            enabled: Some(true),
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
            host: Some("wrong-length-builder.test.local".to_string()),
            arch: "x86_64-linux".to_string(),
            public_key: Some(wrong_length_key),
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: None,
            enabled: Some(true),
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
            host: Some("test.local".to_string()),
            arch: "x86_64-linux".to_string(),
            public_key: Some("".to_string()),
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: None,
            enabled: None,
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

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_update_public_key_response_contains_fingerprint_of_new_key() {
        let pool = test_pool().await;

        // Create builder with an initial keypair
        let initial_key = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let initial_pub_b64 = base64::engine::general_purpose::STANDARD
            .encode(initial_key.verifying_key().to_bytes());

        let request = CreateBuilderRequest {
            name: "fingerprint-rotation-test".to_string(),
            host: Some("fingerprint-rotation.test.local".to_string()),
            arch: "x86_64-linux".to_string(),
            public_key: Some(initial_pub_b64),
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: None,
            enabled: Some(true),
            environment_ids: vec![],
        };

        let (builder, _) = create_builder(&pool, &request)
            .await
            .expect("Failed to create builder");
        assert!(
            !builder.public_key_fingerprint.is_empty(),
            "create fingerprint must be set"
        );

        // Rotate to a new keypair
        let new_key = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let new_pub_b64 =
            base64::engine::general_purpose::STANDARD.encode(new_key.verifying_key().to_bytes());

        let updated = update_builder_public_key(&pool, &builder.id, &new_pub_b64, &builder.name)
            .await
            .expect("Failed to rotate public key");

        assert!(
            !updated.public_key_fingerprint.is_empty(),
            "rotation response fingerprint must be set"
        );
        assert_ne!(
            builder.public_key_fingerprint, updated.public_key_fingerprint,
            "fingerprint must change after key rotation"
        );

        // Verify GET returns the same fingerprint as the mutation response
        let fetched = get_builder_by_id(&pool, &builder.id)
            .await
            .expect("Failed to fetch builder")
            .expect("Builder not found");

        assert_eq!(
            fetched.public_key_fingerprint, updated.public_key_fingerprint,
            "GET fingerprint must match key-rotation response fingerprint"
        );
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_force_cancel_transition_from_cancelling() {
        let pool = test_pool().await;
        let now = Utc::now();

        let job_id = create_queued_job(
            &pool,
            "https://example.com/force-cancel-cancelling.git",
            "force-cancel-cancelling",
            "fcancelcancelling000000000000000000000001",
            now,
            "drv-force-cancel-cancelling",
            1.0,
            now,
        )
        .await;

        set_build_job_status(&pool, job_id, "cancelling").await;

        let updated = force_cancel_build_job(&pool, &job_id)
            .await
            .expect("force-cancel from cancelling should succeed");

        assert_eq!(updated.status, "cancelled");
        assert!(updated.completed_at.is_some());
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_force_cancel_rejects_building_status() {
        let pool = test_pool().await;
        let now = Utc::now();

        let job_id = create_queued_job(
            &pool,
            "https://example.com/force-cancel-building.git",
            "force-cancel-building",
            "fcancelbuilding000000000000000000000001",
            now,
            "drv-force-cancel-building",
            1.0,
            now,
        )
        .await;

        set_build_job_status(&pool, job_id, "building").await;

        let err = force_cancel_build_job(&pool, &job_id)
            .await
            .expect_err("force-cancel should reject building status");

        assert!(
            err.to_string()
                .contains("Cannot force-cancel a building job; use regular cancel")
        );

        let status_after = get_build_job_status(&pool, &job_id)
            .await
            .expect("status lookup should succeed")
            .expect("job should exist");
        assert_eq!(status_after, "building");
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_force_cancel_rejects_terminal_status() {
        let pool = test_pool().await;
        let now = Utc::now();

        let job_id = create_queued_job(
            &pool,
            "https://example.com/force-cancel-terminal.git",
            "force-cancel-terminal",
            "fcancelterminal000000000000000000000001",
            now,
            "drv-force-cancel-terminal",
            1.0,
            now,
        )
        .await;

        set_build_job_status(&pool, job_id, "failed").await;

        let err = force_cancel_build_job(&pool, &job_id)
            .await
            .expect_err("force-cancel should fail for terminal status");

        assert!(
            err.to_string()
                .contains("Cannot force-cancel a failed build")
        );
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_force_cancel_race_safe_does_not_overwrite_terminal_status() {
        let pool = test_pool().await;
        let now = Utc::now();

        let job_id = create_queued_job(
            &pool,
            "https://example.com/force-cancel-race-safe.git",
            "force-cancel-race-safe",
            "fcancelracesafe000000000000000000000001",
            now,
            "drv-force-cancel-race-safe",
            1.0,
            now,
        )
        .await;

        // Simulate another worker finishing the job before force-cancel applies.
        set_build_job_status(&pool, job_id, "success").await;

        let err = force_cancel_build_job(&pool, &job_id)
            .await
            .expect_err("force-cancel should fail when job is no longer cancellable");
        assert!(
            err.to_string()
                .contains("Cannot force-cancel a completed build")
        );

        let final_status = get_build_job_status(&pool, &job_id)
            .await
            .expect("status lookup should succeed")
            .expect("job should still exist");
        assert_eq!(final_status, "success");
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_requeue_orphaned_building_jobs_keeps_active_builder_jobs() {
        let pool = queue_test_pool().await;
        let now = Utc::now();

        let active_builder = create_active_test_builder(&pool, "requeue-active-builder").await;
        let stale_builder = create_active_test_builder(&pool, "requeue-stale-builder").await;

        sqlx::query("UPDATE builders SET status = 'offline' WHERE id = $1")
            .bind(stale_builder.id)
            .execute(&pool)
            .await
            .expect("Failed to mark stale builder offline");

        let active_job = create_queued_job(
            &pool,
            "https://example.com/requeue-active.git",
            "requeue-active",
            "requeueactive000000000000000000000001",
            now,
            "drv-requeue-active",
            1.0,
            now,
        )
        .await;
        set_build_job_status(&pool, active_job, "building").await;
        sqlx::query("UPDATE build_jobs SET builder_id = $2 WHERE id = $1")
            .bind(active_job)
            .bind(active_builder.id)
            .execute(&pool)
            .await
            .expect("Failed to assign active job to active builder");

        let orphan_job = create_queued_job(
            &pool,
            "https://example.com/requeue-orphan.git",
            "requeue-orphan",
            "requeueorphan000000000000000000000001",
            now,
            "drv-requeue-orphan",
            1.0,
            now,
        )
        .await;
        set_build_job_status(&pool, orphan_job, "building").await;
        sqlx::query("UPDATE build_jobs SET builder_id = NULL WHERE id = $1")
            .bind(orphan_job)
            .execute(&pool)
            .await
            .expect("Failed to null builder assignment for orphan job");

        let stale_job = create_queued_job(
            &pool,
            "https://example.com/requeue-stale.git",
            "requeue-stale",
            "requeuestale000000000000000000000001",
            now,
            "drv-requeue-stale",
            1.0,
            now,
        )
        .await;
        set_build_job_status(&pool, stale_job, "building").await;
        sqlx::query("UPDATE build_jobs SET builder_id = $2 WHERE id = $1")
            .bind(stale_job)
            .bind(stale_builder.id)
            .execute(&pool)
            .await
            .expect("Failed to assign stale job to stale builder");

        let recovered =
            requeue_orphaned_building_jobs_with_reason(&pool, "startup builder recovery")
                .await
                .expect("Failed to recover orphaned jobs");

        let recovered_ids: std::collections::HashSet<Uuid> =
            recovered.iter().map(|job| job.id).collect();
        assert!(recovered_ids.contains(&orphan_job));
        assert!(recovered_ids.contains(&stale_job));
        assert!(!recovered_ids.contains(&active_job));

        let active_status = get_build_job_status(&pool, &active_job)
            .await
            .expect("active job status lookup should succeed")
            .expect("active job should exist");
        assert_eq!(active_status, "building");

        let orphan_status = get_build_job_status(&pool, &orphan_job)
            .await
            .expect("orphan job status lookup should succeed")
            .expect("orphan job should exist");
        assert_eq!(orphan_status, "queued");

        let orphan_row = get_build_job_by_id(&pool, &orphan_job)
            .await
            .expect("orphan job fetch should succeed")
            .expect("orphan job should exist");
        assert!(
            orphan_row
                .logs
                .as_deref()
                .unwrap_or_default()
                .contains("startup builder recovery"),
            "recovered job logs should include recovery reason"
        );

        let stale_status = get_build_job_status(&pool, &stale_job)
            .await
            .expect("stale job status lookup should succeed")
            .expect("stale job should exist");
        assert_eq!(stale_status, "queued");
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_requeue_builder_assigned_building_jobs_recovers_restart_stale_job() {
        let pool = queue_test_pool().await;
        let now = Utc::now();
        let old_session = Uuid::new_v4();
        let new_session = Uuid::new_v4();
        let other_session = Uuid::new_v4();

        let restarting_builder =
            create_active_test_builder(&pool, "restart-recovery-builder").await;
        let other_builder = create_active_test_builder(&pool, "restart-other-builder").await;

        sqlx::query(
            r#"
            UPDATE builders
            SET current_session_id = $2,
                current_session_started_at = now() - interval '10 minutes',
                last_heartbeat_at = now() - interval '10 minutes',
                status = 'active'
            WHERE id = $1
            "#,
        )
        .bind(restarting_builder.id)
        .bind(old_session)
        .execute(&pool)
        .await
        .expect("old restarting builder session should be recorded as stale");

        sqlx::query(
            r#"
            UPDATE builders
            SET current_session_id = $2,
                current_session_started_at = now(),
                last_heartbeat_at = now(),
                status = 'active'
            WHERE id = $1
            "#,
        )
        .bind(other_builder.id)
        .bind(other_session)
        .execute(&pool)
        .await
        .expect("other builder session should remain active");

        let stale_job = create_queued_job(
            &pool,
            "https://example.com/restart-stale.git",
            "restart-stale",
            &format!("restartstale{}", Uuid::new_v4().simple()),
            now,
            "drv-restart-stale",
            1.0,
            now,
        )
        .await;
        sqlx::query(
            "UPDATE build_jobs SET status = 'building', builder_id = $2, builder_session_id = $3 WHERE id = $1",
        )
        .bind(stale_job)
        .bind(restarting_builder.id)
        .bind(old_session)
        .execute(&pool)
        .await
        .expect("stale job should be assigned to old restarting session");

        let other_job = create_queued_job(
            &pool,
            "https://example.com/restart-other.git",
            "restart-other",
            &format!("restartother{}", Uuid::new_v4().simple()),
            now,
            "drv-restart-other",
            1.0,
            now,
        )
        .await;
        sqlx::query(
            "UPDATE build_jobs SET status = 'building', builder_id = $2, builder_session_id = $3 WHERE id = $1",
        )
        .bind(other_job)
        .bind(other_builder.id)
        .bind(other_session)
        .execute(&pool)
        .await
        .expect("other job should remain assigned to another active builder session");

        let recovered = establish_builder_session(
            &pool,
            &restarting_builder.id,
            &new_session,
            60,
            "builder startup recovery",
        )
        .await
        .expect("startup recovery should succeed");

        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].id, stale_job);

        let stale_row = get_build_job_by_id(&pool, &stale_job)
            .await
            .expect("stale job fetch should succeed")
            .expect("stale job should exist");
        assert_eq!(stale_row.status, "queued");
        assert_eq!(stale_row.builder_id, None);
        assert_eq!(stale_row.builder_session_id, None);
        assert!(
            stale_row
                .logs
                .as_deref()
                .unwrap_or_default()
                .contains("builder startup recovery"),
            "startup recovery should append an auditable log"
        );

        let other_row = get_build_job_by_id(&pool, &other_job)
            .await
            .expect("other job fetch should succeed")
            .expect("other job should exist");
        assert_eq!(other_row.status, "building");
        assert_eq!(other_row.builder_id, Some(other_builder.id));
        assert_eq!(other_row.builder_session_id, Some(other_session));
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_establish_builder_session_rejects_overlap_with_fresh_session() {
        let pool = queue_test_pool().await;
        let active_session = Uuid::new_v4();
        let overlapping_session = Uuid::new_v4();
        let builder = create_active_test_builder(&pool, "fresh-overlap-builder").await;

        sqlx::query(
            r#"
            UPDATE builders
            SET current_session_id = $2,
                current_session_started_at = now(),
                last_heartbeat_at = now(),
                status = 'active'
            WHERE id = $1
            "#,
        )
        .bind(builder.id)
        .bind(active_session)
        .execute(&pool)
        .await
        .expect("fresh active builder session should be recorded");

        let result = establish_builder_session(
            &pool,
            &builder.id,
            &overlapping_session,
            60,
            "builder startup recovery",
        )
        .await;

        assert!(
            result
                .expect_err("overlapping fresh session should be rejected")
                .to_string()
                .contains("active_builder_session_exists"),
            "fresh active session should prevent unsafe recovery"
        );
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_superseded_builder_session_cannot_claim_new_jobs() {
        let pool = queue_test_pool().await;
        let now = Utc::now();
        let builder = create_active_test_builder(&pool, "superseded-session-builder").await;
        let session_a = Uuid::new_v4();
        let session_b = Uuid::new_v4();

        establish_builder_session(&pool, &builder.id, &session_a, 60, "session A startup")
            .await
            .expect("session A should establish");

        sqlx::query(
            r#"
            UPDATE builders
            SET last_heartbeat_at = now() - interval '10 minutes',
                current_session_started_at = now() - interval '10 minutes'
            WHERE id = $1
            "#,
        )
        .bind(builder.id)
        .execute(&pool)
        .await
        .expect("session A should be made stale");

        let queued_job = create_queued_job(
            &pool,
            "https://example.com/superseded-session.git",
            "superseded-session",
            &format!("superseded{}", Uuid::new_v4().simple()),
            now,
            "drv-superseded-session",
            10.0,
            now,
        )
        .await;

        establish_builder_session(&pool, &builder.id, &session_b, 60, "session B startup")
            .await
            .expect("session B should take over stale session A");

        let superseded_claim = claim_next_job_atomic(
            &pool,
            &builder.id,
            builder.max_concurrent_jobs,
            &[],
            RemoteBuildExecutionStrategy::ServerDerivation,
            Some(&session_a),
        )
        .await;

        assert!(
            superseded_claim
                .expect_err("superseded session A must be rejected")
                .to_string()
                .contains("builder_session_mismatch"),
            "obsolete sessions must not claim after takeover"
        );

        let unchanged = get_build_job_by_id(&pool, &queued_job)
            .await
            .expect("queued job fetch should succeed")
            .expect("queued job should exist");
        assert_eq!(unchanged.status, "queued");
        assert_eq!(unchanged.builder_id, None);
        assert_eq!(unchanged.builder_session_id, None);

        let claimed_by_current = claim_next_job_atomic(
            &pool,
            &builder.id,
            builder.max_concurrent_jobs,
            &[],
            RemoteBuildExecutionStrategy::ServerDerivation,
            Some(&session_b),
        )
        .await
        .expect("current session B claim should succeed")
        .expect("current session B should receive the queued job");

        assert_eq!(claimed_by_current.id, queued_job);
        assert_eq!(claimed_by_current.builder_id, Some(builder.id));
        assert_eq!(claimed_by_current.builder_session_id, Some(session_b));
        assert_eq!(claimed_by_current.status, "building");
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_sessionless_claim_rejected_when_session_established() {
        let pool = queue_test_pool().await;
        let now = Utc::now();
        let builder = create_active_test_builder(&pool, "sessionless-vs-established-builder").await;
        let session = Uuid::new_v4();

        // Verify builder starts sessionless (current_session_id = NULL)
        let fresh = get_builder_by_id(&pool, &builder.id)
            .await
            .expect("fresh builder fetch should succeed")
            .expect("fresh builder should exist");
        assert_eq!(
            fresh.current_session_id, None,
            "test builder must start without a session"
        );

        // Establish a session
        establish_builder_session(&pool, &builder.id, &session, 60, "established session")
            .await
            .expect("session should establish");

        let queued_job = create_queued_job(
            &pool,
            "https://example.com/sessionless-vs-established.git",
            "sessionless-vs-established",
            &format!("se-{}", Uuid::new_v4().simple()),
            now,
            "drv-sessionless-vs-established",
            10.0,
            now,
        )
        .await;

        // Sessionless claim (None) after session is established → must be rejected
        let sessionless_claim = claim_next_job_atomic(
            &pool,
            &builder.id,
            builder.max_concurrent_jobs,
            &[],
            RemoteBuildExecutionStrategy::ServerDerivation,
            None,
        )
        .await;

        assert!(
            sessionless_claim
                .expect_err("sessionless claim after session establishment must be rejected")
                .to_string()
                .contains("builder_session_mismatch"),
            "legacy sessionless claim must not bypass established session guard"
        );

        // Job must still be queued and unclaimed
        let unchanged = get_build_job_by_id(&pool, &queued_job)
            .await
            .expect("queued job fetch should succeed")
            .expect("queued job should exist");
        assert_eq!(unchanged.status, "queued");
        assert_eq!(unchanged.builder_id, None);
        assert_eq!(unchanged.builder_session_id, None);

        // Established session must still be able to claim the job
        let claimed = claim_next_job_atomic(
            &pool,
            &builder.id,
            builder.max_concurrent_jobs,
            &[],
            RemoteBuildExecutionStrategy::ServerDerivation,
            Some(&session),
        )
        .await
        .expect("established session claim should succeed")
        .expect("established session should receive the queued job");

        assert_eq!(claimed.id, queued_job);
        assert_eq!(claimed.builder_id, Some(builder.id));
        assert_eq!(claimed.builder_session_id, Some(session));
        assert_eq!(claimed.status, "building");
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_mark_stale_builders_offline_then_requeue_building_jobs() {
        let pool = queue_test_pool().await;
        let now = Utc::now();

        let builder = create_active_test_builder(&pool, "stale-offline-requeue-builder").await;

        sqlx::query(
            "UPDATE builders SET last_heartbeat_at = now() - interval '10 minutes' WHERE id = $1",
        )
        .bind(builder.id)
        .execute(&pool)
        .await
        .expect("Failed to backdate builder heartbeat");

        let job_id = create_queued_job(
            &pool,
            "https://example.com/requeue-runtime.git",
            "requeue-runtime",
            "requeueruntime0000000000000000000001",
            now,
            "drv-requeue-runtime",
            1.0,
            now,
        )
        .await;
        set_build_job_status(&pool, job_id, "building").await;
        sqlx::query("UPDATE build_jobs SET builder_id = $2 WHERE id = $1")
            .bind(job_id)
            .bind(builder.id)
            .execute(&pool)
            .await
            .expect("Failed to assign runtime test job to builder");

        let marked = mark_stale_builders_offline(&pool, 60)
            .await
            .expect("Failed to mark stale builders offline");
        assert_eq!(marked, 1, "expected one stale builder to be marked offline");

        let recovered =
            requeue_orphaned_building_jobs_with_reason(&pool, "runtime builder liveness recovery")
                .await
                .expect("Failed to requeue stale-builder jobs");
        assert_eq!(recovered.len(), 1, "expected one recovered job");
        assert_eq!(recovered[0].id, job_id);

        let status = get_build_job_status(&pool, &job_id)
            .await
            .expect("status lookup should succeed")
            .expect("job should exist");
        assert_eq!(status, "queued");

        let row = get_build_job_by_id(&pool, &job_id)
            .await
            .expect("job fetch should succeed")
            .expect("job should exist");
        assert!(
            row.logs
                .as_deref()
                .unwrap_or_default()
                .contains("runtime builder liveness recovery"),
            "runtime recovery should record an explicit reason in job logs"
        );
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_requeue_orphaned_building_jobs_treats_disabled_active_builder_as_orphaned() {
        let pool = queue_test_pool().await;
        let now = Utc::now();

        let builder = create_active_test_builder(&pool, "disabled-active-requeue-builder").await;
        sqlx::query("UPDATE builders SET enabled = false, status = 'active' WHERE id = $1")
            .bind(builder.id)
            .execute(&pool)
            .await
            .expect("Failed to disable active builder");

        let job_id = create_queued_job(
            &pool,
            "https://example.com/requeue-disabled-active.git",
            "requeue-disabled-active",
            &format!("requeuedisabledactive{}", Uuid::new_v4().simple()),
            now,
            "drv-requeue-disabled-active",
            1.0,
            now,
        )
        .await;
        assign_job_to_builder(&pool, &job_id, &builder.id)
            .await
            .expect("disabled active builder owns building job in test setup");

        let recovered =
            requeue_orphaned_building_jobs_with_reason(&pool, "runtime builder liveness recovery")
                .await
                .expect("recovery should succeed");

        assert!(
            recovered.iter().any(|job| job.id == job_id),
            "disabled active builder job should be recovered"
        );

        let row = get_build_job_by_id(&pool, &job_id)
            .await
            .expect("job fetch should succeed")
            .expect("job should exist");
        assert_eq!(row.status, "queued");
        assert_eq!(row.builder_id, None);
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_requeue_orphaned_building_jobs_preserves_log_size_limit() {
        let pool = queue_test_pool().await;
        let now = Utc::now();
        const MAX_LOG_BYTES: i64 = 10 * 1024 * 1024;

        let job_id = create_queued_job(
            &pool,
            "https://example.com/requeue-log-limit.git",
            "requeue-log-limit",
            &format!("requeueloglimit{}", Uuid::new_v4().simple()),
            now,
            "drv-requeue-log-limit",
            1.0,
            now,
        )
        .await;
        set_build_job_status(&pool, job_id, "building").await;
        sqlx::query(
            r#"
            UPDATE build_jobs
            SET builder_id = NULL,
                logs = repeat('é', ($2 / 2)::int)
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .bind(MAX_LOG_BYTES)
        .execute(&pool)
        .await
        .expect("Failed to seed near-limit logs");

        let recovered =
            requeue_orphaned_building_jobs_with_reason(&pool, "startup builder recovery")
                .await
                .expect("recovery should succeed");
        assert!(recovered.iter().any(|job| job.id == job_id));

        let (log_bytes, has_reason): (i64, bool) = sqlx::query_as(
            r#"
            SELECT
                OCTET_LENGTH(COALESCE(logs, ''))::bigint,
                COALESCE(logs, '') LIKE '%startup builder recovery%'
            FROM build_jobs
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .fetch_one(&pool)
        .await
        .expect("Failed to fetch recovered log length");

        assert!(
            log_bytes <= MAX_LOG_BYTES,
            "recovery log append must preserve the 10 MiB limit, got {log_bytes} bytes"
        );
        assert!(has_reason, "recovery reason should still be appended");
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_late_stale_builder_completion_does_not_clobber_requeued_job() {
        let pool = queue_test_pool().await;
        let now = Utc::now();

        let builder_a = create_active_test_builder(&pool, "late-builder-a").await;
        let builder_b = create_active_test_builder(&pool, "late-builder-b").await;

        let job_id = create_queued_job(
            &pool,
            "https://example.com/late-complete.git",
            "late-complete",
            "latecomplete000000000000000000000001",
            now,
            "drv-late-complete",
            1.0,
            now,
        )
        .await;

        assign_job_to_builder(&pool, &job_id, &builder_a.id)
            .await
            .expect("builder A should claim job");

        sqlx::query("UPDATE builders SET status = 'offline' WHERE id = $1")
            .bind(builder_a.id)
            .execute(&pool)
            .await
            .expect("Failed to mark builder A offline");

        let recovered = requeue_orphaned_building_jobs(&pool)
            .await
            .expect("recovery should succeed");
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].id, job_id);

        assign_job_to_builder(&pool, &job_id, &builder_b.id)
            .await
            .expect("builder B should claim requeued job");

        let stale_complete = mark_job_complete(&pool, &job_id, &builder_a.id, None).await;
        assert!(
            stale_complete.is_err(),
            "stale builder A completion must be rejected"
        );

        let still_building = get_build_job_status(&pool, &job_id)
            .await
            .expect("status lookup should succeed")
            .expect("job should exist");
        assert_eq!(still_building, "building");

        let row = get_build_job_by_id(&pool, &job_id)
            .await
            .expect("job fetch should succeed")
            .expect("job should exist");
        assert_eq!(row.builder_id, Some(builder_b.id));
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_late_stale_builder_failure_does_not_clobber_requeued_job() {
        let pool = queue_test_pool().await;
        let now = Utc::now();

        let builder_a = create_active_test_builder(&pool, "late-fail-builder-a").await;
        let builder_b = create_active_test_builder(&pool, "late-fail-builder-b").await;

        let job_id = create_queued_job(
            &pool,
            "https://example.com/late-fail.git",
            "late-fail",
            "latefail0000000000000000000000000001",
            now,
            "drv-late-fail",
            1.0,
            now,
        )
        .await;

        assign_job_to_builder(&pool, &job_id, &builder_a.id)
            .await
            .expect("builder A should claim job");

        sqlx::query("UPDATE builders SET status = 'offline' WHERE id = $1")
            .bind(builder_a.id)
            .execute(&pool)
            .await
            .expect("Failed to mark builder A offline");

        requeue_orphaned_building_jobs(&pool)
            .await
            .expect("recovery should succeed");

        assign_job_to_builder(&pool, &job_id, &builder_b.id)
            .await
            .expect("builder B should claim requeued job");

        let stale_fail = mark_job_failed_with_retry(
            &pool,
            &job_id,
            &builder_a.id,
            None,
            Some("late failure from stale builder"),
            RetryFailureClass::Unknown,
        )
        .await;
        assert!(stale_fail.is_err(), "stale builder A fail must be rejected");

        let row = get_build_job_by_id(&pool, &job_id)
            .await
            .expect("job fetch should succeed")
            .expect("job should exist");
        assert_eq!(row.status, "building");
        assert_eq!(row.builder_id, Some(builder_b.id));
    }
}
