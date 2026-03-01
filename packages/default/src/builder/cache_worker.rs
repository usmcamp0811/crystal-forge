//! Cache push worker for the builder module.
//!
//! This module handles pushing completed derivations to binary caches,
//! with support for parallel uploads and robust error handling.

use crate::config::{BuildConfig, CacheConfig, CacheType, CrystalForgeConfig};
use crate::log::{WorkerState, WorkerStatus, get_build_status};
use crate::queries::cache_push::{
    CachePushJob, cleanup_stale_cache_push_jobs, get_pending_cache_push_jobs,
    mark_cache_push_completed, mark_cache_push_failed, mark_cache_push_in_progress,
};
use crate::queries::derivations::{batch_queue_cache_jobs, get_derivation_by_id};
use anyhow::{Context, Result};
use futures::FutureExt;
use sqlx::PgPool;
use tokio::time::{Duration, sleep, timeout};
use tracing::{debug, error, info, warn};

/// Runs cache push workers with parallel uploads and job creation
pub async fn run_cache_push_workers(pool: PgPool) {
    let cfg = CrystalForgeConfig::load().unwrap_or_default();
    let cache_cfg = cfg.get_cache_config();

    if cache_cfg.push_to.is_none() {
        info!("📤 Cache push disabled (no destination configured)");
        return;
    }

    let build_cfg = cfg.get_build_config();
    let worker_count = cache_cfg.parallel_uploads.max(1) as usize;

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
        let destination = cache_cfg.push_to.clone().unwrap(); // Safe because we checked above
        tokio::spawn(async move {
            info!("📤 Starting cache job creation loop (every 30s)...");
            loop {
                match batch_queue_cache_jobs(&pool, &destination).await {
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

/// Runs the periodic cache push loop with robust error handling
pub async fn run_cache_push_loop(pool: PgPool) {
    let cfg = CrystalForgeConfig::load().unwrap_or_default();
    let cache_cfg = cfg.get_cache_config();

    if cache_cfg.push_to.is_none() {
        info!("📤 Cache push disabled (no destination configured)");
        return;
    }

    let worker_count = match cache_cfg.cache_type {
        CacheType::S3 => cache_cfg.parallel_uploads.max(1) as usize,
        CacheType::Attic => 1,
        CacheType::Http | CacheType::Nix => cache_cfg.parallel_uploads.max(1) as usize,
    };

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
    job: CachePushJob,
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

    // Do the push using your existing implementation on Derivation
    let started = std::time::Instant::now();
    match derivation.push_to_cache(&path, cache_cfg, build_cfg).await {
        Ok(()) => {
            let duration_ms = (started.elapsed().as_millis() as i32).max(0);
            mark_cache_push_completed(pool, job.id, None, Some(duration_ms)).await?;
            info!(
                "✅ cache-worker {worker_id}: pushed {} (job {})",
                derivation.derivation_name, job.id
            );
        }
        Err(e) => {
            mark_cache_push_failed(pool, job.id, &e.to_string()).await?;
            warn!(
                "❌ cache-worker {worker_id}: push failed for {} (job {}): {e}",
                derivation.derivation_name, job.id
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

/// Process cache pushes for completed builds (one at a time to avoid batching issues)
pub async fn process_cache_pushes(
    pool: &PgPool,
    cache_config: &CacheConfig,
    build_config: &BuildConfig,
) -> Result<usize> {
    // ← Changed from Result<()> to Result<usize>
    let Some(destination) = cache_config.push_to.as_deref() else {
        debug!("⭐️ No cache destination configured, skipping cache push");
        return Ok(0); // ← Changed from Ok(()) to Ok(0)
    };

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

    // Process up to 3 jobs concurrently
    let mut tasks = Vec::new();

    for job in jobs {
        let pool = pool.clone();
        let cache_config = cache_config.clone();
        let build_config = build_config.clone();

        let task = tokio::spawn(async move {
            if let Some(store_path) = job.store_path {
                // Check if path exists
                if !tokio::fs::try_exists(&store_path).await.unwrap_or(false) {
                    warn!("❌ Store path doesn't exist: {}", store_path);
                    let _ = mark_cache_push_failed(
                        &pool,
                        job.id,
                        &format!("Store path does not exist: {}", store_path),
                    )
                    .await;
                    return;
                }

                // Mark in-progress
                if mark_cache_push_in_progress(&pool, job.id).await.is_err() {
                    return;
                }

                // Get derivation
                let derivation = match crate::queries::derivations::get_derivation_by_id(
                    &pool,
                    job.derivation_id,
                )
                .await
                {
                    Ok(d) => d,
                    Err(e) => {
                        let _ = mark_cache_push_failed(&pool, job.id, &e.to_string()).await;
                        return;
                    }
                };

                // Push with retry
                let start = std::time::Instant::now();
                match derivation
                    .push_to_cache_with_retry(&store_path, &cache_config, &build_config)
                    .await
                {
                    Ok(()) => {
                        let duration_ms = start.elapsed().as_millis() as i32;
                        let _ =
                            mark_cache_push_completed(&pool, job.id, None, Some(duration_ms)).await;
                        info!("✅ Pushed {} (job {})", derivation.derivation_name, job.id);
                    }
                    Err(e) => {
                        let _ = mark_cache_push_failed(&pool, job.id, &e.to_string()).await;
                        error!("❌ Failed to push job {}: {}", job.id, e);
                    }
                }
            }
        });

        tasks.push(task);

        // Limit concurrency - wait if we have 3 running
        if tasks.len() >= 3 {
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
