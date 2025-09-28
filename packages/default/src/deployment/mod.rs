use crate::models::config::CrystalForgeConfig;
use crate::models::systems::DeploymentPolicy;
use crate::queries::deployment::{get_systems_with_auto_latest_policy, update_desired_target};
use crate::queries::derivations::get_latest_successful_derivation_for_flake;
use anyhow::{Context, Result};
use sqlx::PgPool;
use std::collections::HashMap;
use tokio::time::{Duration, Instant, sleep};
use tracing::{debug, error, info, warn};
pub mod agent;
pub use agent::*;
/// Manages automatic deployment policies for systems
/// Only handles auto_latest policy - manual and pinned policies are set by admin intervention
pub struct DeploymentPolicyManager {
    config: CrystalForgeConfig,
    pool: PgPool,
}

impl DeploymentPolicyManager {
    pub fn new(config: CrystalForgeConfig, pool: PgPool) -> Self {
        Self { config, pool }
    }

    /// Main deployment policy management loop
    /// Only processes systems with auto_latest policy - manual/pinned policies don't need automatic updates
    pub async fn run(&self) -> Result<()> {
        let interval = self.config.deployment.deployment_poll_interval;
        info!(
            "🚀 Starting deployment policy manager (poll interval: {:?})",
            interval
        );

        loop {
            let start_time = Instant::now();

            match self.update_auto_latest_policies().await {
                Ok(stats) => {
                    let elapsed = start_time.elapsed();
                    info!(
                        "✅ Policy update completed: {} systems checked, {} updated ({:.2}s)",
                        stats.systems_checked,
                        stats.systems_updated,
                        elapsed.as_secs_f64()
                    );
                }
                Err(e) => {
                    error!("❌ Policy update failed: {:#}", e);
                }
            }

            sleep(interval).await;
        }
    }

    /// Update desired_target for all systems with auto_latest policy
    async fn update_auto_latest_policies(&self) -> Result<PolicyUpdateStats> {
        let mut stats = PolicyUpdateStats::default();

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
            match self.update_flake_systems_to_latest(flake_id, systems).await {
                Ok(updated_count) => {
                    stats.systems_updated += updated_count;
                }
                Err(e) => {
                    error!("Failed to update systems for flake {}: {:#}", flake_id, e);
                }
            }
        }

        Ok(stats)
    }

    /// Update all systems using a specific flake to the latest successful derivation
    async fn update_flake_systems_to_latest(
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

        // Get the flake target from the successful derivation
        let latest_flake_target = self.get_derivation_flake_target(&latest_derivation)?;
        let mut updated_count = 0;

        for system in systems {
            // Skip if system already has the latest flake target as desired target
            if system.desired_target.as_ref() == Some(&latest_flake_target) {
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

            // Update the system's desired target to the latest flake target
            match update_desired_target(&self.pool, &system.hostname, Some(&latest_flake_target))
                .await
            {
                Ok(()) => {
                    info!(
                        "📋 Updated desired target for {}: {} -> {}",
                        system.hostname,
                        system.desired_target.as_deref().unwrap_or("(none)"),
                        latest_flake_target
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

    /// Get the flake target from a successful derivation for deployment
    /// For deployments, we need the flake target that agents can actually deploy
    fn get_derivation_flake_target(
        &self,
        derivation: &crate::models::derivations::Derivation,
    ) -> Result<String> {
        // derivation_target should contain the flake target for deployments
        if let Some(target) = &derivation.derivation_target {
            Ok(target.clone())
        } else {
            anyhow::bail!(
                "Derivation {} has no derivation_target - cannot determine flake target for deployment",
                derivation.id
            );
        }
    }
}

#[derive(Default)]
struct PolicyUpdateStats {
    systems_checked: usize,
    systems_updated: usize,
}

/// Spawn the deployment policy manager as a background task
pub async fn spawn_deployment_policy_manager(
    config: CrystalForgeConfig,
    pool: PgPool,
) -> Result<tokio::task::JoinHandle<()>> {
    let manager = DeploymentPolicyManager::new(config, pool);

    let handle = tokio::spawn(async move {
        if let Err(e) = manager.run().await {
            error!("💥 Deployment policy manager crashed: {:#}", e);
        }
    });

    Ok(handle)
}
