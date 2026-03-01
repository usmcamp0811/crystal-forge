use crate::config::{CrystalForgeConfig, FlakeConfig};
use crate::deployment::spawn_deployment_policy_manager;
use crate::flake::commits::sync_all_watched_flakes_commits;
use crate::log::log_builder_worker_status;
use crate::models::commits::Commit;
use crate::models::deployment_policies::DeploymentPolicy;
use crate::models::evaluate_with_policies::evaluate_with_nix_eval_jobs;
use crate::models::flakes::Flake;
// NOTE: removed increment_commit_list_attempt_count – we now rely on the new evaluation_* fields
use crate::queries::flakes::get_all_flakes_from_db;
use anyhow::Result;
use sqlx::PgPool;
use tokio::time;
use tokio::time::Duration;
use tokio::time::Instant;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

// ⬇️ bring in the commit-eval helpers you said you added in queries/commits.rs
use crate::queries::commits::{
    get_commits_pending_evaluation, mark_commit_evaluation_complete, mark_commit_evaluation_failed,
    mark_commit_evaluation_started, reset_stuck_commit_evaluations,
};
use crate::queries::derivations::cleanup_partial_derivations;

pub fn spawn_background_tasks(cfg: CrystalForgeConfig, pool: PgPool) {
    let flake_pool = pool.clone();
    let commit_pool = pool.clone();
    let target_pool = pool.clone();
    let deployment_pool = pool.clone();
    let artifact_pool = pool.clone();

    // Get the flake config with a fallback
    let flake_config = cfg.flakes.clone();

    tokio::spawn(run_flake_polling_loop(flake_pool, flake_config.clone()));
    tokio::spawn(run_commit_evaluation_loop(
        commit_pool,
        flake_config.commit_evaluation_interval,
    ));
    tokio::spawn(run_commit_artifact_hydration_loop(artifact_pool));

    tokio::spawn(spawn_deployment_policy_manager(cfg, deployment_pool));
}

/// Runs the periodic flake polling loop to check for new commits
async fn run_flake_polling_loop(pool: PgPool, flake_config: FlakeConfig) {
    info!("🔄 Starting periodic flake polling loop...");
    loop {
        // Get all flakes from database instead of just config ones
        match get_all_flakes_from_db(&pool, &flake_config).await {
            Ok(db_flakes) => {
                if !db_flakes.is_empty() {
                    if let Err(e) = sync_all_watched_flakes_commits(&pool, &db_flakes).await {
                        error!("❌ Error in flake polling cycle: {e}");
                    }
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

    // ⬇️ cleanup any stranded 'in_progress' from previous runs
    if let Err(e) = reset_stuck_commit_evaluations(&pool).await {
        error!("❌ Failed to reset stuck commit evaluations: {}", e);
    }

    if let Err(e) = cleanup_partial_derivations(&pool).await {
        error!("❌ Failed to reset partial derivations: {}", e);
    }

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
                let server_config = cfg.get_server_config();

                // Set up deployment policies - check CF agent for all systems
                // Using non-strict mode to collect data without failing evaluations
                let policies = vec![DeploymentPolicy::RequireCrystalForgeAgent { strict: false }];

                // ⬇️ mark STARTED (bumps evaluation_attempt_count internally)
                if let Err(e) = mark_commit_evaluation_started(pool, commit.id).await {
                    error!(
                        "❌ Could not mark commit {} evaluation started: {}",
                        commit.git_commit_hash, e
                    );
                    continue;
                }

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
                    &server_config,
                    &policies, // Check deployment policies
                )
                .await
                {
                    Ok((results, policy_checks)) => {
                        // ⬇️ mark COMPLETE
                        if let Err(e) = mark_commit_evaluation_complete(pool, commit.id).await {
                            error!(
                                "❌ Failed to mark commit {} evaluation complete: {}",
                                commit.git_commit_hash, e
                            );
                        }

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

                        // ⬇️ mark FAILED (function will set 'pending' or terminal 'failed'
                        // depending on attempt limit inside your SQL)
                        if let Err(mark_err) =
                            mark_commit_evaluation_failed(pool, commit.id, &e.to_string()).await
                        {
                            error!(
                                "❌ Failed to mark commit {} evaluation failed: {}",
                                commit.git_commit_hash, mark_err
                            );
                        }
                    }
                }
            }
        }
        Err(e) => error!("❌ Failed to get pending commits: {e}"),
    }
    Ok(())
}

/// Background task to hydrate commit artifact cache (nixosConfigurations + changed files).
/// Processes commits with missing cache entries, with progressive backoff on failure.
async fn run_commit_artifact_hydration_loop(pool: PgPool) {
    use crate::flake::commits::{get_commit_changed_files, get_commit_nixos_configurations};
    use crate::queries::commits_artifacts::{
        get_commits_needing_artifact_cache, mark_commit_artifact_hydration_failed,
        upsert_commit_artifact_cache,
    };

    info!("🔁 Starting commit artifact hydration background task...");

    let pool = pool.clone();
    let mut ticker = interval(Duration::from_secs(120)); // Check every 2 minutes

    loop {
        ticker.tick().await;

        // Process 1 commit at a time to avoid overwhelming nix eval
        match get_commits_needing_artifact_cache(&pool, 1).await {
            Ok(commits) if !commits.is_empty() => {
                for (commit_id, commit_hash, repo_url) in commits {
                    info!(
                        "🔍 Hydrating commit artifacts for {} @ {}",
                        repo_url, commit_hash
                    );

                    // Try to get nixosConfigurations
                    let configs = match get_commit_nixos_configurations(&repo_url, &[commit_hash.clone()])
                        .await
                        .remove(&commit_hash)
                    {
                        Some(configs) => configs,
                        None => {
                            warn!(
                                "⚠️  Failed to get nixosConfigurations for {} @ {}, marking as failed",
                                repo_url, commit_hash
                            );
                            let _ = mark_commit_artifact_hydration_failed(&pool, commit_id).await;
                            continue;
                        }
                    };

                    // Try to get changed files (best effort)
                    let changed_files = get_commit_changed_files(&repo_url, &[commit_hash.clone()])
                        .await
                        .ok()
                        .and_then(|mut map| map.remove(&commit_hash))
                        .unwrap_or_default();

                    // Persist to cache
                    match upsert_commit_artifact_cache(&pool, commit_id, &configs, &changed_files)
                        .await
                    {
                        Ok(_) => {
                            info!(
                                "✅ Cached {} configs, {} files for {} @ {}",
                                configs.len(),
                                changed_files.len(),
                                repo_url,
                                commit_hash
                            );
                        }
                        Err(err) => {
                            error!(
                                "❌ Failed to persist cache for {} @ {}: {:#}",
                                repo_url, commit_hash, err
                            );
                        }
                    }
                }
            }
            Ok(_) => {
                debug!("No commits need artifact hydration");
            }
            Err(err) => {
                error!("❌ Failed to query commits needing artifact cache: {:#}", err);
            }
        }
    }
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
