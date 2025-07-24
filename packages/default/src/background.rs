use crate::models::config::CrystalForgeConfig;
use crate::models::config::VulnixConfig;
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

/// Spawns both background evaluation loops
pub fn spawn_server_background_tasks(pool: PgPool) {
    let commit_pool = pool.clone();
    let target_pool = pool.clone();
    let cve_pool = pool.clone();

    tokio::spawn(run_commit_evaluation_loop(commit_pool));
    tokio::spawn(run_target_evaluation_loop(target_pool));
    tokio::spawn(run_cve_scanning_loop(cve_pool));
}

/// Runs the periodic commit evaluation check loop
async fn run_commit_evaluation_loop(pool: PgPool) {
    info!("🔁 Starting periodic commit evaluation check loop (every 60s)...");
    loop {
        if let Err(e) = process_pending_commits(&pool).await {
            error!("❌ Error in commit evaluation cycle: {e}");
        }
        sleep(Duration::from_secs(60)).await;
    }
}

/// Runs the periodic evaluation target resolution loop  
async fn run_target_evaluation_loop(pool: PgPool) {
    info!("🔍 Starting periodic evaluation target check loop (every 60s)...");
    loop {
        if let Err(e) = process_pending_targets(&pool).await {
            error!("❌ Error in target evaluation cycle: {e}");
        }
        sleep(Duration::from_secs(60)).await;
    }
}

async fn process_pending_commits(pool: &PgPool) -> Result<()> {
    match get_commits_pending_evaluation(&pool).await {
        Ok(pending_commits) => {
            info!("📌 Found {} pending commits", pending_commits.len());
            for commit in pending_commits {
                let target_type = "nixos";
                match list_nixos_configurations_from_commit(&pool, &commit).await {
                    Ok(nixos_targets) => {
                        info!(
                            "📂 Commit {} has {} nixos targets",
                            commit.git_commit_hash,
                            nixos_targets.len()
                        );
                        for target_name in nixos_targets {
                            match insert_evaluation_target(
                                &pool,
                                &commit,
                                &target_name,
                                target_type,
                            )
                            .await
                            {
                                Ok(_) => info!(
                                    "✅ Inserted evaluation target: {} (commit {})",
                                    target_name, commit.git_commit_hash
                                ),
                                Err(e) => {
                                    error!("❌ Failed to insert target for {}: {}", target_name, e)
                                }
                            }
                        }
                    }
                    Err(e) => error!(
                        "❌ Failed to list nixos configs for commit {}: {}",
                        commit.git_commit_hash, e
                    ),
                }
            }
        }
        Err(e) => error!("❌ Failed to get pending commits: {e}"),
    }
    Ok(())
}

async fn process_pending_targets(pool: &PgPool) -> Result<()> {
    update_scheduled_at(pool).await?;
    match get_pending_targets(pool).await {
        Ok(pending_targets) => {
            info!("📦 Found {} pending targets", pending_targets.len());
            let concurrency_limit = 4; // adjust as needed

            // Use FuturesUnordered instead of stream::iter for better concurrency
            let mut futures = FuturesUnordered::new();

            for mut target in pending_targets {
                let pool = pool.clone();
                futures.push(async move {
                    if let Err(e) = mark_target_in_progress(&pool, target.id).await {
                        error!("❌ Failed to mark target in-progress: {e}");
                        return;
                    }
                    match target.resolve_derivation_path(&pool).await {
                        Ok(path) => {
                            match update_evaluation_target_path(&pool, &target, &path).await {
                                Ok(updated) => info!("✅ Updated: {:?}", updated),
                                Err(e) => error!("❌ Failed to update path: {e}"),
                            }
                        }
                        Err(e) => {
                            if let Err(inc_err) =
                                increment_evaluation_target_attempt_count(&pool, &target, &e).await
                            {
                                error!("❌ Failed to increment attempt count: {inc_err}");
                            }

                            // Mark as failed instead of just incrementing attempts
                            if let Err(mark_err) =
                                mark_target_failed(&pool, target.id, &e.to_string()).await
                            {
                                error!("❌ Failed to mark target as failed: {mark_err}");
                            } else {
                                info!("💥 Marked target {} as failed", target.target_name);
                            }
                        }
                    }
                });
            }

            // Process futures with concurrency limit
            while let Some(_) = futures.next().await {
                // Each future completes independently
            }
        }
        Err(e) => error!("❌ Failed to get pending targets: {e}"),
    }
    Ok(())
}

/// Runs the periodic CVE scanning loop
async fn run_cve_scanning_loop(pool: PgPool) {
    info!("🔍 Starting periodic CVE scanning loop (every 300s)...");

    // Check if vulnix is available before starting the loop
    if !VulnixRunner::check_vulnix_available().await {
        error!("❌ vulnix is not available - CVE scanning disabled");
        return;
    }

    // Get vulnix version once for all scans
    let vulnix_version = VulnixRunner::get_vulnix_version().await.ok();
    info!("🔧 Using vulnix version: {:?}", vulnix_version);

    // Load vulnix config from Crystal Forge config or use default
    let vulnix_config = match CrystalForgeConfig::load() {
        Ok(cf_config) => cf_config.vulnix.unwrap_or_else(|| {
            info!("No vulnix config found in Crystal Forge config, using defaults");
            VulnixConfig::default()
        }),
        Err(e) => {
            warn!(
                "Failed to load Crystal Forge config: {}, using default vulnix config",
                e
            );
            VulnixConfig::default()
        }
    };

    info!(
        "🔧 Vulnix config: timeout={}s, whitelist={}, extra_args={:?}",
        vulnix_config.timeout_seconds, vulnix_config.enable_whitelist, vulnix_config.extra_args
    );

    // Create vulnix runner with loaded configuration
    let runner = VulnixRunner::with_config(vulnix_config);

    loop {
        if let Err(e) = process_cve_scans(&pool, &runner, vulnix_version.clone()).await {
            error!("❌ Error in CVE scanning cycle: {e}");
        }
        sleep(Duration::from_secs(300)).await; // 5 minutes between scans
    }
}

/// Process targets that need CVE scanning
async fn process_cve_scans(
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
