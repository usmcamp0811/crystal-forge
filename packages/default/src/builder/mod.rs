use crate::models::config::{BuildConfig, CrystalForgeConfig, VulnixConfig};
use crate::queries::cve_scans::{get_targets_needing_cve_scan, mark_cve_scan_failed};
use crate::queries::evaluation_targets::update_scheduled_at;
use crate::queries::evaluation_targets::{mark_target_failed, mark_target_in_progress};
use crate::vulnix::vulnix_runner::VulnixRunner;
use anyhow::Result;
use futures::stream::{FuturesUnordered, StreamExt};
use sqlx::PgPool;
use tokio::time::{Duration, sleep};
use tracing::{debug, error, info, warn};

use crate::flake::eval::list_nixos_configurations_from_commit;
use crate::queries::commits::get_commits_pending_evaluation;
use crate::queries::evaluation_targets::{
    get_pending_targets, increment_evaluation_target_attempt_count, insert_evaluation_target,
    update_evaluation_target_path,
};

pub fn spawn_background_tasks(pool: PgPool) {
    let cve_pool = pool.clone();
    tokio::spawn(run_build_loop(cve_pool));
}

async fn run_nix_build_loop(pool: PgPool) {}

/// Runs the periodic CVE scanning loop
async fn run_build_loop(pool: PgPool) {
    // Load vulnix config from Crystal Forge config or use default
    let cfg = CrystalForgeConfig::load().unwrap_or_else(|e| {
        warn!("Failed to load Crystal Forge config: {}, using defaults", e);
        CrystalForgeConfig::default()
    });

    info!(
        "🔍 Starting Derivation Build scanning loop (every {}s)...",
        cfg.build.as_ref().unwrap().poll_interval
    );

    // Check if vulnix is available before starting the loop
    if !VulnixRunner::check_vulnix_available().await {
        error!("❌ vulnix is not available - CVE scanning disabled");
        return;
    }

    // Get vulnix version once for all scans
    let vulnix_version = VulnixRunner::get_vulnix_version().await.ok();
    let vulnix_config = cfg.vulnix.as_ref().unwrap();

    debug!("🔧 Using vulnix version: {:?}", vulnix_version);

    debug!(
        "🔧 Vulnix config: timeout={}s, whitelist={}, extra_args={:?}",
        vulnix_config.timeout_seconds, vulnix_config.enable_whitelist, vulnix_config.extra_args
    );

    // Create vulnix runner with loaded configuration
    let vulnix_runner = VulnixRunner::with_config(vulnix_config);
    // let build_runner =

    loop {
        if let Err(e) = build_evaluation_targets(&pool, &runner, vulnix_version.clone()).await {
            error!("❌ Error in CVE scanning cycle: {e}");
        }
        sleep(Duration::from_secs(
            cfg.build.as_ref().unwrap().poll_interval,
        ))
        .await; // 5 minutes between scans
    }
}

/// Process targets that need CVE scanning
async fn build_evaluation_targets(
    pool: &PgPool,
    runner: &VulnixRunner,
    vulnix_version: Option<String>,
) -> Result<()> {
    match get_targets_needing_cve_scan(pool, Some(10)).await {
        Ok(targets) => {
            if targets.is_empty() {
                debug!("🔍 No targets need CVE scanning");
                return Ok(());
            }

            info!("🎯 Found {} targets needing CVE scan", targets.len());

            for target in targets {
                info!("🔍 Starting CVE scan for target: {}", target.target_name);

                match runner
                    .scan_target(pool, target.id, vulnix_version.clone())
                    .await
                {
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

                        // Mark scan as failed in database
                        if let Err(save_err) =
                            mark_cve_scan_failed(pool, &target, &e.to_string()).await
                        {
                            error!("❌ Failed to mark CVE scan as failed: {save_err}");
                        }
                    }
                }
            }
        }
        Err(e) => error!("❌ Failed to get targets needing CVE scan: {e}"),
    }
    Ok(())
}
