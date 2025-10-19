use crate::handlers::agent::heartbeat::LogResponse;
use crate::models::config::deployment::DeploymentConfig;
use anyhow::{Context, Result};
use std::process::Command;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

/// Result of a deployment operation
#[derive(Debug, Clone)]
pub enum DeploymentResult {
    NoDeploymentNeeded,
    AlreadyOnTarget,
    SuccessFromCache {
        cache_url: String,
    },
    SuccessLocalBuild,
    Started {
        // ADD THIS
        unit_name: String,
    },
    Failed {
        error: String,
        desired_target: String,
    },
}

impl DeploymentResult {
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            DeploymentResult::NoDeploymentNeeded
                | DeploymentResult::AlreadyOnTarget
                | DeploymentResult::SuccessFromCache { .. }
                | DeploymentResult::SuccessLocalBuild
                | DeploymentResult::Started { .. } // ADD THIS
        )
    }

    pub fn description(&self) -> String {
        match self {
            DeploymentResult::NoDeploymentNeeded => "No deployment needed".to_string(),
            DeploymentResult::AlreadyOnTarget => "Already on target".to_string(),
            DeploymentResult::SuccessFromCache { cache_url } => {
                format!("Successfully deployed from cache: {}", cache_url)
            }
            DeploymentResult::SuccessLocalBuild => {
                "Successfully deployed with local build".to_string()
            }
            DeploymentResult::Started { unit_name } => {
                // ADD THIS
                format!("Deployment started in unit: {}", unit_name)
            }
            DeploymentResult::Failed {
                error,
                desired_target,
            } => {
                format!("Deployment failed for {}: {}", desired_target, error)
            }
        }
    }

    pub fn change_reason(&self) -> &'static str {
        match self {
            DeploymentResult::SuccessFromCache { .. }
            | DeploymentResult::SuccessLocalBuild
            | DeploymentResult::Started { .. } => {
                // ADD THIS
                "cf_deployment"
            }
            _ => "heartbeat",
        }
    }
}

/// Agent deployment manager handles applying deployments from server
pub struct AgentDeploymentManager {
    config: DeploymentConfig,
    current_target: Option<String>,
    deployment_lock: Arc<Semaphore>,
}

impl AgentDeploymentManager {
    pub fn new(config: DeploymentConfig) -> Self {
        Self {
            config,
            current_target: None,
            deployment_lock: Arc::new(Semaphore::new(1)),
        }
    }

    pub async fn process_heartbeat_response(
        &mut self,
        response: LogResponse,
    ) -> Result<DeploymentResult> {
        debug!("Processing heartbeat response");

        let Some(desired_target) = response.desired_target else {
            debug!("No desired target in heartbeat response");
            return Ok(DeploymentResult::NoDeploymentNeeded);
        };

        info!("Received desired target: {}", desired_target);

        if let Some(ref current) = self.current_target {
            if current == &desired_target {
                debug!("Already on target, skipping deployment");
                return Ok(DeploymentResult::AlreadyOnTarget);
            }
        }

        match self.execute_deployment(&desired_target).await {
            Ok(result) => {
                info!("Deployment completed successfully");
                self.current_target = Some(desired_target.to_string());
                Ok(result)
            }
            Err(e) => {
                error!("Deployment failed: {:#}", e);
                Ok(DeploymentResult::Failed {
                    error: e.to_string(),
                    desired_target: desired_target.to_string(),
                })
            }
        }
    }

    async fn execute_deployment(&self, target: &str) -> Result<DeploymentResult> {
        let _permit = self.deployment_lock.acquire().await?;

        info!("Starting deployment execution for: {}", target);

        if self.config.dry_run_first {
            self.execute_dry_run(target)
                .await
                .context("Dry run failed")?;
        }

        let start_time = std::time::Instant::now();

        let result = if let Some(ref cache_url) = self.config.cache_url {
            // Cache deployment with lock-aware retry
            match self.deploy_from_cache_lockaware(target, cache_url).await {
                Ok(r) => r,
                Err(e) if self.config.fallback_to_local_build => {
                    warn!(
                        "Cache deployment failed, falling back to local build: {}",
                        e
                    );
                    self.deploy_local_build_lockaware(target).await?
                }
                Err(e) => return Err(e),
            }
        } else {
            // Local build with lock-aware retry
            self.deploy_local_build_lockaware(target).await?
        };

        let duration = start_time.elapsed();
        info!(
            "Deployment completed in {:.2} seconds",
            duration.as_secs_f64()
        );

        Ok(result)
    }

    async fn execute_dry_run(&self, target: &str) -> Result<()> {
        info!("Executing dry-run for target: {}", target);

        let output = Command::new("nixos-rebuild")
            .args(&["dry-run", "--flake", target])
            .output()
            .context("Failed to execute nixos-rebuild dry-run")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !stdout.is_empty() {
            debug!("nixos-rebuild dry-run stdout: {}", stdout);
        }
        if !stderr.is_empty() {
            debug!("nixos-rebuild dry-run stderr: {}", stderr);
        }

        if !output.status.success() {
            error!(
                "nixos-rebuild dry-run failed with exit code: {:?}",
                output.status.code()
            );
            error!("stdout: {}", stdout);
            error!("stderr: {}", stderr);
            anyhow::bail!(
                "Dry-run failed with exit code {:?}\nstdout: {}\nstderr: {}",
                output.status.code(),
                stdout.trim(),
                stderr.trim()
            );
        }

        debug!("Dry-run completed successfully");
        Ok(())
    }

    /// Deploy using binary cache (lock-aware)
    async fn deploy_from_cache_lockaware(
        &self,
        target: &str,
        cache_url: &str,
    ) -> Result<DeploymentResult> {
        info!("Deploying from cache: {} (target: {})", cache_url, target);

        // First attempt
        match self.deploy_from_cache_once(target, cache_url).await {
            Ok(ok) => return Ok(ok),
            Err(e) => {
                let (stdout, stderr) = extract_stdouterr(&e);
                if is_lock_error(&stderr) || is_lock_error(&stdout) {
                    warn!("Cache deployment hit a lock; attempting to clear locks then retry");
                    self.handle_and_clear_locks().await?;
                    // One retry after clearing
                    return self.deploy_from_cache_once(target, cache_url).await;
                }
                Err(e)
            }
        }
    }

    async fn deploy_from_cache_once(
        &self,
        target: &str,
        cache_url: &str,
    ) -> Result<DeploymentResult> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let unit_name = format!("crystal-forge-deploy-{}", timestamp);

        let mut args = vec![
            "--unit",
            &unit_name,
            "--no-block",
            "--same-dir",
            "--collect",
            "--",
            "nixos-rebuild",
            "switch",
            "--flake",
            target,
            "--option",
            "substituters",
            cache_url,
        ];

        let public_key_str;
        if let Some(ref public_key) = self.config.cache_public_key {
            public_key_str = public_key.clone();
            args.extend_from_slice(&[
                "--option",
                "trusted-public-keys",
                &public_key_str,
                "--option",
                "require-sigs",
                "true",
            ]);
            debug!("Using cache with signature verification");
        } else {
            warn!("No cache public key configured, skipping signature verification");
            args.extend_from_slice(&["--option", "require-sigs", "false"]);
        }

        let output = Command::new("systemd-run")
            .args(&args)
            .output()
            .context("Failed to execute systemd-run with nixos-rebuild")?;

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("systemd-run failed stdout: {}", stdout);
            error!("systemd-run failed stderr: {}", stderr);
            anyhow::bail!(
                "systemd-run failed with exit code {:?}\nstdout: {}\nstderr: {}",
                output.status.code(),
                stdout.trim(),
                stderr.trim()
            );
        }

        info!("Deployment detached to systemd unit: {}", unit_name);
        Ok(DeploymentResult::Started { unit_name })
    }

    /// Local build (lock-aware, with backoff + clear)
    async fn deploy_local_build_lockaware(&self, target: &str) -> Result<DeploymentResult> {
        info!("Deploying with local build: {}", target);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let unit_name = format!("crystal-forge-deploy-{}", timestamp);

        let output = Command::new("systemd-run")
            .args(&[
                "--unit",
                &unit_name,
                "--no-block",
                "--same-dir",
                "--collect",
                "--",
                "nixos-rebuild",
                "switch",
                "--flake",
                target,
            ])
            .output()
            .context("Failed to execute systemd-run with nixos-rebuild")?;

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("systemd-run failed stdout: {}", stdout);
            error!("systemd-run failed stderr: {}", stderr);
            anyhow::bail!(
                "systemd-run failed with exit code {:?}\nstdout: {}\nstderr: {}",
                output.status.code(),
                stdout.trim(),
                stderr.trim()
            );
        }

        info!("Deployment detached to systemd unit: {}", unit_name);
        Ok(DeploymentResult::Started { unit_name })
    }

    pub fn update_current_target(&mut self, target: Option<String>) {
        self.current_target = target;
    }

    pub fn current_target(&self) -> Option<&str> {
        self.current_target.as_deref()
    }

    /// Combined helper: if another nixos-rebuild is actually running, wait briefly;
    /// then remove known stale profile lock files.
    async fn handle_and_clear_locks(&self) -> Result<()> {
        self.handle_running_rebuild().await?;
        self.force_clear_locks().await?;
        Ok(())
    }

    async fn handle_running_rebuild(&self) -> Result<()> {
        // Check if nixos-rebuild is running
        let check = Command::new("pgrep")
            .args(&["-f", "nixos-reload|nixos-rebuild"])
            .output()
            .context("Failed to run pgrep to check nixos-rebuild")?;

        if check.status.success() && !check.stdout.is_empty() {
            warn!("Another nixos-rebuild appears to be running; waiting 5s");
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
        Ok(())
    }

    async fn force_clear_locks(&self) -> Result<()> {
        use std::fs;

        // Known lock files that can get stuck
        const LOCK_PATHS: &[&str] = &[
            "/nix/var/nix/profiles/system.lock",
            "/nix/var/nix/profiles/per-user/root/profile.lock",
        ];

        warn!("Attempting to clear system profile locks");

        for path in LOCK_PATHS {
            if std::path::Path::new(path).exists() {
                info!("Removing stale lock: {}", path);
                // Best-effort delete; log errors but continue
                if let Err(e) = fs::remove_file(path) {
                    warn!("Failed to remove {}: {}", path, e);
                }
            }
        }
        Ok(())
    }
}

/// Utility: detect various lock messages from nix/nixos-rebuild
fn is_lock_error(s: &str) -> bool {
    let s = s.to_ascii_lowercase();
    s.contains("could not acquire lock")
        || s.contains("cannot acquire write lock")
        || s.contains("timed out while waiting for lock")
        || s.contains("lock is held by")
        || s.contains("error: opening lock file")
}

/// Best effort to pull stdout/stderr from an anyhow error message we created above
fn extract_stdouterr(e: &anyhow::Error) -> (String, String) {
    // Our bail! messages include "stdout: ...\nstderr: ..." — grab those if present.
    let msg = format!("{:#}", e);
    let mut out = String::new();
    let mut err = String::new();
    if let Some(i) = msg.find("stdout:") {
        if let Some(j) = msg[i..].find("\nstderr:") {
            out = msg[i + 7..i + j].trim().to_string();
            err = msg[i + j + 9..].trim().to_string();
        }
    }
    (out, err)
}
