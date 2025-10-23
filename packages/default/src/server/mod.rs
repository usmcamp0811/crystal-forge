use crate::deployment::spawn_deployment_policy_manager;
use crate::flake::commits::sync_all_watched_flakes_commits;
use crate::log::log_builder_worker_status;
use crate::models::commits::Commit;
use crate::models::config::{CrystalForgeConfig, FlakeConfig};
use crate::models::deployment_policies::DeploymentPolicy;
use crate::models::deployment_policies::evaluate_with_nix_eval_jobs;
use crate::models::flakes::Flake;
use crate::queries::commits::increment_commit_list_attempt_count;
use crate::queries::flakes::get_all_flakes_from_db;
use anyhow::Result;
use sqlx::PgPool;
use tokio::time;
use tokio::time::Duration;
use tokio::time::Instant;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::queries::commits::get_commits_pending_evaluation;

pub fn spawn_background_tasks(cfg: CrystalForgeConfig, pool: PgPool) {
    let flake_pool = pool.clone();
    let commit_pool = pool.clone();
    let target_pool = pool.clone();
    let deployment_pool = pool.clone();

    // Get the flake config with a fallback
    let flake_config = cfg.flakes.clone();

    tokio::spawn(run_flake_polling_loop(flake_pool, flake_config.clone()));
    tokio::spawn(run_commit_evaluation_loop(
        commit_pool,
        flake_config.commit_evaluation_interval,
    ));

    tokio::spawn(spawn_deployment_policy_manager(cfg, deployment_pool));
}

/// Runs the periodic flake polling loop to check for new commits
async fn run_flake_polling_loop(pool: PgPool, flake_config: FlakeConfig) {
    info!("🔄 Starting periodic flake polling loop...");
    loop {
        // Get all flakes from database instead of just config ones
        match get_all_flakes_from_db(&pool, &flake_config).await {
            Ok(db_flakes) => {
                if let Err(e) = sync_all_watched_flakes_commits(&pool, &db_flakes).await {
                    error!("❌ Error in flake polling cycle: {e}");
                }
            }
            Err(e) => error!("❌ Failed to get flakes from database: {e}"),
        }
        tokio::time::sleep(flake_config.flake_polling_interval).await;
    }
}

/// Runs the periodic commit evaluation check loop
pub async fn run_commit_evaluation_loop(pool: PgPool, interval: Duration) {
    info!(
        "🔁 Starting periodic commit evaluation check loop (every {:?})...",
        interval
    );

    // `PgPool` is cheap to clone; keep an owned copy in the task.
    let pool = pool.clone();

    // Use an interval ticker to avoid accumulating sleep drift.
    let mut ticker = time::interval_at(Instant::now() + interval, interval);

    loop {
        if let Err(e) = process_pending_commits(&pool).await {
            error!("❌ Error in commit evaluation cycle: {e}");
        }
        ticker.tick().await;
    }
}

async fn process_pending_commits(pool: &PgPool) -> Result<()> {
    match get_commits_pending_evaluation(&pool).await {
        Ok(pending_commits) => {
            info!("📌 Found {} pending commits", pending_commits.len());
            for commit in pending_commits {
                // Get flake info
                let flake = match commit.get_flake(&pool).await {
                    Ok(flake) => flake,
                    Err(e) => {
                        error!(
                            "❌ Failed to get flake for commit {}: {}",
                            commit.git_commit_hash, e
                        );
                        continue;
                    }
                };

                // Load Crystal Forge config to get build settings
                let cfg = match CrystalForgeConfig::load() {
                    Ok(cfg) => cfg,
                    Err(e) => {
                        error!("❌ Failed to load config: {}", e);
                        continue;
                    }
                };
                let build_config = cfg.get_build_config();

                // Set up deployment policies - check CF agent for all systems
                // Using non-strict mode to collect data without failing evaluations
                let policies = vec![DeploymentPolicy::RequireCrystalForgeAgent { strict: false }];

                // Use nix-eval-jobs to discover AND evaluate all nixosConfigurations
                // This will:
                // 1. Evaluate all systems in parallel
                // 2. Check deployment policies (CF agent status) for each system
                // 3. Store policy results in database (cf_agent_enabled column)
                // 4. Insert/update derivation records
                match evaluate_with_nix_eval_jobs(
                    pool,
                    &commit,
                    &flake,
                    &flake.repo_url,
                    &commit.git_commit_hash,
                    "all", // Evaluate all systems
                    &build_config,
                    &policies, // Check deployment policies
                )
                .await
                {
                    Ok((results, policy_checks)) => {
                        let total = results.len();
                        let with_agent = policy_checks
                            .iter()
                            .filter(|check| check.cf_agent_enabled == Some(true))
                            .count();

                        info!(
                            "✅ Evaluated {} NixOS configurations for commit {}",
                            total, commit.git_commit_hash
                        );
                        info!(
                            "   CF agent: {}/{} systems enabled ({:.1}%)",
                            with_agent,
                            policy_checks.len(),
                            if policy_checks.len() > 0 {
                                (with_agent as f64 / policy_checks.len() as f64) * 100.0
                            } else {
                                0.0
                            }
                        );

                        // Log any policy warnings
                        for check in policy_checks.iter().filter(|c| !c.meets_requirements) {
                            for warning in &check.warnings {
                                warn!("⚠️  {}: {}", check.system_name, warning);
                            }
                        }
                    }
                    Err(e) => {
                        error!(
                            "❌ Failed to evaluate commit {}: {}",
                            commit.git_commit_hash, e
                        );

                        // Increment attempt count so we don't retry forever
                        if let Err(inc_err) =
                            increment_commit_list_attempt_count(&pool, &commit).await
                        {
                            error!("❌ Failed to increment attempt count: {}", inc_err);
                        }
                    }
                }
            }
        }
        Err(e) => error!("❌ Failed to get pending commits: {e}"),
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

    log_builder_worker_status().await;
    // Task/thread count
    if let Ok(contents) = tokio::fs::read_to_string("/proc/self/stat").await {
        if let Some(num_threads) = contents.split_whitespace().nth(19) {
            debug!("📊 Threads: {}", num_threads);
        }
    }
}
