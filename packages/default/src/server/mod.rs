use crate::deployment::spawn_deployment_policy_manager;
use crate::flake::commits::sync_all_watched_flakes_commits;
use crate::log::log_builder_worker_status;
use crate::models::config::{CrystalForgeConfig, FlakeConfig};
use crate::models::derivations::build_agent_target;
use crate::queries::commits::get_commit_distance_from_head;
use crate::queries::derivations::{
    get_pending_dry_run_derivations, handle_derivation_failure, insert_derivation_with_target,
    mark_derivation_dry_run_in_progress, update_scheduled_at,
};
use crate::queries::flakes::get_all_flakes_from_db;
use anyhow::Result;
use futures::stream;
use futures::stream::StreamExt;
use tokio::time::interval;

use sqlx::PgPool;
use tokio::time::Duration;
use tracing::{debug, error, info, warn};

use crate::flake::eval::list_nixos_configurations_from_commit;
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
    tokio::spawn(run_derivation_evaluation_loop(
        target_pool,
        flake_config.build_processing_interval,
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

                        // Get flake info to build the derivation target
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

                        for derivation_name in nixos_targets {
                            // Build the complete flake target string
                            let derivation_target = build_agent_target(
                                &flake.repo_url,
                                &commit.git_commit_hash,
                                &derivation_name,
                            );

                            match insert_derivation_with_target(
                                &pool,
                                Some(&commit),
                                &derivation_name,
                                target_type,
                                Some(&derivation_target),
                            )
                            .await
                            {
                                Ok(_) => info!(
                                    "✅ Inserted NixOS derivation: {} (commit {}) with target: {}",
                                    derivation_name, commit.git_commit_hash, derivation_target
                                ),
                                Err(e) => {
                                    error!(
                                        "❌ Failed to insert NixOS derivation for {}: {}",
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

    let cfg = CrystalForgeConfig::load().unwrap_or_else(|e| {
        warn!("Failed to load Crystal Forge config: {}, using defaults", e);
        CrystalForgeConfig::default()
    });
    let build_config = cfg.get_build_config();

    match get_pending_dry_run_derivations(pool).await {
        Ok(pending_targets) => {
            info!("📦 Found {} pending targets", pending_targets.len());

            let concurrency_limit = 20;

            // Initialize worker status
            let dry_run_status = crate::log::get_dry_run_status();
            {
                let mut status = dry_run_status.write().await;
                status.clear();
                for i in 0..concurrency_limit {
                    status.push(crate::log::WorkerStatus {
                        worker_id: i,
                        current_task: None,
                        started_at: None,
                        state: crate::log::WorkerState::Idle,
                    });
                }
            }

            stream::iter(
                pending_targets
                    .into_iter()
                    .enumerate()
                    .map(|(idx, mut target)| {
                        let pool = pool.clone();
                        let build_config = build_config.clone();
                        let worker_id = idx % concurrency_limit;

                        async move {
                            // Get commit info for this derivation
                            let task_description = if let Some(commit_id) = target.commit_id {
                                match crate::queries::commits::get_commit_by_id(&pool, commit_id)
                                    .await
                                {
                                    Ok(commit) => {
                                        // Get distance from HEAD if possible
                                        let distance_info = match commit.get_flake(&pool).await {
                                            Ok(flake) => {
                                                match get_commit_distance_from_head(
                                                    &pool, &flake, &commit,
                                                )
                                                .await
                                                {
                                                    Ok(distance) => format!(" (HEAD~{})", distance),
                                                    Err(_) => String::new(),
                                                }
                                            }
                                            Err(_) => String::new(),
                                        };

                                        format!(
                                            "{} @ {}{}",
                                            target.derivation_name,
                                            &commit.git_commit_hash[..8],
                                            distance_info
                                        )
                                    }
                                    Err(_) => {
                                        format!("{} @ commit#{}", target.derivation_name, commit_id)
                                    }
                                }
                            } else {
                                target.derivation_name.clone()
                            };

                            // Mark worker as working
                            {
                                let mut status = dry_run_status.write().await;
                                if let Some(worker) = status.get_mut(worker_id) {
                                    worker.state = crate::log::WorkerState::Working;
                                    worker.current_task = Some(task_description.clone());
                                    worker.started_at = Some(std::time::Instant::now());
                                }
                            }

                            if let Err(e) =
                                mark_derivation_dry_run_in_progress(&pool, target.id).await
                            {
                                error!("❌ Failed to mark target in-progress: {e}");
                                // Mark worker as idle
                                let mut status = dry_run_status.write().await;
                                if let Some(worker) = status.get_mut(worker_id) {
                                    worker.state = crate::log::WorkerState::Idle;
                                    worker.current_task = None;
                                    worker.started_at = None;
                                }
                                return;
                            }

                            let start = std::time::Instant::now();
                            match target.evaluate_and_build(&pool, false, &build_config).await {
                                Ok(_derivation_path) => {
                                    let duration = start.elapsed();
                                    info!(
                                        "✅ Completed dry-run for {} in {:.2}s",
                                        task_description,
                                        duration.as_secs_f64()
                                    );
                                }
                                Err(e) => {
                                    error!("❌ Failed dry-run for {}: {}", task_description, e);
                                    if let Err(handle_err) =
                                        handle_derivation_failure(&pool, &target, "dry-run", &e)
                                            .await
                                    {
                                        error!(
                                            "❌ Failed to handle derivation failure: {handle_err}"
                                        );
                                    }
                                }
                            }

                            // Mark worker as idle
                            let mut status = dry_run_status.write().await;
                            if let Some(worker) = status.get_mut(worker_id) {
                                worker.state = crate::log::WorkerState::Idle;
                                worker.current_task = None;
                                worker.started_at = None;
                            }
                        }
                    }),
            )
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

    log_builder_worker_status().await;
    // Task/thread count
    if let Ok(contents) = tokio::fs::read_to_string("/proc/self/stat").await {
        if let Some(num_threads) = contents.split_whitespace().nth(19) {
            debug!("📊 Threads: {}", num_threads);
        }
    }
}
