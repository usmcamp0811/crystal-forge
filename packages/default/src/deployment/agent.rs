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
                | DeploymentResult::Started { .. }
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
            | DeploymentResult::Started { .. } => "cf_deployment",
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

        let is_store_path = target.starts_with("/nix/store/");

        // Store paths REQUIRE cache to be configured
        if is_store_path && self.config.cache_url.is_none() {
            anyhow::bail!(
                "Cannot deploy store path without cache configured. Target: {}",
                target
            );
        }

        if self.config.dry_run_first && !is_store_path {
            self.execute_dry_run(target)
                .await
                .context("Dry run failed")?;
        }

        let start_time = std::time::Instant::now();

        let result = if is_store_path {
            // Store paths: ALWAYS deploy from cache
            let cache_url = self.config.cache_url.as_ref().unwrap(); // Safe because we checked above
            self.deploy_store_path_from_cache(target, cache_url).await?
        } else if let Some(ref cache_url) = self.config.cache_url {
            // Flake URLs: Try cache first, optionally fall back to local build
            match self.deploy_flake_from_cache(target, cache_url).await {
                Ok(r) => r,
                Err(e) if self.config.fallback_to_local_build => {
                    warn!(
                        "Cache deployment failed, falling back to local build: {}",
                        e
                    );
                    self.deploy_flake_local_build(target).await?
                }
                Err(e) => return Err(e),
            }
        } else {
            // Flake URLs without cache: local build only
            self.deploy_flake_local_build(target).await?
        };

        let duration = start_time.elapsed();
        info!(
            "Deployment completed in {:.2} seconds",
            duration.as_secs_f64()
        );

        Ok(result)
    }

    async fn execute_dry_run(&self, target: &str) -> Result<()> {
        info!("Executing dry-run for flake target: {}", target);

        let output = Command::new("nixos-rebuild")
            .args(&["dry-run", "--flake", target])
            .output()
            .context("Failed to execute nixos-rebuild dry-run")?;

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
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

    async fn deploy_store_path_from_cache(
        &self,
        store_path: &str,
        cache_url: &str,
    ) -> Result<DeploymentResult> {
        info!("Deploying store path from cache: {}", store_path);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let unit_name = format!("crystal-forge-deploy-{}", timestamp);

        // Build nix copy command
        let mut copy_args = vec!["copy", "--from", cache_url, store_path];

        let public_key_str;
        if let Some(ref public_key) = self.config.cache_public_key {
            public_key_str = public_key.clone();
            copy_args.extend_from_slice(&[
                "--option",
                "trusted-public-keys",
                &public_key_str,
                "--option",
                "http-connections",
                "50",
                "--option",
                "require-sigs",
                "true",
            ]);
            debug!("Using cache with signature verification");
        } else {
            warn!("No cache public key configured, skipping signature verification");
            copy_args.extend_from_slice(&["--option", "require-sigs", "false"]);
        }

        // Step 1: Copy from cache
        info!("Copying {} from cache...", store_path);
        let mut child = Command::new("nix")
            .args(&copy_args)
            .spawn()
            .context("Failed to spawn nix copy")?;

        let start = std::time::Instant::now();
        loop {
            match child.try_wait()? {
                Some(status) => {
                    if !status.success() {
                        anyhow::bail!("nix copy failed with exit code {:?}", status.code(),);
                    }
                    break;
                }
                None => {
                    let elapsed = start.elapsed().as_secs();
                    info!(
                        "Still copying {} from cache... ({}s elapsed)",
                        store_path, elapsed
                    );
                    std::thread::sleep(std::time::Duration::from_secs(30));
                }
            }
        }

        info!("Successfully copied {} from cache", store_path);

        // Step 2: Activate the configuration using systemd-run
        let switch_script = format!("{}/bin/switch-to-configuration", store_path);

        let output = Command::new("systemd-run")
            .args(&[
                "--unit",
                &unit_name,
                "--no-block",
                "--same-dir",
                "--collect",
                "--",
                &switch_script,
                "switch",
            ])
            .output()
            .context("Failed to execute systemd-run with switch-to-configuration")?;

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

    async fn deploy_flake_from_cache(
        &self,
        target: &str,
        cache_url: &str,
    ) -> Result<DeploymentResult> {
        info!("Deploying flake from cache (legacy method): {}", target);

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

    /// Local build for flake URLs only
    async fn deploy_flake_local_build(&self, target: &str) -> Result<DeploymentResult> {
        info!("Deploying flake with local build: {}", target);

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
}
