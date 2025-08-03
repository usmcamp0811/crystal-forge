use crate::flake::commits::sync_all_watched_flakes_commits;
use crate::models::config::VulnixConfig;
use crate::models::config::{CrystalForgeConfig, FlakeConfig};
use crate::queries::cve_scans::{get_targets_needing_cve_scan, mark_cve_scan_failed};
use crate::queries::derivations::{
    get_pending_dry_run_derivations, increment_derivation_attempt_count, insert_derivation,
    mark_derivation_dry_run_in_progress, mark_target_dry_run_complete, mark_target_failed,
    update_derivation_path, update_scheduled_at,
};
use crate::vulnix::vulnix_runner::VulnixRunner;
use anyhow::Result;
use futures::stream;
use futures::stream::{FuturesUnordered, StreamExt};
use tokio::time::interval;

use sqlx::PgPool;
use tokio::time::{Duration, sleep};
use tracing::{debug, error, info, warn};

use crate::flake::eval::list_nixos_configurations_from_commit;
use crate::queries::commits::get_commits_pending_evaluation;

pub fn spawn_background_tasks(cfg: CrystalForgeConfig, pool: PgPool) {
    let flake_pool = pool.clone();
    let commit_pool = pool.clone();
    let target_pool = pool.clone();

    // Get the flake config with a fallback
    let flake_config = cfg.flakes;

    tokio::spawn(run_flake_polling_loop(flake_pool, flake_config.clone()));
    tokio::spawn(run_commit_evaluation_loop(
        commit_pool,
        flake_config.commit_evaluation_interval,
    ));
    tokio::spawn(run_derivation_evaluation_loop(
        target_pool,
        flake_config.build_processing_interval,
    ));
}

/// Runs the periodic flake polling loop to check for new commits
async fn run_flake_polling_loop(pool: PgPool, flake_config: FlakeConfig) {
    info!(
        "🔁 Starting periodic flake polling loop (every {:?})...",
        flake_config.flake_polling_interval
    );
    loop {
        if let Err(e) = sync_all_watched_flakes_commits(&pool, &flake_config.watched).await {
            error!("❌ Error in flake polling cycle: {e}");
        }
        tokio::time::sleep(flake_config.flake_polling_interval).await;
    }
}

/// Runs the periodic commit evaluation check loop
async fn run_commit_evaluation_loop(pool: PgPool, interval: Duration) {
    info!(
        "🔁 Starting periodic commit evaluation check loop (every {:?})...",
        interval
    );
    loop {
        if let Err(e) = process_pending_commits(&pool).await {
            error!("❌ Error in commit evaluation cycle: {e}");
        }
        tokio::time::sleep(interval).await;
    }
}

/// Runs the periodic evaluation target resolution loop  
async fn run_derivation_evaluation_loop(pool: PgPool, interval: Duration) {
    info!(
        "🔍 Starting periodic evaluation target check loop (every {:?})...",
        interval
    );
    loop {
        if let Err(e) = process_pending_derivations(&pool).await {
            error!("❌ Error in target evaluation cycle: {e}");
        }
        tokio::time::sleep(interval).await;
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
                        for mut derivation_name in nixos_targets {
                            match insert_derivation(
                                &pool,
                                Some(&commit),
                                &derivation_name,
                                target_type,
                            )
                            .await
                            {
                                Ok(_) => info!(
                                    "✅ Inserted evaluation target: {} (commit {})",
                                    derivation_name, commit.git_commit_hash
                                ),
                                Err(e) => {
                                    error!(
                                        "❌ Failed to insert target for {}: {}",
                                        derivation_name, e
                                    )
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

async fn process_pending_derivations(pool: &PgPool) -> Result<()> {
    update_scheduled_at(pool).await?;
    match get_pending_dry_run_derivations(pool).await {
        Ok(pending_targets) => {
            info!("📦 Found {} pending targets", pending_targets.len());
            let concurrency_limit = 1; // adjust as needed
            stream::iter(pending_targets.into_iter().map(|mut target| {
                let pool = pool.clone();
                async move {
                    if let Err(e) = mark_derivation_dry_run_in_progress(&pool, target.id).await {
                        error!("❌ Failed to mark target in-progress: {e}");
                        return;
                    }
                    match target.resolve_derivation_path(&pool).await {
                        Ok(path) => match update_derivation_path(&pool, &target, &path).await {
                            Ok(updated) => info!("✅ Updated: {:?}", updated),
                            Err(e) => error!("❌ Failed to update path: {e}"),
                        },
                        Err(e) => {
                            if let Err(inc_err) =
                                increment_derivation_attempt_count(&pool, &target, &e).await
                            {
                                error!("❌ Failed to increment attempt count: {inc_err}");
                            }

                            // Mark as failed instead of just incrementing attempts
                            if let Err(mark_err) =
                                mark_target_failed(&pool, target.id, "dry-run", &e.to_string())
                                    .await
                            {
                                error!("❌ Failed to mark target as failed: {mark_err}");
                            } else {
                                info!("💥 Marked target {} as failed", target.derivation_name);
                            }
                        }
                    }
                }
            }))
            .for_each_concurrent(concurrency_limit, |fut| fut)
            .await;
        }
        Err(e) => error!("❌ Failed to get pending targets: {e}"),
    }
    Ok(())
}

pub async fn memory_monitor_task(pool: PgPool) {
    let mut interval = interval(Duration::from_secs(30));
    loop {
        interval.tick().await;
        log_memory_usage(&pool).await;
    }
}

async fn log_memory_usage(pool: &PgPool) {
    // Memory stats from /proc/self/status
    if let Ok(contents) = tokio::fs::read_to_string("/proc/self/status").await {
        let mut vm_rss = None;
        let mut vm_size = None;
        let mut vm_peak = None;

        for line in contents.lines() {
            if line.starts_with("VmRSS:") {
                vm_rss = line.split_whitespace().nth(1);
            } else if line.starts_with("VmSize:") {
                vm_size = line.split_whitespace().nth(1);
            } else if line.starts_with("VmPeak:") {
                vm_peak = line.split_whitespace().nth(1);
            }
        }

        debug!(
            "📊 Memory - RSS: {} kB, Size: {} kB, Peak: {} kB",
            vm_rss.unwrap_or("?"),
            vm_size.unwrap_or("?"),
            vm_peak.unwrap_or("?")
        );
    }

    // Database pool statistics
    let pool_size = pool.size() as usize;
    let idle_count = pool.num_idle();

    debug!(
        "📊 DB Pool - Total: {}, Idle: {}, Active: {}",
        pool_size,
        idle_count,
        pool_size - idle_count
    );

    // Task/thread count
    if let Ok(contents) = tokio::fs::read_to_string("/proc/self/stat").await {
        if let Some(num_threads) = contents.split_whitespace().nth(19) {
            debug!("📊 Threads: {}", num_threads);
        }
    }
}
