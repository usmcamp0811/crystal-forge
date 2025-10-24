use crate::log::{WorkerState, WorkerStatus, get_build_status, get_cve_status};
use crate::models::config::{BuildConfig, CacheConfig, CrystalForgeConfig};
use crate::models::derivations::{Derivation, DerivationType};
use crate::queries::build_reservations;
use crate::queries::cache_push::{
    cleanup_stale_cache_push_jobs, get_pending_cache_push_jobs, mark_cache_push_completed,
    mark_cache_push_failed, mark_cache_push_in_progress,
};
use crate::queries::cve_scans::{
    create_cve_scan, get_targets_needing_cve_scan, mark_cve_scan_failed, mark_scan_in_progress,
    save_scan_results,
};
use crate::queries::derivations::{
    EvaluationStatus, handle_derivation_failure, mark_target_build_complete,
    update_derivation_status,
};
use crate::queries::derivations::{batch_queue_cache_jobs, reset_derivation_for_rebuild};
use crate::vulnix::vulnix_runner::VulnixRunner;
use anyhow::Result;
use futures::FutureExt;
use sqlx::PgPool;
use tokio::fs;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Build a single derivation directly using nix-store --realise
async fn build_single_derivation(
    pool: &PgPool,
    derivation: &mut Derivation,
    build_config: &BuildConfig,
) -> Result<String> {
    let drv_path = derivation
        .derivation_path
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Derivation missing derivation_path"))?;

    if derivation.derivation_type == DerivationType::Package {
        // For packages, build the .drv directly
        derivation
            .build_derivation_from_path(pool, drv_path, build_config)
            .await
    } else {
        // For NixOS systems, use the full evaluate_and_build process
        derivation.build(pool, build_config).await
    }
}

/// Runs the continuous build loop with multiple workers
pub async fn run_build_loop(pool: PgPool) {
    let cfg = CrystalForgeConfig::load().unwrap_or_else(|e| {
        warn!("Failed to load Crystal Forge config: {}, using defaults", e);
        CrystalForgeConfig::default()
    });
    let build_config = cfg.get_build_config();
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
        let worker_uuid = format!("{}-worker-{}", hostname, worker_id);

        let handle = tokio::spawn(async move {
            build_worker(worker_id, worker_uuid, pool, build_config).await;
        });
        handles.push(handle);
    }

    // Wait for all workers
    for handle in handles {
        let _ = handle.await;
    }
}

/// Build a task description for display/logging
///
/// This helper function encapsulates all the commit-related queries
/// that were previously embedded in build_worker
async fn build_task_description(pool: &PgPool, derivation: &Derivation) -> String {
    if let Some(commit_id) = derivation.commit_id {
        match crate::queries::commits::get_commit_by_id(pool, commit_id).await {
            Ok(commit) => {
                // Try to get distance from HEAD
                let distance_info = match commit.get_flake(pool).await {
                    Ok(flake) => {
                        match crate::queries::commits::get_commit_distance_from_head(
                            pool, &flake, &commit,
                        )
                        .await
                        {
                            Ok(distance) => format!(" (HEAD~{})", distance),
                            Err(_) => String::new(),
                        }
                    }
                    Err(_) => String::new(),
                };

                format!(
                    "{} @ {}{}",
                    derivation.derivation_name,
                    &commit.git_commit_hash[..8],
                    distance_info
                )
            }
            Err(_) => format!("{} @ commit#{}", derivation.derivation_name, commit_id),
        }
    } else {
        derivation.derivation_name.clone()
    }
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

                let start = std::time::Instant::now();
                let build_result =
                    build_single_derivation(&pool, &mut derivation, &build_config).await;

                info!(
                    "🏁 Worker {} FINISHED BUILD for {} after {:.1}s",
                    worker_id,
                    derivation.derivation_name,
                    start.elapsed().as_secs_f64()
                );

                // Build task description using helper function (no embedded SQL)
                let task_description = build_task_description(&pool, &derivation).await;

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

                // CRITICAL: Add timeout to prevent stuck workers
                // This wraps the build in a tokio::time::timeout
                let build_result = tokio::time::timeout(
                    build_timeout,
                    build_single_derivation(&pool, &mut derivation, &build_config),
                )
                .await;

                match build_result {
                    // Build succeeded within timeout
                    Ok(Ok(store_path)) => {
                        let duration = start.elapsed();
                        info!(
                            "✅ Worker {} completed {} in {:.1}s: {}",
                            worker_id,
                            task_description,
                            duration.as_secs_f64(),
                            store_path
                        );

                        if let Err(e) = mark_build_complete_and_release(
                            &pool,
                            &worker_uuid,
                            derivation.id,
                            &store_path,
                        )
                        .await
                        {
                            error!("Failed to mark build complete: {}", e);
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

/// Runs the periodic cache push loop with robust error handling
pub async fn run_cache_push_loop(pool: PgPool) {
    let cfg = CrystalForgeConfig::load().unwrap_or_else(|e| {
        warn!("Failed to load Crystal Forge config: {}, using defaults", e);
        CrystalForgeConfig::default()
    });
    let cache_config = cfg.get_cache_config();
    let build_config = cfg.get_build_config();

    if cache_config.push_to.is_none() {
        info!("📤 Cache push loop disabled - no cache destination configured");
        return;
    }

    info!(
        "🔄 Starting Cache Push loop (every {}s)...",
        cache_config.poll_interval.as_secs()
    );
    debug!(
        "🔧 Cache config: push_to={:?}, cache_type={:?}",
        cache_config.push_to, cache_config.cache_type
    );

    let mut iteration = 0;
    let mut consecutive_failures = 0;

    loop {
        iteration += 1;
        debug!("📤 Cache push loop iteration {} starting", iteration);

        // Batch queue any missing cache jobs first
        if let Some(destination) = cache_config.push_to.as_deref() {
            if let Err(e) = batch_queue_cache_jobs(&pool, destination).await {
                warn!("Failed to batch queue cache jobs: {}", e);
            }
        }

        let cycle_timeout =
            std::time::Duration::from_secs(cache_config.push_timeout_seconds.max(300));

        let processed_jobs = match tokio::time::timeout(
            cycle_timeout,
            process_cache_pushes_safe(&pool, &cache_config, &build_config),
        )
        .await
        {
            Ok(Ok(count)) => {
                // Success - reset failure counter
                consecutive_failures = 0;
                debug!("📤 Cache push cycle completed successfully");
                count
            }
            Ok(Err(e)) => {
                error!("❌ Error in cache push cycle: {e}");
                consecutive_failures += 1;
                0
            }
            Err(_) => {
                error!(
                    "⏱️ Cache push cycle timed out after {}s",
                    cycle_timeout.as_secs()
                );
                consecutive_failures += 1;
                0
            }
        };

        // Smart delay based on work done and failures
        let delay = if consecutive_failures > 0 {
            // Has failures - use graduated backoff
            if consecutive_failures < 3 {
                cache_config.poll_interval * 2
            } else if consecutive_failures < 10 {
                std::time::Duration::from_secs(30)
            } else {
                warn!(
                    "🟡 {} consecutive failures in cache push loop",
                    consecutive_failures
                );
                std::time::Duration::from_secs(60)
            }
        } else if processed_jobs == 0 {
            // No work to do - wait the full interval
            cache_config.poll_interval
        } else {
            // Successfully processed jobs - minimal delay, keep pushing!
            std::time::Duration::from_millis(100)
        };

        debug!(
            "📤 Cache push loop iteration {} complete, sleeping for {}ms (processed {} jobs)",
            iteration,
            delay.as_millis(),
            processed_jobs
        );
        sleep(delay).await;
    }
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
            if let Some(ref path) = derivation.derivation_path {
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
        cleanup_stale_cache_push_jobs(pool, 10),
    )
    .await;

    // Get pending jobs (up to 50 at a time for batching)
    let jobs_result =
        tokio::time::timeout(db_timeout, get_pending_cache_push_jobs(pool, Some(50))).await;

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
    _build_config: &BuildConfig,
) -> Result<()> {
    if jobs.is_empty() {
        return Ok(());
    }

    info!("📤 Processing batch of {} cache push jobs", jobs.len());

    // Mark all as in-progress and validate paths exist
    let mut valid_jobs = Vec::new();

    // Mark all as in-progress
    for job in &jobs {
        mark_cache_push_in_progress(pool, job.id).await?;

        if let Some(ref store_path) = job.store_path {
            // Check if path actually exists locally
            if tokio::fs::try_exists(store_path).await.unwrap_or(false) {
                valid_jobs.push(job.clone());
            } else {
                warn!("❌ Store path doesn't exist locally: {}", store_path);

                // Mark job as failed
                mark_cache_push_failed(
                    pool,
                    job.id,
                    &format!("Store path does not exist: {}", store_path),
                )
                .await?;

                // Reset derivation to dry-run-complete so it can be rebuilt
                if let Err(e) = reset_derivation_for_rebuild(pool, job.derivation_id).await {
                    warn!(
                        "Failed to reset derivation {} for rebuild: {}",
                        job.derivation_id, e
                    );
                }
            }
        } else {
            mark_cache_push_failed(pool, job.id, "No store path specified").await?;
        }
    }

    // Collect all store paths with their job IDs
    let paths_with_jobs: Vec<(i32, String)> = jobs
        .iter()
        .filter_map(|job| job.store_path.clone().map(|path| (job.id, path)))
        .collect();

    if paths_with_jobs.is_empty() {
        warn!("No valid store paths in batch");
        return Ok(());
    }

    let store_paths: Vec<&str> = paths_with_jobs.iter().map(|(_, p)| p.as_str()).collect();
    let start_time = std::time::Instant::now();

    // Execute single nix copy command with all paths
    let result = tokio::process::Command::new("nix")
        .arg("copy")
        .arg("--to")
        .arg(cache_config.push_to.as_ref().unwrap())
        .args(&store_paths)
        .output()
        .await?;

    let duration_ms = start_time.elapsed().as_millis() as i32;

    if result.status.success() {
        info!(
            "✅ Batch cache push completed: {} paths in {}ms ({:.1} paths/sec)",
            store_paths.len(),
            duration_ms,
            (store_paths.len() as f64) / (duration_ms as f64 / 1000.0)
        );

        // Mark all jobs as completed
        for (job_id, _) in &paths_with_jobs {
            if let Err(e) = mark_cache_push_completed(pool, *job_id, None, Some(duration_ms)).await
            {
                warn!("Failed to mark job {} as complete: {}", job_id, e);
            }
        }
    } else {
        let stderr = String::from_utf8_lossy(&result.stderr);
        error!("❌ Batch cache push failed, checking individual paths...");

        // Batch failed - check each path individually to see which ones succeeded
        for (job_id, store_path) in &paths_with_jobs {
            // Check if this specific path is in the cache now
            let check_result = tokio::process::Command::new("nix")
                .arg("path-info")
                .arg("--store")
                .arg(cache_config.push_to.as_ref().unwrap())
                .arg(store_path)
                .output()
                .await;

            match check_result {
                Ok(check) if check.status.success() => {
                    // Path is in cache - mark as succeeded
                    info!("✅ Path {} was successfully pushed", store_path);
                    if let Err(e) =
                        mark_cache_push_completed(pool, *job_id, None, Some(duration_ms)).await
                    {
                        warn!("Failed to mark job {} as complete: {}", job_id, e);
                    }
                }
                _ => {
                    // Path is not in cache - mark as failed
                    warn!("❌ Path {} failed to push", store_path);
                    if let Err(e) = mark_cache_push_failed(pool, *job_id, &stderr).await {
                        warn!("Failed to mark job {} as failed: {}", job_id, e);
                    }
                }
            }
        }
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

/// Create a GC root to prevent garbage collection until cache push
async fn create_gc_root(store_path: &str, derivation_id: i32) -> Result<()> {
    let gc_root_dir = "/var/cache/crystal-forge/gc-roots";

    // Ensure directory exists
    tokio::fs::create_dir_all(gc_root_dir).await?;

    let gc_root_path = format!("{}/derivation-{}", gc_root_dir, derivation_id);

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
