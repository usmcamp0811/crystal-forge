use crate::flake::eval::list_nixos_configurations_from_commit;
use crate::models::config::{BuildConfig, CacheConfig, CrystalForgeConfig, VulnixConfig};
use crate::models::derivations::{Derivation, DerivationType};
use crate::queries::cache_push::{
    create_cache_push_job, get_derivations_needing_cache_push, get_pending_cache_push_jobs,
    mark_cache_push_completed, mark_cache_push_failed, mark_cache_push_in_progress,
    mark_derivation_cache_pushed,
};
use crate::queries::commits::get_commits_pending_evaluation;
use crate::queries::cve_scans::{
    create_cve_scan, get_targets_needing_cve_scan, mark_cve_scan_failed, mark_scan_in_progress,
    save_scan_results,
};
use crate::queries::derivations::{
    EvaluationStatus, get_derivations_ready_for_build, mark_target_build_in_progress,
    mark_target_failed, update_derivation_status,
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
                mark_target_build_complete(pool, derivation.id).await?;
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
    // Step 1: Discover all transitive dependencies for NixOS systems that haven't been analyzed yet
    discover_and_queue_all_transitive_dependencies(pool, build_config).await?;

    // Step 2: Build individual derivations that are ready (dependencies satisfied)
    let ready_derivations = get_derivations_ready_for_build_with_dependencies(pool).await?;

    if ready_derivations.is_empty() {
        info!("No derivations ready for building");
        return Ok(());
    }

    info!(
        "Found {} derivations ready for building",
        ready_derivations.len()
    );

    // Step 3: Build derivations one at a time to avoid resource conflicts
    for mut derivation in ready_derivations
        .into_iter()
        .take(build_config.max_concurrent_derivations.unwrap_or(1) as usize)
    {
        mark_target_build_in_progress(pool, derivation.id).await?;

        // Build individual derivation using nix-store --realise directly
        match build_single_derivation(pool, &mut derivation, build_config).await {
            Ok(store_path) => {
                info!("Built {}: {}", derivation.derivation_name, store_path);
                mark_target_build_complete(pool, derivation.id).await?;
            }
            Err(e) => {
                error!("Build failed for {}: {}", derivation.derivation_name, e);
                handle_derivation_failure(pool, &derivation, "build", &e).await?;
            }
        }
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

    info!(
        "🔍 Starting Build loop (every {}s)...",
        build_config.poll_interval.as_secs()
    );

    debug!(
        "🔧 Build config: cores={}, max_jobs={}, substitutes={}, poll_interval={}s",
        build_config.cores,
        build_config.max_jobs,
        build_config.use_substitutes,
        build_config.poll_interval.as_secs()
    );

    loop {
        if let Err(e) = build_derivations(&pool, &build_config).await {
            error!("❌ Error in build cycle: {e}");
        }

        sleep(build_config.poll_interval).await;
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
async fn process_cache_pushes(
    pool: &PgPool,
    cache_config: &CacheConfig,
    build_config: &BuildConfig,
) -> Result<()> {
    // Step 1: Queue new cache push jobs for derivations that need them
    match get_derivations_needing_cache_push(pool, Some(10)).await {
        Ok(derivations) => {
            for derivation in derivations {
                if let Some(store_path) = &derivation.derivation_path {
                    // Check if we should push this target
                    if !cache_config.should_push(&derivation.derivation_name) {
                        info!(
                            "⏭️ Skipping cache push for {} (filtered out)",
                            derivation.derivation_name
                        );
                        continue;
                    }

                    info!(
                        "📤 Queuing cache push for derivation: {}",
                        derivation.derivation_name
                    );

                    if let Err(e) = create_cache_push_job(
                        pool,
                        derivation.id,
                        store_path,
                        cache_config.push_to.as_deref(),
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

    // Step 2: Process pending cache push jobs
    match get_pending_cache_push_jobs(pool, Some(5)).await {
        Ok(jobs) => {
            if jobs.is_empty() {
                info!("🔍 No cache push jobs need processing");
                return Ok(());
            }

            for job in jobs {
                // Use store_path if available, otherwise fall back to derivation_path
                let path_to_push = if let Some(store_path) = &job.store_path {
                    store_path.clone()
                } else {
                    // Fall back to using the derivation path from the derivation record
                    match crate::queries::derivations::get_derivation_by_id(pool, job.derivation_id)
                        .await
                    {
                        Ok(derivation) => {
                            if let Some(drv_path) = &derivation.derivation_path {
                                info!("📤 Using derivation path for cache push: {}", drv_path);
                                drv_path.clone()
                            } else {
                                warn!(
                                    "❌ Cache push job {} has no store path and derivation has no derivation_path",
                                    job.id
                                );
                                if let Err(e) = mark_cache_push_failed(
                                    pool,
                                    job.id,
                                    "No store path or derivation path available",
                                )
                                .await
                                {
                                    error!("❌ Failed to mark cache push job as failed: {e}");
                                }
                                continue;
                            }
                        }
                        Err(e) => {
                            error!("❌ Failed to get derivation {}: {}", job.derivation_id, e);
                            if let Err(save_err) = mark_cache_push_failed(
                                pool,
                                job.id,
                                &format!("Failed to get derivation: {}", e),
                            )
                            .await
                            {
                                error!("❌ Failed to mark cache push job as failed: {save_err}");
                            }
                            continue;
                        }
                    }
                };

                info!(
                    "📤 Processing cache push job {} for derivation {} with path: {}",
                    job.id, job.derivation_id, path_to_push
                );

                // Mark job as in progress
                if let Err(e) = mark_cache_push_in_progress(pool, job.id).await {
                    error!("❌ Failed to mark cache push job as in progress: {}", e);
                    continue;
                }

                let start_time = std::time::Instant::now();

                // Get derivation details for push_to_cache
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
                                    "✅ Cache push completed for derivation {}: {} (took {}ms)",
                                    derivation.derivation_name, path_to_push, duration_ms
                                );

                                // Mark job as completed
                                if let Err(e) = mark_cache_push_completed(
                                    pool,
                                    job.id,
                                    None, // TODO: get actual push size if needed
                                    Some(duration_ms),
                                )
                                .await
                                {
                                    error!("❌ Failed to mark cache push job as completed: {}", e);
                                }

                                // Update derivation status to cache-pushed
                                if let Err(e) =
                                    mark_derivation_cache_pushed(pool, derivation.id).await
                                {
                                    error!("❌ Failed to mark derivation as cache-pushed: {}", e);
                                }
                            }
                            Err(e) => {
                                error!(
                                    "❌ Cache push failed for derivation {}: {}",
                                    derivation.derivation_name, e
                                );

                                if let Err(save_err) =
                                    mark_cache_push_failed(pool, job.id, &e.to_string()).await
                                {
                                    error!(
                                        "❌ Failed to mark cache push job as failed: {save_err}"
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("❌ Failed to get derivation {}: {}", job.derivation_id, e);
                        if let Err(save_err) = mark_cache_push_failed(
                            pool,
                            job.id,
                            &format!("Failed to get derivation: {}", e),
                        )
                        .await
                        {
                            error!("❌ Failed to mark cache push job as failed: {save_err}");
                        }
                    }
                }
            }
        }
        Err(e) => error!("❌ Failed to get pending cache push jobs: {e}"),
    }

    Ok(())
}
