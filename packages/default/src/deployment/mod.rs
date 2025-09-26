use crate::models::config::CrystalForgeConfig;
use crate::models::systems::DeploymentPolicy;
use crate::queries::derivations::get_latest_successful_derivation_for_flake;
use crate::queries::systems::{get_systems_with_auto_latest_policy, update_desired_target};
use anyhow::{Context, Result};
use sqlx::PgPool;
use std::collections::HashMap;
use tokio::time::{Duration, Instant, sleep};
use tracing::{debug, error, info, warn};

/// Deployment evaluator that manages automatic deployment policies
pub struct DeploymentEvaluator {
    config: CrystalForgeConfig,
    pool: PgPool,
}

impl DeploymentEvaluator {
    pub fn new(config: CrystalForgeConfig, pool: PgPool) -> Self {
        Self { config, pool }
    }

    /// Main deployment evaluation loop
    pub async fn run(&self) -> Result<()> {
        let interval = self.config.deployment.poll_interval;
        info!(
            "🚀 Starting deployment evaluator (poll interval: {:?})",
            interval
        );

        loop {
            let start_time = Instant::now();

            match self.evaluate_deployments().await {
                Ok(stats) => {
                    let elapsed = start_time.elapsed();
                    info!(
                        "✅ Deployment evaluation completed: {} systems checked, {} updated ({:.2}s)",
                        stats.systems_checked,
                        stats.systems_updated,
                        elapsed.as_secs_f64()
                    );
                }
                Err(e) => {
                    error!("❌ Deployment evaluation failed: {:#}", e);
                }
            }

            sleep(interval).await;
        }
    }

    /// Evaluate deployment policies for all systems
    async fn evaluate_deployments(&self) -> Result<EvaluationStats> {
        let mut stats = EvaluationStats::default();

        // Get all systems with auto_latest policy
        let auto_latest_systems = get_systems_with_auto_latest_policy(&self.pool)
            .await
            .context("Failed to fetch systems with auto_latest policy")?;

        stats.systems_checked = auto_latest_systems.len();

        if auto_latest_systems.is_empty() {
            debug!("No systems with auto_latest policy found");
            return Ok(stats);
        }

        // Group systems by flake_id to batch flake queries
        let mut systems_by_flake: HashMap<i32, Vec<_>> = HashMap::new();
        for system in auto_latest_systems {
            if let Some(flake_id) = system.flake_id {
                systems_by_flake.entry(flake_id).or_default().push(system);
            } else {
                warn!(
                    "System {} has auto_latest policy but no flake_id",
                    system.hostname
                );
            }
        }

        // Process each flake
        for (flake_id, systems) in systems_by_flake {
            match self.evaluate_flake_deployments(flake_id, systems).await {
                Ok(updated_count) => {
                    stats.systems_updated += updated_count;
                }
                Err(e) => {
                    error!(
                        "Failed to evaluate deployments for flake {}: {:#}",
                        flake_id, e
                    );
                }
            }
        }

        Ok(stats)
    }

    /// Evaluate deployments for all systems using a specific flake
    async fn evaluate_flake_deployments(
        &self,
        flake_id: i32,
        systems: Vec<crate::models::systems::System>,
    ) -> Result<usize> {
        // Get the latest successful derivation for this flake
        let latest_derivation =
            match get_latest_successful_derivation_for_flake(&self.pool, flake_id).await? {
                Some(derivation) => derivation,
                None => {
                    debug!("No successful derivations found for flake {}", flake_id);
                    return Ok(0);
                }
            };

        let latest_target = self.build_flake_target(&latest_derivation)?;
        let mut updated_count = 0;

        for system in systems {
            // Skip if system already has the latest target
            if system.desired_target.as_ref() == Some(&latest_target) {
                debug!("System {} already has latest target", system.hostname);
                continue;
            }

            // Skip if deployment policy is not auto_latest (defensive check)
            match system.get_deployment_policy() {
                Ok(DeploymentPolicy::AutoLatest) => {}
                Ok(policy) => {
                    warn!(
                        "System {} has policy {:?} but was in auto_latest query",
                        system.hostname, policy
                    );
                    continue;
                }
                Err(e) => {
                    warn!(
                        "System {} has invalid deployment policy: {}",
                        system.hostname, e
                    );
                    continue;
                }
            }

            // Update the system's desired target
            match update_desired_target(&self.pool, &system.hostname, Some(&latest_target)).await {
                Ok(()) => {
                    info!(
                        "📋 Updated desired target for {}: {} -> {}",
                        system.hostname,
                        system.desired_target.as_deref().unwrap_or("(none)"),
                        latest_target
                    );
                    updated_count += 1;
                }
                Err(e) => {
                    error!(
                        "Failed to update desired target for {}: {:#}",
                        system.hostname, e
                    );
                }
            }
        }

        Ok(updated_count)
    }

    /// Build flake target string from derivation
    fn build_flake_target(
        &self,
        derivation: &crate::models::derivations::Derivation,
    ) -> Result<String> {
        // This would build the flake target string from the derivation
        // For now, return the derivation target if available, or construct from derivation name
        if let Some(target) = &derivation.derivation_target {
            Ok(target.clone())
        } else {
            // Construct flake target - this would need to be implemented based on your flake structure
            // For example: "git+https://gitlab.com/user/repo?rev=commit#nixosConfigurations.hostname.config.system.build.toplevel"
            anyhow::bail!(
                "Derivation {} has no target and target construction not implemented",
                derivation.id
            );
        }
    }
}

#[derive(Default)]
struct EvaluationStats {
    systems_checked: usize,
    systems_updated: usize,
}

/// Spawn the deployment evaluator as a background task
pub async fn spawn_deployment_evaluator(
    config: CrystalForgeConfig,
    pool: PgPool,
) -> Result<tokio::task::JoinHandle<()>> {
    let evaluator = DeploymentEvaluator::new(config, pool);

    let handle = tokio::spawn(async move {
        if let Err(e) = evaluator.run().await {
            error!("💥 Deployment evaluator crashed: {:#}", e);
        }
    });

    Ok(handle)
}
