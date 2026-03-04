use crate::config::{CrystalForgeConfig, FlakeConfig};
use crate::deployment::spawn_deployment_policy_manager;
use crate::flake::commits::sync_all_watched_flakes_commits;
use crate::log::log_builder_worker_status;
use crate::models::commits::Commit;
use crate::models::deployment_policies::DeploymentPolicy;
use crate::models::evaluate_with_policies::evaluate_with_nix_eval_jobs;
use crate::models::flakes::Flake;
use crate::queue::QueueNotifier;
// NOTE: removed increment_commit_list_attempt_count – we now rely on the new evaluation_* fields
use crate::queries::flakes::get_all_flakes_from_db;
use anyhow::Result;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::time;
use tokio::time::Duration;
use tokio::time::Instant;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

// ⬇️ bring in the commit-eval helpers you said you added in queries/commits.rs
use crate::queries::build_jobs::create_build_jobs_for_commit;
use crate::queries::builders::cleanup_expired_build_logs;
use crate::queries::commits::{
    get_commits_pending_evaluation, mark_commit_evaluation_complete, mark_commit_evaluation_failed,
    mark_commit_evaluation_started, reset_stuck_commit_evaluations,
};
use crate::queries::derivations::{cleanup_partial_derivations, reset_stuck_builds};

pub fn spawn_background_tasks(
    cfg: CrystalForgeConfig,
    pool: PgPool,
    cf_state: Arc<crate::handlers::agent_request::CFState>,
    queue_notifier: Arc<QueueNotifier>,
) {
    let flake_pool = pool.clone();
    let commit_pool = pool.clone();
    let target_pool = pool.clone();
    let deployment_pool = pool.clone();
    let artifact_pool = pool.clone();
    let build_log_pool = pool.clone();

    // Get the flake config with a fallback
    let flake_config = cfg.flakes.clone();

    tokio::spawn(run_flake_polling_loop(
        flake_pool,
        flake_config.clone(),
        queue_notifier.clone(),
    ));
    tokio::spawn(run_commit_evaluation_loop(
        commit_pool,
        flake_config.commit_evaluation_interval,
        cf_state,
        queue_notifier.clone(),
    ));
    tokio::spawn(run_commit_artifact_hydration_loop(artifact_pool));
    tokio::spawn(run_build_log_retention_loop(
        build_log_pool,
        cfg.server.build_log_retention_days,
        cfg.server.failed_build_log_retention_days,
    ));

    tokio::spawn(spawn_deployment_policy_manager(cfg, deployment_pool));
}

/// Runs daily build log retention cleanup.
///
/// Clears old logs to prevent unbounded growth in build_jobs.logs.
async fn run_build_log_retention_loop(
    pool: PgPool,
    success_retention_days: i32,
    failed_retention_days: i32,
) {
    info!(
        "🔁 Starting build log retention loop (success={}d, failed={}d)",
        success_retention_days, failed_retention_days
    );

    let mut ticker = interval(Duration::from_secs(24 * 60 * 60));

    loop {
        match cleanup_expired_build_logs(&pool, success_retention_days, failed_retention_days).await
        {
            Ok((success_cleared, failed_cleared)) => {
                if success_cleared > 0 || failed_cleared > 0 {
                    info!(
                        "🧹 Cleared expired build logs: success={}, failed={}",
                        success_cleared, failed_cleared
                    );
                } else {
                    debug!("Build log retention: no expired logs to clear");
                }
            }
            Err(err) => {
                error!("❌ Build log retention cleanup failed: {:#}", err);
            }
        }

        ticker.tick().await;
    }
}

/// Runs the periodic flake polling loop to check for new commits
async fn run_flake_polling_loop(
    pool: PgPool,
    flake_config: FlakeConfig,
    queue_notifier: Arc<QueueNotifier>,
) {
    info!("🔄 Starting periodic flake polling loop...");
    loop {
        // Get all flakes from database instead of just config ones
        match get_all_flakes_from_db(&pool, &flake_config).await {
            Ok(db_flakes) => {
                if !db_flakes.is_empty() {
                    match sync_all_watched_flakes_commits(&pool, &db_flakes).await {
                        Ok(total_inserted) => {
                            if total_inserted > 0 {
                                info!("📥 Inserted {} new commits, notifying eval queue", total_inserted);
                                queue_notifier.notify_eval_queue();
                            }
                        }
                        Err(e) => error!("❌ Error in flake polling cycle: {e}"),
                    }
                }
            }
            Err(e) => error!("❌ Failed to get flakes from database: {e}"),
        }
        tokio::time::sleep(flake_config.flake_polling_interval).await;
    }
}

/// Runs the event-driven commit evaluation loop with fallback polling.
///
/// Uses `tokio::select!` to listen for:
/// 1. Queue notifications (immediate processing when commits arrive)
/// 2. Periodic ticker (fallback to catch any missed notifications)
pub async fn run_commit_evaluation_loop(
    pool: PgPool,
    interval: Duration,
    cf_state: Arc<crate::handlers::agent_request::CFState>,
    queue_notifier: Arc<QueueNotifier>,
) {
    info!(
        "🔁 Starting event-driven commit evaluation loop (fallback every {:?})...",
        interval
    );

    // ⬇️ cleanup any stranded 'in_progress' from previous runs
    if let Err(e) = reset_stuck_commit_evaluations(&pool).await {
        error!("❌ Failed to reset stuck commit evaluations: {}", e);
    }

    if let Err(e) = reset_stuck_builds(&pool).await {
        error!("❌ Failed to reset stuck builds: {}", e);
    }

    if let Err(e) = cleanup_partial_derivations(&pool).await {
        error!("❌ Failed to reset partial derivations: {}", e);
    }

    // `PgPool` is cheap to clone; keep an owned copy in the task.
    let pool = pool.clone();

    // Use an interval ticker as fallback to catch missed notifications
    let mut ticker = time::interval_at(Instant::now() + interval, interval);

    loop {
        // ALWAYS check for pending work first (in case notification was sent before we started waiting)
        if let Err(e) = process_pending_commits(&pool, &cf_state, &queue_notifier).await {
            error!("❌ Error in commit evaluation cycle: {e}");
        }

        // Wait for either a notification or the periodic ticker before checking again
        tokio::select! {
            _ = ticker.tick() => {
                debug!("⏰ Eval loop: periodic tick (fallback polling)");
            }
            _ = queue_notifier.wait_for_eval_work() => {
                debug!("🔔 Eval loop: notified of new work");
            }
        }
    }
}

async fn process_pending_commits(
    pool: &PgPool,
    cf_state: &Arc<crate::handlers::agent_request::CFState>,
    queue_notifier: &Arc<QueueNotifier>,
) -> Result<()> {
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
                    let error_text = e.to_string();
                    if error_text.contains("another commit is already being evaluated") {
                        debug!(
                            "⏭️ Eval start race for commit {} ({}): another worker/loop iteration already claimed in_progress",
                            commit.id,
                            commit.git_commit_hash
                        );
                    } else {
                        error!(
                            "❌ Could not mark commit {} evaluation started: {}",
                            commit.git_commit_hash, e
                        );
                    }
                    continue;
                }
                
                // CRITICAL: Create broadcast channel BEFORE eval starts
                // This ensures WebSocket clients can subscribe before messages are sent
                crate::handlers::api::commits::ensure_eval_channel(&cf_state, commit.id).await;
                
                // Broadcast eval start status to WebSocket clients
                crate::handlers::api::commits::broadcast_eval_status(
                    &cf_state,
                    commit.id,
                    "started".to_string(),
                    Some(format!("Starting evaluation for commit {}", &commit.git_commit_hash[..7.min(commit.git_commit_hash.len())])),
                ).await;
                crate::handlers::api::commits::broadcast_eval_log(
                    &cf_state,
                    commit.id,
                    format!("🚀 Starting evaluation for commit {}", commit.git_commit_hash)
                ).await;

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
                    Some(&cf_state), // Pass CFState for WebSocket broadcasting
                )
                .await
                {
                    Ok((results, policy_checks)) => {
                        // Broadcast completion status
                        crate::handlers::api::commits::broadcast_eval_status(
                            &cf_state,
                            commit.id,
                            "complete".to_string(),
                            Some(format!("Evaluated {} systems", results.len())),
                        ).await;
                        crate::handlers::api::commits::broadcast_eval_log(
                            &cf_state,
                            commit.id,
                            format!("✅ Evaluation complete for commit {}", commit.git_commit_hash)
                        ).await;
                        
                        // Cleanup WebSocket broadcast channel
                        crate::handlers::api::commits::cleanup_eval_channel(&cf_state, commit.id).await;
                        
                        // ⬇️ mark COMPLETE
                        if let Err(e) = mark_commit_evaluation_complete(pool, commit.id).await {
                            error!(
                                "❌ Failed to mark commit {} evaluation complete: {}",
                                commit.git_commit_hash, e
                            );
                        }

                        // ⬇️ CREATE BUILD JOBS for evaluated derivations
                        match create_build_jobs_for_commit(pool, commit.id).await {
                            Ok(job_count) if job_count > 0 => {
                                info!(
                                    "📋 Queued {} build jobs for commit {}, notifying build workers",
                                    job_count, commit.git_commit_hash
                                );
                                // Notify build queue that new work is available
                                queue_notifier.notify_build_queue();
                            }
                            Ok(_) => {
                                debug!(
                                    "No new build jobs for commit {} (already queued or no ready derivations)",
                                    commit.git_commit_hash
                                );
                            }
                            Err(e) => {
                                error!(
                                    "❌ Failed to create build jobs for commit {}: {}",
                                    commit.git_commit_hash, e
                                );
                                // Don't fail the whole evaluation if job creation fails
                            }
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
                        
                        // Broadcast failure status
                        crate::handlers::api::commits::broadcast_eval_status(
                            &cf_state,
                            commit.id,
                            "failed".to_string(),
                            Some(format!("Evaluation failed: {}", e)),
                        ).await;
                        crate::handlers::api::commits::broadcast_eval_log(
                            &cf_state,
                            commit.id,
                            format!("❌ Evaluation failed: {}", e)
                        ).await;
                        
                        // Cleanup WebSocket broadcast channel
                        crate::handlers::api::commits::cleanup_eval_channel(&cf_state, commit.id).await;

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
    let mut ticker = interval(Duration::from_secs(30)); // Check every 30 seconds

    loop {
        ticker.tick().await;

        // Process up to 3 commits per cycle (sequential to avoid overwhelming nix eval)
        match get_commits_needing_artifact_cache(&pool, 3).await {
            Ok(commits) if !commits.is_empty() => {
                for (commit_id, commit_hash, repo_url) in commits {
                    info!(
                        "🔍 Hydrating commit artifacts for {} @ {}",
                        repo_url, commit_hash
                    );

                    // Try to get nixosConfigurations
                    let configs = match get_commit_nixos_configurations(
                        &repo_url,
                        &[commit_hash.clone()],
                    )
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
                error!(
                    "❌ Failed to query commits needing artifact cache: {:#}",
                    err
                );
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
