use crate::builder::remove_gc_root;
use crate::config::CacheConfig;
use crate::models::cache_destination::CacheDestination;
use crate::queries::cache_destinations::{filter_caches_by_environment, get_global_caches};
use crate::queries::derivations::get_derivation_by_id;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use tracing::{debug, warn};

/// Records a queued publication and its immutable destination provenance.
///
/// Database jobs retain their selected ID after destination deletion. Legacy
/// references require an unambiguous eligible database match; only explicitly
/// static jobs can use static configuration.
#[derive(Debug, FromRow, Clone, Serialize)]
pub struct CachePushJob {
    /// Identifies the queue row.
    pub id: i32,
    /// Identifies the derivation whose output must be published.
    pub derivation_id: i32,
    /// Records the pending, running, completed or failure lifecycle state.
    pub status: String,
    /// Records the queued output path, if available.
    pub store_path: Option<String>,
    /// Records when publication was scheduled.
    pub scheduled_at: DateTime<Utc>,
    /// Records when the current attempt started.
    pub started_at: Option<DateTime<Utc>>,
    /// Records when the latest attempt finished.
    pub completed_at: Option<DateTime<Utc>>,
    /// Counts attempts for retry backoff and exhaustion.
    pub attempts: i32,
    /// Records the latest credential-safe failure diagnostic.
    pub error_message: Option<String>,
    /// Records uploaded bytes when measured.
    pub push_size_bytes: Option<i64>,
    /// Records the latest upload duration in milliseconds.
    pub push_duration_ms: Option<i32>,
    /// Retains the human-readable destination name or historical URL reference.
    pub cache_destination: Option<String>,
    /// Identifies the selected database destination, including after deletion.
    pub cache_destination_id: Option<i32>,
    /// Distinguishes `database`, `static`, and uncertain historical `legacy` jobs.
    pub cache_destination_source: String,
}

fn select_derivation_environment(matches: &[(uuid::Uuid, i32)]) -> Result<Option<uuid::Uuid>> {
    let Some((environment, priority)) = matches.first() else {
        return Ok(None);
    };
    anyhow::ensure!(
        matches
            .iter()
            .filter(|(_, rank)| rank == priority)
            .all(|(id, _)| id == environment),
        "Ambiguous derivation environment"
    );
    Ok(Some(*environment))
}

/// Returns current enabled destinations eligible for a derivation's environment.
///
/// Uses active systems from the commit's flake, preferring configuration-name
/// matches over hostname matches. Equally preferred cross-environment matches
/// fail closed. Enabled environment assignments override global destinations;
/// globals apply when there are no enabled assigned destinations.
///
/// # Errors
/// Returns an error for ambiguous environments, database or decryption failures.
///
/// # Examples
/// ```no_run
/// # async fn eligible(
/// #     pool: &sqlx::PgPool,
/// #     derivation: &crystal_forge::derivations::Derivation,
/// # ) -> anyhow::Result<()> {
/// use crystal_forge::queries::cache_push::
///     eligible_cache_destinations_for_derivation;
///
/// let destinations =
///     eligible_cache_destinations_for_derivation(pool, derivation).await?;
/// assert!(destinations.iter().all(|destination| destination.enabled));
/// # Ok(()) }
/// ```
pub async fn eligible_cache_destinations_for_derivation(
    pool: &PgPool,
    derivation: &crate::derivations::Derivation,
) -> Result<Vec<CacheDestination>> {
    let matches = if let Some(commit_id) = derivation.commit_id {
        sqlx::query_as::<_, (uuid::Uuid, i32)>(
            r#"SELECT s.environment_id,
                      CASE WHEN NULLIF(s.system_configuration_name, '') = $2 THEN 0 ELSE 1 END
               FROM systems s JOIN commits c ON c.flake_id = s.flake_id
               WHERE c.id = $1 AND s.environment_id IS NOT NULL AND s.is_active = TRUE
                 AND (s.hostname = $2 OR NULLIF(s.system_configuration_name, '') = $2)
               ORDER BY 2"#,
        )
        .bind(commit_id)
        .bind(&derivation.derivation_name)
        .fetch_all(pool)
        .await?
    } else {
        Vec::new()
    };
    let mut eligible = if let Some(environment) = select_derivation_environment(&matches)? {
        filter_caches_by_environment(pool, Some(environment)).await?
    } else {
        Vec::new()
    };
    eligible.retain(|destination| destination.enabled);
    if eligible.is_empty() {
        eligible = get_global_caches(pool).await?;
    }
    eligible.retain(|destination| destination.enabled);
    Ok(eligible)
}

fn select_job_destination<'a>(
    source: &str,
    id: Option<i32>,
    reference: Option<&str>,
    eligible: &'a [CacheDestination],
    static_config: &CacheConfig,
) -> Result<Option<&'a CacheDestination>> {
    match source {
        "static" => {
            anyhow::ensure!(id.is_none(), "Static job has database identity");
            anyhow::ensure!(
                static_config.push_after_build,
                "Static publication disabled"
            );
            anyhow::ensure!(
                reference.is_some() && reference == static_config.push_to.as_deref(),
                "Static cache destination mismatch"
            );
            Ok(None)
        }
        "database" => {
            let id = id.ok_or_else(|| anyhow::anyhow!("Database job missing destination ID"))?;
            // SECURITY: Never resolve a deleted/ineligible ID by reference.
            let destination = eligible
                .iter()
                .find(|destination| destination.id == id && destination.enabled)
                .ok_or_else(|| {
                    anyhow::anyhow!("Selected database destination deleted, disabled or ineligible")
                })?;
            Ok(Some(destination))
        }
        "legacy" => {
            anyhow::ensure!(id.is_none(), "Legacy job has inconsistent identity");
            let reference = reference
                .ok_or_else(|| anyhow::anyhow!("Legacy job missing destination reference"))?;
            let mut matches = eligible.iter().filter(|destination| {
                destination.enabled
                    && (destination.name == reference
                        || destination.push_to.as_deref() == Some(reference))
            });
            let destination = matches
                .next()
                .ok_or_else(|| anyhow::anyhow!("Unresolved legacy destination"))?;
            anyhow::ensure!(matches.next().is_none(), "Ambiguous legacy destination");
            Ok(Some(destination))
        }
        _ => anyhow::bail!("Unknown cache destination provenance"),
    }
}

/// Resolves a job's current destination and pins a legacy match before pushing.
///
/// Only `static` jobs return `None`, authorizing the caller's static settings.
/// Legacy jobs without an eligible database match never use static settings,
/// even when no destination rows remain. After pinning, retries retain the ID.
///
/// # Errors
/// Returns an error for invalid provenance, missing/ambiguous/ineligible identity,
/// database or decryption failure, or concurrent legacy identity changes.
///
/// # Examples
/// Resolves an existing legacy job with one eligible name or URL match. Success
/// persists `database` provenance and the selected ID before publication;
/// unresolved or ambiguous legacy references return an error without static
/// fallback. The caller must use the returned current destination settings.
///
/// ```no_run
/// # async fn pin_legacy(
/// #     pool: &sqlx::PgPool,
/// #     legacy_job_id: i32,
/// #     static_config: &crystal_forge::config::CacheConfig,
/// # ) -> anyhow::Result<()> {
/// use crystal_forge::queries::cache_push::{
///     get_cache_push_job_detail, resolve_cache_push_destination,
/// };
/// use crystal_forge::queries::derivations::get_derivation_by_id;
///
/// let mut job = get_cache_push_job_detail(pool, legacy_job_id)
///     .await?
///     .ok_or_else(|| anyhow::anyhow!("Legacy job not found"))?;
/// assert_eq!(job.cache_destination_source, "legacy");
/// let derivation = get_derivation_by_id(pool, job.derivation_id).await?;
/// let destination =
///     resolve_cache_push_destination(pool, &mut job, &derivation, static_config)
///         .await?
///         .ok_or_else(|| anyhow::anyhow!("Legacy job requires a DB match"))?;
/// assert_eq!(job.cache_destination_source, "database");
/// assert_eq!(job.cache_destination_id, Some(destination.id));
/// let persisted = get_cache_push_job_detail(pool, job.id)
///     .await?
///     .ok_or_else(|| anyhow::anyhow!("Pinned job not found"))?;
/// assert_eq!(persisted.cache_destination_source, "database");
/// assert_eq!(persisted.cache_destination_id, Some(destination.id));
/// # Ok(()) }
/// ```
pub async fn resolve_cache_push_destination(
    pool: &PgPool,
    job: &mut CachePushJob,
    derivation: &crate::derivations::Derivation,
    static_config: &CacheConfig,
) -> Result<Option<CacheDestination>> {
    // Refresh provenance too: a concurrent worker might have pinned this legacy
    // row while this worker waited for the Niks3 execution slot.
    *job = get_cache_push_job_detail(pool, job.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Cache push job disappeared"))?;
    anyhow::ensure!(
        job.derivation_id == derivation.id,
        "Cache push derivation identity changed"
    );
    let eligible = eligible_cache_destinations_for_derivation(pool, derivation).await?;
    let destination = select_job_destination(
        &job.cache_destination_source,
        job.cache_destination_id,
        job.cache_destination.as_deref(),
        &eligible,
        static_config,
    )?
    .cloned();
    if job.cache_destination_source == "legacy" {
        let id = destination
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Legacy identity not resolved"))?
            .id;
        let updated = sqlx::query(
            "UPDATE cache_push_jobs SET cache_destination_id = $2, cache_destination_source = 'database'
             WHERE id = $1 AND cache_destination_source = 'legacy'
               AND cache_destination IS NOT DISTINCT FROM $3",
        ).bind(job.id).bind(id).bind(&job.cache_destination).execute(pool).await?;
        anyhow::ensure!(
            updated.rows_affected() == 1,
            "Legacy identity changed concurrently"
        );
        job.cache_destination_id = Some(id);
        job.cache_destination_source = "database".into();
    }
    Ok(destination)
}

struct EnqueueDestination {
    id: Option<i32>,
    source: &'static str,
    reference: String,
}

fn same_publication_identity(
    source: &str,
    id: Option<i32>,
    reference: Option<&str>,
    destination: &EnqueueDestination,
) -> bool {
    source == destination.source
        && id == destination.id
        && (source == "database" || reference == Some(destination.reference.as_str()))
}

fn select_enqueue_destination(
    database_present: bool,
    eligible: &[CacheDestination],
    static_config: &CacheConfig,
    selected_id: Option<i32>,
) -> Result<Option<EnqueueDestination>> {
    if let Some(id) = selected_id {
        let destination = eligible
            .iter()
            .find(|destination| destination.id == id && destination.enabled)
            .ok_or_else(|| anyhow::anyhow!("Selected enqueue destination ineligible"))?;
        return Ok(Some(EnqueueDestination {
            id: Some(id),
            source: "database",
            reference: destination.name.clone(),
        }));
    }
    if database_present {
        let mut enabled = eligible.iter().filter(|destination| destination.enabled);
        let Some(destination) = enabled.next() else {
            return Ok(None);
        };
        anyhow::ensure!(
            enabled.next().is_none(),
            "Enqueue destination is ambiguous; supply selected ID"
        );
        return Ok(Some(EnqueueDestination {
            id: Some(destination.id),
            source: "database",
            reference: destination.name.clone(),
        }));
    }
    anyhow::ensure!(
        eligible.is_empty(),
        "Database destinations changed during enqueue resolution"
    );
    Ok(static_config
        .push_to
        .as_ref()
        .filter(|_| static_config.push_after_build)
        .map(|reference| EnqueueDestination {
            id: None,
            source: "static",
            reference: reference.clone(),
        }))
}

/// Enqueues publication using the single current eligible database destination.
///
/// This is the shared local-build and remote-completion-without-push producer.
/// Database-only configurations do not require a static `push_to`. Returns
/// `None` when no eligible destination exists or static publication is disabled.
/// Multiple eligible destinations require [`enqueue_cache_push_for_destination`]
/// with the producer's explicit selected ID; this helper never chooses a first
/// row. Static provenance is emitted only when no database destinations exist.
/// A completed remote build's recorded destination ID takes precedence over
/// current implicit selection, including during periodic missing-job recovery.
/// A recorded no-cache dispatch suppresses enqueue and rejects explicit ID
/// overrides. Only absence of a recorded dispatch permits implicit selection.
/// Existing jobs retain their identity, status and retry backoff.
///
/// # Errors
/// Returns an error for missing persisted output, ambiguous/invalid selection,
/// database or credential decryption failure.
///
/// # Examples
/// ```no_run
/// # async fn enqueue(
/// #     pool: &sqlx::PgPool,
/// #     config: &crystal_forge::config::CacheConfig,
/// # ) -> anyhow::Result<()> {
/// use crystal_forge::queries::cache_push::enqueue_cache_push_for_derivation;
/// let job_id = enqueue_cache_push_for_derivation(pool, 42, config).await?;
/// # Ok(()) }
/// ```
pub async fn enqueue_cache_push_for_derivation(
    pool: &PgPool,
    derivation_id: i32,
    static_config: &CacheConfig,
) -> Result<Option<i32>> {
    enqueue_for_derivation(pool, derivation_id, static_config, None).await
}

/// Enqueues publication to an explicitly selected eligible database destination.
///
/// A missing/deleted/disabled/ineligible ID never falls back to static settings.
/// Existing jobs are not retargeted. This supports producers whose dispatch
/// already recorded the intended destination, including a missing remote push.
///
/// # Errors
/// Returns an error for missing output, ineligible ID, database or decryption failure.
///
/// # Examples
/// Uses the producer's selected ID rather than selecting another eligible cache.
/// The derivation must already have persisted output.
///
/// ```no_run
/// # async fn enqueue_selected(
/// #     pool: &sqlx::PgPool,
/// #     derivation_id: i32,
/// #     selected_destination_id: i32,
/// # ) -> anyhow::Result<()> {
/// use crystal_forge::queries::cache_push::{
///     enqueue_cache_push_for_destination, get_cache_push_job_detail,
/// };
///
/// if let Some(job_id) = enqueue_cache_push_for_destination(
///     pool, derivation_id, selected_destination_id,
/// ).await? {
///     let job = get_cache_push_job_detail(pool, job_id)
///         .await?
///         .ok_or_else(|| anyhow::anyhow!("Queued job not found"))?;
///     assert_eq!(job.cache_destination_source, "database");
///     assert_eq!(job.cache_destination_id, Some(selected_destination_id));
/// }
/// # Ok(()) }
/// ```
pub async fn enqueue_cache_push_for_destination(
    pool: &PgPool,
    derivation_id: i32,
    destination_id: i32,
) -> Result<Option<i32>> {
    enqueue_for_derivation(
        pool,
        derivation_id,
        &CacheConfig::default(),
        Some(destination_id),
    )
    .await
}

async fn enqueue_for_derivation(
    pool: &PgPool,
    derivation_id: i32,
    static_config: &CacheConfig,
    selected_id: Option<i32>,
) -> Result<Option<i32>> {
    let derivation = get_derivation_by_id(pool, derivation_id).await?;
    let store_path = derivation
        .store_path
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Publication requires persisted build output"))?;
    // SECURITY: Recovery must not reinterpret a missing remote publication as
    // permission to use a new cache after deletion or environment reassignment.
    // The remote dispatch ID is authoritative even before a cache job exists.
    let dispatched_id = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT dispatched_cache_destination_id FROM build_jobs
         WHERE derivation_id = $1 AND status = 'success' AND cache_dispatch_recorded_at IS NOT NULL
         ORDER BY completed_at DESC NULLS LAST LIMIT 1",
    )
    .bind(derivation_id)
    .fetch_optional(pool)
    .await?;
    let selected_id = match dispatched_id {
        Some(recorded) => {
            if let Some(requested) = selected_id {
                anyhow::ensure!(
                    Some(requested) == recorded,
                    "Enqueue ID differs from recorded remote dispatch"
                );
            }
            // SECURITY: Some(None) is an explicit no-cache dispatch, not
            // unbound legacy work. Recovery cannot opportunistically publish
            // to a new database or static destination added after dispatch.
            let Some(recorded) = recorded else {
                return Ok(None);
            };
            Some(recorded)
        }
        None => selected_id,
    };
    let database_present =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM cache_destinations)")
            .fetch_one(pool)
            .await?;
    let eligible = eligible_cache_destinations_for_derivation(pool, &derivation).await?;
    let Some(destination) =
        select_enqueue_destination(database_present, &eligible, static_config, selected_id)?
    else {
        return Ok(None);
    };
    Ok(Some(
        insert_cache_push_job(pool, derivation_id, store_path, &destination).await?,
    ))
}

async fn insert_cache_push_job(
    pool: &PgPool,
    derivation_id: i32,
    store_path: &str,
    destination: &EnqueueDestination,
) -> Result<i32> {
    let mut tx = pool.begin().await?;
    let id = insert_cache_push_job_tx(&mut tx, derivation_id, store_path, destination).await?;
    tx.commit().await?;
    Ok(id)
}

/// Enqueues an already resolved database destination in the caller's transaction.
///
/// The caller MUST validate current enabled/environment eligibility and hold
/// the destination's publication-configuration lock until commit. The caller
/// MUST lock the derivation row before acquiring that destination lock.
/// `store_path`
/// MUST be the authoritative persisted or transaction-local completed output.
/// This helper locks the derivation row, preserves existing job identity and
/// retry state, and writes `database` provenance. It does not commit. Database
/// completion owners can therefore save output, queue missing publication and
/// transition build state atomically without opening a second connection.
///
/// # Errors
/// Returns an error for database failure or conflicting existing job identity.
///
/// # Examples
/// ```no_run
/// # async fn queue(
/// #     tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
/// #     locked_eligible_id: i32,
/// #     persisted_output: &str,
/// # ) -> anyhow::Result<()> {
/// use crystal_forge::queries::cache_push::enqueue_cache_push_for_destination_tx;
/// enqueue_cache_push_for_destination_tx(
///     tx, 42, persisted_output, locked_eligible_id, "selected-cache",
/// ).await?;
/// # Ok(()) }
/// ```
pub async fn enqueue_cache_push_for_destination_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    derivation_id: i32,
    store_path: &str,
    destination_id: i32,
    destination_name: &str,
) -> Result<i32> {
    insert_cache_push_job_tx(
        tx,
        derivation_id,
        store_path,
        &EnqueueDestination {
            id: Some(destination_id),
            source: "database",
            reference: destination_name.into(),
        },
    )
    .await
}

async fn insert_cache_push_job_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    derivation_id: i32,
    store_path: &str,
    destination: &EnqueueDestination,
) -> Result<i32> {
    // CONCURRENCY: Serialize cooperating producers per derivation. Reuse only
    // the same durable identity; do not overwrite a failed job's destination or
    // reset its retry backoff. The derivation row exists for every valid job.
    sqlx::query("SELECT id FROM derivations WHERE id = $1 FOR UPDATE")
        .bind(derivation_id)
        .fetch_one(&mut **tx)
        .await?;
    let existing = sqlx::query_as::<_, CachePushJob>(
        "SELECT * FROM cache_push_jobs WHERE derivation_id = $1 ORDER BY scheduled_at DESC FOR UPDATE",
    ).bind(derivation_id).fetch_all(&mut **tx).await?;
    let id = if let Some(job) = existing.first() {
        // A new producer cannot turn an old DB/legacy job into a static job or
        // change its selected ID. Uncertain legacy rows are resolved by the
        // worker, not relabelled from the producer's current preferences.
        anyhow::ensure!(
            existing.iter().all(|existing| same_publication_identity(
                &existing.cache_destination_source,
                existing.cache_destination_id,
                existing.cache_destination.as_deref(),
                destination
            )),
            "Existing cache publication identity differs; cannot retarget job"
        );
        job.id
    } else {
        sqlx::query_scalar::<_, i32>(
            "INSERT INTO cache_push_jobs (derivation_id, store_path, cache_destination,
                 cache_destination_id, cache_destination_source, status)
             VALUES ($1, $2, $3, $4, $5, 'pending') RETURNING id",
        )
        .bind(derivation_id)
        .bind(store_path)
        .bind(&destination.reference)
        .bind(destination.id)
        .bind(destination.source)
        .fetch_one(&mut **tx)
        .await?
    };
    Ok(id)
}

/// Enqueues missing built-derivation jobs with the canonical per-derivation policy.
///
/// Individual resolution failures are skipped, without substituting another
/// cache. Retries belong to existing jobs; this pass never rewrites them.
///
/// # Errors
/// Returns an error if the candidate query fails.
///
/// # Examples
/// Runs one bounded recovery pass. Each candidate uses its own environment and
/// recorded dispatch policy; existing publication jobs retain their retry state.
///
/// ```no_run
/// # async fn recover(
/// #     pool: &sqlx::PgPool,
/// #     static_config: &crystal_forge::config::CacheConfig,
/// # ) -> anyhow::Result<usize> {
/// use crystal_forge::queries::cache_push::enqueue_missing_cache_push_jobs;
///
/// let queued = enqueue_missing_cache_push_jobs(pool, static_config).await?;
/// # Ok(queued) }
/// ```
pub async fn enqueue_missing_cache_push_jobs(
    pool: &PgPool,
    static_config: &CacheConfig,
) -> Result<usize> {
    let ids = sqlx::query_scalar::<_, i32>(
        "SELECT d.id FROM derivations d WHERE d.status_id = 10 AND d.store_path IS NOT NULL
         AND NOT EXISTS (SELECT 1 FROM cache_push_jobs j WHERE j.derivation_id = d.id)
         ORDER BY d.completed_at ASC NULLS LAST LIMIT 100",
    )
    .fetch_all(pool)
    .await?;
    // Bound each recovery tick to 100 derivations so resolution/decryption work
    // does not monopolize the worker loop on a large completed-build backlog.
    let mut count = 0;
    for id in ids {
        match enqueue_cache_push_for_derivation(pool, id, static_config).await {
            Ok(Some(_)) => count += 1,
            Ok(None) => {}
            Err(_) => warn!(
                derivation_id = id,
                "Cache enqueue destination resolution failed"
            ),
        }
    }
    Ok(count)
}

/// Get derivations that need cache pushing (build-complete status)
/// Prioritizes dependencies of newest NixOS systems first, then the systems themselves
pub async fn get_derivations_needing_cache_push_for_dest(
    pool: &PgPool,
    destination: &str,
    limit: Option<i32>,
    max_attempts: i32,
) -> Result<Vec<crate::derivations::Derivation>> {
    use crate::queries::derivations::EvaluationStatus;

    let sql = r#"
        SELECT
            d.id, d.commit_id, d.derivation_type, d.derivation_name,
            d.derivation_path, d.derivation_target, d.scheduled_at,
            d.completed_at, d.started_at, d.attempt_count,
            d.evaluation_duration_ms, d.error_message, d.pname,
            d.version, d.status_id, d.build_elapsed_seconds,
            d.build_current_target, d.build_last_activity_seconds,
            d.build_last_heartbeat, d.cf_agent_enabled, d.store_path
        FROM view_cache_push_queue v
        JOIN derivations d ON d.id = v.id
        WHERE (v.cache_destination = $3 OR v.cache_destination IS NULL)
            AND v.derivation_status_id = $2
            AND v.push_status IN ('no_job', 'retryable')
            AND (v.current_max_attempts IS NULL OR v.current_max_attempts < $4)
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

/// Enqueues an explicitly referenced, unambiguous eligible database destination.
///
/// Compatibility API for name/URL producers. New producers should use
/// [`enqueue_cache_push_for_derivation`] or [`enqueue_cache_push_for_destination`].
/// A NULL or unmatched reference cannot prove static provenance. Existing jobs
/// retain their destination and retry state instead of being retargeted.
///
/// # Errors
/// Returns an error for missing/ambiguous/ineligible references, or database and
/// credential decryption failures.
pub async fn create_cache_push_job(
    pool: &PgPool,
    derivation_id: i32,
    store_path: &str,
    cache_destination: Option<&str>,
) -> Result<i32> {
    let derivation = get_derivation_by_id(pool, derivation_id).await?;
    let eligible = eligible_cache_destinations_for_derivation(pool, &derivation).await?;
    let destination = select_job_destination(
        "legacy",
        None,
        cache_destination,
        &eligible,
        &CacheConfig::default(),
    )?
    .ok_or_else(|| anyhow::anyhow!("Missing database destination"))?;
    insert_cache_push_job(
        pool,
        derivation_id,
        store_path,
        &EnqueueDestination {
            id: Some(destination.id),
            source: "database",
            reference: destination.name.clone(),
        },
    )
    .await
}

/// Claims a pending or due-retry job for one local attempt.
///
/// Concurrent workers cannot both claim the same selected queue row. The claim
/// increments attempts and clears the prior attempt's diagnostic and retry time.
///
/// # Errors
/// Returns an error for database failure or a job that is no longer claimable.
pub async fn mark_cache_push_in_progress(pool: &PgPool, job_id: i32) -> Result<()> {
    // CONCURRENCY: Selection is advisory; the conditional write owns the claim.
    // Do not overwrite completed or already active jobs selected by a rival.
    let claimed = sqlx::query(
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
          AND (status = 'pending' OR
               (status = 'failed' AND retry_after IS NOT NULL AND retry_after <= NOW()))
        "#,
    )
    .bind(job_id)
    .execute(pool)
    .await?;
    anyhow::ensure!(
        claimed.rows_affected() == 1,
        "Cache push job no longer claimable"
    );

    debug!("Marked cache push job {} as in progress", job_id);
    Ok(())
}

/// Marks a cache push job as completed and prompts waiting CVE scan promotion.
///
/// Cache publication commits before promotion starts. Promotion and GC-root
/// cleanup are best effort and do not turn successful publication into an
/// error.
///
/// # Errors
///
/// Returns an error when the cache-push job lookup or completion update fails.
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

    if let Err(error) = crate::queries::cve_scans::promote_waiting_cve_scans(
        pool,
        crate::queries::cve_scans::EVENT_PROMOTION_LIMIT,
    )
    .await
    {
        warn!(
            cache_push_job_id = job_id,
            derivation_id,
            %error,
            "Failed to promote waiting CVE scans after cache publication"
        );
    }

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
/// Prioritizes jobs from newest commits first
pub async fn get_pending_cache_push_jobs(
    pool: &PgPool,
    limit: Option<i32>,
) -> Result<Vec<CachePushJob>> {
    let jobs = sqlx::query_as::<_, CachePushJob>(
        r#"
        SELECT 
            cpj.id, cpj.derivation_id, cpj.status, cpj.store_path, cpj.scheduled_at, cpj.started_at, 
            cpj.completed_at, cpj.attempts, cpj.error_message, cpj.push_size_bytes, 
            cpj.push_duration_ms, cpj.cache_destination,
            cpj.cache_destination_id, cpj.cache_destination_source
        FROM cache_push_jobs cpj
        JOIN derivations d ON d.id = cpj.derivation_id
        JOIN commits c ON c.id = d.commit_id
        WHERE 
            (cpj.status = 'pending')
            OR 
            (cpj.status = 'failed' AND cpj.retry_after IS NOT NULL AND cpj.retry_after <= NOW())
        ORDER BY 
            CASE 
                WHEN cpj.status = 'pending' THEN 0
                WHEN cpj.status = 'failed' THEN 1
            END,
            c.commit_timestamp DESC,
            d.completed_at ASC NULLS LAST
        LIMIT $1
        "#,
    )
    .bind(limit.unwrap_or(10) as i64)
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

/// List cache push jobs with optional filtering
pub async fn list_cache_push_jobs(
    pool: &PgPool,
    status_filter: Option<&str>,
    cache_destination_filter: Option<&str>,
    limit: Option<i32>,
    offset: Option<i32>,
) -> Result<Vec<CachePushJob>> {
    let mut query = String::from(
        r#"
        SELECT 
            cpj.id, cpj.derivation_id, cpj.status, cpj.store_path, cpj.scheduled_at, cpj.started_at, 
            cpj.completed_at, cpj.attempts, cpj.error_message, cpj.push_size_bytes, 
            cpj.push_duration_ms, cpj.cache_destination,
            cpj.cache_destination_id, cpj.cache_destination_source
        FROM cache_push_jobs cpj
        WHERE 1=1
        "#,
    );

    let mut bind_idx = 1;
    if status_filter.is_some() {
        query.push_str(&format!(" AND cpj.status = ${}", bind_idx));
        bind_idx += 1;
    }
    if cache_destination_filter.is_some() {
        query.push_str(&format!(" AND cpj.cache_destination = ${}", bind_idx));
        bind_idx += 1;
    }

    query.push_str(" ORDER BY cpj.scheduled_at DESC");
    query.push_str(&format!(" LIMIT ${}", bind_idx));
    bind_idx += 1;
    query.push_str(&format!(" OFFSET ${}", bind_idx));

    let mut q = sqlx::query_as::<_, CachePushJob>(&query);

    if let Some(status) = status_filter {
        q = q.bind(status);
    }
    if let Some(cache_dest) = cache_destination_filter {
        q = q.bind(cache_dest);
    }
    q = q.bind(limit.unwrap_or(50) as i64);
    q = q.bind(offset.unwrap_or(0) as i64);

    let jobs = q.fetch_all(pool).await?;
    Ok(jobs)
}

/// Get a single cache push job by ID with derivation details
pub async fn get_cache_push_job_detail(pool: &PgPool, job_id: i32) -> Result<Option<CachePushJob>> {
    let job = sqlx::query_as::<_, CachePushJob>("SELECT * FROM cache_push_jobs WHERE id = $1")
        .bind(job_id)
        .fetch_optional(pool)
        .await?;

    Ok(job)
}

/// Retry a failed cache push job (admin action)
pub async fn retry_cache_push_job(pool: &PgPool, job_id: i32) -> Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE cache_push_jobs
        SET 
            status = 'pending',
            error_message = NULL,
            retry_after = NULL,
            scheduled_at = NOW()
        WHERE id = $1 AND status IN ('failed', 'permanently_failed', 'cancelled')
        "#,
    )
    .bind(job_id)
    .execute(pool)
    .await?;

    let retried = result.rows_affected() > 0;
    if retried {
        debug!("Manually retried cache push job {}", job_id);
    }

    Ok(retried)
}

/// Cancel a pending or failed cache push job (admin action)
pub async fn cancel_cache_push_job(pool: &PgPool, job_id: i32) -> Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE cache_push_jobs
        SET 
            status = 'cancelled',
            completed_at = NOW(),
            error_message = 'Manually cancelled by administrator'
        WHERE id = $1 AND status IN ('pending', 'failed')
        "#,
    )
    .bind(job_id)
    .execute(pool)
    .await?;

    let cancelled = result.rows_affected() > 0;
    if cancelled {
        debug!("Manually cancelled cache push job {}", job_id);
    }

    Ok(cancelled)
}

/// Bulk retry multiple cache push jobs
pub async fn bulk_retry_cache_push_jobs(pool: &PgPool, job_ids: &[i32]) -> Result<i64> {
    let result = sqlx::query(
        r#"
        UPDATE cache_push_jobs
        SET 
            status = 'pending',
            error_message = NULL,
            retry_after = NULL,
            scheduled_at = NOW()
        WHERE id = ANY($1) AND status IN ('failed', 'permanently_failed', 'cancelled')
        "#,
    )
    .bind(job_ids)
    .execute(pool)
    .await?;

    let count = result.rows_affected();
    if count > 0 {
        debug!("Bulk retried {} cache push jobs", count);
    }

    Ok(count as i64)
}

/// Bulk cancel multiple cache push jobs
pub async fn bulk_cancel_cache_push_jobs(pool: &PgPool, job_ids: &[i32]) -> Result<i64> {
    let result = sqlx::query(
        r#"
        UPDATE cache_push_jobs
        SET 
            status = 'cancelled',
            completed_at = NOW(),
            error_message = 'Bulk cancelled by administrator'
        WHERE id = ANY($1) AND status IN ('pending', 'failed')
        "#,
    )
    .bind(job_ids)
    .execute(pool)
    .await?;

    let count = result.rows_affected();
    if count > 0 {
        debug!("Bulk cancelled {} cache push jobs", count);
    }

    Ok(count as i64)
}

#[cfg(test)]
mod niks3_tests {
    use super::*;

    fn destination(id: i32, name: &str, url: &str) -> CacheDestination {
        CacheDestination {
            id,
            name: name.into(),
            enabled: true,
            cache_type: "Niks3".into(),
            push_to: Some(url.into()),
            niks3_server_url: Some(format!("https://write-{id}.example")),
            niks3_public_keys: vec!["cache:key".into()],
            niks3_write_auth_mode: Some("token".into()),
            niks3_auth_token: Some("current-token".into()),
            niks3_read_auth_mode: Some("none".into()),
            ..Default::default()
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified isolated database and database creation privileges"]
    async fn niks3_recorded_no_cache_dispatch_differs_from_unrecorded_enqueue(pool: PgPool) {
        let derivation = crate::queries::derivations::insert_derivation_with_target(
            &pool,
            None,
            "dispatch-without-cache",
            "nixos",
            Some("dispatch-without-cache"),
            Some(true),
        )
        .await
        .unwrap();
        sqlx::query("UPDATE derivations SET store_path = '/nix/store/completed-output', status_id = 10 WHERE id = $1")
            .bind(derivation.id).execute(&pool).await.unwrap();
        let job_id: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO build_jobs (derivation_id, status, completed_at, cache_dispatch_recorded_at,
                 dispatched_cache_destination_id) VALUES ($1, 'success', NOW(), NOW(), NULL) RETURNING id",
        ).bind(derivation.id).fetch_one(&pool).await.unwrap();
        // Add the cache after dispatch. Neither implicit recovery nor an
        // explicit override may reinterpret the recorded no-cache decision.
        let id: i32 = sqlx::query_scalar(
            "INSERT INTO cache_destinations (name, cache_type, push_to, enabled, niks3_server_url,
                 niks3_public_keys, niks3_write_auth_mode, niks3_auth_token, niks3_read_auth_mode)
             VALUES ('added-after-dispatch', 'Niks3', 'https://new-read.example', TRUE,
                 'https://new-write.example', ARRAY['cache:key'], 'token', 'test-token', 'none') RETURNING id",
        ).fetch_one(&pool).await.unwrap();
        let static_config = CacheConfig {
            push_to: Some("s3://opportunistic-static".into()),
            push_after_build: true,
            ..Default::default()
        };
        assert_eq!(
            enqueue_cache_push_for_derivation(&pool, derivation.id, &static_config)
                .await
                .unwrap(),
            None
        );
        assert!(
            enqueue_cache_push_for_destination(&pool, derivation.id, id)
                .await
                .is_err()
        );
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM cache_push_jobs WHERE derivation_id = $1")
                .bind(derivation.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
        // No recorded dispatch is a different state: canonical selection is
        // allowed and queues the sole current eligible database destination.
        sqlx::query("UPDATE build_jobs SET cache_dispatch_recorded_at = NULL WHERE id = $1")
            .bind(job_id)
            .execute(&pool)
            .await
            .unwrap();
        let queued = enqueue_cache_push_for_derivation(&pool, derivation.id, &static_config)
            .await
            .unwrap()
            .unwrap();
        let job = get_cache_push_job_detail(&pool, queued)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(job.cache_destination_id, Some(id));
        assert_eq!(job.cache_destination_source, "database");
        // A recorded non-NULL ID is authoritative, too. Explicit mismatches
        // must fail before any existing publication identity can be changed.
        sqlx::query("UPDATE build_jobs SET cache_dispatch_recorded_at = NOW(), dispatched_cache_destination_id = $2 WHERE id = $1")
            .bind(job_id).bind(id).execute(&pool).await.unwrap();
        assert!(
            enqueue_cache_push_for_destination(&pool, derivation.id, id + 1)
                .await
                .is_err()
        );
        assert_eq!(
            enqueue_cache_push_for_destination(&pool, derivation.id, id)
                .await
                .unwrap(),
            Some(queued)
        );
        sqlx::query("DELETE FROM cache_destinations WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE build_jobs SET dispatched_cache_destination_id = NULL WHERE id = $1")
            .bind(job_id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            enqueue_cache_push_for_derivation(&pool, derivation.id, &static_config)
                .await
                .unwrap(),
            None
        );
    }

    #[test]
    fn database_only_local_and_remote_missing_push_enqueue_share_selected_identity() {
        let eligible = vec![destination(7, "private", "https://read.example")];
        let static_config = CacheConfig::default();
        assert!(static_config.push_to.is_none());
        let local = select_enqueue_destination(true, &eligible, &static_config, None)
            .unwrap()
            .unwrap();
        let remote_without_push =
            select_enqueue_destination(true, &eligible, &static_config, Some(7))
                .unwrap()
                .unwrap();
        assert_eq!(local.source, "database");
        assert_eq!(local.id, Some(7));
        assert_eq!(remote_without_push.id, local.id);
        assert_eq!(remote_without_push.reference, "private");
        assert!(
            select_job_destination(
                local.source,
                local.id,
                Some(&local.reference),
                &eligible,
                &static_config
            )
            .unwrap()
            .is_some()
        );
    }

    #[test]
    fn deleted_last_database_destination_cannot_use_same_read_url_static_write_identity() {
        let static_config = CacheConfig {
            cache_type: crate::config::CacheType::Niks3,
            push_to: Some("https://shared-read.example".into()),
            push_after_build: true,
            niks3_server_url: Some("https://different-write.example".into()),
            ..Default::default()
        };
        assert!(
            select_job_destination(
                "database",
                Some(7),
                static_config.push_to.as_deref(),
                &[],
                &static_config
            )
            .is_err()
        );
        let replacement = destination(8, "replacement", "https://shared-read.example");
        assert!(
            select_job_destination(
                "database",
                Some(7),
                static_config.push_to.as_deref(),
                &[replacement],
                &static_config
            )
            .is_err()
        );
        let now_static = select_enqueue_destination(false, &[], &static_config, None)
            .unwrap()
            .unwrap();
        assert!(!same_publication_identity(
            "database",
            Some(7),
            static_config.push_to.as_deref(),
            &now_static
        ));
        assert!(!same_publication_identity(
            "legacy",
            None,
            static_config.push_to.as_deref(),
            &now_static
        ));
    }

    #[test]
    fn legacy_name_and_url_resolution_is_eligible_unambiguous_and_never_static() {
        let a = destination(7, "private", "https://read.example");
        let static_config = CacheConfig {
            push_to: a.push_to.clone(),
            push_after_build: true,
            ..Default::default()
        };
        for reference in [Some("private"), a.push_to.as_deref()] {
            let resolved = select_job_destination(
                "legacy",
                None,
                reference,
                std::slice::from_ref(&a),
                &static_config,
            )
            .unwrap()
            .unwrap();
            assert_eq!(resolved.id, 7);
            assert!(
                select_job_destination("legacy", None, reference, &[], &static_config).is_err()
            );
        }
        assert!(select_job_destination("legacy", None, None, &[], &static_config).is_err());
        let b = destination(8, "https://read.example", "https://other.example");
        assert!(
            select_job_destination(
                "legacy",
                None,
                a.push_to.as_deref(),
                &[a.clone(), b],
                &static_config
            )
            .is_err()
        );
        let duplicate_name = destination(9, "private", "https://third.example");
        assert!(
            select_job_destination(
                "legacy",
                None,
                Some("private"),
                &[a.clone(), duplicate_name],
                &static_config
            )
            .is_err()
        );
        let unrelated = destination(10, "unrelated", "https://unrelated.example");
        assert!(
            select_job_destination(
                "legacy",
                None,
                Some("private"),
                &[unrelated],
                &static_config
            )
            .is_err()
        );
    }

    #[test]
    fn database_identity_obeys_current_enabled_environment_and_credentials() {
        let mut current = destination(7, "private", "https://read.example");
        let static_config = CacheConfig::default();
        current.enabled = false;
        assert!(
            select_job_destination(
                "database",
                Some(7),
                Some("private"),
                &[current.clone()],
                &static_config
            )
            .is_err()
        );
        current.enabled = true;
        current.niks3_auth_token = Some("rotated".into());
        let eligible = [current];
        let resolved = select_job_destination(
            "database",
            Some(7),
            Some("old-name-or-url"),
            &eligible,
            &static_config,
        )
        .unwrap()
        .unwrap();
        assert_eq!(resolved.niks3_auth_token.as_deref(), Some("rotated"));
        let other_environment = [destination(8, "other", "https://other.example")];
        assert!(
            select_job_destination(
                "database",
                Some(7),
                Some("private"),
                &other_environment,
                &static_config
            )
            .is_err()
        );
        assert!(
            select_enqueue_destination(true, &other_environment, &static_config, Some(7)).is_err()
        );
        assert!(select_enqueue_destination(true, &[], &static_config, Some(7)).is_err());
        assert!(
            select_enqueue_destination(true, &[], &static_config, None)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn enqueue_ambiguity_requires_explicit_id_and_never_first_cache() {
        let eligible = [
            destination(7, "a", "https://read-a.example"),
            destination(8, "b", "https://read-b.example"),
        ];
        assert!(
            select_enqueue_destination(true, &eligible, &CacheConfig::default(), None).is_err()
        );
        assert_eq!(
            select_enqueue_destination(true, &eligible, &CacheConfig::default(), Some(8))
                .unwrap()
                .unwrap()
                .id,
            Some(8)
        );
        assert!(
            select_job_destination(
                "database",
                None,
                Some("a"),
                &eligible,
                &CacheConfig::default()
            )
            .is_err()
        );
        assert!(
            select_job_destination(
                "unknown",
                Some(7),
                Some("a"),
                &eligible,
                &CacheConfig::default()
            )
            .is_err()
        );
    }

    #[test]
    fn static_fallback_requires_explicit_provenance_and_matching_enabled_config() {
        let mut static_config = CacheConfig {
            push_to: Some("s3://legacy".into()),
            push_after_build: true,
            ..Default::default()
        };
        let selected = select_enqueue_destination(false, &[], &static_config, None)
            .unwrap()
            .unwrap();
        assert_eq!(selected.source, "static");
        assert_eq!(selected.id, None);
        assert!(
            select_job_destination("static", None, Some("s3://legacy"), &[], &static_config)
                .unwrap()
                .is_none()
        );
        assert!(
            select_job_destination("legacy", None, Some("s3://legacy"), &[], &static_config)
                .is_err()
        );
        assert!(select_job_destination("static", None, None, &[], &static_config).is_err());
        assert!(
            select_job_destination("static", Some(7), Some("s3://legacy"), &[], &static_config)
                .is_err()
        );
        assert!(
            select_job_destination("static", None, Some("s3://other"), &[], &static_config)
                .is_err()
        );
        static_config.push_after_build = false;
        assert!(
            select_enqueue_destination(false, &[], &static_config, None)
                .unwrap()
                .is_none()
        );
        assert!(
            select_job_destination("static", None, Some("s3://legacy"), &[], &static_config)
                .is_err()
        );
    }

    #[test]
    fn canonical_environment_rejects_ties_and_preserves_configuration_precedence() {
        let a = uuid::Uuid::new_v4();
        let b = uuid::Uuid::new_v4();
        assert_eq!(select_derivation_environment(&[]).unwrap(), None);
        assert_eq!(
            select_derivation_environment(&[(a, 0), (b, 1)]).unwrap(),
            Some(a)
        );
        assert_eq!(
            select_derivation_environment(&[(a, 0), (a, 0)]).unwrap(),
            Some(a)
        );
        assert!(select_derivation_environment(&[(a, 0), (b, 0)]).is_err());
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified isolated database and database creation privileges"]
    async fn niks3_queue_persists_provenance_pins_legacy_and_retains_deleted_identity(
        pool: PgPool,
    ) {
        let output = "/nix/store/local-output";
        let repo = "https://example.com/local-cache-queue.git";
        let hash = "1".repeat(40);
        crate::queries::flakes::insert_flake(
            &pool,
            "local-cache-queue",
            repo,
            "main",
            "all_configs",
        )
        .await
        .unwrap();
        crate::queries::commits::insert_commit_with_metadata(
            &pool,
            &hash,
            repo,
            Utc::now(),
            Some("queue fixture"),
            Some("test"),
        )
        .await
        .unwrap();
        let commit = crate::queries::commits::get_commit_by_hash(&pool, &hash)
            .await
            .unwrap();
        let local = crate::queries::derivations::insert_derivation_with_target(
            &pool,
            Some(&commit),
            "local-queue",
            "nixos",
            Some("local-queue"),
            Some(true),
        )
        .await
        .unwrap();
        let historical = crate::queries::derivations::insert_derivation_with_target(
            &pool,
            Some(&commit),
            "legacy-queue",
            "nixos",
            Some("legacy-queue"),
            Some(true),
        )
        .await
        .unwrap();
        sqlx::query("UPDATE derivations SET store_path = $1, status_id = 10 WHERE id IN ($2, $3)")
            .bind(output)
            .bind(local.id)
            .bind(historical.id)
            .execute(&pool)
            .await
            .unwrap();
        let destination_id: i32 = sqlx::query_scalar(
            "INSERT INTO cache_destinations (name, cache_type, push_to, enabled, niks3_server_url,
                 niks3_public_keys, niks3_write_auth_mode, niks3_auth_token, niks3_read_auth_mode)
             VALUES ('legacy-name', 'Niks3', 'https://shared-read.example', TRUE,
                 'https://original-write.example', ARRAY['cache:key'], 'token', 'test-token', 'none') RETURNING id",
        ).fetch_one(&pool).await.unwrap();
        let no_static = CacheConfig::default();
        let (a, b) = tokio::join!(
            enqueue_cache_push_for_derivation(&pool, local.id, &no_static),
            enqueue_cache_push_for_destination(&pool, local.id, destination_id),
        );
        let id = a.unwrap().unwrap();
        assert_eq!(b.unwrap(), Some(id));
        let mut job = get_cache_push_job_detail(&pool, id).await.unwrap().unwrap();
        assert_eq!(job.cache_destination_id, Some(destination_id));
        assert_eq!(job.cache_destination_source, "database");
        let pending = get_pending_cache_push_jobs(&pool, Some(10)).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].cache_destination_id, Some(destination_id));
        let (claim_a, claim_b) = tokio::join!(
            mark_cache_push_in_progress(&pool, id),
            mark_cache_push_in_progress(&pool, id),
        );
        assert_ne!(claim_a.is_ok(), claim_b.is_ok());
        assert_eq!(
            list_cache_push_jobs(&pool, None, None, None, None)
                .await
                .unwrap()[0]
                .cache_destination_id,
            Some(destination_id)
        );
        sqlx::query("UPDATE cache_push_jobs SET status = 'failed', attempts = 2, retry_after = NOW() + INTERVAL '30 minutes' WHERE id = $1")
            .bind(id).execute(&pool).await.unwrap();
        assert_eq!(
            enqueue_cache_push_for_derivation(&pool, local.id, &no_static)
                .await
                .unwrap(),
            Some(id)
        );
        let unchanged = get_cache_push_job_detail(&pool, id).await.unwrap().unwrap();
        assert_eq!(unchanged.status, "failed");
        assert_eq!(unchanged.attempts, 2);
        sqlx::query(
            "UPDATE cache_destinations SET niks3_auth_token = 'rotated-token' WHERE id = $1",
        )
        .bind(destination_id)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            resolve_cache_push_destination(&pool, &mut job, &local, &no_static)
                .await
                .unwrap()
                .unwrap()
                .niks3_auth_token
                .as_deref(),
            Some("rotated-token")
        );
        sqlx::query("UPDATE cache_destinations SET enabled = FALSE WHERE id = $1")
            .bind(destination_id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            resolve_cache_push_destination(&pool, &mut job, &local, &no_static)
                .await
                .is_err()
        );
        sqlx::query("UPDATE cache_destinations SET enabled = TRUE WHERE id = $1")
            .bind(destination_id)
            .execute(&pool)
            .await
            .unwrap();
        let legacy_id: i32 = sqlx::query_scalar(
            "INSERT INTO cache_push_jobs (derivation_id, store_path, cache_destination, status)
             VALUES ($1, $2, 'legacy-name', 'pending') RETURNING id",
        )
        .bind(historical.id)
        .bind(output)
        .fetch_one(&pool)
        .await
        .unwrap();
        let mut legacy = get_cache_push_job_detail(&pool, legacy_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(legacy.cache_destination_source, "legacy");
        assert_eq!(
            resolve_cache_push_destination(&pool, &mut legacy, &historical, &no_static)
                .await
                .unwrap()
                .unwrap()
                .id,
            destination_id
        );
        let pinned = get_cache_push_job_detail(&pool, legacy_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(pinned.cache_destination_source, "database");
        assert_eq!(pinned.cache_destination_id, Some(destination_id));
        let environment: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO environments (name, description, is_active) VALUES ('unrelated-queue-env', 'test', TRUE) RETURNING id",
        ).fetch_one(&pool).await.unwrap();
        sqlx::query("INSERT INTO cache_destination_environments (cache_destination_id, environment_id) VALUES ($1, $2)")
            .bind(destination_id).bind(environment).execute(&pool).await.unwrap();
        assert!(
            resolve_cache_push_destination(&pool, &mut job, &local, &no_static)
                .await
                .is_err()
        );
        assert!(
            enqueue_cache_push_for_destination(&pool, local.id, destination_id)
                .await
                .is_err()
        );
        sqlx::query("DELETE FROM cache_destinations WHERE id = $1")
            .bind(destination_id)
            .execute(&pool)
            .await
            .unwrap();
        let retained = get_cache_push_job_detail(&pool, id).await.unwrap().unwrap();
        assert_eq!(retained.cache_destination_id, Some(destination_id));
        let static_config = CacheConfig {
            cache_type: crate::config::CacheType::Niks3,
            push_to: Some("https://shared-read.example".into()),
            push_after_build: true,
            niks3_server_url: Some("https://different-write.example".into()),
            ..Default::default()
        };
        assert!(
            resolve_cache_push_destination(&pool, &mut job, &local, &static_config)
                .await
                .is_err()
        );
        assert!(
            resolve_cache_push_destination(&pool, &mut legacy, &historical, &static_config)
                .await
                .is_err()
        );
        assert!(
            enqueue_cache_push_for_derivation(&pool, local.id, &static_config)
                .await
                .is_err()
        );
    }
}
