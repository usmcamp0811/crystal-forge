use super::Derivation;
use super::utils::*;
use crate::models::config::BuildConfig;
use anyhow::{Result, anyhow, bail};
use sqlx::PgPool;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::{Duration, Instant, interval};
use tracing::{debug, error, info, warn};

impl Derivation {
    /// Main entry point for building a derivation
    /// Routes to appropriate build method based on derivation type
    pub async fn build(&mut self, pool: &PgPool, build_config: &BuildConfig) -> Result<String> {
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

                // For NixOS, we need to evaluate first to get the .drv path
                // Then build that .drv
                self.build_nixos_system(pool, &flake_target, build_config)
                    .await
            }
            crate::models::derivations::DerivationType::Package => {
                let drv_path = self
                    .derivation_path
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Package derivation missing derivation_path"))?;

                self.build_derivation_from_path(pool, drv_path, build_config)
                    .await
            }
        }
    }

    /// Build a NixOS system from a flake target
    /// This uses nix-store --realise on the toplevel derivation
    async fn build_nixos_system(
        &self,
        pool: &PgPool,
        flake_target: &str,
        build_config: &BuildConfig,
    ) -> Result<String> {
        info!("🏗️ Building NixOS system: {}", flake_target);

        // Get the derivation path for this flake target
        let drv_path = Self::get_derivation_path(flake_target, build_config).await?;

        // Now build it
        self.build_derivation_from_path(pool, &drv_path, build_config)
            .await
    }

    /// Get the .drv path for a flake target without building
    async fn get_derivation_path(flake_target: &str, build_config: &BuildConfig) -> Result<String> {
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

        let drv_path = String::from_utf8_lossy(&output.stdout).trim().to_string();

        if drv_path.is_empty() || !drv_path.ends_with(".drv") {
            bail!(
                "Got invalid derivation path '{}' for {}",
                drv_path,
                flake_target
            );
        }

        Ok(drv_path)
    }

    /// Build a derivation from its .drv path using nix-store --realise
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

    /// Run a build command with streaming output and periodic database updates
    async fn run_streaming_build(
        mut cmd: Command,
        drv_path: &str,
        derivation_id: i32,
        pool: &PgPool,
    ) -> Result<String> {
        let start_time = Instant::now();

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().expect("Failed to capture stdout");
        let stderr = child.stderr.take().expect("Failed to capture stderr");

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut heartbeat_interval = interval(Duration::from_secs(5));
        let mut current_target: Option<String> = None;

        let pool_clone = pool.clone();
        let mut last_output = Instant::now();

        loop {
            tokio::select! {
                // Read stdout
                line_result = stdout_reader.next_line() => {
                    match line_result {
                        Ok(Some(line)) => {
                            last_output = Instant::now();
                            info!("build stdout: {}", line);

                            // Try to extract current build target from output
                            if line.contains("building '") || line.contains("copying path '") {
                                current_target = Some(line.clone());
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            error!("Error reading stdout: {}", e);
                            break;
                        }
                    }
                }

                // Read stderr
                line_result = stderr_reader.next_line() => {
                    match line_result {
                        Ok(Some(line)) => {
                            last_output = Instant::now();
                            debug!("build stderr: {}", line);

                            // Try to extract current build target from error output
                            if line.contains("building '") || line.contains("copying path '") {
                                current_target = Some(line.clone());
                            }
                        }
                        Ok(None) => {},
                        Err(e) => {
                            error!("Error reading stderr: {}", e);
                        }
                    }
                }

                // Periodic heartbeat updates to database
                _ = heartbeat_interval.tick() => {
                    let elapsed = start_time.elapsed().as_secs() as i32;
                    let last_activity = last_output.elapsed().as_secs() as i32;

                    if let Err(e) = crate::queries::derivations::update_build_heartbeat(
                        &pool_clone,
                        derivation_id,
                        elapsed,
                        current_target.as_deref(),
                        last_activity,
                    ).await {
                        warn!("Failed to update build heartbeat: {}", e);
                    }
                }
            }
        }

        // Wait for the process to complete
        let status = child.wait().await?;

        if !status.success() {
            let exit_code = status.code().unwrap_or(-1);
            bail!("Build failed for {} with exit code {}", drv_path, exit_code);
        }

        // Get the output store path
        let store_path = Self::resolve_store_path_from_drv(drv_path).await?;
        Ok(store_path)
    }

    /// Fallback: build directly without systemd isolation
    async fn build_with_direct_nix_store(
        &self,
        pool: &PgPool,
        drv_path: &str,
        build_config: &BuildConfig,
    ) -> Result<String> {
        info!(
            "🔨 Building with direct nix-store (no systemd): {}",
            drv_path
        );

        let mut cmd = Command::new("nix-store");
        cmd.args(["--realise", drv_path]);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        build_config.apply_to_command(&mut cmd);

        Self::run_streaming_build(cmd, drv_path, self.id, pool).await
    }

    /// Resolve a .drv path to its output store path
    async fn resolve_store_path_from_drv(drv_path: &str) -> Result<String> {
        let output = Command::new("nix-store")
            .args(["--query", "--outputs", drv_path])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Failed to get store path for {}: {}", drv_path, stderr);
        }

        let store_path = String::from_utf8_lossy(&output.stdout).trim().to_string();

        if store_path.is_empty() {
            bail!("Got empty store path for {}", drv_path);
        }

        Ok(store_path)
    }

    /// Check if an error is a systemd-specific error (for fallback logic)
    fn is_systemd_error(error: &anyhow::Error) -> bool {
        let error_str = error.to_string().to_lowercase();
        error_str.contains("systemd")
            || error_str.contains("dbus")
            || error_str.contains("scope")
            || error_str.contains("failed to create")
    }
}
