use crate::models::config::{BuildConfig, CacheConfig, CrystalForgeConfig, VulnixConfig};
use crate::queries::cve_scans::{get_targets_needing_cve_scan, mark_cve_scan_failed};
use crate::queries::evaluation_targets::mark_target_failed;
use crate::vulnix::vulnix_runner::VulnixRunner;
use anyhow::Result;
use sqlx::PgPool;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::flake::eval::list_nixos_configurations_from_commit;
use crate::queries::commits::get_commits_pending_evaluation;

/// Runs the periodic build and CVE scanning loop
pub async fn run_build_loop(pool: PgPool) {
    // Load config from Crystal Forge config or use default
    let cfg = CrystalForgeConfig::load().unwrap_or_else(|e| {
        warn!("Failed to load Crystal Forge config: {}, using defaults", e);
        CrystalForgeConfig::default()
    });
    let build_config = cfg.get_build_config();
    let vulnix_config = cfg.get_vulnix_config();
    let cache_config = cfg.get_cache_config();
    info!(
        "🔍 Starting Derivation Build loop (every {}s)...",
        build_config.poll_interval.as_secs()
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

    let vulnix_runner = VulnixRunner::with_config(&vulnix_config);

    loop {
        if let Err(e) = build_evaluation_targets(
            &pool,
            &vulnix_runner,
            vulnix_version.clone(),
            &build_config,
            &cache_config,
        )
        .await
        {
            error!("❌ Error in build cycle: {e}");
        }

        sleep(build_config.poll_interval).await;
    }
}

/// Process targets that need CVE scanning
async fn build_evaluation_targets(
    pool: &PgPool,
    vulnix_runner: &VulnixRunner,
    vulnix_version: Option<String>,
    build_config: &BuildConfig,
    cache_config: &CacheConfig,
) -> Result<()> {
    // Get targets that need building (not just CVE scanning)
    match get_targets_needing_cve_scan(pool, Some(1)).await {
        Ok(targets) => {
            if targets.is_empty() {
                info!("🔍 No targets need building");
                return Ok(());
            }

            let target = &targets[0];
            info!("🏗️ Starting build for target: {}", target.target_name);

            mark_target_build_in_progress(pool, target.id).await?;

            // Step 1: Build the target (sequential, expensive)
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
                    // Mark build as failed in database
                    if let Err(save_err) =
                        mark_target_failed(pool, target.id, "build", &e.to_string()).await
                    {
                        error!("❌ Failed to mark build as failed: {save_err}");
                    }
                    return Err(e);
                }
            };

            // TODO: Update Status to Scanning
            // Step 2: Run CVE scan and cache push in parallel (both use the built store path)
            let cve_scan_future = async {
                info!("🔍 Starting CVE scan for target: {}", target.target_name);
                vulnix_runner
                    .scan_target(pool, target.id, vulnix_version.clone())
                    .await
            };

            let cache_push_future = async {
                if cache_config.push_after_build {
                    info!("📤 Starting cache push for target: {}", target.target_name);
                    target.push_to_cache(&store_path, cache_config).await
                } else {
                    info!("⏭️ Skipping cache push (push_after_build = false)");
                    Ok(())
                }
            };

            // Run both operations in parallel
            let (cve_result, cache_result) = tokio::join!(cve_scan_future, cache_push_future);

            // Handle CVE scan result
            match cve_result {
                Ok(vulnix_entries) => {
                    let stats = crate::vulnix::vulnix_parser::VulnixParser::calculate_stats(
                        &vulnix_entries,
                    );
                    // TODO: Save results to database here
                    // save_scan_results(pool, target.id, &vulnix_entries, &stats).await?;
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

            // Handle cache push result
            match cache_result {
                Ok(_) => {
                    info!("✅ Cache push completed for {}", target.target_name);
                }
                Err(e) => {
                    warn!("⚠️ Cache push failed for {}: {}", target.target_name, e);
                    // Cache push failures are non-fatal, just log and continue
                }
            }
        }
        Err(e) => error!("❌ Failed to get targets needing build: {e}"),
    }
    Ok(())
}
