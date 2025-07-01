use anyhow::Result;
use sqlx::PgPool;
use tokio::time::{Duration, sleep};
use tracing::{debug, error, info};

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
    match get_pending_targets(&pool).await {
        Ok(pending_targets) => {
            info!("📦 Found {} pending targets", pending_targets.len());
            for mut target in pending_targets {
                match target.resolve_derivation_path().await {
                    Ok(path) => match update_evaluation_target_path(&pool, &target, &path).await {
                        Ok(updated) => info!("✅ Updated: {:?}", updated),
                        Err(e) => error!("❌ Failed to update path: {e}"),
                    },
                    Err(e) => {
                        error!("❌ Failed to resolve derivation path: {e}");
                        match increment_evaluation_target_attempt_count(&pool, &target).await {
                            Ok(_) => debug!(
                                "✅ Incremented attempt count for target: {}",
                                target.target_name
                            ),
                            Err(inc_err) => {
                                error!("❌ Failed to increment attempt count: {inc_err}")
                            }
                        }
                    }
                }
            }
        }
        Err(e) => error!("❌ Failed to get pending targets: {e}"),
    }
    Ok(())
}
