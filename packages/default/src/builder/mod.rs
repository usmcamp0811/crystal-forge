use crate::flake::eval::list_nixos_configurations_from_commit;
use crate::models::config::{BuildConfig, CacheConfig, CrystalForgeConfig, VulnixConfig};
use crate::models::derivations::{Derivation, DerivationType};
use crate::queries::cache_push::{
    create_cache_push_job, get_derivations_needing_cache_push_for_dest,
    get_pending_cache_push_jobs, mark_cache_push_completed, mark_cache_push_failed,
    mark_cache_push_in_progress, mark_derivation_cache_pushed,
};
use crate::queries::commits::get_commits_pending_evaluation;
use crate::queries::cve_scans::{
    create_cve_scan, get_targets_needing_cve_scan, mark_cve_scan_failed, mark_scan_in_progress,
    save_scan_results,
};
use crate::queries::derivations::{
    EvaluationStatus, claim_next_derivation, get_derivations_ready_for_build,
    mark_target_build_in_progress, mark_target_failed, update_derivation_status,
};
use crate::queries::derivations::{
    discover_and_queue_all_transitive_dependencies,
    get_derivations_ready_for_build_with_dependencies, handle_derivation_failure,
    has_all_dependencies_built, mark_target_build_complete,
};
use crate::vulnix::vulnix_runner::VulnixRunner;
use anyhow::Result;
use anyhow::bail;
use sqlx::PgPool;
use tokio::fs;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Alternative simpler approach: Build dependencies in dependency order globally
async fn build_derivations_simple_dependency_order(
    pool: &PgPool,
    build_config: &BuildConfig,
) -> Result<()> {
    // This approach builds all ready dependencies regardless of which NixOS system they belong to
    let derivations = get_derivations_ready_for_build_with_dependencies(pool).await?;
    if derivations.is_empty() {
        info!("🔍 No derivations ready for building");
        return Ok(());
    }
    info!(
        "🏗️ Found {} derivations ready for building",
        derivations.len()
    );
    for mut derivation in derivations {
        // Double-check that all dependencies are built
        if !has_all_dependencies_built(pool, derivation.id).await? {
            debug!(
                "⏭️ Skipping {} - dependencies not ready",
                derivation.derivation_name
            );
            continue;
        }
        info!(
            "🔨 Building: {} (type: {:?})",
            derivation.derivation_name, derivation.derivation_type
        );
        mark_target_build_in_progress(pool, derivation.id).await?;
        match derivation
            .evaluate_and_build(pool, true, build_config)
            .await
        {
            Ok(store_path) => {
                info!("✅ Built {}: {}", derivation.derivation_name, store_path);
                mark_target_build_complete(pool, derivation.id, &store_path).await?;
            }
            Err(e) => {
                error!("❌ Build failed for {}: {}", derivation.derivation_name, e);
                handle_derivation_failure(pool, &derivation, "build", &e).await?;
            }
        }
        // Limit to building one derivation per cycle to allow other systems to progress
        break;
    }
    Ok(())
}

/// New build strategy: discover all dependencies first, then build them individually
async fn build_derivations_with_dependency_discovery(
    pool: &PgPool,
    build_config: &BuildConfig,
) -> Result<()> {
    // Get ordered list of what to build (already prioritized by the query)
    let ready_derivations = get_derivations_ready_for_build(pool).await?;

    if ready_derivations.is_empty() {
        info!("No derivations ready for building");
        return Ok(());
    }

    let max_concurrent = build_config.max_concurrent_derivations.unwrap_or(6) as usize;
    let to_build: Vec<_> = ready_derivations.into_iter().take(max_concurrent).collect();

    info!("Building {} derivations concurrently", to_build.len());

    // Build them all concurrently
    let mut tasks = Vec::new();
    for mut derivation in to_build {
        let pool = pool.clone();
        let build_config = build_config.clone();

        tasks.push(tokio::spawn(async move {
            mark_target_build_in_progress(&pool, derivation.id)
                .await
                .ok();

            match build_single_derivation(&pool, &mut derivation, &build_config).await {
                Ok(store_path) => {
                    info!("Built {}: {}", derivation.derivation_name, store_path);
                    mark_target_build_complete(&pool, derivation.id, &store_path)
                        .await
                        .ok();
                }
                Err(e) => {
                    error!("Build failed for {}: {}", derivation.derivation_name, e);
                    handle_derivation_failure(&pool, &derivation, "build", &e)
                        .await
                        .ok();
                }
            }
        }));
    }

    for task in tasks {
        let _ = task.await;
    }

    Ok(())
}

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

/// Runs the periodic build loop
pub async fn run_build_loop(pool: PgPool) {
    let cfg = CrystalForgeConfig::load().unwrap_or_else(|e| {
        warn!("Failed to load Crystal Forge config: {}, using defaults", e);
        CrystalForgeConfig::default()
    });
    let build_config = cfg.get_build_config();
    let num_workers = build_config.max_concurrent_derivations.unwrap_or(6) as usize;

    info!("Starting {} continuous build workers...", num_workers);
    debug!(
        "Build config: cores={}, max_jobs={}, substitutes={}",
        build_config.cores, build_config.max_jobs, build_config.use_substitutes,
    );

    // Spawn worker pool
    let mut handles = Vec::new();
    for worker_id in 0..num_workers {
        let pool = pool.clone();
        let build_config = build_config.clone();

        let handle = tokio::spawn(async move {
            build_worker(worker_id, pool, build_config).await;
        });
        handles.push(handle);
    }

    // Wait for all workers (they run forever)
    for handle in handles {
        let _ = handle.await;
    }
}

async fn build_worker(worker_id: usize, pool: PgPool, build_config: BuildConfig) {
    info!("Worker {} started", worker_id);

    loop {
        // Claim next work item
        match claim_next_derivation(&pool).await {
            Ok(Some(mut derivation)) => {
                info!(
                    "Worker {} claimed: {}",
                    worker_id, derivation.derivation_name
                );

                match build_single_derivation(&pool, &mut derivation, &build_config).await {
                    Ok(store_path) => {
                        info!(
                            "Worker {} built {}: {}",
                            worker_id, derivation.derivation_name, store_path
                        );
                        if let Err(e) =
                            mark_target_build_complete(&pool, derivation.id, &store_path).await
                        {
                            error!("Worker {} failed to mark complete: {}", worker_id, e);
                        }
                    }
                    Err(e) => {
                        error!(
                            "Worker {} build failed for {}: {}",
                            worker_id, derivation.derivation_name, e
                        );
                        if let Err(e2) =
                            handle_derivation_failure(&pool, &derivation, "build", &e).await
                        {
                            error!("Worker {} failed to handle failure: {}", worker_id, e2);
                        }
                    }
                }
            }
            Ok(None) => {
                // No work available, sleep briefly
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

/// Runs the periodic cache push loop
pub async fn run_cache_push_loop(pool: PgPool) {
    let cfg = CrystalForgeConfig::load().unwrap_or_else(|e| {
        warn!("Failed to load Crystal Forge config: {}, using defaults", e);
        CrystalForgeConfig::default()
    });

    let cache_config = cfg.get_cache_config();
    let build_config = cfg.get_build_config();

    // Only start cache loop if push_after_build is enabled
    if !cache_config.push_after_build {
        info!("📤 Cache push disabled in configuration");
        return;
    }

    info!(
        "🔍 Starting Cache Push loop (every {}s)...",
        cache_config.poll_interval.as_secs()
    );

    debug!(
        "🔧 Cache config: push_after_build={}, push_to={:?}",
        cache_config.push_after_build, cache_config.push_to
    );

    loop {
        if let Err(e) = process_cache_pushes(&pool, &cache_config, &build_config).await {
            error!("❌ Error in cache push cycle: {e}");
        }

        sleep(cache_config.poll_interval).await;
    }
}

/// Process derivations that need building
async fn build_derivations(pool: &PgPool, build_config: &BuildConfig) -> Result<()> {
    build_derivations_with_dependency_discovery(pool, build_config).await
}

/// Process derivations that need CVE scanning
async fn scan_derivations(
    pool: &PgPool,
    vulnix_runner: &VulnixRunner,
    vulnix_version: Option<String>,
) -> Result<()> {
    // Get derivations that need CVE scanning (those with build-complete status)
    match get_targets_needing_cve_scan(pool, Some(1)).await {
        Ok(derivations) => {
            if derivations.is_empty() {
                info!("🔍 No derivations need CVE scanning");
                return Ok(());
            }

            let derivation = &derivations[0];

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

/// Process cache pushes for completed builds
pub async fn process_cache_pushes(
    pool: &PgPool,
    cache_config: &CacheConfig,
    build_config: &BuildConfig,
) -> Result<()> {
    // Require a destination (e.g., "attic://my-cache" or "s3://bucket/prefix")
    let Some(destination) = cache_config.push_to.as_deref() else {
        info!(
            "⏭️ No cache destination configured (cache_config.push_to is None); skipping cache push pass"
        );
        return Ok(());
    };

    // Step 1: queue jobs for this destination only
    match get_derivations_needing_cache_push_for_dest(
        pool,
        destination,
        Some(10),
        cache_config.max_retries as i32, // attempts cap
    )
    .await
    {
        Ok(derivations) => {
            for derivation in derivations {
                if let Some(store_path) = &derivation.store_path {
                    if !cache_config.should_push(&derivation.derivation_name) {
                        info!(
                            "⏭️ Skipping cache push for {} (filtered out)",
                            derivation.derivation_name
                        );
                        continue;
                    }

                    info!(
                        "📤 Queuing cache push for {} → {}",
                        derivation.derivation_name, destination
                    );

                    if let Err(e) = create_cache_push_job(
                        pool,
                        derivation.id,
                        store_path,
                        Some(destination), // <-- ensure job is for this destination
                    )
                    .await
                    {
                        error!("❌ Failed to queue cache push job: {}", e);
                    }
                }
            }
        }
        Err(e) => error!("❌ Failed to get derivations needing cache push: {e}"),
    }

    // Step 2: process pending jobs (your existing fetch can stay global or be dest-scoped)
    match get_pending_cache_push_jobs(pool, Some(5)).await {
        Ok(jobs) => {
            if jobs.is_empty() {
                info!("🔍 No cache push jobs need processing");
                return Ok(());
            }

            for job in jobs {
                // Prefer store_path; fall back to derivation_path
                let path_to_push = if let Some(store_path) = &job.store_path {
                    store_path.clone()
                } else {
                    match crate::queries::derivations::get_derivation_by_id(pool, job.derivation_id)
                        .await
                    {
                        Ok(derivation) => {
                            if let Some(drv_path) = &derivation.derivation_path {
                                info!("📤 Using derivation path for cache push: {}", drv_path);
                                drv_path.clone()
                            } else {
                                warn!("❌ Job {}: no store path and no derivation_path", job.id);
                                if let Err(e) = mark_cache_push_failed(
                                    pool,
                                    job.id,
                                    "No store path or derivation path available",
                                )
                                .await
                                {
                                    error!("❌ Failed to mark job failed: {e}");
                                }
                                continue;
                            }
                        }
                        Err(e) => {
                            error!("❌ Failed to load derivation {}: {}", job.derivation_id, e);
                            if let Err(se) = mark_cache_push_failed(
                                pool,
                                job.id,
                                &format!("Failed to get derivation: {}", e),
                            )
                            .await
                            {
                                error!("❌ Failed to mark job failed: {se}");
                            }
                            continue;
                        }
                    }
                };

                info!(
                    "📤 Processing job {} (derivation {}): {}",
                    job.id, job.derivation_id, path_to_push
                );

                if let Err(e) = mark_cache_push_in_progress(pool, job.id).await {
                    error!("❌ Failed to mark job in_progress: {}", e);
                    continue;
                }

                let start_time = std::time::Instant::now();

                match crate::queries::derivations::get_derivation_by_id(pool, job.derivation_id)
                    .await
                {
                    Ok(derivation) => {
                        match derivation
                            .push_to_cache(&path_to_push, cache_config, build_config)
                            .await
                        {
                            Ok(()) => {
                                let duration_ms = start_time.elapsed().as_millis() as i32;
                                info!(
                                    "✅ Cache push completed for {} ({}) in {}ms",
                                    derivation.derivation_name, path_to_push, duration_ms
                                );

                                if let Err(e) =
                                    mark_cache_push_completed(pool, job.id, None, Some(duration_ms))
                                        .await
                                {
                                    error!("❌ Failed to mark job completed: {}", e);
                                }
                                // Note: no derivation-level cache-pushed status write
                            }
                            Err(e) => {
                                error!(
                                    "❌ Cache push failed for {}: {}",
                                    derivation.derivation_name, e
                                );
                                if let Err(se) =
                                    mark_cache_push_failed(pool, job.id, &e.to_string()).await
                                {
                                    error!("❌ Failed to mark job failed: {se}");
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("❌ Failed to load derivation {}: {}", job.derivation_id, e);
                        if let Err(se) = mark_cache_push_failed(
                            pool,
                            job.id,
                            &format!("Failed to get derivation: {}", e),
                        )
                        .await
                        {
                            error!("❌ Failed to mark job failed: {se}");
                        }
                    }
                }
            }
        }
        Err(e) => error!("❌ Failed to get pending cache push jobs: {e}"),
    }

    Ok(())
}
