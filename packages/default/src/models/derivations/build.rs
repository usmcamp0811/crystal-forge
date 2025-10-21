use super::Derivation;
use super::utils::*;
use crate::models::commits::Commit;
use crate::models::config::BuildConfig;
use crate::models::derivations::resolve_drv_to_store_path_static;
use crate::models::flakes::Flake;
use crate::queries::derivations::insert_derivation_with_target;
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::option::Option;
use std::path::Path;
use std::process::{Output, Stdio};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::{Duration, Instant, interval};
use tracing::{debug, error, info, warn};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NixEvalJobResult {
    pub attr: String,
    #[serde(rename = "attrPath")]
    pub attr_path: Vec<String>,
    #[serde(rename = "drvPath")]
    pub drv_path: Option<String>,
    pub outputs: Option<std::collections::HashMap<String, String>>,
    pub system: Option<String>,
    pub error: Option<String>,
    #[serde(rename = "cacheStatus")]
    pub cache_status: Option<String>, // "local", "cached", "notBuilt"
}

#[derive(Debug, Clone)]
pub struct EvaluationResult {
    pub main_derivation_path: String,
    pub dependency_derivation_paths: Vec<String>,
    pub cf_agent_enabled: bool,
    pub store_path: String,
}

impl Derivation {
    /// High-level function that handles both NixOS and Package derivations
    pub async fn evaluate_and_build(
        &mut self,
        pool: &PgPool,
        full_build: bool,
        build_config: &BuildConfig,
    ) -> Result<String> {
        match self.derivation_type {
            crate::models::derivations::DerivationType::NixOS => {
                let commit_id = self.commit_id.ok_or_else(|| {
                    anyhow::anyhow!("Cannot build NixOS derivation without commit_id")
                })?;

                let commit = crate::queries::commits::get_commit_by_id(pool, commit_id).await?;
                let flake = crate::queries::flakes::get_flake_by_id(pool, commit.flake_id).await?;
                let flake_target = build_evaluation_target(
                    &flake.repo_url,
                    &commit.git_commit_hash,
                    &self.derivation_name,
                );

                if full_build {
                    self.evaluate_and_build_nixos(pool, &flake_target, build_config)
                        .await
                } else {
                    // TODO: This can probably go away now
                    self.evaluate_nixos_dry_run(pool, &flake_target, build_config)
                        .await
                }
            }
            crate::models::derivations::DerivationType::Package => {
                let drv_path = self
                    .derivation_path
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Package derivation missing derivation_path"))?;

                if full_build {
                    self.build_derivation_from_path(pool, drv_path, build_config)
                        .await
                } else {
                    Ok(drv_path.clone())
                }
            }
        }
    }

    async fn evaluate_and_build_nixos(
        &self,
        pool: &PgPool,
        flake_target: &str,
        build_config: &BuildConfig,
    ) -> Result<String> {
        // Phase 1: Evaluate
        let eval_result = Self::dry_run_derivation_path(flake_target, build_config).await?;

        // Insert dependencies into database
        if !eval_result.dependency_derivation_paths.is_empty() {
            crate::queries::derivations::discover_and_insert_packages(
                pool,
                self.id,
                &eval_result
                    .dependency_derivation_paths
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>(),
            )
            .await?;
        }

        // Phase 2: Build if needed
        if eval_result.main_derivation_path.ends_with(".drv") {
            self.build_derivation_from_path(pool, &eval_result.main_derivation_path, build_config)
                .await
        } else {
            Ok(eval_result.main_derivation_path)
        }
    }

    /// Refactored dry-run using nix-eval-jobs for parallel evaluation
    async fn evaluate_nixos_dry_run(
        &mut self,
        pool: &PgPool,
        flake_target: &str,
        build_config: &BuildConfig,
    ) -> Result<String> {
        info!("🔍 Starting parallel evaluation with nix-eval-jobs");

        let commit_id = self
            .commit_id
            .ok_or_else(|| anyhow::anyhow!("Cannot evaluate NixOS derivation without commit_id"))?;

        let commit = crate::queries::commits::get_commit_by_id(pool, commit_id).await?;
        let flake = crate::queries::flakes::get_flake_by_id(pool, commit.flake_id).await?;

        // Use nix-eval-jobs to evaluate all nixosConfigurations in parallel
        // This also inserts all discovered systems into the database
        let eval_results = Self::evaluate_with_nix_eval_jobs(
            pool,
            &commit,
            &flake,
            &flake.repo_url,
            &commit.git_commit_hash,
            &self.derivation_name,
            build_config,
        )
        .await?;

        // Find our specific configuration's result
        let main_result = eval_results
            .iter()
            .find(|r| r.attr_path.last() == Some(&self.derivation_name))
            .ok_or_else(|| {
                anyhow!(
                    "Could not find evaluation result for {}",
                    self.derivation_name
                )
            })?;

        if let Some(error) = &main_result.error {
            bail!("Evaluation failed for {}: {}", self.derivation_name, error);
        }

        let main_drv_path = main_result
            .drv_path
            .as_ref()
            .ok_or_else(|| anyhow!("No derivation path in eval result"))?;

        let store_path = main_result
            .outputs
            .as_ref()
            .and_then(|o| o.get("out"))
            .ok_or_else(|| anyhow!("No output path in eval result"))?;

        info!("📦 Main derivation: {}", main_drv_path);
        info!("📦 Store path: {}", store_path);

        // Check cache status
        let cache_status = main_result.cache_status.as_deref().unwrap_or("notBuilt");
        info!("💾 Cache status: {}", cache_status);

        // Check CF agent status
        let cf_agent_enabled = match is_cf_agent_enabled(flake_target, build_config).await {
            Ok(enabled) => enabled,
            Err(e) => {
                warn!("Could not determine Crystal Forge agent status: {}", e);
                false
            }
        };
        self.cf_agent_enabled = Some(cf_agent_enabled);

        // Get complete closure to discover all dependencies
        let all_deps = get_complete_closure_with_cache_status(main_drv_path, build_config).await?;

        let built_count = all_deps.iter().filter(|(_, _, is_built)| *is_built).count();
        info!(
            "📦 Found {} total dependencies: {} already built, {} need building",
            all_deps.len(),
            built_count,
            all_deps.len() - built_count
        );

        // Insert all dependencies
        if !all_deps.is_empty() {
            let all_paths: Vec<&str> = all_deps
                .iter()
                .map(|(drv_path, _, _)| drv_path.as_str())
                .collect();

            crate::queries::derivations::discover_and_insert_packages(pool, self.id, &all_paths)
                .await?;

            // Batch mark already-built derivations as complete
            let built_deps: Vec<(String, String)> = all_deps
                .iter()
                .filter(|(_, _, is_built)| *is_built)
                .map(|(drv_path, store_path, _)| (drv_path.clone(), store_path.clone()))
                .collect();

            if !built_deps.is_empty() {
                let built_drv_paths: Vec<&str> =
                    built_deps.iter().map(|(p, _)| p.as_str()).collect();

                let derivations =
                    crate::queries::derivations::get_derivations_by_paths(pool, &built_drv_paths)
                        .await?;

                let deriv_map: std::collections::HashMap<String, i32> = derivations
                    .into_iter()
                    .filter_map(|d| d.derivation_path.map(|path| (path, d.id)))
                    .collect();

                let mut deriv_ids = Vec::new();
                let mut store_paths = Vec::new();

                for (drv_path, store_path) in built_deps {
                    if let Some(&deriv_id) = deriv_map.get(&drv_path) {
                        deriv_ids.push(deriv_id);
                        store_paths.push(store_path);
                    }
                }

                if !deriv_ids.is_empty() {
                    if let Err(e) = crate::queries::derivations::batch_mark_derivations_complete(
                        pool,
                        &deriv_ids,
                        &store_paths,
                    )
                    .await
                    {
                        warn!("Failed to batch mark derivations as built: {}", e);
                    } else {
                        info!(
                            "✅ Marked {} pre-built derivations as complete",
                            deriv_ids.len()
                        );
                    }
                }
            }
        }

        // Update this derivation's status
        crate::queries::derivations::update_derivation_status(
            pool,
            self.id,
            crate::queries::derivations::EvaluationStatus::DryRunComplete,
            Some(main_drv_path),
            None,
            Some(store_path),
        )
        .await?;

        crate::queries::derivations::update_cf_agent_enabled(pool, self.id, cf_agent_enabled)
            .await?;

        Ok(main_drv_path.clone())
    }

    /// Use nix-eval-jobs to evaluate all nixosConfigurations in parallel
    async fn evaluate_with_nix_eval_jobs(
        pool: &PgPool,
        commit: &Commit,
        flake: &Flake,
        repo_url: &str,
        commit_hash: &str,
        target_system: &str,
        build_config: &BuildConfig,
    ) -> Result<Vec<NixEvalJobResult>> {
        let flake_ref = build_flake_reference(repo_url, commit_hash);

        let mut cmd = Command::new("nix-eval-jobs");
        cmd.args([
            "--flake",
            &format!("{}#nixosConfigurations", flake_ref),
            "--check-cache-status", // This tells us what's already built!
            "--workers",
            &num_cpus::get().to_string(),
            "--max-memory-size",
            "4096", // 4GB per worker
        ]);

        build_config.apply_to_command(&mut cmd);

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        info!("🚀 Running: nix-eval-jobs for {}", target_system);
        debug!(
            "Command: nix-eval-jobs --flake {}#nixosConfigurations",
            flake_ref
        );

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut results = Vec::new();
        let mut found_target = false;
        let mut stderr_output = Vec::new();

        let mut stdout_done = false;
        let mut stderr_done = false;

        // Read both stdout and stderr concurrently
        loop {
            tokio::select! {
                line_result = stdout_reader.next_line(), if !stdout_done => {
                    match line_result? {
                        Some(line) if !line.trim().is_empty() => {
                            match serde_json::from_str::<NixEvalJobResult>(&line) {
                                Ok(result) => {
                                    debug!("📦 Evaluated: attr={:?}, cache={:?}", result.attr_path, result.cache_status);

                                    // **NEW: Insert immediately as we discover systems**
                                    if let Some(system_name) = result.attr_path.last() {
                                        let derivation_target = build_agent_target(
                                            &flake.repo_url,
                                            &commit.git_commit_hash,
                                            system_name
                                        );

                                        match insert_derivation_with_target(
                                            pool,
                                            Some(commit),
                                            system_name,
                                            "nixos",
                                            Some(&derivation_target),
                                        ).await {
                                            Ok(_) => debug!("✅ Inserted/updated {}", system_name),
                                            Err(e) => warn!("⚠️ Failed to insert {}: {}", system_name, e),
                                        }
                                    }

                                    // Check if this is our target system
                                    if result.attr_path.last() == Some(&target_system.to_string()) {
                                        found_target = true;
                                        info!("✅ Found target system: {}", target_system);
                                    }

                                    if let Some(error) = &result.error {
                                        warn!("⚠️ Evaluation error for {}: {}", result.attr, error);
                                    }

                                    results.push(result);
                                }
                                Err(e) => {
                                    warn!("Failed to parse nix-eval-jobs output: {}\nLine: {}", e, line);
                                }
                            }
                        }
                        Some(_) => {}, // empty line
                        None => stdout_done = true,
                    }
                }
                line_result = stderr_reader.next_line(), if !stderr_done => {
                    match line_result? {
                        Some(line) => {
                            // Log stderr immediately for debugging
                            if line.contains("error:") {
                                error!("nix-eval-jobs stderr: {}", line);
                            } else {
                                debug!("nix-eval-jobs stderr: {}", line);
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
        if !status.success() {
            let stderr_text = stderr_output.join("\n");
            bail!(
                "nix-eval-jobs failed with exit code: {}\nStderr:\n{}",
                status.code().unwrap_or(-1),
                stderr_text
            );
        }

        if !found_target {
            bail!(
                "nix-eval-jobs did not evaluate target system: {}\nEvaluated systems: {:?}",
                target_system,
                results.iter().map(|r| r.attr.as_str()).collect::<Vec<_>>()
            );
        }

        info!("✅ Evaluated {} configurations in parallel", results.len());

        Ok(results)
    }

    /// Phase 2: Build a derivation from its .drv path
    pub async fn build_derivation_from_path(
        &self,
        pool: &PgPool,
        drv_path: &str,
        build_config: &BuildConfig,
    ) -> Result<String> {
        info!("🔨 Building derivation: {}", drv_path);

        if !drv_path.ends_with(".drv") {
            bail!("Expected .drv path, got: {}", drv_path);
        }

        let mut cmd = if build_config.should_use_systemd() {
            let mut scoped = Command::new("systemd-run");
            scoped.args(["--scope", "--collect", "--quiet"]);
            apply_systemd_props_for_scope(build_config, &mut scoped);
            scoped.args(["--", "nix-store", "--realise", drv_path]);
            scoped
        } else {
            let mut direct = Command::new("nix-store");
            direct.args(["--realise", drv_path]);
            direct
        };

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        build_config.apply_to_command(&mut cmd);

        match Self::run_streaming_build(cmd, drv_path, self.id, pool).await {
            Ok(output_path) => Ok(output_path),
            Err(e) if build_config.should_use_systemd() && Self::is_systemd_error(&e) => {
                warn!(
                    "⚠️ Systemd scope creation failed, falling back to direct execution: {}",
                    e
                );
                self.build_with_direct_nix_store(pool, drv_path, build_config)
                    .await
            }
            Err(e) => Err(e),
        }
    }

    /// Phase 1: Evaluate and discover all derivation paths (dry-run only)
    pub async fn dry_run_derivation_path(
        flake_target: &str,
        build_config: &BuildConfig,
    ) -> Result<EvaluationResult> {
        info!("🔍 Evaluating derivation paths for: {}", flake_target);

        // Step 1: Get the main derivation path using nix path-info
        let mut cmd = Command::new("nix");
        cmd.args(["path-info", "--derivation", flake_target]);
        build_config.apply_to_command(&mut cmd);

        let output = cmd.output().await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "nix path-info --derivation failed for target '{}': {}",
                flake_target,
                stderr.trim()
            );
        }

        let main_drv = String::from_utf8_lossy(&output.stdout).trim().to_string();

        if main_drv.is_empty() {
            bail!("Got empty derivation path for {}", flake_target);
        }

        info!("🔍 main drv: {}", main_drv);

        // Step 2: Get dependencies using nix derivation show
        let mut deps_cmd = Command::new("nix");
        deps_cmd.args(["derivation", "show", &main_drv]);
        build_config.apply_to_command(&mut deps_cmd);

        let deps_output = deps_cmd.output().await?;
        let deps = if deps_output.status.success() {
            parse_input_drvs_from_json(&String::from_utf8_lossy(&deps_output.stdout))?
        } else {
            warn!("Could not parse dependencies, continuing without them");
            Vec::new()
        };

        // Step 3: Get the store path
        let store_path = resolve_drv_to_store_path_static(&main_drv).await?;

        info!("🔍 store path: {}", store_path);
        info!("🔍 {} immediate input drvs", deps.len());

        // Step 4: Check CF agent
        let cf_agent_enabled = match is_cf_agent_enabled(flake_target, build_config).await {
            Ok(enabled) => enabled,
            Err(e) => {
                warn!("Could not determine Crystal Forge agent status: {}", e);
                false
            }
        };

        Ok(EvaluationResult {
            main_derivation_path: main_drv,
            dependency_derivation_paths: deps,
            cf_agent_enabled,
            store_path,
        })
    }

    async fn build_with_direct_nix_store(
        &self,
        pool: &PgPool,
        drv_path: &str,
        build_config: &BuildConfig,
    ) -> Result<String> {
        let mut cmd = Command::new("nix-store");
        cmd.args(["--realise", drv_path]);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        build_config.apply_to_command(&mut cmd);

        Self::run_streaming_build(cmd, drv_path, self.id, pool).await
    }

    async fn run_streaming_build(
        mut cmd: Command,
        drv_path: &str,
        derivation_id: i32,
        pool: &PgPool,
    ) -> Result<String> {
        use sqlx::Acquire;
        use tokio::time::timeout;

        let start_time = Instant::now();
        let mut heartbeat = interval(Duration::from_secs(30));
        heartbeat.tick().await; // Skip first immediate tick

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();
        let mut output_lines = Vec::new();
        let mut last_activity = Instant::now();

        // Prime with drv basename so early heartbeats have a target label
        let mut current_build_target = Some(
            Path::new(drv_path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        );

        // Acquire heartbeat connection once, with timeout
        let mut hb_conn = match timeout(Duration::from_secs(2), pool.acquire()).await {
            Ok(Ok(conn)) => Some(conn),
            _ => {
                warn!("⚠️ Could not acquire heartbeat DB conn quickly, proceeding without it");
                None
            }
        };

        let mut stdout_done = false;
        let mut stderr_done = false;

        loop {
            tokio::select! {
                line_result = stdout_reader.next_line(), if !stdout_done => {
                    match line_result? {
                        Some(line) => {
                            last_activity = Instant::now();
                            debug!("nix stdout: {}", line);
                            if line.starts_with("/nix/store/") && !line.contains(".drv") {
                                output_lines.push(line);
                            }
                        }
                        None => stdout_done = true,
                    }
                }
                line_result = stderr_reader.next_line(), if !stderr_done => {
                    match line_result? {
                        Some(line) => {
                            last_activity = Instant::now();
                            current_build_target = Self::parse_build_status(&line, current_build_target);
                        }
                        None => stderr_done = true,
                    }
                }
                _ = heartbeat.tick() => {
                    Self::log_build_progress(start_time, last_activity, &current_build_target);

                    // Best-effort heartbeat: don't block the build if DB is slow
                    if let Some(ref mut conn) = hb_conn {
                        if let Ok(Ok(_)) = timeout(
                            Duration::from_secs(2),
                            crate::queries::derivations::update_derivation_build_status(
                                &mut **conn,
                                derivation_id,
                                start_time.elapsed().as_secs() as i32,
                                current_build_target.as_deref(),
                                last_activity.elapsed().as_secs() as i32,
                            )
                        ).await {
                            // Heartbeat succeeded
                        } else {
                            warn!("⚠️ Heartbeat update skipped (timeout or conn lost)");
                        }
                    }
                }
            }

            // Exit only when BOTH streams are done
            if stdout_done && stderr_done {
                break;
            }
        }

        let status = child.wait().await?;
        if !status.success() {
            bail!(
                "nix-store --realise failed with exit code: {}",
                status.code().unwrap_or(-1)
            );
        }

        let output_path = output_lines
            .into_iter()
            .find(|line| !line.contains(".drv"))
            .ok_or_else(|| anyhow!("No output path found in nix-store output"))?;

        let elapsed = start_time.elapsed();
        info!(
            "✅ Built derivation {} -> {} (took {}m {}s)",
            drv_path,
            output_path,
            elapsed.as_secs() / 60,
            elapsed.as_secs() % 60
        );

        let _ =
            crate::queries::derivations::clear_derivation_build_status(pool, derivation_id).await;

        Ok(output_path)
    }

    fn parse_build_status(line: &str, current: Option<String>) -> Option<String> {
        if line.contains("building '") {
            // New target announced
            Self::extract_quoted_target(line, "building '")
        } else if line.contains("built '") {
            // Target finished; keep last-known so progress log isn't blank between targets
            if let Some(t) = Self::extract_quoted_target(line, "built '") {
                info!("✅ Completed building: {}", t);
            }
            current
        } else if line.contains("downloading '") || line.contains("fetching ") {
            debug!("📥 {}", line);
            current
        } else if line.contains("error:") || line.contains("failed") {
            error!("❌ Build error: {}", line);
            current
        } else {
            current
        }
    }

    fn extract_quoted_target(line: &str, prefix: &str) -> Option<String> {
        if let Some(start) = line.find(prefix) {
            let start_pos = start + prefix.len();
            if let Some(end) = line[start_pos..].find("'") {
                let target = line[start_pos..start_pos + end].to_string();
                info!("🔨 Started building: {}", target);
                return Some(target);
            }
        }
        None
    }

    fn log_build_progress(
        start_time: Instant,
        last_activity: Instant,
        current_build_target: &Option<String>,
    ) {
        let elapsed = start_time.elapsed();
        let mins = elapsed.as_secs() / 60;
        let secs = elapsed.as_secs() % 60;
        let last_activity_secs = last_activity.elapsed().as_secs();

        match current_build_target {
            Some(target) => {
                info!(
                    "⏳ Still building {}: {}m {}s elapsed, last activity {}s ago",
                    target, mins, secs, last_activity_secs
                );
            }
            None => {
                info!(
                    "⏳ Build in progress: {}m {}s elapsed, last activity {}s ago",
                    mins, secs, last_activity_secs
                );
            }
        }
    }

    fn is_systemd_error(e: &anyhow::Error) -> bool {
        Self::is_systemd_error_str(&e.to_string())
    }

    fn is_systemd_error_str(error_msg: &str) -> bool {
        error_msg.contains("Failed to connect to user scope bus")
            || error_msg.contains("DBUS_SESSION_BUS_ADDRESS")
            || error_msg.contains("XDG_RUNTIME_DIR")
            || error_msg.contains("Failed to create scope")
            || error_msg.contains("Interactive authentication required")
            || error_msg.contains("systemd")
    }
}

/// Check if this derivation has Crystal Forge agent enabled
pub async fn is_cf_agent_enabled(flake_target: &str, build_config: &BuildConfig) -> Result<bool> {
    // Test environment check
    if std::env::var("CF_TEST_ENVIRONMENT").is_ok() || flake_target.contains("cf-test-sys") {
        return Ok(true);
    }

    // Extract the system name from the flake target
    let system_name = flake_target
        .split('#')
        .nth(1)
        .and_then(|s| s.split('.').nth(1))
        .unwrap_or("unknown");

    // Extract the flake URL (before the #)
    let flake_url = flake_target.split('#').next().unwrap_or(flake_target);

    // Use a simpler evaluation that's less likely to fail
    let eval_expr = format!(
        "let flake = builtins.getFlake \"{}\"; cfg = flake.nixosConfigurations.{}.config.services.crystal-forge or {{}}; in {{ enable = cfg.enable or false; client_enable = cfg.client.enable or false; }}",
        flake_url, system_name
    );

    let mut cmd = Command::new("nix");
    cmd.args(["eval", "--json", "--expr", &eval_expr]);
    build_config.apply_to_command(&mut cmd);

    match cmd.output().await {
        Ok(output) if output.status.success() => {
            let json_str = String::from_utf8_lossy(&output.stdout);
            match serde_json::from_str::<serde_json::Value>(&json_str) {
                Ok(json) => {
                    let cf_enabled = json
                        .get("enable")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let cf_client_enabled = json
                        .get("client_enable")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    Ok(cf_enabled && cf_client_enabled)
                }
                Err(e) => {
                    warn!("Failed to parse CF agent status JSON: {}", e);
                    Ok(false)
                }
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("CF agent evaluation failed: {}", stderr);
            Ok(false)
        }
        Err(e) => {
            warn!("Failed to run CF agent evaluation: {}", e);
            Ok(false)
        }
    }
}

fn parse_input_drvs_from_json(json_str: &str) -> Result<Vec<String>> {
    let parsed: serde_json::Value = serde_json::from_str(json_str)?;

    let mut deps = Vec::new();

    // nix derivation show returns a map of derivation paths to their info
    if let Some(obj) = parsed.as_object() {
        for (_drv_path, drv_info) in obj {
            if let Some(input_drvs) = drv_info.get("inputDrvs") {
                if let Some(inputs) = input_drvs.as_object() {
                    for (input_drv, _outputs) in inputs {
                        deps.push(input_drv.to_string());
                    }
                }
            }
        }
    }

    Ok(deps)
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

/// Build flake target for agent deployment (nixos-rebuild compatible)
pub fn build_agent_target(repo_url: &str, commit_hash: &str, system_name: &str) -> String {
    let flake_ref = build_flake_reference(repo_url, commit_hash);
    debug!("Making Deployment Target for {system_name} ==> {flake_ref}#{system_name}");
    format!("{flake_ref}#{system_name}")
}

/// Build flake target for evaluation (nix path-info compatible)
pub fn build_evaluation_target(repo_url: &str, commit_hash: &str, system_name: &str) -> String {
    let flake_ref = build_flake_reference(repo_url, commit_hash);
    format!("{flake_ref}#nixosConfigurations.{system_name}.config.system.build.toplevel")
}

async fn get_complete_closure(
    derivation_path: &str,
    build_config: &BuildConfig,
) -> Result<Vec<(String, bool)>> {
    // Return (drv_path, is_built)
    let output = Command::new("nix")
        .args(["path-info", "--derivation", "--recursive", derivation_path])
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to get closure: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let drv_paths: Vec<String> = String::from_utf8(output.stdout)?
        .lines()
        .filter(|line| !line.is_empty() && *line != derivation_path)
        .map(|s| s.to_string())
        .collect();

    // Check which ones are already built in the local store
    let mut closure = Vec::new();
    for drv_path in drv_paths {
        let is_built = check_if_built(&drv_path).await.unwrap_or(false);
        closure.push((drv_path, is_built));
    }

    Ok(closure)
}

async fn check_if_built(drv_path: &str) -> Result<bool> {
    // Check if all outputs of this derivation exist in the store
    let output = Command::new("nix")
        .args(["path-info", "--json", drv_path])
        .output()
        .await?;

    Ok(output.status.success())
}

async fn get_store_path_from_drv(drv_path: &str) -> Result<String> {
    let output = Command::new("nix-store")
        .args(["--query", "--outputs", drv_path])
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!("Failed to get store path for {}", drv_path);
    }

    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

/// Enhanced version that gets closure with cache status in one pass
async fn get_complete_closure_with_cache_status(
    derivation_path: &str,
    build_config: &BuildConfig,
) -> Result<Vec<(String, String, bool)>> {
    // Returns (drv_path, store_path, is_built)

    // Get all derivations in closure
    let output = Command::new("nix")
        .args(["path-info", "--derivation", "--recursive", derivation_path])
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to get closure: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let drv_paths: Vec<String> = String::from_utf8(output.stdout)?
        .lines()
        .filter(|line| !line.is_empty() && *line != derivation_path)
        .map(|s| s.to_string())
        .collect();

    info!(
        "🔍 Checking build status for {} derivations...",
        drv_paths.len()
    );

    // Check status in batches for better performance
    let mut closure = Vec::new();
    for drv_path in drv_paths {
        let (store_path, is_built) = match get_store_path_and_build_status(&drv_path).await {
            Ok(result) => result,
            Err(e) => {
                warn!("Failed to check status for {}: {}", drv_path, e);
                (String::new(), false)
            }
        };
        closure.push((drv_path, store_path, is_built));
    }

    Ok(closure)
}

/// Get store path and check if it's built in one call
async fn get_store_path_and_build_status(drv_path: &str) -> Result<(String, bool)> {
    // First get the store path
    let output = Command::new("nix-store")
        .args(["--query", "--outputs", drv_path])
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!("Failed to get store path for {}", drv_path);
    }

    let store_path = String::from_utf8(output.stdout)?.trim().to_string();

    // Check if it exists
    let is_built = Command::new("nix")
        .args(["path-info", &store_path])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);

    Ok((store_path, is_built))
}
