use super::Derivation;
use super::utils::*;
use crate::models::config::BuildConfig;
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
                let flake_target = self
                    .build_flake_target_string(&flake, &commit, true)
                    .await?;

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
        self.cf_agent_enabled = Some(eval_result.cf_agent_enabled); // This line needs &mut self

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

        let main_drv = eval_main_drv_path(flake_target, build_config).await?;
        let deps = list_immediate_input_drvs(&main_drv, build_config).await?;
        let cf_agent_enabled = is_cf_agent_enabled(flake_target, build_config).await?;

        info!("🔍 main drv: {main_drv}");
        info!("🔍 {} immediate input drvs", deps.len());

        Ok(EvaluationResult {
            main_derivation_path: main_drv,
            dependency_derivation_paths: deps,
            cf_agent_enabled: cf_agent_enabled,
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
        let mut current_build_target = None::<String>;

        loop {
            tokio::select! {
                line_result = stdout_reader.next_line() => {
                    match line_result? {
                        Some(line) => {
                            last_activity = Instant::now();
                            debug!("nix stdout: {}", line);
                            if line.starts_with("/nix/store/") && !line.contains(".drv") {
                                output_lines.push(line);
                            }
                        }
                        None => break,
                    }
                }

                line_result = stderr_reader.next_line() => {
                    match line_result? {
                        Some(line) => {
                            last_activity = Instant::now();
                            current_build_target = Self::parse_build_status(&line, current_build_target);
                        }
                        None => break,
                    }
                }

                _ = heartbeat.tick() => {
                    Self::log_build_progress(start_time, last_activity, &current_build_target);
                    let _ = crate::queries::derivations::update_derivation_build_status(
                        pool,
                        derivation_id,
                        start_time.elapsed().as_secs() as i32,
                        current_build_target.as_deref(),
                        last_activity.elapsed().as_secs() as i32
                    ).await;
                }
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
            Self::extract_quoted_target(line, "building '")
        } else if line.contains("built '") {
            if let Some(target) = Self::extract_quoted_target(line, "built '") {
                info!("✅ Completed building: {}", target);
            }
            None
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

    /// Helper to build the flake target string
    async fn build_flake_target_string(
        &self,
        flake: &crate::models::flakes::Flake,
        commit: &crate::models::commits::Commit,
        include_build_toplevel: bool,
    ) -> Result<String> {
        let system = &self.derivation_name;
        let flake_ref = if Path::new(&flake.repo_url).exists() {
            let abs = std::fs::canonicalize(&flake.repo_url)
                .with_context(|| format!("canonicalize {}", flake.repo_url))?;
            format!(
                "git+file://{}?rev={}",
                abs.display(),
                commit.git_commit_hash
            )
        } else if flake.repo_url.starts_with("git+") {
            if flake.repo_url.contains("?rev=") {
                flake.repo_url.clone()
            } else {
                format!("{}?rev={}", flake.repo_url, commit.git_commit_hash)
            }
        } else {
            let separator = if flake.repo_url.contains('?') {
                "&"
            } else {
                "?"
            };
            format!(
                "git+{}{separator}rev={}",
                flake.repo_url, commit.git_commit_hash
            )
        };
        if include_build_toplevel {
            Ok(format!(
                "{flake_ref}#nixosConfigurations.{system}.config.system.build.toplevel"
            ))
        } else {
            Ok(format!("{flake_ref}#nixosConfigurations.{system}"))
        }
    }
}

/// Check if this derivation has Crystal Forge agent enabled
pub async fn is_cf_agent_enabled(flake_target: &str, build_config: &BuildConfig) -> Result<bool> {
    let cf_enabled_systemd_cmd = || {
        let mut cmd = build_config.systemd_scoped_cmd_base();
        cmd.args([
            "nix",
            "eval",
            "--json",
            &format!("{}.config.services.crystal-forge.enable", flake_target),
        ]);
        cmd
    };
    let cf_enabled_output = Derivation::execute_with_systemd_fallback(
        build_config,
        cf_enabled_systemd_cmd,
        &[
            "nix",
            "eval",
            "--json",
            &format!("{}.config.services.crystal-forge.enable", flake_target),
        ],
        "check if Crystal Forge module is enabled",
    )
    .await?;

    let cf_client_enabled_systemd_cmd = || {
        let mut cmd = build_config.systemd_scoped_cmd_base();
        cmd.args([
            "nix",
            "eval",
            "--json",
            &format!(
                "{}.config.services.crystal-forge.client.enable",
                flake_target
            ),
        ]);
        cmd
    };
    let cf_client_enabled_output = Derivation::execute_with_systemd_fallback(
        build_config,
        cf_client_enabled_systemd_cmd,
        &[
            "nix",
            "eval",
            "--json",
            &format!(
                "{}.config.services.crystal-forge.client.enable",
                flake_target
            ),
        ],
        "check if Crystal Forge client is enabled",
    )
    .await?;

    if !cf_enabled_output.status.success() {
        bail!(
            "nix eval failed: {}",
            String::from_utf8_lossy(&cf_enabled_output.stderr).trim()
        );
    }
    if !cf_client_enabled_output.status.success() {
        bail!(
            "nix eval failed: {}",
            String::from_utf8_lossy(&cf_client_enabled_output.stderr).trim()
        );
    }
    let cf_enabled = serde_json::from_slice::<bool>(&cf_enabled_output.stdout)?;
    let cf_client_enabled = serde_json::from_slice::<bool>(&cf_client_enabled_output.stdout)?;
    Ok(cf_enabled && cf_client_enabled)
}

async fn eval_main_drv_path(flake_target: &str, build_config: &BuildConfig) -> Result<String> {
    let systemd_cmd = || {
        let mut cmd = build_config.systemd_scoped_cmd_base();
        cmd.args(["nix", "eval", "--raw", &format!("{flake_target}.drvPath")]);
        cmd
    };

    let output = Derivation::execute_with_systemd_fallback(
        build_config,
        systemd_cmd,
        &["nix", "eval", "--raw", &format!("{flake_target}.drvPath")],
        "derivation path evaluation",
    )
    .await?;

    if !output.status.success() {
        bail!(
            "nix eval failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
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
