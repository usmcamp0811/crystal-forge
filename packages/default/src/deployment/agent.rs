use crate::handlers::agent::heartbeat::LogResponse;
use crate::models::config::deployment::DeploymentConfig;
use anyhow::{Context, Result};
use std::process::Command;
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
    Failed {
        error: String,
        desired_target: String,
    },
}

impl DeploymentResult {
    /// Check if the deployment was successful
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            DeploymentResult::NoDeploymentNeeded
                | DeploymentResult::AlreadyOnTarget
                | DeploymentResult::SuccessFromCache { .. }
                | DeploymentResult::SuccessLocalBuild
        )
    }

    /// Get a human-readable description
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
            DeploymentResult::Failed {
                error,
                desired_target,
            } => {
                format!("Deployment failed for {}: {}", desired_target, error)
            }
        }
    }

    /// Determine the change reason for system state reporting
    pub fn change_reason(&self) -> &'static str {
        match self {
            DeploymentResult::SuccessFromCache { .. } | DeploymentResult::SuccessLocalBuild => {
                "cf_deployment"
            }
            _ => "heartbeat", // No change occurred
        }
    }
}

/// Agent deployment manager handles applying deployments from server
pub struct AgentDeploymentManager {
    config: DeploymentConfig,
    current_target: Option<String>,
}

impl AgentDeploymentManager {
    pub fn new(config: DeploymentConfig) -> Self {
        Self {
            config,
            current_target: None,
        }
    }

    /// Process a heartbeat response from the server and apply deployment if needed
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

        // Check if we're already on the target
        if let Some(ref current) = self.current_target {
            if current == &desired_target {
                debug!("Already on target, skipping deployment");
                return Ok(DeploymentResult::AlreadyOnTarget);
            }
        }

        // Attempt deployment
        match self.execute_deployment(&desired_target).await {
            Ok(result) => {
                info!("Deployment completed successfully");
                self.current_target = Some(desired_target);
                Ok(result)
            }
            Err(e) => {
                error!("Deployment failed: {:#}", e);
                Ok(DeploymentResult::Failed {
                    error: e.to_string(),
                    desired_target,
                })
            }
        }
    }

    /// Execute the actual deployment
    async fn execute_deployment(&self, target: &str) -> Result<DeploymentResult> {
        info!("Starting deployment execution for: {}", target);

        // Optionally run dry-run first
        if self.config.dry_run_first {
            self.execute_dry_run(target)
                .await
                .context("Dry run failed")?;
        }

        // Execute the actual deployment
        let start_time = std::time::Instant::now();

        let result = if let Some(ref cache_url) = self.config.cache_url {
            // Try cache-based deployment first
            match self.deploy_from_cache(target, cache_url).await {
                Ok(result) => result,
                Err(e) if self.config.fallback_to_local_build => {
                    warn!(
                        "Cache deployment failed, falling back to local build: {}",
                        e
                    );
                    self.deploy_local_build(target).await?
                }
                Err(e) => return Err(e),
            }
        } else {
            // Direct local build
            self.deploy_local_build(target).await?
        };

        let duration = start_time.elapsed();
        info!(
            "Deployment completed in {:.2} seconds",
            duration.as_secs_f64()
        );

        Ok(result)
    }

    /// Execute a dry-run to verify the deployment would work
    async fn execute_dry_run(&self, target: &str) -> Result<()> {
        info!("Executing dry-run for target: {}", target);

        let output = Command::new("nixos-rebuild")
            .args(&["dry-run", "--flake", target])
            .output()
            .context("Failed to execute nixos-rebuild dry-run")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Dry-run failed: {}", stderr);
        }

        debug!("Dry-run completed successfully");
        Ok(())
    }

    /// Deploy using binary cache
    async fn deploy_from_cache(&self, target: &str, cache_url: &str) -> Result<DeploymentResult> {
        info!("Deploying from cache: {} (target: {})", cache_url, target);

        let output = Command::new("nixos-rebuild")
            .args(&[
                "switch",
                "--flake",
                target,
                "--option",
                "substituters",
                cache_url,
                "--option",
                "require-sigs",
                "true",
            ])
            .output()
            .context("Failed to execute nixos-rebuild switch with cache")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Cache deployment failed: {}", stderr);
        }

        Ok(DeploymentResult::SuccessFromCache {
            cache_url: cache_url.to_string(),
        })
    }

    /// Deploy using local build
    async fn deploy_local_build(&self, target: &str) -> Result<DeploymentResult> {
        info!("Deploying with local build: {}", target);

        let output = Command::new("nixos-rebuild")
            .args(&["switch", "--flake", target])
            .output()
            .context("Failed to execute nixos-rebuild switch")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Local build deployment failed: {}", stderr);
        }

        Ok(DeploymentResult::SuccessLocalBuild)
    }

    /// Update current target (called when system state changes)
    pub fn update_current_target(&mut self, target: Option<String>) {
        self.current_target = target;
    }

    /// Get current target
    pub fn current_target(&self) -> Option<&str> {
        self.current_target.as_deref()
    }
}
