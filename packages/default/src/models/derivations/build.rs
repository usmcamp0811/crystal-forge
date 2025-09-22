use super::Derivation;
use super::utils::*;
use crate::models::config::BuildConfig;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use sqlx::PgPool;
use std::path::Path;
use std::process::{Output, Stdio};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::{Duration, Instant, interval, sleep};
use tracing::{debug, error, info, warn};

// Re-export EvaluationResult here since it's closely tied to evaluation/build
#[derive(Debug, Clone)]
pub struct EvaluationResult {
    pub main_derivation_path: String,
    pub dependency_derivation_paths: Vec<String>,
}

impl Derivation {
    /// High-level function that handles both NixOS and Package derivations
    pub async fn evaluate_and_build(
        &self,
        pool: &PgPool,
        full_build: bool,
        build_config: &BuildConfig,
    ) -> Result<String> {
        match self.derivation_type {
            crate::models::derivations::DerivationType::NixOS => {
                // NixOS derivations need commit_id for flake evaluation
                let commit_id = self.commit_id.ok_or_else(|| {
                    anyhow::anyhow!("Cannot build NixOS derivation without commit_id")
                })?;

                // Use the provided pool instead of creating a new one
                let commit = crate::queries::commits::get_commit_by_id(pool, commit_id).await?;
                let flake = crate::queries::flakes::get_flake_by_id(pool, commit.flake_id).await?;
                let flake_target = self.build_flake_target_string(&flake, &commit).await?;

                if full_build {
                    // Phase 1: Evaluate
                    let eval_result =
                        Self::dry_run_derivation_path(&flake_target, build_config).await?;
                    // Insert dependencies into database
                    if !eval_result.dependency_derivation_paths.is_empty() {
                        crate::queries::derivations::discover_and_insert_packages(
                            pool, // Use provided pool
                            self.id,
                            &eval_result
                                .dependency_derivation_paths
                                .iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>(),
                        )
                        .await?;
                    }
                    // Phase 2: Build the main derivation
                    if eval_result.main_derivation_path.ends_with(".drv") {
                        Self::build_derivation_from_path(
                            &self,
                            &pool,
                            &eval_result.main_derivation_path,
                            build_config,
                        )
                        .await
                    } else {
                        // Already built, return the output path
                        Ok(eval_result.main_derivation_path)
                    }
                } else {
                    // Dry-run only
                    let eval_result =
                        Self::dry_run_derivation_path(&flake_target, build_config).await?;
                    // Insert dependencies into database
                    if !eval_result.dependency_derivation_paths.is_empty() {
                        crate::queries::derivations::discover_and_insert_packages(
                            pool, // Use provided pool
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
            }
            crate::models::derivations::DerivationType::Package => {
                // Package derivations just need their .drv path built
                let drv_path = self
                    .derivation_path
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Package derivation missing derivation_path"))?;
                if full_build {
                    // Build the package from its .drv path
                    Self::build_derivation_from_path(&self, &pool, drv_path, build_config).await
                } else {
                    // For dry-run, just return the existing .drv path
                    Ok(drv_path.clone())
                }
            }
        }
    }

    /// Phase 2: Build a derivation from its .drv path
    /// This can run on any machine that has access to the required inputs
    pub async fn build_derivation_from_path(
        &self,
        pool: &PgPool,
        drv_path: &str,
        build_config: &BuildConfig,
    ) -> Result<String> {
        info!("🔨 Building derivation: {}", drv_path);

        if !drv_path.ends_with(".drv") {
            anyhow::bail!("Expected .drv path, got: {}", drv_path);
        }

        // Check if systemd should be used
        if !build_config.should_use_systemd() {
            info!("📋 Systemd disabled in config, using direct execution for nix-store");
            return Self::run_direct_nix_store_streaming(
                &self,
                drv_path,
                build_config,
                self.id,
                pool,
            )
            .await;
        }

        // Try systemd-run scoped build first
        let mut scoped = Command::new("systemd-run");
        scoped.args(["--scope", "--collect", "--quiet"]);

        // Add systemd properties from config
        apply_systemd_props_for_scope(build_config, &mut scoped);

        scoped.args(["--", "nix-store", "--realise", drv_path]);
        scoped.stdout(Stdio::piped()).stderr(Stdio::piped());
        build_config.apply_to_command(&mut scoped);

        match Self::run_streaming_build(scoped, drv_path, self.id, pool).await {
            Ok(output_path) => Ok(output_path),
            Err(e) => {
                let error_msg = e.to_string();
                // Check for systemd-specific issues that indicate fallback needed
                if error_msg.contains("Failed to connect to user scope bus")
                    || error_msg.contains("DBUS_SESSION_BUS_ADDRESS")
                    || error_msg.contains("XDG_RUNTIME_DIR")
                    || error_msg.contains("Failed to create scope")
                    || error_msg.contains("Interactive authentication required")
                    || error_msg.contains("systemd")
                {
                    warn!(
                        "⚠️ Systemd scope creation failed, falling back to direct execution: {}",
                        error_msg
                    );

                    Self::run_direct_nix_store_streaming(
                        &self,
                        drv_path,
                        build_config,
                        self.id,
                        pool,
                    )
                    .await
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Phase 1: Evaluate and discover all derivation paths (dry-run only)
    /// This can run on any machine and doesn't require commit_id
    pub async fn dry_run_derivation_path(
        flake_target: &str,
        build_config: &BuildConfig,
    ) -> Result<EvaluationResult> {
        info!("🔍 Evaluating derivation paths for: {}", flake_target);

        // robust & deterministic: never scrape stderr, never depend on dry-run text
        let main_drv = eval_main_drv_path(flake_target, build_config).await?;
        let deps = list_immediate_input_drvs(&main_drv, build_config).await?;

        info!("🔍 main drv: {main_drv}");
        info!("🔍 {} immediate input drvs", deps.len());

        Ok(EvaluationResult {
            main_derivation_path: main_drv,
            dependency_derivation_paths: deps,
        })
    }

    async fn run_print_out_paths(flake_target: &str, build_config: &BuildConfig) -> Result<Output> {
        // Check if systemd should be used
        if !build_config.should_use_systemd() {
            info!("📋 Systemd disabled in config, using direct execution for store path lookup");
            let mut direct = Command::new("nix");
            direct.args(["build", flake_target, "--print-out-paths"]);
            build_config.apply_to_command(&mut direct);
            return Ok(direct.output().await?);
        }

        let mut scoped = build_config.systemd_scoped_cmd_base();
        scoped.args([flake_target, "--print-out-paths"]);
        build_config.apply_to_command(&mut scoped);

        match scoped.output().await {
            Ok(output) => {
                if output.status.success() {
                    Ok(output)
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    // Check for systemd-specific issues
                    if stderr.contains("Failed to connect to user scope bus")
                        || stderr.contains("DBUS_SESSION_BUS_ADDRESS")
                        || stderr.contains("XDG_RUNTIME_DIR")
                        || stderr.contains("Failed to create scope")
                        || stderr.contains("systemd")
                    {
                        info!(
                            "📋 systemd-run not available for store path lookup, using direct execution"
                        );
                        let mut direct = Command::new("nix");
                        direct.args(["build", flake_target, "--print-out-paths"]);
                        build_config.apply_to_command(&mut direct);
                        Ok(direct.output().await?)
                    } else {
                        Ok(output)
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                info!("📋 systemd-run not available for store path lookup");
                let mut direct = Command::new("nix");
                direct.args(["build", flake_target, "--print-out-paths"]);
                build_config.apply_to_command(&mut direct);
                Ok(direct.output().await?)
            }
            Err(e) => {
                warn!("⚠️ systemd-run failed for store path lookup: {}", e);
                let mut direct = Command::new("nix");
                direct.args(["build", flake_target, "--print-out-paths"]);
                build_config.apply_to_command(&mut direct);
                Ok(direct.output().await?)
            }
        }
    }

    async fn run_dry_run(flake_target: &str, build_config: &BuildConfig) -> Result<Output> {
        // Check if systemd should be used
        if !build_config.should_use_systemd() {
            info!("📋 Systemd disabled in config, using direct execution");
            return Self::run_direct_dry_run(flake_target, build_config).await;
        }

        // Try systemd-run scoped build first
        let mut scoped = build_config.systemd_scoped_cmd_base();
        scoped.args([flake_target, "--dry-run", "--print-out-paths", "--no-link"]);
        build_config.apply_to_command(&mut scoped);

        match scoped.output().await {
            Ok(output) => {
                if output.status.success() {
                    info!("✅ Systemd-scoped evaluation completed successfully");
                    Ok(output)
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    // Check for systemd-specific issues that indicate fallback needed
                    if stderr.contains("Failed to connect to user scope bus")
                        || stderr.contains("DBUS_SESSION_BUS_ADDRESS")
                        || stderr.contains("XDG_RUNTIME_DIR")
                        || stderr.contains("Failed to create scope")
                        || stderr.contains("systemd")
                    {
                        warn!(
                            "⚠️ Systemd scope creation failed (no user session), falling back to direct execution"
                        );
                        Self::run_direct_dry_run(flake_target, build_config).await
                    } else {
                        // Other failure - return the systemd error
                        Ok(output)
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                info!("📋 systemd-run not available, using direct execution");
                Self::run_direct_dry_run(flake_target, build_config).await
            }
            Err(e) => {
                warn!("⚠️ systemd-run failed: {}, trying direct execution", e);
                Self::run_direct_dry_run(flake_target, build_config).await
            }
        }
    }

    async fn run_direct_dry_run(flake_target: &str, build_config: &BuildConfig) -> Result<Output> {
        let mut direct = Command::new("nix");
        direct.args([
            "build",
            flake_target,
            "--dry-run",
            "--print-out-paths",
            "--no-link",
        ]);
        build_config.apply_to_command(&mut direct);
        Ok(direct.output().await?)
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

        // Take stdout and stderr for streaming
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut output_lines = Vec::new();
        let mut last_activity = Instant::now();
        let mut current_build_target = None::<String>;

        loop {
            tokio::select! {
                // Read stdout
                line_result = stdout_reader.next_line() => {
                    match line_result? {
                        Some(line) => {
                            last_activity = Instant::now();
                            debug!("nix stdout: {}", line);

                            // Check if this looks like an output path
                            if line.starts_with("/nix/store/") && !line.contains(".drv") {
                                output_lines.push(line);
                            }
                        }
                        None => break,
                    }
                }

                // Read stderr (where build logs go)
                line_result = stderr_reader.next_line() => {
                    match line_result? {
                        Some(line) => {
                            last_activity = Instant::now();

                            // Parse build status from nix output
                            if line.contains("building '") {
                                if let Some(start) = line.find("building '") {
                                    if let Some(end) = line[start + 10..].find("'") {
                                        current_build_target = Some(line[start + 10..start + 10 + end].to_string());
                                        info!("🔨 Started building: {}", current_build_target.as_ref().unwrap());
                                    }
                                }
                            } else if line.contains("built '") {
                                if let Some(start) = line.find("built '") {
                                    if let Some(end) = line[start + 7..].find("'") {
                                        let built_target = &line[start + 7..start + 7 + end];
                                        info!("✅ Completed building: {}", built_target);
                                        current_build_target = None;
                                    }
                                }
                            } else if line.contains("downloading '") || line.contains("fetching ") {
                                debug!("📥 {}", line);
                            } else if line.contains("error:") || line.contains("failed") {
                                error!("❌ Build error: {}", line);
                            }
                        }
                        None => break,
                    }
                }

                // Periodic heartbeat
                _ = heartbeat.tick() => {
                    let elapsed = start_time.elapsed();
                    let mins = elapsed.as_secs() / 60;
                    let secs = elapsed.as_secs() % 60;
                    let last_activity_secs = last_activity.elapsed().as_secs();

                    if let Some(ref target) = current_build_target {
                        info!("⏳ Still building {}: {}m {}s elapsed, last activity {}s ago",
                              target, mins, secs, last_activity_secs);
                    } else {
                        info!("⏳ Build in progress: {}m {}s elapsed, last activity {}s ago",
                              mins, secs, last_activity_secs);
                    }

                    // Database heartbeat update
                    let _ = crate::queries::derivations::update_derivation_build_status(
                        pool,
                        derivation_id,
                        elapsed.as_secs() as i32,
                        current_build_target.as_deref(),
                        last_activity_secs as i32
                    ).await;
                }
            }
        }

        // Wait for process to complete
        let status = child.wait().await?;

        if !status.success() {
            anyhow::bail!(
                "nix-store --realise failed with exit code: {}",
                status.code().unwrap_or(-1)
            );
        }

        // Find the output path
        let output_path = output_lines
            .into_iter()
            .find(|line| !line.contains(".drv"))
            .ok_or_else(|| anyhow::anyhow!("No output path found in nix-store output"))?;

        let elapsed = start_time.elapsed();
        info!(
            "✅ Built derivation {} -> {} (took {}m {}s)",
            drv_path,
            output_path,
            elapsed.as_secs() / 60,
            elapsed.as_secs() % 60
        );

        // Clear build progress on completion
        let _ =
            crate::queries::derivations::clear_derivation_build_status(pool, derivation_id).await;

        Ok(output_path)
    }

    async fn run_direct_nix_store_streaming(
        &self,
        drv_path: &str,
        build_config: &BuildConfig,
        derivation_id: i32,
        pool: &PgPool,
    ) -> Result<String> {
        let mut cmd = Command::new("nix-store");
        cmd.args(["--realise", drv_path]);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        build_config.apply_to_command(&mut cmd);

        Self::run_streaming_build(cmd, drv_path, derivation_id, pool).await
    }

    async fn run_direct_nix_store(drv_path: &str, build_config: &BuildConfig) -> Result<String> {
        let mut cmd = Command::new("nix-store");
        cmd.args(["--realise", drv_path]);

        // Add this line to apply auth configuration
        build_config.apply_to_command(&mut cmd);

        let output = cmd.output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("❌ nix-store --realise failed: {}", stderr.trim());
            anyhow::bail!("nix-store --realise failed: {}", stderr.trim());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let output_path = stdout.trim();

        if output_path.is_empty() {
            anyhow::bail!("No output path returned from nix-store --realise");
        }

        info!("✅ Built derivation {} -> {}", drv_path, output_path);
        Ok(output_path.to_string())
    }

    /// Helper to build the flake target string
    async fn build_flake_target_string(
        &self,
        flake: &crate::models::flakes::Flake,
        commit: &crate::models::commits::Commit,
    ) -> Result<String> {
        let system = &self.derivation_name;
        // Always use a git flake ref pinned to the commit, even for local repos.
        let flake_ref = if Path::new(&flake.repo_url).exists() {
            let abs = std::fs::canonicalize(&flake.repo_url)
                .with_context(|| format!("canonicalize {}", flake.repo_url))?;
            format!(
                "git+file://{}?rev={}",
                abs.display(),
                commit.git_commit_hash
            )
        } else if flake.repo_url.starts_with("git+") {
            // ensure we pin a rev (append if not already present)
            if flake.repo_url.contains("?rev=") {
                flake.repo_url.clone()
            } else {
                format!("{}?rev={}", flake.repo_url, commit.git_commit_hash)
            }
        } else {
            // Check if URL already has query parameters to avoid malformed URLs
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
        Ok(format!(
            "{flake_ref}#nixosConfigurations.{system}.config.system.build.toplevel"
        ))
    }
}

// These functions are called by the main evaluation logic but need to be accessible
async fn eval_main_drv_path(flake_target: &str, build_config: &BuildConfig) -> Result<String> {
    // Example: "<flake>#…config.system.build.toplevel" -> ask Nix for ".drvPath"
    let mut cmd = Command::new("nix");
    cmd.args(["eval", "--raw", &format!("{flake_target}.drvPath")]);
    build_config.apply_to_command(&mut cmd);

    let out = cmd.output().await?;
    if !out.status.success() {
        bail!(
            "nix eval failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

async fn list_immediate_input_drvs(
    drv_path: &str,
    build_config: &BuildConfig,
) -> Result<Vec<String>> {
    // `nix derivation show <drv>` yields {"<drv>": {"inputDrvs": {"<drv2>": ["out"], …}, …}}
    let mut cmd = Command::new("nix");
    cmd.args(["derivation", "show", drv_path]);
    build_config.apply_to_command(&mut cmd);

    let out = cmd.output().await?;
    if !out.status.success() {
        bail!(
            "nix derivation show failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let v: Value = serde_json::from_slice(&out.stdout)?;
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
