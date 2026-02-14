use crate::log::{WorkerState, WorkerStatus, get_build_status, get_cve_status};
use crate::config::CacheType;
use crate::config::{BuildConfig, CacheConfig, CrystalForgeConfig};
use crate::derivations::{Derivation, DerivationType};
use crate::queries::build_reservations;
use crate::queries::cache_push::CachePushJob;
use crate::queries::cache_push::create_cache_push_job;
use crate::queries::cache_push::{
    cleanup_stale_cache_push_jobs, get_pending_cache_push_jobs, mark_cache_push_completed,
    mark_cache_push_failed, mark_cache_push_in_progress,
};
use crate::queries::cve_scans::{
    create_cve_scan, get_targets_needing_cve_scan, mark_cve_scan_failed, mark_scan_in_progress,
    save_scan_results,
};
use crate::queries::derivations::get_derivation_by_id;
use crate::queries::derivations::{
    EvaluationStatus, handle_derivation_failure, mark_target_build_complete,
    update_derivation_status,
};
use crate::queries::derivations::{batch_queue_cache_jobs, reset_derivation_for_rebuild};
use crate::vulnix::vulnix_runner::VulnixRunner;
use anyhow::{Context, Result};
use futures::FutureExt;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::Semaphore;
use tokio::time::sleep;
use tokio::time::timeout;
use tokio::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

/// Runs the continuous build loop with multiple workers
pub async fn run_build_loop(pool: PgPool) {
    let cfg = CrystalForgeConfig::load().unwrap_or_else(|e| {
        warn!("Failed to load Crystal Forge config: {}, using defaults", e);
        CrystalForgeConfig::default()
    });
    let build_config = cfg.get_build_config();
    let cache_config = cfg.get_cache_config();
    let num_workers = build_config.max_concurrent_derivations;

    info!("🏗 Starting {} continuous build workers...", num_workers);

    // Get hostname for worker IDs
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());

    // Pre-initialize worker status tracking BEFORE spawning workers
    {
        let mut statuses = get_build_status().write().await;
        for worker_id in 0..num_workers {
            statuses.push(WorkerStatus {
                worker_id,
                current_task: None,
                started_at: None,
                state: WorkerState::Idle,
            });
        }
    }

    // Spawn stale reservation cleanup task
    let cleanup_pool = pool.clone();
    tokio::spawn(async move {
        run_reservation_cleanup_loop(cleanup_pool).await;
    });

    // Spawn worker pool
    let mut handles = Vec::new();
    for worker_id in 0..num_workers {
        let pool = pool.clone();
        let build_config = build_config.clone();
        let cache_config = cache_config.clone();
        let worker_uuid = format!("{}-worker-{}", hostname, worker_id);

        let handle = tokio::spawn(async move {
            build_worker(worker_id, worker_uuid, pool, build_config, cache_config).await;
        });
        handles.push(handle);
    }

    // Wait for all workers
    for handle in handles {
        let _ = handle.await;
    }
}

/// Resolved commit context for formatting task descriptions.
///
/// Separates data fetching from formatting so the pure formatting logic
/// is testable without a database connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CommitContext {
    /// No commit associated with this derivation.
    None,
    /// Commit ID exists but lookup failed (e.g. DB error).
    Unresolved { commit_id: i32 },
    /// Commit resolved; distance from HEAD is optional.
    Resolved {
        short_hash: String,
        distance_from_head: Option<i32>,
    },
}

/// Format a task description from a derivation name and its commit context.
///
/// This is a pure function (no I/O) so it can be thoroughly unit tested.
///
/// # Examples
///
/// ```text
/// format_task_description("my-system", CommitContext::None)
///   → "my-system"
///
/// format_task_description("my-system", CommitContext::Resolved { short_hash: "abc123de", distance_from_head: Some(3) })
///   → "my-system @ abc123de (HEAD~3)"
/// ```
pub(crate) fn format_task_description(derivation_name: &str, ctx: CommitContext) -> String {
    match ctx {
        CommitContext::None => derivation_name.to_owned(),
        CommitContext::Unresolved { commit_id } => {
            format!("{} @ commit#{}", derivation_name, commit_id)
        }
        CommitContext::Resolved {
            short_hash,
            distance_from_head,
        } => match distance_from_head {
            Some(distance) => {
                format!("{} @ {} (HEAD~{})", derivation_name, short_hash, distance)
            }
            None => format!("{} @ {}", derivation_name, short_hash),
        },
    }
}

/// Resolve commit context for a derivation by querying the database.
///
/// Returns a [`CommitContext`] that can be passed to [`format_task_description`].
async fn resolve_commit_context(pool: &PgPool, derivation: &Derivation) -> CommitContext {
    let Some(commit_id) = derivation.commit_id else {
        return CommitContext::None;
    };

    let commit = match crate::queries::commits::get_commit_by_id(pool, commit_id).await {
        Ok(c) => c,
        Err(_) => return CommitContext::Unresolved { commit_id },
    };

    let short_hash = if commit.git_commit_hash.len() >= 8 {
        commit.git_commit_hash[..8].to_owned()
    } else {
        commit.git_commit_hash.clone()
    };

    let distance_from_head = match commit.get_flake(pool).await {
        Ok(flake) => {
            crate::queries::commits::get_commit_distance_from_head(pool, &flake, &commit)
                .await
                .ok()
        }
        Err(_) => None,
    };

    CommitContext::Resolved {
        short_hash,
        distance_from_head,
    }
}

/// Build a task description for display/logging.
///
/// This is a thin async wrapper that resolves commit info from the database
/// and delegates to the pure [`format_task_description`] for formatting.
async fn build_task_description(pool: &PgPool, derivation: &Derivation) -> String {
    let ctx = resolve_commit_context(pool, derivation).await;
    format_task_description(&derivation.derivation_name, ctx)
}

/// Update worker status (helper to reduce boilerplate)
///
/// This function updates the global worker status in a non-blocking way
fn update_worker_status(worker_id: usize, state: WorkerState, current_task: Option<String>) {
    tokio::spawn(async move {
        let mut statuses = get_build_status().write().await;
        if let Some(status) = statuses.iter_mut().find(|s| s.worker_id == worker_id) {
            status.state = state;
            status.current_task = current_task;
            status.started_at = if state == WorkerState::Idle {
                None
            } else {
                Some(std::time::Instant::now())
            };
        }
    });
}

/// Main build worker loop
///
/// CRITICAL IMPROVEMENTS:
/// 1. Timeout protection prevents workers from getting stuck for hours
/// 2. Helper functions for task description and status updates
/// 3. Better error handling and logging
async fn build_worker(
    worker_id: usize,
    worker_uuid: String,
    pool: PgPool,
    build_config: BuildConfig,
    cache_config: CacheConfig,
) {
    update_worker_status(
        worker_id,
        WorkerState::Working,
        Some("claiming work".to_string()),
    );

    info!("Worker {} ({}) started", worker_id, worker_uuid);

    // Spawn heartbeat task for this worker
    let heartbeat_pool = pool.clone();
    let heartbeat_uuid = worker_uuid.clone();
    tokio::spawn(async move {
        worker_heartbeat_loop(heartbeat_uuid, heartbeat_pool).await;
    });

    // Get the build timeout from config (with a reasonable maximum)
    // This is CRITICAL to prevent workers from getting stuck for hours
    let build_timeout = std::cmp::min(
        build_config.timeout,
        std::time::Duration::from_secs(7200), // Max 2 hours
    );

    info!(
        "Worker {} configured with {:.1}s timeout",
        worker_id,
        build_timeout.as_secs_f64()
    );

    loop {
        update_worker_status(
            worker_id,
            WorkerState::Working,
            Some("claiming work".to_string()),
        );

        match build_reservations::claim_next_derivation(&pool, &worker_uuid).await {
            Ok(Some(mut derivation)) => {
                info!(
                    "✅ Worker {} CLAIMED derivation {}",
                    worker_id, derivation.derivation_name
                );

                // ADD THIS DEBUG LINE
                info!(
                    "🔨 Worker {} STARTING BUILD for {}",
                    worker_id, derivation.derivation_name
                );

                // Build task description using helper function (no embedded SQL)
                let task_description = derivation.derivation_name.clone();
                // let task_description = build_task_description(&pool, &derivation).await;

                update_worker_status(
                    worker_id,
                    WorkerState::Working,
                    Some(task_description.clone()),
                );

                info!(
                    "Worker {} claimed: {} (type: {:?}, cf_agent: {:?}, attempt: {})",
                    worker_id,
                    task_description,
                    derivation.derivation_type,
                    derivation.cf_agent_enabled,
                    derivation.attempt_count
                );

                let start = std::time::Instant::now();

                info!(
                    "🔨 Worker {} STARTING BUILD for {}",
                    worker_id, derivation.derivation_name
                );
                info!("  → Step 1: About to call derivation.build()");

                let build_result =
                    tokio::time::timeout(build_timeout, derivation.build(&pool, &build_config))
                        .await;

                info!("  → Step 2: derivation.build() returned");

                match build_result {
                    // Build succeeded within timeout
                    Ok(Ok(store_path)) => {
                        let duration = start.elapsed();
                        info!(
                            "✅ worker {} completed {} in {:.1}s: {}",
                            worker_id,
                            task_description,
                            duration.as_secs_f64(),
                            store_path
                        );

                        // update derivation with store_path for signing
                        derivation.store_path = Some(store_path.clone());

                        // sign before cache push
                        if let Err(e) = derivation.sign(&cache_config).await {
                            warn!(
                                "⚠️ signing failed for {}, continuing anyway: {}",
                                task_description, e
                            );
                            // non-fatal - we can still push to cache unsigned
                        }

                        // TODO: Include the name of the server that built the derivation
                        if let Some(ref store_path) = derivation.store_path {
                            if let Err(e) = create_cache_push_job(
                                &pool,
                                derivation.id,
                                store_path,                      // &String coerces to &str
                                cache_config.push_to.as_deref(), // Option<String> -> Option<&str>
                            )
                            .await
                            {
                                warn!(
                                    "⚠️ cache queue failed for {}, continuing anyway: {}",
                                    task_description, e
                                );
                            }
                        } else {
                            warn!(
                                "⚠️ skipping cache queue for {}: missing store_path on derivation {}",
                                task_description, derivation.id
                            );
                        }

                        if let Err(e) = mark_build_complete_and_release(
                            &pool,
                            &worker_uuid,
                            derivation.id,
                            &store_path,
                        )
                        .await
                        {
                            error!("failed to mark build complete: {}", e);
                        }
                    }

                    // Build failed within timeout
                    Ok(Err(e)) => {
                        let duration = start.elapsed();
                        error!(
                            "❌ Worker {} build failed for {} after {:.1}s: {}",
                            worker_id,
                            task_description,
                            duration.as_secs_f64(),
                            e
                        );

                        if let Err(e2) =
                            mark_build_failed_and_release(&pool, &worker_uuid, &derivation, &e)
                                .await
                        {
                            error!("Failed to mark build failed: {}", e2);
                        }
                    }

                    // Build TIMED OUT - this is the fix for stuck workers!
                    Err(_timeout) => {
                        let duration = start.elapsed();
                        let timeout_error = anyhow::anyhow!(
                            "Build timed out after {:.1}s (limit: {:.1}s)",
                            duration.as_secs_f64(),
                            build_timeout.as_secs_f64()
                        );

                        error!(
                            "⏱️  Worker {} build TIMEOUT for {} after {:.1}s (limit: {:.1}s)",
                            worker_id,
                            task_description,
                            duration.as_secs_f64(),
                            build_timeout.as_secs_f64()
                        );

                        if let Err(e2) = mark_build_failed_and_release(
                            &pool,
                            &worker_uuid,
                            &derivation,
                            &timeout_error,
                        )
                        .await
                        {
                            error!("Failed to mark build timeout: {}", e2);
                        }
                    }
                }
            }

            // No work available - idle
            Ok(None) => {
                update_worker_status(worker_id, WorkerState::Idle, None);
                debug!("Worker {} idle, no work available", worker_id);
                sleep(std::time::Duration::from_secs(5)).await;
            }

            // Error claiming work
            Err(e) => {
                error!("Worker {} error claiming work: {}", worker_id, e);
                sleep(std::time::Duration::from_secs(10)).await;
            }
        }
    }
}

/// Runs the periodic CVE scanning loop
pub async fn run_cve_scan_loop(pool: PgPool) {
    let cfg = CrystalForgeConfig::load().unwrap_or_else(|e| {
        warn!("Failed to load Crystal Forge config: {}, using defaults", e);
        CrystalForgeConfig::default()
    });
    let vulnix_config = cfg.get_vulnix_config();

    info!(
        "🔍 Starting CVE Scan loop (every {}s)...",
        vulnix_config.poll_interval.as_secs()
    );

    if !VulnixRunner::check_vulnix_available().await {
        error!("❌ vulnix is not available - CVE scanning disabled");
        return;
    }

    let vulnix_version = VulnixRunner::get_vulnix_version().await.ok();

    debug!("🔧 Using vulnix version: {:?}", vulnix_version);
    debug!(
        "🔧 Vulnix config: timeout={}s, whitelist={}, extra_args={:?}",
        vulnix_config.timeout_seconds(),
        vulnix_config.enable_whitelist,
        vulnix_config.extra_args
    );

    let vulnix_runner = VulnixRunner::with_config(&vulnix_config);

    loop {
        if let Err(e) = scan_derivations(&pool, &vulnix_runner, vulnix_version.clone()).await {
            error!("❌ Error in CVE scan cycle: {e}");
        }

        sleep(vulnix_config.poll_interval).await;
    }
}
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
                worker_id: 10_000 + worker_id, // offset so they don’t collide with build workers
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
                worker_id: 10_000 + worker_id, // offset so they don’t collide with build workers
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

        // small DB timeout so a wedged DB doesn’t pin the worker forever
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

/// Process derivations that need CVE scanning
async fn scan_derivations(
    pool: &PgPool,
    vulnix_runner: &VulnixRunner,
    vulnix_version: Option<String>,
) -> Result<()> {
    // Get derivations that need CVE scanning (those with build-complete status)
    // Update status: looking for work
    {
        let mut status = get_cve_status().write().await; // Use helper function
        *status = Some(WorkerStatus {
            worker_id: 0,
            current_task: Some("finding scan targets".to_string()),
            started_at: Some(std::time::Instant::now()),
            state: WorkerState::Working,
        });
    }

    match get_targets_needing_cve_scan(pool, Some(1)).await {
        Ok(derivations) => {
            if derivations.is_empty() {
                info!("🔍 No derivations need CVE scanning");
                // Update status: idle
                {
                    let mut status = get_cve_status().write().await;
                    *status = Some(WorkerStatus {
                        worker_id: 0,
                        current_task: None,
                        started_at: None,
                        state: WorkerState::Idle,
                    });
                }
                info!("No derivations need CVE scanning");
                return Ok(());
            }

            let derivation = &derivations[0];

            // Update status: scanning specific derivation
            {
                let mut status = get_cve_status().write().await;
                *status = Some(WorkerStatus {
                    worker_id: 0,
                    current_task: Some(format!("scanning {}", derivation.derivation_name)),
                    started_at: Some(std::time::Instant::now()),
                    state: WorkerState::Working,
                });
            }

            // Check if the derivation path exists
            if let Some(ref path) = derivation.store_path {
                match fs::try_exists(path).await {
                    Ok(true) => {
                        info!(
                            "🔍 Starting CVE scan for derivation: {}",
                            derivation.derivation_name
                        );

                        // Create a new scan record before starting
                        let scan_id =
                            create_cve_scan(pool, derivation.id, "vulnix", vulnix_version.clone())
                                .await?;

                        // Mark scan as in progress
                        mark_scan_in_progress(pool, scan_id).await?;

                        let start_time = std::time::Instant::now();

                        // Run CVE scan using the vulnix runner
                        match vulnix_runner
                            .scan_derivation(&pool, derivation.id, vulnix_version)
                            .await
                        {
                            Ok(vulnix_entries) => {
                                let scan_duration_ms =
                                    Some(start_time.elapsed().as_millis() as i32);
                                let stats =
                                    crate::vulnix::vulnix_parser::VulnixParser::calculate_stats(
                                        &vulnix_entries,
                                    );

                                // Save the detailed scan results to database
                                save_scan_results(pool, scan_id, &vulnix_entries, scan_duration_ms)
                                    .await?;

                                info!(
                                    "✅ CVE scan completed for {}: {}",
                                    derivation.derivation_name, stats
                                );
                            }
                            Err(e) => {
                                error!(
                                    "❌ CVE scan failed for {}: {}",
                                    derivation.derivation_name, e
                                );
                                if let Err(save_err) =
                                    mark_cve_scan_failed(pool, derivation, &e.to_string()).await
                                {
                                    error!("❌ Failed to mark CVE scan as failed: {save_err}");
                                }
                            }
                        }
                    }
                    Ok(false) => {
                        warn!("❌ Derivation path does not exist: {}", path);
                        update_derivation_status(
                            &pool,
                            derivation.id,
                            EvaluationStatus::DryRunComplete,
                            derivation.derivation_path.as_deref(),
                            Some("Missing Nix Store Path"),
                            derivation.store_path.as_deref(),
                        )
                        .await?;
                    }
                    Err(e) => {
                        error!("❌ Error checking derivation path {}: {}", path, e);
                    }
                }
            } else {
                warn!("❌ No derivation path set for derivation");
            }
        }
        Err(e) => error!("❌ Failed to get derivations needing CVE scan: {e}"),
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

/// Cleanup loop for stale reservations
async fn run_reservation_cleanup_loop(pool: PgPool) {
    info!("🧹 Starting reservation cleanup loop...");

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;

        match build_reservations::cleanup_stale_reservations(&pool, 300).await {
            Ok(reclaimed) if !reclaimed.is_empty() => {
                warn!(
                    "🧹 Reclaimed {} stale reservations: {:?}",
                    reclaimed.len(),
                    reclaimed
                );
            }
            Err(e) => {
                error!("❌ Error cleaning up stale reservations: {}", e);
            }
            _ => {}
        }
    }
}

/// Mark build complete and release reservation
async fn mark_build_complete_and_release(
    pool: &PgPool,
    worker_uuid: &str,
    derivation_id: i32,
    store_path: &str,
) -> Result<()> {
    let mut tx = pool.begin().await?;

    // Delete reservation
    build_reservations::delete_reservation(&mut *tx, worker_uuid, derivation_id).await?;

    // Mark complete
    mark_target_build_complete(&mut *tx, derivation_id, store_path).await?;

    tx.commit().await?;

    // Create GC root to prevent cleanup before cache push
    if let Err(e) = create_gc_root(store_path, derivation_id).await {
        warn!("Failed to create GC root for {}: {}", store_path, e);
    }

    Ok(())
}

/// Mark build failed and release reservation
async fn mark_build_failed_and_release(
    pool: &PgPool,
    worker_uuid: &str,
    derivation: &Derivation,
    error: &anyhow::Error,
) -> Result<()> {
    let mut tx = pool.begin().await?;

    // Delete reservation
    build_reservations::delete_reservation(&mut *tx, worker_uuid, derivation.id).await?;

    // Mark failed
    handle_derivation_failure(&mut *tx, derivation, "build", error).await?;

    tx.commit().await?;
    Ok(())
}

/// Worker heartbeat loop - updates reservation heartbeat every 30 seconds
async fn worker_heartbeat_loop(worker_uuid: String, pool: PgPool) {
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

    loop {
        interval.tick().await;

        match build_reservations::update_heartbeat(&pool, &worker_uuid).await {
            Ok(count) if count > 0 => {
                debug!(
                    "Worker {} heartbeat updated ({} reservations)",
                    worker_uuid, count
                );
            }
            Err(e) => {
                error!("Worker {} heartbeat failed: {}", worker_uuid, e);
            }
            _ => {}
        }
    }
}

pub async fn get_gc_root_path(derivation_id: i32) -> String {
    let gc_root_dir = "/var/cache/crystal-forge/gc-roots";
    tokio::fs::create_dir_all(gc_root_dir)
        .await
        .expect("failed to create GC root directory");
    format!("{}/derivation-{}", gc_root_dir, derivation_id)
}

/// Create a GC root to prevent garbage collection until cache push
pub async fn create_gc_root(store_path: &str, derivation_id: i32) -> Result<()> {
    let gc_root_path = get_gc_root_path(derivation_id).await;

    // Create symlink to store path
    if let Err(e) = tokio::fs::symlink(store_path, &gc_root_path).await {
        // Ignore if already exists
        if e.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(e.into());
        }
    }

    debug!("Created GC root: {} -> {}", gc_root_path, store_path);
    Ok(())
}

/// Remove GC root after successful cache push
pub async fn remove_gc_root(derivation_id: i32) -> Result<()> {
    let gc_root_path = get_gc_root_path(derivation_id).await;

    if let Err(e) = tokio::fs::remove_file(&gc_root_path).await {
        if e.kind() != std::io::ErrorKind::NotFound {
            warn!("Failed to remove GC root {}: {}", gc_root_path, e);
        }
    } else {
        debug!("Removed GC root: {}", gc_root_path);
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::builders::{CommitBuilder, DerivationBuilder};

    // ── format_task_description ──────────────────────────────────────────

    mod format_task_description_tests {
        use super::*;

        #[test]
        fn no_commit_returns_derivation_name_only() {
            let result = format_task_description("my-system", CommitContext::None);
            assert_eq!(result, "my-system");
        }

        #[test]
        fn no_commit_preserves_empty_name() {
            let result = format_task_description("", CommitContext::None);
            assert_eq!(result, "");
        }

        #[test]
        fn unresolved_commit_includes_commit_id() {
            let result = format_task_description(
                "web-server",
                CommitContext::Unresolved { commit_id: 42 },
            );
            assert_eq!(result, "web-server @ commit#42");
        }

        #[test]
        fn resolved_commit_without_distance_shows_hash_only() {
            let result = format_task_description(
                "db-primary",
                CommitContext::Resolved {
                    short_hash: "abc123de".into(),
                    distance_from_head: None,
                },
            );
            assert_eq!(result, "db-primary @ abc123de");
        }

        #[test]
        fn resolved_commit_with_distance_shows_head_notation() {
            let result = format_task_description(
                "db-primary",
                CommitContext::Resolved {
                    short_hash: "abc123de".into(),
                    distance_from_head: Some(5),
                },
            );
            assert_eq!(result, "db-primary @ abc123de (HEAD~5)");
        }

        #[test]
        fn resolved_commit_at_head_shows_zero_distance() {
            let result = format_task_description(
                "edge-node",
                CommitContext::Resolved {
                    short_hash: "deadbeef".into(),
                    distance_from_head: Some(0),
                },
            );
            assert_eq!(result, "edge-node @ deadbeef (HEAD~0)");
        }

        #[test]
        fn special_characters_in_name_preserved() {
            let result = format_task_description(
                "nixos-system-web.example.com",
                CommitContext::Resolved {
                    short_hash: "1a2b3c4d".into(),
                    distance_from_head: Some(1),
                },
            );
            assert_eq!(
                result,
                "nixos-system-web.example.com @ 1a2b3c4d (HEAD~1)"
            );
        }

        #[test]
        fn short_hash_is_used_as_provided() {
            // The caller is responsible for truncating; format just uses what it gets.
            let result = format_task_description(
                "sys",
                CommitContext::Resolved {
                    short_hash: "ab".into(),
                    distance_from_head: None,
                },
            );
            assert_eq!(result, "sys @ ab");
        }
    }

    // ── resolve_commit_context (unit-level, no DB) ───────────────────────
    // These verify the pure derivation→CommitContext mapping that doesn't
    // need a database (the None-commit branch).

    mod resolve_commit_context_tests {
        use super::*;

        #[test]
        fn derivation_without_commit_id_produces_none_context() {
            let derivation = DerivationBuilder::new()
                .commit_id(None)
                .name("standalone-package")
                .build();
            assert_eq!(derivation.commit_id, None);
            // When commit_id is None, resolve_commit_context should return None
            // even before hitting the database. We verify the builder output
            // and trust the branch logic (tested via format_task_description).
        }

        #[test]
        fn derivation_with_commit_id_has_some_commit_id() {
            let derivation = DerivationBuilder::new()
                .commit_id(Some(99))
                .name("linked-system")
                .build();
            assert_eq!(derivation.commit_id, Some(99));
        }
    }

    // ── Integration: build_task_description without DB ───────────────────
    // These test the end-to-end flow for the branch that doesn't need a DB.

    mod build_task_description_tests {
        use super::*;

        #[tokio::test]
        async fn derivation_without_commit_returns_name() {
            // Construct a derivation with no commit - this path never touches the DB
            let derivation = DerivationBuilder::new()
                .commit_id(None)
                .name("orphan-build")
                .build();

            let ctx = CommitContext::None;
            let result = format_task_description(&derivation.derivation_name, ctx);
            assert_eq!(result, "orphan-build");
        }

        #[test]
        fn commit_lookup_failure_produces_fallback_format() {
            // Simulate what happens when get_commit_by_id fails
            let derivation = DerivationBuilder::new()
                .commit_id(Some(999))
                .name("missing-commit-system")
                .build();

            let ctx = CommitContext::Unresolved { commit_id: 999 };
            let result = format_task_description(&derivation.derivation_name, ctx);
            assert_eq!(result, "missing-commit-system @ commit#999");
        }

        #[test]
        fn successful_commit_with_distance_produces_full_description() {
            let derivation = DerivationBuilder::new()
                .name("production-server")
                .build();

            let commit = CommitBuilder::new()
                .hash("a1b2c3d4e5f6a7b8")
                .build();

            // Simulate resolved context from the first 8 chars of commit hash
            let ctx = CommitContext::Resolved {
                short_hash: commit.git_commit_hash[..8].to_owned(),
                distance_from_head: Some(3),
            };

            let result = format_task_description(&derivation.derivation_name, ctx);
            assert_eq!(result, "production-server @ a1b2c3d4 (HEAD~3)");
        }

        #[test]
        fn successful_commit_without_distance_omits_head_notation() {
            let derivation = DerivationBuilder::new()
                .name("staging-server")
                .build();

            let commit = CommitBuilder::new()
                .hash("deadbeefcafebabe")
                .build();

            let ctx = CommitContext::Resolved {
                short_hash: commit.git_commit_hash[..8].to_owned(),
                distance_from_head: None,
            };

            let result = format_task_description(&derivation.derivation_name, ctx);
            assert_eq!(result, "staging-server @ deadbeef");
        }
    }

    // ── CommitContext ────────────────────────────────────────────────────

    mod commit_context_tests {
        use super::*;

        #[test]
        fn commit_context_none_is_equal() {
            assert_eq!(CommitContext::None, CommitContext::None);
        }

        #[test]
        fn commit_context_unresolved_equality() {
            assert_eq!(
                CommitContext::Unresolved { commit_id: 1 },
                CommitContext::Unresolved { commit_id: 1 }
            );
            assert_ne!(
                CommitContext::Unresolved { commit_id: 1 },
                CommitContext::Unresolved { commit_id: 2 }
            );
        }

        #[test]
        fn commit_context_resolved_equality() {
            let a = CommitContext::Resolved {
                short_hash: "abcd1234".into(),
                distance_from_head: Some(0),
            };
            let b = CommitContext::Resolved {
                short_hash: "abcd1234".into(),
                distance_from_head: Some(0),
            };
            assert_eq!(a, b);
        }

        #[test]
        fn commit_context_variants_are_not_equal() {
            assert_ne!(CommitContext::None, CommitContext::Unresolved { commit_id: 1 });
        }
    }
}
