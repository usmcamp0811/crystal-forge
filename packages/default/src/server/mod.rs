use crate::deployment::spawn_deployment_policy_manager;
use crate::flake::commits::sync_all_watched_flakes_commits;
use crate::log::log_builder_worker_status;
use crate::models::commits::Commit;
use crate::models::config::{CrystalForgeConfig, FlakeConfig};
use crate::models::derivations::NixEvalJobResult;
use crate::models::derivations::build_agent_target;
use crate::models::flakes::Flake;
use crate::queries::commits::get_commit_distance_from_head;
use crate::queries::commits::increment_commit_list_attempt_count;
use crate::queries::derivations::{
    get_pending_dry_run_derivations, handle_derivation_failure, insert_derivation_with_target,
    mark_derivation_dry_run_in_progress, update_scheduled_at,
};
use crate::queries::flakes::get_all_flakes_from_db;
use anyhow::{Context, Result, bail};
use futures::stream;
use futures::stream::StreamExt;
use serde_json::Value;
use std::process::Stdio;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Command;
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
    // tokio::spawn(run_derivation_evaluation_loop(
    //     target_pool,
    //     flake_config.build_processing_interval,
    // ));

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

// Runs the periodic evaluation target resolution loop
// async fn run_derivation_evaluation_loop(pool: PgPool, interval: Duration) {
//     info!(
//         "🔍 Starting periodic evaluation target check loop (every {:?})...",
//         interval
//     );
//     loop {
//         if let Err(e) = process_pending_derivations(&pool).await {
//             error!("❌ Error in target evaluation cycle: {e}");
//         }
//         tokio::time::sleep(interval).await;
//     }
// }

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

                // Use nix-eval-jobs to discover AND evaluate all nixosConfigurations in one pass
                match evaluate_and_discover_nixos_configs(pool, &commit, &flake).await {
                    Ok(count) => {
                        info!(
                            "✅ Evaluated and inserted {} NixOS configurations for commit {}",
                            count, commit.git_commit_hash
                        );
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

// async fn process_pending_derivations(pool: &PgPool) -> Result<()> {
//     update_scheduled_at(pool).await?;
//
//     let cfg = CrystalForgeConfig::load().unwrap_or_else(|e| {
//         warn!("Failed to load Crystal Forge config: {}, using defaults", e);
//         CrystalForgeConfig::default()
//     });
//     let build_config = cfg.get_build_config();
//
//     match get_pending_dry_run_derivations(pool).await {
//         Ok(pending_targets) => {
//             info!("📦 Found {} pending targets", pending_targets.len());
//
//             let concurrency_limit = 5;
//
//             // Initialize worker status
//             let dry_run_status = crate::log::get_dry_run_status();
//             {
//                 let mut status = dry_run_status.write().await;
//                 status.clear();
//                 for i in 0..concurrency_limit {
//                     status.push(crate::log::WorkerStatus {
//                         worker_id: i,
//                         current_task: None,
//                         started_at: None,
//                         state: crate::log::WorkerState::Idle,
//                     });
//                 }
//             }
//
//             stream::iter(
//                 pending_targets
//                     .into_iter()
//                     .enumerate()
//                     .map(|(idx, mut target)| {
//                         let pool = pool.clone();
//                         let build_config = build_config.clone();
//                         let worker_id = idx % concurrency_limit;
//
//                         async move {
//                             // Get commit info for this derivation
//                             let task_description = if let Some(commit_id) = target.commit_id {
//                                 match crate::queries::commits::get_commit_by_id(&pool, commit_id)
//                                     .await
//                                 {
//                                     Ok(commit) => {
//                                         // Get distance from HEAD if possible
//                                         let distance_info = match commit.get_flake(&pool).await {
//                                             Ok(flake) => {
//                                                 match get_commit_distance_from_head(
//                                                     &pool, &flake, &commit,
//                                                 )
//                                                 .await
//                                                 {
//                                                     Ok(distance) => format!(" (HEAD~{})", distance),
//                                                     Err(_) => String::new(),
//                                                 }
//                                             }
//                                             Err(_) => String::new(),
//                                         };
//
//                                         format!(
//                                             "{} @ {}{}",
//                                             target.derivation_name,
//                                             &commit.git_commit_hash[..8],
//                                             distance_info
//                                         )
//                                     }
//                                     Err(_) => {
//                                         format!("{} @ commit#{}", target.derivation_name, commit_id)
//                                     }
//                                 }
//                             } else {
//                                 target.derivation_name.clone()
//                             };
//
//                             // Mark worker as working
//                             {
//                                 let mut status = dry_run_status.write().await;
//                                 if let Some(worker) = status.get_mut(worker_id) {
//                                     worker.state = crate::log::WorkerState::Working;
//                                     worker.current_task = Some(task_description.clone());
//                                     worker.started_at = Some(std::time::Instant::now());
//                                 }
//                             }
//
//                             if let Err(e) =
//                                 mark_derivation_dry_run_in_progress(&pool, target.id).await
//                             {
//                                 error!("❌ Failed to mark target in-progress: {e}");
//                                 // Mark worker as idle
//                                 let mut status = dry_run_status.write().await;
//                                 if let Some(worker) = status.get_mut(worker_id) {
//                                     worker.state = crate::log::WorkerState::Idle;
//                                     worker.current_task = None;
//                                     worker.started_at = None;
//                                 }
//                                 return;
//                             }
//
//                             let start = std::time::Instant::now();
//                             match target.evaluate_and_build(&pool, false, &build_config).await {
//                                 Ok(_derivation_path) => {
//                                     let duration = start.elapsed();
//                                     info!(
//                                         "✅ Completed dry-run for {} in {:.2}s",
//                                         task_description,
//                                         duration.as_secs_f64()
//                                     );
//                                 }
//                                 Err(e) => {
//                                     error!("❌ Failed dry-run for {}: {}", task_description, e);
//                                     if let Err(handle_err) =
//                                         handle_derivation_failure(&pool, &target, "dry-run", &e)
//                                             .await
//                                     {
//                                         error!(
//                                             "❌ Failed to handle derivation failure: {handle_err}"
//                                         );
//                                     }
//                                 }
//                             }
//
//                             // Mark worker as idle
//                             let mut status = dry_run_status.write().await;
//                             if let Some(worker) = status.get_mut(worker_id) {
//                                 worker.state = crate::log::WorkerState::Idle;
//                                 worker.current_task = None;
//                                 worker.started_at = None;
//                             }
//                         }
//                     }),
//             )
//             .for_each_concurrent(concurrency_limit, |fut| fut)
//             .await;
//         }
//         Err(e) => error!("❌ Failed to get pending targets: {e}"),
//     }
//     Ok(())
// }

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

/// Use nix-eval-jobs to discover AND evaluate all nixosConfigurations in one parallel pass
pub async fn evaluate_and_discover_nixos_configs(
    pool: &PgPool,
    commit: &Commit,
    flake: &Flake,
) -> Result<usize> {
    let workers = 8usize;
    let max_mem_mb = 4096usize;

    // 1) Get narHash once (cache/store in DB later if you want)
    let nar_hash = prefetch_nar_hash(&flake.repo_url, &commit.git_commit_hash).await?;

    // 2) Pure expr: fetchTree -> getFlake "path:${src}"
    let nix_expr = format!(
        r#"
        let
          src = builtins.fetchTree {{
            type = "git";
            url  = "{url}";
            // keep `ref` if you use it in your workflow; it's ignored if `rev` is present
            ref  = "nixos";
            rev  = "{rev}";
            narHash = "{nar}";
          }};
          flake = builtins.getFlake "path:${{src}}";
        in
          builtins.mapAttrs (_: cfg: cfg.config.system.build.toplevel)
            flake.nixosConfigurations
        "#,
        url = flake.repo_url.trim_start_matches("git+"),
        rev = commit.git_commit_hash,
        nar = nar_hash,
    );

    let mut cmd = Command::new("nix-eval-jobs");
    cmd.env("HOME", "/var/lib/crystal-forge")
        .env("XDG_CONFIG_HOME", "/var/lib/crystal-forge/.config")
        .env("NIX_CONF_DIR", "/dev/null")
        .env("NIX_USER_CONF_FILES", "/dev/null")
        .env("NIX_REGISTRY", "/dev/null")
        .env(
            "NIX_CONFIG",
            "experimental-features = nix-command flakes\nflake-registry =\n",
        )
        .args([
            "--expr",
            &nix_expr,
            "--workers",
            &workers.to_string(),
            "--max-memory-size",
            &max_mem_mb.to_string(),
            "--check-cache-status",
            "--meta",
            "--show-trace",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // ✅ Log the exact command and environment before running
    let env_preview = [
        ("HOME", "/var/lib/crystal-forge"),
        ("XDG_CONFIG_HOME", "/var/lib/crystal-forge/.config"),
        ("NIX_CONF_DIR", "/dev/null"),
        ("NIX_USER_CONF_FILES", "/dev/null"),
        ("NIX_REGISTRY", "/dev/null"),
        (
            "NIX_CONFIG",
            "experimental-features = nix-command flakes\nflake-registry =\n",
        ),
    ];
    let cmd_str = format!(
        "nix-eval-jobs -- --expr '{}' --workers {} --max-memory-size {} --check-cache-status --meta",
        nix_expr.replace('\n', " "),
        workers,
        max_mem_mb
    );
    debug!(
        "💻 Executing command:\n{}\nEnvironment:\n{:#?}",
        cmd_str, env_preview
    );

    let mut child = cmd.spawn()?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    let mut eval_results = Vec::<NixEvalJobResult>::new();
    let mut stderr_output = Vec::<String>::new();
    let (mut stdout_done, mut stderr_done) = (false, false);

    loop {
        tokio::select! {
            line = stdout_reader.next_line(), if !stdout_done => {
                match line? {
                    Some(line) if !line.trim().is_empty() => {
                        match serde_json::from_str::<NixEvalJobResult>(&line) {
                            Ok(result) => {
                                debug!("📦 Evaluated: attr={:?}, cache={:?}", result.attr_path, result.cache_status);
                                eval_results.push(result);
                            }
                            Err(e) => warn!("Failed to parse nix-eval-jobs json line: {e}\nLine: {line}"),
                        }
                    }
                    Some(_) => {}
                    None => stdout_done = true,
                }
            }
            line = stderr_reader.next_line(), if !stderr_done => {
                match line? {
                    Some(line) => {
                        if line.contains("error:") {
                            error!("nix-eval-jobs: {line}");
                        } else if !line.starts_with("warning:") {
                            debug!("nix-eval-jobs: {line}");
                        }
                        stderr_output.push(line);
                    }
                    None => stderr_done = true,
                }
            }
        }
        if stdout_done && stderr_done {
            break;
        }
    }

    let status = child.wait().await?;

    // ✅ Log full command + captured stderr if it fails
    if !status.success() {
        let stderr_text = stderr_output.join("\n");
        error!(
            "❌ nix-eval-jobs failed (exit code {})\n\
             Command:\n{}\n\
             Environment:\n{:#?}\n\
             Stderr:\n{}",
            status.code().unwrap_or(-1),
            cmd_str,
            env_preview,
            stderr_text
        );
        bail!(
            "nix-eval-jobs failed with exit code: {}\nSee logs above for details.",
            status.code().unwrap_or(-1)
        );
    }

    let status = child.wait().await?;
    if !status.success() {
        let stderr_text = stderr_output.join("\n");
        bail!(
            "nix-eval-jobs failed with exit code: {}\nStderr:\n{}",
            status.code().unwrap_or(-1),
            stderr_text
        );
    }

    info!("✅ Evaluated {} nixosConfigurations", eval_results.len());

    // Insert/update results exactly like before
    let mut inserted_count = 0usize;
    for result in eval_results {
        let system_name = match result.attr_path.first() {
            Some(name) => name.as_str(),
            None => {
                warn!(
                    "Could not extract system name from attrPath: {:?}",
                    result.attr_path
                );
                continue;
            }
        };

        // If evaluation failed for this attr, record the failure
        if let Some(error_msg) = &result.error {
            error!("❌ Evaluation failed for {}: {}", system_name, error_msg);

            let derivation_target =
                build_agent_target(&flake.repo_url, &commit.git_commit_hash, system_name);

            match insert_derivation_with_target(
                pool,
                Some(commit),
                system_name,
                "nixos",
                Some(&derivation_target),
            )
            .await
            {
                Ok(deriv) => {
                    let _ = crate::queries::derivations::update_derivation_status(
                        pool,
                        deriv.id,
                        crate::queries::derivations::EvaluationStatus::DryRunFailed,
                        None,
                        Some(error_msg),
                        None,
                    )
                    .await;
                }
                Err(e) => error!("Failed to insert failed derivation {}: {}", system_name, e),
            }
            continue;
        }

        let drv_path = match &result.drv_path {
            Some(p) => p.as_str(),
            None => {
                warn!("No derivation path for {}", system_name);
                continue;
            }
        };

        let store_path = result
            .outputs
            .as_ref()
            .and_then(|o| o.get("out"))
            .map(|s| s.as_str());

        let derivation_target =
            build_agent_target(&flake.repo_url, &commit.git_commit_hash, system_name);

        match insert_derivation_with_target(
            pool,
            Some(commit),
            system_name,
            "nixos",
            Some(&derivation_target),
        )
        .await
        {
            Ok(deriv) => {
                info!(
                    "✅ Inserted NixOS derivation: {} (commit {}) -> {}",
                    system_name, commit.git_commit_hash, derivation_target
                );

                let _ = crate::queries::derivations::update_derivation_status(
                    pool,
                    deriv.id,
                    crate::queries::derivations::EvaluationStatus::DryRunComplete,
                    Some(drv_path),
                    None,
                    store_path,
                )
                .await;

                if result.cache_status.as_deref() == Some("local") {
                    if let Some(sp) = store_path {
                        info!("💾 {} is already built locally", system_name);
                        let _ = crate::queries::derivations::mark_derivation_build_complete(
                            pool, deriv.id, sp,
                        )
                        .await;
                    }
                }

                inserted_count += 1;
            }
            Err(e) => {
                error!(
                    "❌ Failed to insert NixOS derivation for {}: {}",
                    system_name, e
                );
            }
        }
    }

    Ok(inserted_count)
}

/// Build the base flake reference (git+url?rev=hash)
fn build_flake_reference(repo_url: &str, commit_hash: &str) -> String {
    if repo_url.starts_with("git+") {
        if repo_url.contains("?rev=") {
            repo_url.to_string()
        } else {
            format!("{}?rev={}", repo_url, commit_hash)
        }
    } else {
        let separator = if repo_url.contains('?') { "&" } else { "?" };
        format!("git+{}{separator}rev={}", repo_url, commit_hash)
    }
}

async fn prefetch_nar_hash(repo_url: &str, commit: &str) -> Result<String> {
    // Normalize to a flake URL, preserving any existing query
    let flake_ref = if repo_url.starts_with("git+") {
        if repo_url.contains("?rev=") {
            repo_url.to_string()
        } else {
            format!("{}?rev={}", repo_url, commit)
        }
    } else {
        let sep = if repo_url.contains('?') { "&" } else { "?" };
        format!("git+{}{sep}rev={}", repo_url, commit)
    };

    let output = Command::new("nix")
        .args([
            "flake",
            "prefetch",
            &flake_ref,
            "--json",
            "--extra-experimental-features",
            "nix-command flakes",
        ])
        .env("NIX_CONF_DIR", "/dev/null")
        .env("NIX_USER_CONF_FILES", "/dev/null")
        .env("NIX_REGISTRY", "/dev/null")
        .env(
            "NIX_CONFIG",
            "experimental-features = nix-command flakes\nflake-registry =\n",
        )
        .output()
        .await
        .context("running `nix flake prefetch`")?;

    if !output.status.success() {
        bail!(
            "prefetch failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let v: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parsing prefetch JSON")?;

    // Accept multiple shapes:
    // - Newer Nix: { hash: "sha256-…", storePath: "…" }
    // - Some versions: { locked: { narHash: "sha256-…" }, … }
    // - Older: { narHash: "sha256-…" }
    let nar_hash = v
        .get("locked")
        .and_then(|l| l.get("narHash"))
        .and_then(|s| s.as_str())
        .or_else(|| v.get("narHash").and_then(|s| s.as_str()))
        .or_else(|| v.get("hash").and_then(|s| s.as_str())) // <-- your case
        .map(str::to_owned)
        .ok_or_else(|| {
            let pretty =
                serde_json::to_string_pretty(&v).unwrap_or_else(|_| "<unprintable>".into());
            anyhow::anyhow!(format!(
                "missing narHash/hash in prefetch output; got:\n{}",
                pretty
            ))
        })?;

    Ok(nar_hash)
}
