use super::Derivation;
use super::utils::*;
use crate::builder::get_gc_root_path;
use crate::config::BuildConfig;
use crate::config::CacheConfig;
use anyhow::Context;
use anyhow::{Result, anyhow, bail};
use sqlx::PgPool;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::{Duration, Instant, interval};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Sentinel error returned when `run_streaming_build` exits because the server
/// marked the job as `cancelling`.  The caller in `builder.rs` checks for this
/// variant to choose the finalize-cancelled path instead of the fail path.
#[derive(Debug)]
pub struct BuildCancelledError;

impl std::fmt::Display for BuildCancelledError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "build cancelled by operator")
    }
}
impl std::error::Error for BuildCancelledError {}

impl Derivation {
    /// Main entry point for building a derivation.
    ///
    /// `job_id` is the `build_jobs.id` for this run.  When provided,
    /// `run_streaming_build` will poll the DB every ~15 s and abort the nix
    /// child process if the server has set the job's status to `cancelling`.
    /// In that case the returned error downcasts to [`BuildCancelledError`].
    pub async fn build(
        &mut self,
        pool: &PgPool,
        build_config: &BuildConfig,
        job_id: Option<Uuid>,
    ) -> Result<String> {
        // Both types use derivation_path from database (populated during dry-run phase)
        let drv_path = self.derivation_path.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "{:?} derivation missing derivation_path",
                self.derivation_type
            )
        })?;

        info!("🔨 Building derivation: {}", drv_path);

        if !drv_path.ends_with(".drv") {
            bail!("Expected .drv path, got: {}", drv_path);
        }

        let gc_root_path = get_gc_root_path(self.id).await;

        // Build the command
        let mut cmd = if build_config.should_use_systemd() {
            info!("  → Using systemd-run for {}", drv_path);
            let mut scoped = Command::new("systemd-run");
            scoped.args(["--scope", "--collect", "--quiet"]);
            apply_systemd_props_for_scope(build_config, &mut scoped);
            // IMPORTANT: add-root + indirect before the drv
            scoped.args([
                "--",
                "nix-store",
                "--realise",
                "--add-root",
                &gc_root_path,
                "--indirect",
                drv_path,
            ]);
            scoped
        } else {
            info!("  → Using direct nix-store for {}", drv_path);
            let mut direct = Command::new("nix-store");
            direct.args([
                "--realise",
                "--add-root",
                &gc_root_path,
                "--indirect",
                drv_path,
            ]);
            direct
        };

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        build_config.apply_to_command(&mut cmd);

        info!("  → About to spawn command for {}", drv_path);

        // Try to run with systemd
        match Self::run_streaming_build(cmd, drv_path, self.id, pool, job_id).await {
            Ok(output_path) => {
                info!("✅ Build succeeded: {}", output_path);
                Ok(output_path)
            }
            // Propagate cancellation immediately — do not fall back to direct build.
            Err(e) if e.downcast_ref::<BuildCancelledError>().is_some() => Err(e),
            Err(e) if build_config.should_use_systemd() && Self::is_systemd_error(&e) => {
                warn!(
                    "⚠️  Systemd scope creation failed, falling back to direct execution: {}",
                    e
                );
                self.build_with_direct_nix_store(pool, drv_path, build_config, job_id)
                    .await
            }
            Err(e) => {
                error!("❌ Build failed for {}: {}", drv_path, e);
                error!("   Error details: {:?}", e);
                Err(e)
            }
        }
    }

    /// Sign a store path recursively with streaming output
    pub async fn sign(&self, cache_config: &CacheConfig) -> Result<()> {
        let store_path = self
            .store_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Derivation {} missing store_path", self.id))?;

        let sign_cmd = match cache_config.sign_command(store_path) {
            Some(cmd) => cmd,
            None => {
                debug!("No signing key configured, skipping sign");
                return Ok(());
            }
        };

        info!("Signing store path: {}", store_path);

        let mut cmd = Command::new(&sign_cmd.command);
        cmd.args(&sign_cmd.args);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let success = Self::run_streaming_command(&mut cmd, "nix store sign").await?;

        if success {
            info!("Successfully signed {}", store_path);
            Ok(())
        } else {
            bail!("Failed to sign store path: {}", store_path)
        }
    }
    /// Generic streaming command runner for non-build operations
    pub async fn run_streaming_command(cmd: &mut Command, operation_name: &str) -> Result<bool> {
        info!("  → Spawning {}", operation_name);

        let mut child = cmd.spawn().context("Failed to spawn command")?;

        let stdout = child.stdout.take().expect("Failed to capture stdout");
        let stderr = child.stderr.take().expect("Failed to capture stderr");

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        loop {
            tokio::select! {
                line_result = stdout_reader.next_line() => {
                    match line_result {
                        Ok(Some(line)) => {
                            debug!("{} stdout: {}", operation_name, line);
                        }
                        Ok(None) => break,
                        Err(e) => {
                            error!("Error reading stdout: {}", e);
                            break;
                        }
                    }
                }

                line_result = stderr_reader.next_line() => {
                    match line_result {
                        Ok(Some(line)) => {
                            debug!("{} stderr: {}", operation_name, line);
                        }
                        Ok(None) => {},
                        Err(e) => {
                            error!("Error reading stderr: {}", e);
                        }
                    }
                }
            }
        }

        let status = child.wait().await?;
        Ok(status.success())
    }

    /// Verify a store path is signed (if signing is configured)
    pub async fn verify_signed(&self, cache_config: &CacheConfig) -> Result<()> {
        // Only check if signing is configured
        if cache_config.signing_key.is_none() {
            debug!("Signing not configured, skipping verification");
            return Ok(());
        }

        let store_path = self
            .store_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Derivation {} missing store_path", self.id))?;

        let output = Command::new("nix")
            .args(["store", "info", store_path, "--json"])
            .output()
            .await?;

        if !output.status.success() {
            bail!("Failed to verify signatures for {}", store_path);
        }

        // Check if path has valid signatures (you might parse JSON here)
        let info = String::from_utf8_lossy(&output.stdout);
        if !info.contains("signatures") {
            warn!(
                "Path {} has no signatures, but signing is configured",
                store_path
            );
            // Decide: fail or warn?
            // bail!("Path is not signed");  // <- strict mode
        }

        Ok(())
    }

    /// Run a build command with streaming output and periodic database updates
    async fn run_streaming_build(
        mut cmd: Command,
        drv_path: &str,
        derivation_id: i32,
        pool: &PgPool,
        job_id: Option<Uuid>,
    ) -> Result<String> {
        let start_time = Instant::now();
        info!("  → Spawning build process for {}", drv_path);

        let mut child = match cmd.spawn() {
            Ok(child) => {
                info!(
                    "  ✅ Process spawned successfully (PID: {})",
                    child.id().unwrap_or(0)
                );
                child
            }
            Err(e) => {
                error!("  ❌ SPAWN FAILED for {}: {}", drv_path, e);
                error!("     Error kind: {:?}", e.kind());
                error!("     OS error: {:?}", e.raw_os_error());
                return Err(anyhow::anyhow!("Failed to spawn build process: {}", e));
            }
        };

        let stdout = child.stdout.take().expect("Failed to capture stdout");
        let stderr = child.stderr.take().expect("Failed to capture stderr");

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut heartbeat_interval = interval(Duration::from_secs(5));
        // Cancel-check runs every 15 s — infrequent enough to not hammer the DB.
        let mut cancel_check_interval = interval(Duration::from_secs(15));
        // Consume the first (immediate) tick so the check doesn't fire at t=0.
        cancel_check_interval.tick().await;

        let mut current_target: Option<String> = None;
        let pool_clone = pool.clone();
        let mut last_output = Instant::now();
        let mut cancelled = false;

        loop {
            tokio::select! {
                // Read stdout
                line_result = stdout_reader.next_line() => {
                    match line_result {
                        Ok(Some(line)) => {
                            last_output = Instant::now();
                            info!("build stdout: {}", line);
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

                // Periodic derivation heartbeat (DB progress update)
                _ = heartbeat_interval.tick() => {
                    let elapsed = start_time.elapsed().as_secs() as i32;
                    let last_activity = last_output.elapsed().as_secs() as i32;
                    if let Err(e) = Self::update_build_heartbeat(
                        &pool_clone,
                        derivation_id,
                        elapsed,
                        current_target.as_deref(),
                        last_activity,
                    ).await {
                        warn!("Failed to update build heartbeat: {}", e);
                    }
                }

                // Cancel-check: poll the job's status in build_jobs every ~15 s.
                // If an operator has set it to 'cancelling', kill the child and break.
                _ = cancel_check_interval.tick() => {
                    if let Some(jid) = job_id {
                        match crate::queries::builders::get_build_job_status(&pool_clone, &jid).await {
                            Ok(Some(ref status)) if status == "cancelling" => {
                                info!("🛑 Job {} marked cancelling — sending SIGTERM to build process", jid);
                                // Send SIGTERM; if the process doesn't exit within 30 s we SIGKILL.
                                let _ = child.start_kill(); // sends SIGKILL on tokio::process::Child
                                // Give nix a brief window to flush output before we hard-kill.
                                tokio::time::sleep(Duration::from_secs(2)).await;
                                let _ = child.kill().await;
                                cancelled = true;
                                break;
                            }
                            Ok(_) => {} // still running normally
                            Err(e) => {
                                warn!("Cancel-check DB query failed (non-fatal): {}", e);
                            }
                        }
                    }
                }
            }
        }

        if cancelled {
            // Don't wait for the process — we killed it.
            return Err(anyhow::anyhow!(BuildCancelledError));
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

    /// Update the database with build progress information.
    ///
    /// Delegates to [`crate::queries::derivations::update_build_heartbeat`].
    async fn update_build_heartbeat(
        pool: &PgPool,
        derivation_id: i32,
        elapsed_seconds: i32,
        current_target: Option<&str>,
        last_activity_seconds: i32,
    ) -> Result<()> {
        crate::queries::derivations::update_build_heartbeat(
            pool,
            derivation_id,
            elapsed_seconds,
            current_target,
            last_activity_seconds,
        )
        .await
    }

    /// Fallback: build directly without systemd isolation
    async fn build_with_direct_nix_store(
        &self,
        pool: &PgPool,
        drv_path: &str,
        build_config: &BuildConfig,
        job_id: Option<Uuid>,
    ) -> Result<String> {
        info!(
            "🔨 Building with direct nix-store (no systemd): {}",
            drv_path
        );

        let mut cmd = Command::new("nix-store");
        cmd.args(["--realise", drv_path]);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        build_config.apply_to_command(&mut cmd);

        Self::run_streaming_build(cmd, drv_path, self.id, pool, job_id).await
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
