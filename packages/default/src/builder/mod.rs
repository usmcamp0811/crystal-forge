use crate::log::{WorkerState, WorkerStatus, get_build_status, get_cve_status};
use crate::models::config::{BuildConfig, CacheConfig, CrystalForgeConfig, VulnixConfig};
use crate::models::derivations::{Derivation, DerivationType};
use crate::queries::build_reservations;
use crate::queries::cache_push::{
    cleanup_stale_cache_push_jobs, create_cache_push_job,
    get_derivations_needing_cache_push_for_dest, get_pending_cache_push_jobs,
    mark_cache_push_completed, mark_cache_push_failed, mark_cache_push_in_progress,
};
use crate::queries::cve_scans::{
    create_cve_scan, get_targets_needing_cve_scan, mark_cve_scan_failed, mark_scan_in_progress,
    save_scan_results,
};
use crate::queries::derivations::{
    EvaluationStatus, handle_derivation_failure, mark_target_build_complete,
    update_derivation_status,
};
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
        derivation
            .evaluate_and_build(pool, true, build_config)
            .await
    }
}

/// Runs the continuous build loop with multiple workers
pub async fn run_build_loop(pool: PgPool) {
    let cfg = CrystalForgeConfig::load().unwrap_or_else(|e| {
        warn!("Failed to load Crystal Forge config: {}, using defaults", e);
        CrystalForgeConfig::default()
    });
    let build_config = cfg.get_build_config();
    let num_workers = build_config.max_concurrent_derivations.unwrap_or(6) as usize;

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

async fn build_worker(
    worker_id: usize,
    worker_uuid: String,
    pool: PgPool,
    build_config: BuildConfig,
) {
    // Initialize status
    {
        let mut statuses = get_build_status().write().await;
        if let Some(status) = statuses.iter_mut().find(|s| s.worker_id == worker_id) {
            status.state = WorkerState::Working;
            status.current_task = Some("claiming work".to_string());
            status.started_at = Some(std::time::Instant::now());
        }
    }

    info!("Worker {} ({}) started", worker_id, worker_uuid);

    // Spawn heartbeat task for this worker
    let heartbeat_pool = pool.clone();
    let heartbeat_uuid = worker_uuid.clone();
    tokio::spawn(async move {
        worker_heartbeat_loop(heartbeat_uuid, heartbeat_pool).await;
    });

    loop {
        // Update status: looking for work
        {
            let mut statuses = get_build_status().write().await;
            if let Some(status) = statuses.iter_mut().find(|s| s.worker_id == worker_id) {
                status.state = WorkerState::Working;
                status.current_task = Some("claiming work".to_string());
                status.started_at = Some(std::time::Instant::now());
            }
        }

        // Get wait_for_cache_push setting from config
        let wait_for_cache = build_config.wait_for_cache_push.unwrap_or(false);

        match build_reservations::claim_next_derivation(&pool, &worker_uuid, wait_for_cache).await {
            Ok(Some(mut derivation)) => {
                // Update status: building
                {
                    let mut statuses = get_build_status().write().await;
                    if let Some(status) = statuses.iter_mut().find(|s| s.worker_id == worker_id) {
                        status.current_task =
                            Some(derivation.derivation_path.clone().unwrap_or_else(|| {
                                format!("{} <unknown derivation path>", derivation.derivation_name)
                            }));
                        status.started_at = Some(std::time::Instant::now());
                    }
                }

                info!(
                    "Worker {} claimed: {} (type: {:?})",
                    worker_id, derivation.derivation_name, derivation.derivation_type
                );

                match build_single_derivation(&pool, &mut derivation, &build_config).await {
                    Ok(store_path) => {
                        info!(
                            "Worker {} built {}: {}",
                            worker_id, derivation.derivation_name, store_path
                        );

                        // Mark complete and delete reservation in transaction
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
                    Err(e) => {
                        error!(
                            "Worker {} build failed for {}: {}",
                            worker_id, derivation.derivation_name, e
                        );

                        // Mark failed and delete reservation
                        if let Err(e2) =
                            mark_build_failed_and_release(&pool, &worker_uuid, &derivation, &e)
                                .await
                        {
                            error!("Failed to mark build failed: {}", e2);
                        }
                    }
                }
            }
            Ok(None) => {
                // Update status: idle
                {
                    let mut statuses = get_build_status().write().await;
                    if let Some(status) = statuses.iter_mut().find(|s| s.worker_id == worker_id) {
                        status.state = WorkerState::Idle;
                        status.current_task = None;
                        status.started_at = None;
                    }
                }

                debug!("Worker {} idle, no work available", worker_id);
                sleep(std::time::Duration::from_secs(5)).await;
            }
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

        // Add a global timeout for the entire cache push cycle
        // This prevents any single operation from hanging indefinitely
        let cycle_timeout = std::time::Duration::from_secs(
            cache_config.push_timeout_seconds.max(300), // At least 5 minutes
        );

        match tokio::time::timeout(
            cycle_timeout,
            process_cache_pushes_safe(&pool, &cache_config, &build_config),
        )
        .await
        {
            Ok(Ok(())) => {
                // Success - reset failure counter
                consecutive_failures = 0;
                debug!("📤 Cache push cycle completed successfully");
            }
            Ok(Err(e)) => {
                // Error in processing, but didn't timeout
                error!("❌ Error in cache push cycle: {e}");
                consecutive_failures += 1;
            }
            Err(_) => {
                // Timeout occurred
                error!(
                    "⏱️ Cache push cycle timed out after {}s",
                    cycle_timeout.as_secs()
                );
                consecutive_failures += 1;
            }
        }

        // Graduated delays based on failure count - keeps trying but backs off
        let delay = if consecutive_failures == 0 {
            cache_config.poll_interval
        } else if consecutive_failures < 3 {
            // Minor issues: double the normal interval
            cache_config.poll_interval * 2
        } else if consecutive_failures < 10 {
            // Persistent issues: wait 30 seconds
            std::time::Duration::from_secs(30)
        } else {
            // Major issues: wait 60 seconds but keep trying
            warn!(
                "🟡 {} consecutive failures in cache push loop, backing off to 60s intervals",
                consecutive_failures
            );
            // Don't reset counter - let it naturally recover when pushes succeed
            std::time::Duration::from_secs(60)
        };

        debug!(
            "📤 Cache push loop iteration {} complete, sleeping for {}s",
            iteration,
            delay.as_secs()
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
) -> Result<()> {
    // Catch any panics and convert to errors
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
) -> Result<()> {
    let Some(destination) = cache_config.push_to.as_deref() else {
        debug!("⭐️ No cache destination configured, skipping cache push");
        return Ok(());
    };

    // Add timeout for database operations
    let db_timeout = std::time::Duration::from_secs(30);

    // Step 1: Queue new job (one at a time)
    let queue_result = tokio::time::timeout(
        db_timeout,
        get_derivations_needing_cache_push_for_dest(
            pool,
            destination,
            Some(1), // Process one at a time - FIXED
            1,
        ),
    )
    .await;

    match queue_result {
        Ok(Ok(derivations)) => {
            for derivation in derivations {
                if let Some(store_path) = &derivation.store_path {
                    if !cache_config.should_push(&derivation.derivation_name) {
                        debug!(
                            "⭐️ Skipping cache push for {} (filtered)",
                            derivation.derivation_name
                        );
                        continue;
                    }

                    info!(
                        "📤 Queuing cache push for {} → {}",
                        derivation.derivation_name, destination
                    );

                    // Don't let a single job creation failure stop the loop
                    if let Err(e) =
                        create_cache_push_job(pool, derivation.id, store_path, Some(destination))
                            .await
                    {
                        error!("❌ Failed to queue cache push job: {}", e);
                    }
                }
            }
        }
        Ok(Err(e)) => {
            error!("❌ Failed to get derivations needing cache push: {e}");
        }
        Err(_) => {
            error!("⏱️ Timeout getting derivations needing cache push");
        }
    }

    // Always try to cleanup stale jobs, but don't fail if it doesn't work
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        cleanup_stale_cache_push_jobs(pool, 10),
    )
    .await;

    // Step 2: Process pending job (one at a time)
    let jobs_result =
        tokio::time::timeout(db_timeout, get_pending_cache_push_jobs(pool, Some(1))).await; // FIXED

    match jobs_result {
        Ok(Ok(jobs)) => {
            for job in jobs {
                // Process each job in isolation
                if let Err(e) =
                    process_single_cache_push_job(pool, job, cache_config, build_config).await
                {
                    error!("❌ Failed to process cache push job: {}", e);
                    // Continue to next job even if this one failed
                }
            }
        }
        Ok(Err(e)) => {
            error!("❌ Failed to get pending cache push jobs: {e}");
        }
        Err(_) => {
            error!("⏱️ Timeout getting pending cache push jobs");
        }
    }

    Ok(())
}

/// Process a single cache push job with proper error isolation
async fn process_single_cache_push_job(
    pool: &PgPool,
    job: crate::queries::cache_push::CachePushJob, // Fixed: use the correct type
    cache_config: &CacheConfig,
    build_config: &BuildConfig,
) -> Result<()> {
    // Wrap entire job processing in a timeout
    let job_timeout = std::time::Duration::from_secs(
        cache_config.push_timeout_seconds.max(600), // At least 10 minutes per job
    );

    match tokio::time::timeout(
        job_timeout,
        process_job_inner(pool, job.clone(), cache_config, build_config),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => {
            error!(
                "⏱️ Cache push job {} timed out after {}s",
                job.id,
                job_timeout.as_secs()
            );
            // Mark job as failed due to timeout
            mark_cache_push_failed(pool, job.id, "Job timed out")
                .await
                .ok();
            Err(anyhow::anyhow!("Job timed out"))
        }
    }
}

/// Inner job processing logic with re-queue on failure
async fn process_job_inner(
    pool: &PgPool,
    job: crate::queries::cache_push::CachePushJob, // Fixed: use the correct type
    cache_config: &CacheConfig,
    build_config: &BuildConfig,
) -> Result<()> {
    // Mark job as in-progress
    mark_cache_push_in_progress(pool, job.id).await?;

    // Determine what to push
    let path_to_push = match &job.store_path {
        Some(p) => p.clone(),
        None => {
            // Fallback to derivation_path if no store_path
            match crate::queries::derivations::get_derivation_by_id(pool, job.derivation_id).await {
                Ok(deriv) if deriv.derivation_path.is_some() => {
                    info!("📤 Using derivation_path for job {}", job.id);
                    deriv.derivation_path.unwrap()
                }
                Ok(_) => {
                    error!("❌ Job {}: no store_path or derivation_path", job.id);
                    mark_cache_push_failed(pool, job.id, "No path available")
                        .await
                        .ok();
                    return Err(anyhow::anyhow!("No path available"));
                }
                Err(e) => {
                    error!("❌ Failed to load derivation {}: {}", job.derivation_id, e);
                    mark_cache_push_failed(pool, job.id, &e.to_string())
                        .await
                        .ok();
                    return Err(e);
                }
            }
        }
    };

    // Get the derivation
    let derivation =
        crate::queries::derivations::get_derivation_by_id(pool, job.derivation_id).await?;

    let start_time = std::time::Instant::now();

    // Push with built-in retries (already has timeout handling)
    match derivation
        .push_to_cache_with_retry(&path_to_push, cache_config, build_config)
        .await
    {
        Ok(()) => {
            let duration_ms = start_time.elapsed().as_millis() as i32;
            info!(
                "✅ Cache push completed for {} in {}ms",
                derivation.derivation_name, duration_ms
            );

            mark_cache_push_completed(pool, job.id, None, Some(duration_ms)).await?;
            Ok(())
        }
        Err(e) => {
            error!(
                "❌ Cache push failed for {}: {}",
                derivation.derivation_name, e
            );

            // Mark as failed
            mark_cache_push_failed(pool, job.id, &e.to_string()).await?;

            // Re-queue for retry if applicable
            // Note: Since we don't have retry_count in the job, we could track it in the error message
            // or just let the system naturally retry through get_derivations_needing_cache_push_for_dest
            let error_msg = e.to_string();
            if !error_msg.contains("(retry 3)") && !error_msg.contains("terminal error") {
                // Extract retry count from error message if present
                let retry_num = if error_msg.contains("(retry 2)") {
                    3
                } else if error_msg.contains("(retry 1)") {
                    2
                } else {
                    1
                };

                if retry_num <= 3 {
                    info!(
                        "🔄 Re-queuing failed cache push for derivation {} (retry {})",
                        job.derivation_id, retry_num
                    );

                    // Create a new job for retry with retry marker in destination
                    if let Some(store_path) = &job.store_path {
                        let destination_with_retry = format!(
                            "{} (retry {})",
                            cache_config.push_to.as_deref().unwrap_or("cache"),
                            retry_num
                        );

                        if let Err(queue_err) = create_cache_push_job(
                            pool,
                            job.derivation_id,
                            store_path,
                            Some(&destination_with_retry),
                        )
                        .await
                        {
                            error!("❌ Failed to re-queue cache push: {}", queue_err);
                        }
                    }
                } else {
                    warn!(
                        "⚠️ Cache push for derivation {} exceeded max retries",
                        job.derivation_id
                    );
                }
            }

            Err(e)
        }
    }
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
