//! Cache push worker for the builder module.
//!
//! This module handles pushing completed derivations to binary caches,
//! with support for parallel uploads and robust error handling.

use crate::config::{BuildConfig, CacheConfig, CacheType, CrystalForgeConfig};
use crate::log::{WorkerState, WorkerStatus, get_build_status};
use crate::models::cache_destination::CacheDestination;
use crate::queries::cache_destinations::update_cache_destination_last_used;
use crate::queries::cache_push::{
    CachePushJob, cleanup_stale_cache_push_jobs, enqueue_missing_cache_push_jobs,
    get_pending_cache_push_jobs, mark_cache_push_completed, mark_cache_push_failed,
    mark_cache_push_in_progress, resolve_cache_push_destination,
};
use crate::queries::derivations::get_derivation_by_id;
use anyhow::{Context, Result};
use futures::FutureExt;
use sqlx::PgPool;
use tokio::sync::Mutex;
use tokio::time::{Duration, sleep, timeout};
use tracing::{debug, error, info, warn};

/// Convert a database CacheDestination to a CacheConfig
fn cache_destination_to_config(dest: &CacheDestination) -> Result<CacheConfig> {
    if dest.cache_type == "Niks3" {
        let (url, keys, auth) = dest.read_config().map_err(anyhow::Error::msg)?;
        return Ok(CacheConfig {
            cache_type: CacheType::Niks3,
            push_to: Some(url),
            push_after_build: true,
            signing_key: dest.signing_key_path.clone(),
            niks3_server_url: dest.niks3_server_url.clone(),
            niks3_write_auth: Some(dest.niks3_write_auth().map_err(anyhow::Error::msg)?),
            niks3_public_keys: keys,
            niks3_read_auth: auth,
            parallel_uploads: dest.parallel_uploads.unwrap_or(1).max(1) as u32,
            max_retries: dest.max_retries.unwrap_or(3).max(0) as u32,
            retry_delay_seconds: dest.retry_delay_seconds.unwrap_or(5).max(0) as u64,
            push_timeout_seconds: dest.push_timeout_seconds.unwrap_or(3600).max(1) as u64,
            ..CacheConfig::default()
        });
    }
    let cache_type = match dest.cache_type.as_str() {
        "S3" => CacheType::S3,
        "Attic" => CacheType::Attic,
        "Http" => CacheType::Http,
        "Nix" => CacheType::Nix,
        _ => anyhow::bail!("Unknown cache type"),
    };

    Ok(CacheConfig {
        cache_type,
        push_to: dest.push_to.clone(),
        push_after_build: true, // Always true for database-configured caches
        signing_key: dest.signing_key_path.clone(),
        compression: dest.compression.clone(),
        push_filter: None, // Not stored in database (legacy field)
        parallel_uploads: dest.parallel_uploads.unwrap_or(1) as u32,
        s3_region: dest.s3_region.clone(),
        s3_profile: dest.s3_profile.clone(),
        s3_access_key_id: dest.s3_access_key_id.clone(),
        s3_secret_access_key: dest.s3_secret_access_key.clone(),
        s3_session_token: dest.s3_session_token.clone(),
        s3_endpoint_url: dest.s3_endpoint_url.clone(),
        attic_token: dest.attic_token.clone(),
        attic_cache_name: dest.attic_cache_name.clone(),
        attic_public_key: dest.attic_public_key.clone(),
        attic_ignore_upstream_cache_filter: dest.attic_ignore_upstream_cache_filter.unwrap_or(true),
        attic_jobs: dest.attic_jobs.unwrap_or(5) as u32,
        max_retries: dest.max_retries.unwrap_or(3) as u32,
        retry_delay_seconds: dest.retry_delay_seconds.unwrap_or(5) as u64,
        poll_interval: Duration::from_secs(30), // Use default poll interval
        push_timeout_seconds: dest.push_timeout_seconds.unwrap_or(3600) as u64,
        force_repush: dest.force_repush.unwrap_or(false),
        require_sigs: dest.require_sigs.unwrap_or(true),
        ..CacheConfig::default()
    })
}

// CONCURRENCY: All local entry points share one Niks3 execution slot. The CLI
// owns upload concurrency; credentials must be refreshed after waiting here.
static NIKS3_PUSH_GATE: Mutex<()> = Mutex::const_new(());

/// Keeps the execution slot in the process owner through cancellation.
async fn run_owned_push(
    guard: Option<tokio::sync::MutexGuard<'static, ()>>,
    push: impl std::future::Future<Output = Result<()>> + Send + 'static,
) -> Result<()> {
    tokio::spawn(async move {
        let result = push.await;
        drop(guard);
        result
    })
    .await
    .context("join cache push process owner")?
}

/// Loads worker settings without selecting a database publication destination.
async fn load_cache_config(pool: &PgPool) -> Option<CacheConfig> {
    let cfg = CrystalForgeConfig::load().unwrap_or_default();
    let cache_cfg = cfg.get_cache_config().clone();
    match sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM cache_destinations)
             OR EXISTS (SELECT 1 FROM cache_push_jobs WHERE status IN ('pending', 'failed', 'in_progress'))",
    )
        .fetch_one(pool)
        .await
    {
        // Retired database identities still need fail-closed attempt handling
        // after the last destination is deleted and no static cache exists.
        Ok(true) => Some(cache_cfg),
        Ok(false) if cache_cfg.push_to.is_some() => Some(cache_cfg),
        Ok(false) => None,
        Err(_) => {
            warn!("Failed to query cache destinations; cache push disabled");
            None
        }
    }
}

/// Refreshes credentials and eligibility before each queued attempt.
async fn resolve_job_config(
    pool: &PgPool,
    job: &mut CachePushJob,
    derivation: &crate::derivations::Derivation,
    legacy: &CacheConfig,
) -> Result<(CacheConfig, Option<String>)> {
    match resolve_cache_push_destination(pool, job, derivation, legacy).await? {
        Some(destination) => Ok((
            cache_destination_to_config(&destination)?,
            Some(destination.name),
        )),
        None => Ok((legacy.clone(), None)),
    }
}

/// Runs workers that resolve each job's current eligible destination.
///
/// Enqueues missing jobs with explicit database/static provenance through the
/// same environment policy used for execution. Niks3 attempts share one slot,
/// independently of the worker count.
pub async fn run_cache_push_workers(pool: PgPool) {
    let cache_cfg = match load_cache_config(&pool).await {
        Some(config) => config,
        None => {
            info!("📤 Cache push disabled (no destination configured)");
            return;
        }
    };

    let cfg = CrystalForgeConfig::load().unwrap_or_default();
    let build_cfg = cfg.get_build_config();
    // PERFORMANCE: Niks3 manages parallel uploads inside one CLI process.
    let worker_count = if cache_cfg.cache_type == CacheType::Niks3 {
        1
    } else {
        cache_cfg.parallel_uploads.max(1) as usize
    };

    info!("🚚 starting {} cache-push worker(s)…", worker_count);

    // (Optional) one tiny background task to reclaim stuck jobs
    {
        let pool = pool.clone();
        tokio::spawn(async move {
            loop {
                if let Err(e) = cleanup_stale_cache_push_jobs(&pool, 60).await {
                    warn!("cleanup_stale_cache_push_jobs: {e:#}");
                }
                sleep(Duration::from_secs(30)).await;
            }
        });
    }
    {
        let pool = pool.clone();
        let static_config = cache_cfg.clone();
        tokio::spawn(async move {
            info!("📤 Starting cache job creation loop (every 30s)...");
            loop {
                let queued = enqueue_missing_cache_push_jobs(&pool, &static_config).await;
                match queued {
                    Ok(count) if count > 0 => {
                        info!("📤 Created {} new cache push jobs", count);
                    }
                    Ok(_) => {
                        debug!("No new cache push jobs needed");
                    }
                    Err(e) => {
                        warn!("Failed to batch queue cache jobs: {}", e);
                    }
                }
                sleep(Duration::from_secs(30)).await;
            }
        });
    }

    let mut handles = Vec::with_capacity(worker_count);
    for worker_id in 0..worker_count {
        let pool = pool.clone();
        let cache_cfg = cache_cfg.clone();
        let build_cfg = build_cfg.clone();

        // Pre-register worker status (reuse build status list, or make a dedicated one)
        {
            let mut statuses = get_build_status().write().await;
            statuses.push(WorkerStatus {
                worker_id: 10_000 + worker_id, // offset so they don't collide with build workers
                current_task: None,
                started_at: None,
                state: WorkerState::Idle,
            });
        }

        handles.push(tokio::spawn(async move {
            cache_worker(worker_id, pool, cache_cfg, build_cfg).await;
        }));
    }

    for h in handles {
        let _ = h.await;
    }
}

/// Runs periodic workers with per-attempt destination and credential resolution.
///
/// Niks3 attempts share one execution slot; the CLI owns upload concurrency.
pub async fn run_cache_push_loop(pool: PgPool) {
    let cache_cfg = match load_cache_config(&pool).await {
        Some(config) => config,
        None => {
            info!("📤 Cache push disabled (no destination configured)");
            return;
        }
    };

    let worker_count = match cache_cfg.cache_type {
        CacheType::S3 => cache_cfg.parallel_uploads.max(1) as usize,
        CacheType::Attic | CacheType::Niks3 => 1,
        CacheType::Http | CacheType::Nix => cache_cfg.parallel_uploads.max(1) as usize,
    };

    let cfg = CrystalForgeConfig::load().unwrap_or_default();
    let build_cfg = cfg.get_build_config();

    info!("🚚 starting {} cache-push worker(s)…", worker_count);

    // (Optional) one tiny background task to reclaim stuck jobs
    {
        let pool = pool.clone();
        tokio::spawn(async move {
            loop {
                if let Err(e) = cleanup_stale_cache_push_jobs(&pool, 60).await {
                    warn!("cleanup_stale_cache_push_jobs: {e:#}");
                }
                sleep(Duration::from_secs(30)).await;
            }
        });
    }

    let mut handles = Vec::with_capacity(worker_count);
    for worker_id in 0..worker_count {
        let pool = pool.clone();
        let cache_cfg = cache_cfg.clone();
        let build_cfg = build_cfg.clone();

        // Pre-register worker status (reuse build status list, or make a dedicated one)
        {
            let mut statuses = get_build_status().write().await;
            statuses.push(WorkerStatus {
                worker_id: 10_000 + worker_id, // offset so they don't collide with build workers
                current_task: None,
                started_at: None,
                state: WorkerState::Idle,
            });
        }

        handles.push(tokio::spawn(async move {
            cache_worker(worker_id, pool, cache_cfg, build_cfg).await;
        }));
    }

    for h in handles {
        let _ = h.await;
    }
}

async fn cache_worker(
    worker_id: usize,
    pool: PgPool,
    cache_cfg: CacheConfig,
    build_cfg: BuildConfig,
) {
    let status_id = 10_000 + worker_id;
    let tick = cache_cfg.poll_interval;

    info!("🚚 cache-worker {worker_id} started (tick {tick:?})");

    loop {
        // update status: looking for work
        {
            let mut s = get_build_status().write().await;
            if let Some(ws) = s.iter_mut().find(|w| w.worker_id == status_id) {
                ws.state = WorkerState::Working;
                ws.current_task = Some("claiming cache job".into());
                ws.started_at = Some(std::time::Instant::now());
            }
        }

        // small DB timeout so a wedged DB doesn't pin the worker forever
        let jobs = match timeout(
            Duration::from_secs(30),
            get_pending_cache_push_jobs(&pool, Some(1)),
        )
        .await
        {
            Ok(Ok(mut v)) => v.pop(),
            Ok(Err(e)) => {
                error!("cache-worker {worker_id}: get_pending_cache_push_jobs failed: {e:#}");
                None
            }
            Err(_) => {
                error!("cache-worker {worker_id}: get_pending_cache_push_jobs timed out");
                None
            }
        };

        let Some(job) = jobs else {
            // no work → idle + sleep
            {
                let mut s = get_build_status().write().await;
                if let Some(ws) = s.iter_mut().find(|w| w.worker_id == status_id) {
                    ws.state = WorkerState::Idle;
                    ws.current_task = None;
                    ws.started_at = None;
                }
            }
            debug!("cache-worker {worker_id}: idle");
            sleep(tick).await;
            continue;
        };

        // mark job in-progress and do the push
        if let Err(e) = mark_cache_push_in_progress(&pool, job.id).await {
            warn!("cache-worker {worker_id}: failed to mark in-progress: {e:#}");
            // brief backoff; another worker can pick it up later
            sleep(Duration::from_secs(2)).await;
            continue;
        }

        if let Err(e) =
            process_one_job(&pool, &cache_cfg, &build_cfg, job, worker_id, status_id).await
        {
            error!("cache-worker {worker_id}: job failed: {e:#}");
        }
    }
}

async fn process_one_job(
    pool: &PgPool,
    cache_cfg: &CacheConfig,
    build_cfg: &BuildConfig,
    mut job: CachePushJob,
    worker_id: usize,
    status_id: usize,
) -> Result<()> {
    // update status for visibility
    {
        let mut s = get_build_status().write().await;
        if let Some(ws) = s.iter_mut().find(|w| w.worker_id == status_id) {
            ws.state = WorkerState::Working;
            ws.current_task = Some(format!(
                "cache-pushing job#{} (derivation @ {})",
                job.id,
                job.store_path
                    .as_deref()
                    .unwrap_or(&job.derivation_id.to_string())
            ));
            ws.started_at = Some(std::time::Instant::now());
        }
    }

    let derivation = get_derivation_by_id(pool, job.derivation_id)
        .await
        .context("fetch derivation")?;

    let (mut current_config, mut destination_name) =
        match resolve_job_config(pool, &mut job, &derivation, cache_cfg).await {
            Ok(config) => config,
            Err(_) => {
                mark_cache_push_failed(pool, job.id, "Cache destination resolution failed").await?;
                return Ok(());
            }
        };
    let mut niks3_guard = None;
    if current_config.cache_type == CacheType::Niks3 {
        niks3_guard = Some(NIKS3_PUSH_GATE.lock().await);
        // Waiting workers must not use credentials or eligibility from before
        // the previous CLI attempt. Failed refresh cannot fall back to legacy.
        match resolve_job_config(pool, &mut job, &derivation, cache_cfg).await {
            Ok((config, name)) => {
                current_config = config;
                destination_name = name;
            }
            Err(_) => {
                mark_cache_push_failed(pool, job.id, "Cache destination refresh failed").await?;
                return Ok(());
            }
        }
    }

    // Prefer job.store_path; else fall back to derivation.store_path / derivation_path (your push method handles .drv → store resolution)
    let path = job
        .store_path
        .or_else(|| derivation.store_path.clone())
        .or_else(|| derivation.derivation_path.clone())
        .ok_or_else(|| anyhow::anyhow!("no store/derivation path for {}", job.derivation_id))?;

    // Fast path check if it looks like a nix store path and actually exists
    if path.starts_with("/nix/store/") && !tokio::fs::try_exists(&path).await.unwrap_or(false) {
        warn!("cache-worker {worker_id}: store path missing: {path}");
        mark_cache_push_failed(pool, job.id, &format!("Store path missing: {path}")).await?;
        return Ok(());
    }

    // Retry through the queue, not an in-process loop with stale credentials.
    // CONCURRENCY: The process owner retains the Niks3 slot through child exit
    // even if this worker is cancelled while awaiting the detached push.
    let derivation_name = derivation.derivation_name.clone();
    let build_cfg = build_cfg.clone();
    let started = std::time::Instant::now();
    let push = run_owned_push(niks3_guard, async move {
        derivation
            .push_to_cache(&path, &current_config, &build_cfg)
            .await
    })
    .await;
    match push {
        Ok(()) => {
            let duration_ms = (started.elapsed().as_millis() as i32).max(0);
            mark_cache_push_completed(pool, job.id, None, Some(duration_ms)).await?;

            // Update last_used_at for the cache destination if using database config
            if let Some(dest_name) = destination_name.as_deref() {
                if let Err(e) = update_cache_destination_last_used(pool, dest_name).await {
                    warn!(
                        "Failed to update last_used_at for cache destination {}: {:#}",
                        dest_name, e
                    );
                }
            }

            info!(
                "✅ cache-worker {worker_id}: pushed {} (job {})",
                derivation_name, job.id
            );
        }
        Err(e) => {
            mark_cache_push_failed(pool, job.id, &e.to_string()).await?;
            warn!(
                "❌ cache-worker {worker_id}: push failed for {} (job {}): {e}",
                derivation_name, job.id
            );
        }
    }

    // back to idle; the outer loop will look for more work
    {
        let mut s = get_build_status().write().await;
        if let Some(ws) = s.iter_mut().find(|w| w.worker_id == status_id) {
            ws.state = WorkerState::Idle;
            ws.current_task = None;
            ws.started_at = None;
        }
    }

    Ok(())
}

/// Wrapper around process_cache_pushes that ensures errors don't propagate
async fn process_cache_pushes_safe(
    pool: &PgPool,
    cache_config: &CacheConfig,
    build_config: &BuildConfig,
) -> Result<usize> {
    let result =
        std::panic::AssertUnwindSafe(process_cache_pushes(pool, cache_config, build_config))
            .catch_unwind()
            .await;

    match result {
        Ok(res) => res,
        Err(_) => {
            error!("💥 Cache push process panicked! Recovering...");
            Err(anyhow::anyhow!("Cache push process panicked"))
        }
    }
}

/// Processes pending cache jobs with cache-specific bounded concurrency.
///
/// Returns the number of selected jobs, not the number of successful uploads.
/// Query failures and query timeouts return zero. Upload failures are recorded
/// on individual jobs. Each attempt resolves current destination settings.
/// Niks3 attempts share one execution slot across worker and batch entry points.
///
/// # Errors
/// Returns an error if batch coordination fails. Individual upload and
/// destination-resolution failures are recorded on their jobs.
pub async fn process_cache_pushes(
    pool: &PgPool,
    cache_config: &CacheConfig,
    build_config: &BuildConfig,
) -> Result<usize> {
    let db_timeout = std::time::Duration::from_secs(30);

    // Always try to cleanup stale jobs first
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        cleanup_stale_cache_push_jobs(pool, 5),
    )
    .await;

    // Get pending jobs (up to 5 at a time for batching)
    let jobs_result =
        tokio::time::timeout(db_timeout, get_pending_cache_push_jobs(pool, Some(5))).await;

    match jobs_result {
        Ok(Ok(jobs)) if !jobs.is_empty() => {
            let job_count = jobs.len();
            if let Err(e) = process_batch_cache_push(pool, jobs, cache_config, build_config).await {
                error!("❌ Failed to process batch cache push: {}", e);
            }
            Ok(job_count)
        }
        Ok(Err(e)) => {
            error!("❌ Failed to get pending cache push jobs: {e}");
            Ok(0)
        }
        Err(_) => {
            error!("⏱️ Timeout getting pending cache push jobs");
            Ok(0)
        }
        _ => Ok(0),
    }
}

async fn process_batch_cache_push(
    pool: &PgPool,
    jobs: Vec<crate::queries::cache_push::CachePushJob>,
    cache_config: &CacheConfig,
    build_config: &BuildConfig,
) -> Result<()> {
    if jobs.is_empty() {
        return Ok(());
    }

    info!("📤 Processing {} cache push jobs (parallel)", jobs.len());

    // Process jobs with cache-specific concurrency.
    let mut tasks = Vec::new();
    // PERFORMANCE: Keep Niks3 concurrency inside its single CLI process.
    let job_concurrency = if cache_config.cache_type == CacheType::Niks3 {
        1
    } else {
        3
    };

    for job in jobs {
        let pool = pool.clone();
        let cache_config = cache_config.clone();
        let build_config = build_config.clone();

        let task = tokio::spawn(async move {
            if mark_cache_push_in_progress(&pool, job.id).await.is_err() {
                return;
            }
            let job_id = job.id;
            // Batch jobs use exactly the same identity, eligibility, refresh,
            // queue retry, and Niks3 process ownership as normal workers.
            if let Err(e) = process_one_job(
                &pool,
                &cache_config,
                &build_config,
                job,
                usize::MAX,
                usize::MAX,
            )
            .await
            {
                let _ = mark_cache_push_failed(&pool, job_id, "Cache push attempt failed").await;
                error!("Failed to process cache push job {job_id}: {e}");
            }
        });

        tasks.push(task);

        // Wait when the cache-specific job limit is reached.
        if tasks.len() >= job_concurrency {
            if let Some(task) = tasks.pop() {
                let _ = task.await;
            }
        }
    }

    // Wait for remaining tasks
    for task in tasks {
        let _ = task.await;
    }

    Ok(())
}

#[cfg(test)]
mod niks3_tests {
    use super::*;

    #[tokio::test]
    async fn niks3_execution_slot_survives_cancelled_waiter_until_cli_exit() {
        let guard = NIKS3_PUSH_GATE.lock().await;
        let (started, ready) = tokio::sync::oneshot::channel();
        let (finished, exited) = tokio::sync::oneshot::channel();
        let (release, released) = tokio::sync::oneshot::channel();
        let waiter = tokio::spawn(run_owned_push(Some(guard), async move {
            let mut child = tokio::process::Command::new("sh")
                .args(["-c", "read value || exit 0"])
                .stdin(std::process::Stdio::piped())
                .spawn()?;
            let _ = started.send(());
            released.await?;
            drop(child.stdin.take());
            let status = child.wait().await?;
            anyhow::ensure!(status.success(), "fake CLI failed");
            let _ = finished.send(());
            Ok(())
        }));
        ready.await.unwrap();
        waiter.abort();
        assert!(
            timeout(Duration::from_millis(30), NIKS3_PUSH_GATE.lock())
                .await
                .is_err()
        );
        release.send(()).unwrap();
        exited.await.unwrap();
        let _guard = timeout(Duration::from_secs(2), NIKS3_PUSH_GATE.lock())
            .await
            .unwrap();
    }

    #[test]
    fn niks3_local_mapping_is_explicit_and_excludes_other_cache_secrets() {
        use crate::models::cache_destination::nix_public_key_fixture;
        let mut destination = CacheDestination {
            cache_type: "Niks3".into(),
            push_to: Some("https://read.example".into()),
            niks3_server_url: Some("https://write.example".into()),
            niks3_public_keys: vec![nix_public_key_fixture("one"), nix_public_key_fixture("two")],
            niks3_write_auth_mode: Some("token".into()),
            niks3_auth_token: Some("write-token".into()),
            niks3_read_auth_mode: Some("none".into()),
            s3_secret_access_key: Some("aws-secret".into()),
            attic_token: Some("attic-token".into()),
            ..Default::default()
        };
        let config = cache_destination_to_config(&destination).unwrap();
        assert_eq!(config.cache_type, CacheType::Niks3);
        assert_eq!(
            config.niks3_server_url.as_deref(),
            Some("https://write.example")
        );
        assert_eq!(config.niks3_public_keys, destination.niks3_public_keys);
        assert!(config.niks3_write_auth.is_some());
        assert!(config.s3_secret_access_key.is_none());
        assert!(config.attic_token.is_none());
        destination.niks3_auth_token = None;
        assert!(cache_destination_to_config(&destination).is_err());
        destination.cache_type = "unknown".into();
        assert!(cache_destination_to_config(&destination).is_err());
    }
}
