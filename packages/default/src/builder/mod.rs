use crate::models::config::{BuildConfig, CacheConfig, CrystalForgeConfig, VulnixConfig};
use crate::queries::cve_scans::{
    create_cve_scan, get_targets_needing_cve_scan, mark_cve_scan_failed, mark_scan_in_progress,
    save_scan_results,
};
use crate::queries::evaluation_targets::{
    get_targets_ready_for_build, mark_target_build_in_progress, mark_target_failed,
};
use crate::vulnix::vulnix_runner::VulnixRunner;
use anyhow::Result;
use sqlx::PgPool;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::flake::eval::list_nixos_configurations_from_commit;
use crate::queries::commits::get_commits_pending_evaluation;

/// Runs the periodic build and cache push loop
pub async fn run_build_loop(pool: PgPool) {
    let cfg = CrystalForgeConfig::load().unwrap_or_else(|e| {
        warn!("Failed to load Crystal Forge config: {}, using defaults", e);
        CrystalForgeConfig::default()
    });
    let build_config = cfg.get_build_config();
    let cache_config = cfg.get_cache_config();

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
    debug!(
        "🔧 Cache config: push_after_build={}, push_to={:?}",
        cache_config.push_after_build, cache_config.push_to
    );

    loop {
        if let Err(e) = build_targets(&pool, &build_config, &cache_config).await {
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
        if let Err(e) = scan_targets(&pool, &vulnix_runner, vulnix_version.clone()).await {
            error!("❌ Error in CVE scan cycle: {e}");
        }

        sleep(vulnix_config.poll_interval).await;
    }
}

/// Process targets that need building and cache pushing
async fn build_targets(
    pool: &PgPool,
    build_config: &BuildConfig,
    cache_config: &CacheConfig,
) -> Result<()> {
    // Get targets ready for building (those with dry-run-complete status)
    match get_targets_ready_for_build(pool).await {
        Ok(targets) => {
            if targets.is_empty() {
                info!("🔍 No targets need building");
                return Ok(());
            }

            let target = &targets[0];
            info!("🏗️ Starting build for target: {}", target.target_name);

            mark_target_build_in_progress(pool, target.id).await?;

            // Build the target
            let store_path = match target
                .evaluate_nixos_system_with_build(true, build_config)
                .await
            {
                Ok(path) => {
                    info!("✅ Build completed for {}: {}", target.target_name, path);
                    path
                }
                Err(e) => {
                    error!("❌ Build failed for {}: {}", target.target_name, e);
                    if let Err(save_err) =
                        mark_target_failed(pool, target.id, "build", &e.to_string()).await
                    {
                        error!("❌ Failed to mark build as failed: {save_err}");
                    }
                    return Err(e);
                }
            };

            // Push to cache if configured
            if cache_config.push_after_build {
                info!("📤 Starting cache push for target: {}", target.target_name);
                match target.push_to_cache(&store_path, cache_config).await {
                    Ok(_) => {
                        info!("✅ Cache push completed for {}", target.target_name);
                    }
                    Err(e) => {
                        warn!("⚠️ Cache push failed for {}: {}", target.target_name, e);
                        // Cache push failures are non-fatal, just log and continue
                    }
                }
            } else {
                info!("⏭️ Skipping cache push (push_after_build = false)");
            }

            // Mark as build complete (this will make it available for CVE scanning)
            use crate::queries::evaluation_targets::mark_target_build_complete;
            mark_target_build_complete(pool, target.id).await?;
        }
        Err(e) => error!("❌ Failed to get targets ready for build: {e}"),
    }
    Ok(())
}

/// Process targets that need CVE scanning
async fn scan_targets(
    pool: &PgPool,
    vulnix_runner: &VulnixRunner,
    vulnix_version: Option<String>,
) -> Result<()> {
    // Get targets that need CVE scanning (those with build-complete status)
    match get_targets_needing_cve_scan(pool, Some(20)).await {
        Ok(targets) => {
            if targets.is_empty() {
                info!("🔍 No targets need CVE scanning");
                return Ok(());
            }

            let target = &targets[0];
            info!("🔍 Starting CVE scan for target: {}", target.target_name);

            // Create a new scan record before starting
            let scan_id =
                create_cve_scan(pool, target.id.into(), "vulnix", vulnix_version.clone()).await?;

            // Mark scan as in progress
            mark_scan_in_progress(pool, scan_id).await?;

            let start_time = std::time::Instant::now();

            // Run CVE scan using the vulnix runner
            match vulnix_runner
                .scan_target(&pool, target.id, vulnix_version)
                .await
            {
                Ok(vulnix_entries) => {
                    let scan_duration_ms = Some(start_time.elapsed().as_millis() as i32);
                    let stats = crate::vulnix::vulnix_parser::VulnixParser::calculate_stats(
                        &vulnix_entries,
                    );

                    // Save the detailed scan results to database
                    save_scan_results(pool, scan_id, &vulnix_entries, scan_duration_ms).await?;

                    info!(
                        "✅ CVE scan completed for {}: {}",
                        target.target_name, stats
                    );
                }
                Err(e) => {
                    error!("❌ CVE scan failed for {}: {}", target.target_name, e);
                    if let Err(save_err) = mark_cve_scan_failed(pool, target, &e.to_string()).await
                    {
                        error!("❌ Failed to mark CVE scan as failed: {save_err}");
                    }
                }
            }
        }
        Err(e) => error!("❌ Failed to get targets needing CVE scan: {e}"),
    }
    Ok(())
}
