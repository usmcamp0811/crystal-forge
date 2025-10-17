use super::Derivation;
use super::utils::*;
use crate::models::config::BuildConfig;
use crate::models::derivations::parse_derivation_paths;
use crate::models::derivations::resolve_drv_to_store_path_static;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use sqlx::PgPool;
use std::option::Option;
use std::path::Path;
use std::process::{Output, Stdio};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::{Duration, Instant, interval};
use tracing::{debug, error, info, warn};

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

    async fn evaluate_nixos_dry_run(
        &mut self,
        pool: &PgPool,
        flake_target: &str,
        build_config: &BuildConfig,
    ) -> Result<String> {
        let eval_result = Self::dry_run_derivation_path(flake_target, build_config).await?;
        let cf_agent_enabled = match is_cf_agent_enabled(flake_target, build_config).await {
            Ok(enabled) => enabled,
            Err(e) => {
                warn!("Could not determine Crystal Forge agent status: {}", e);
                false
            }
        };
        self.cf_agent_enabled = Some(cf_agent_enabled);

        // **NEW**: Get the complete closure (all transitive dependencies)
        let all_deps =
            get_complete_closure(&eval_result.main_derivation_path, build_config).await?;

        // Insert ALL dependencies into database at once
        if !all_deps.is_empty() {
            crate::queries::derivations::discover_and_insert_packages(
                pool,
                self.id,
                &all_deps.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            )
            .await?;
        }

        // Update status and derivation_path
        crate::queries::derivations::update_derivation_status(
            pool,
            self.id,
            crate::queries::derivations::EvaluationStatus::DryRunComplete,
            Some(&eval_result.main_derivation_path),
            None,
            Some(&eval_result.store_path),
        )
        .await?;

        // Update cf_agent_enabled
        crate::queries::derivations::update_cf_agent_enabled(pool, self.id, cf_agent_enabled)
            .await?;

        Ok(eval_result.main_derivation_path)
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

    /// Execute command with systemd fallback pattern
    async fn execute_with_systemd_fallback<F>(
        build_config: &BuildConfig,
        systemd_cmd_builder: F,
        fallback_args: &[&str],
        operation_name: &str,
    ) -> Result<Output>
    where
        F: FnOnce() -> Command,
    {
        if !build_config.should_use_systemd() {
            info!(
                "📋 Systemd disabled in config, using direct execution for {}",
                operation_name
            );
            return Self::run_direct_command(fallback_args, build_config).await;
        }

        let mut cmd = systemd_cmd_builder();
        build_config.apply_to_command(&mut cmd);

        match cmd.output().await {
            Ok(output) if output.status.success() => Ok(output),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if Self::is_systemd_error_str(&stderr) {
                    warn!(
                        "⚠️ Systemd {} failed, falling back to direct execution",
                        operation_name
                    );
                    Self::run_direct_command(fallback_args, build_config).await
                } else {
                    Ok(output)
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                info!(
                    "📋 systemd-run not available for {}, using direct execution",
                    operation_name
                );
                Self::run_direct_command(fallback_args, build_config).await
            }
            Err(e) => {
                warn!(
                    "⚠️ systemd-run failed for {}: {}, trying direct execution",
                    operation_name, e
                );
                Self::run_direct_command(fallback_args, build_config).await
            }
        }
    }

    async fn run_direct_command(args: &[&str], build_config: &BuildConfig) -> Result<Output> {
        let mut cmd = Command::new(args[0]);
        cmd.args(&args[1..]);
        build_config.apply_to_command(&mut cmd);
        Ok(cmd.output().await?)
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

async fn eval_main_drv_path(flake_target: &str, build_config: &BuildConfig) -> Result<String> {
    let mut cmd = Command::new("nix");
    cmd.args(["eval", "--json", &format!("{flake_target}.drvPath")]);
    build_config.apply_to_command(&mut cmd);

    let out = cmd.output().await?;
    if !out.status.success() {
        bail!(
            "nix eval failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    // Parse JSON output instead of raw text
    let json_output = String::from_utf8_lossy(&out.stdout);
    let drv_path: String =
        serde_json::from_str(json_output.trim()).context("Failed to parse derivation path JSON")?;
    Ok(drv_path)
}

async fn list_immediate_input_drvs(
    drv_path: &str,
    build_config: &BuildConfig,
) -> Result<Vec<String>> {
    let systemd_cmd = || {
        let mut cmd = build_config.systemd_scoped_cmd_base();
        cmd.args(["nix", "derivation", "show", drv_path]);
        cmd
    };

    let output = Derivation::execute_with_systemd_fallback(
        build_config,
        systemd_cmd,
        &["nix", "derivation", "show", drv_path],
        "derivation show",
    )
    .await?;

    if !output.status.success() {
        bail!(
            "nix derivation show failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let v: Value = serde_json::from_slice(&output.stdout)?;
    let obj = v
        .as_object()
        .and_then(|m| m.get(drv_path))
        .and_then(|x| x.as_object())
        .ok_or_else(|| anyhow!("bad JSON from nix derivation show"))?;

    let deps = obj
        .get("inputDrvs")
        .and_then(|x| x.as_object())
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_else(Vec::new);

    Ok(deps)
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
) -> Result<Vec<String>> {
    let output = Command::new("nix") // Just use "nix" directly
        .args(["path-info", "--derivation", "--recursive", derivation_path])
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!(
            "Failed to get closure: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let closure = String::from_utf8(output.stdout)?
        .lines()
        .filter(|line| !line.is_empty() && *line != derivation_path) // Dereference line
        .map(|s| s.to_string())
        .collect();

    Ok(closure)
}
