use crate::models::config::CrystalForgeConfig;
use crate::models::config::VulnixConfig;
use crate::queries::cve_scans::{get_targets_needing_cve_scan, mark_cve_scan_failed};
use crate::queries::evaluation_targets::mark_target_dry_run_in_progress;
use crate::queries::evaluation_targets::mark_target_failed;
use crate::queries::evaluation_targets::update_scheduled_at;
use crate::queries::evaluation_targets::{
    get_pending_dry_run_targets, increment_evaluation_target_attempt_count,
    insert_evaluation_target, mark_target_dry_run_complete,
};
use crate::vulnix::vulnix_runner::VulnixRunner;
use anyhow::Result;
use futures::stream::{FuturesUnordered, StreamExt};
use sqlx::PgPool;
use tokio::time::{Duration, sleep};
use tracing::{debug, error, info, warn};

use crate::flake::eval::list_nixos_configurations_from_commit;
use crate::queries::commits::get_commits_pending_evaluation;

/// Spawns both background evaluation loops
pub fn spawn_background_tasks(pool: PgPool) {
    let commit_pool = pool.clone();
    let target_pool = pool.clone();

    tokio::spawn(run_commit_evaluation_loop(commit_pool));
    tokio::spawn(run_target_evaluation_loop(target_pool));
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
                        for mut target_name in nixos_targets {
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
    match get_pending_dry_run_targets(pool).await {
        Ok(pending_targets) => {
            info!("📦 Found {} pending targets", pending_targets.len());
            let concurrency_limit = 4; // adjust as needed
            // Use FuturesUnordered instead of stream::iter for better concurrency
            let mut futures = FuturesUnordered::new();
            for mut target in pending_targets {
                let pool = pool.clone();
                futures.push(async move {
                    if let Err(e) = mark_target_dry_run_in_progress(&pool, target.id).await {
                        error!("❌ Failed to mark target in-progress: {e}");
                        return;
                    }
                    match target.resolve_derivation_path(&pool).await {
                        Ok(path) => {
                            match mark_target_dry_run_complete(&pool, target.id, &path).await {
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
                                mark_target_failed(&pool, target.id, "dry-run", &e.to_string())
                                    .await
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
