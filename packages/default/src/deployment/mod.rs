use crate::models::config::CrystalForgeConfig;
use crate::models::systems::DeploymentPolicy;
use crate::queries::deployment::{get_systems_with_auto_latest_policy, update_desired_target};
use crate::queries::derivations::get_latest_deployable_targets_for_flake_hosts;
use anyhow::{Context, Result};
use sqlx::PgPool;
use std::collections::HashMap;
use tokio::time::{Instant, sleep};
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
        use std::collections::HashMap;

        if systems.is_empty() {
            return Ok(0);
        }

        // Collect hostnames we’re responsible for
        let hostnames: Vec<String> = systems.iter().map(|s| s.hostname.clone()).collect();

        // Fetch per-host latest deployable targets for the latest commit
        let per_host =
            get_latest_deployable_targets_for_flake_hosts(&self.pool, flake_id, &hostnames).await?;
        let latest_by_host: HashMap<_, _> = per_host
            .into_iter()
            .filter_map(|h| h.derivation_target.map(|t| (h.hostname, t)))
            .collect();

        let mut updated_count = 0;

        for system in systems {
            // Defensive: ensure auto-latest
            match system.get_deployment_policy() {
                Ok(DeploymentPolicy::AutoLatest) => {}
                Ok(other) => {
                    warn!(
                        "System {} has {:?}; skipping auto_latest updater",
                        system.hostname, other
                    );
                    continue;
                }
                Err(e) => {
                    warn!("System {} has invalid policy: {}", system.hostname, e);
                    continue;
                }
            }

            let Some(latest_target_for_host) = latest_by_host.get(&system.hostname) else {
                debug!(
                    "No deployable nixos derivation on latest commit for host {}",
                    system.hostname
                );
                continue;
            };

            if system.desired_target.as_deref() == Some(latest_target_for_host.as_str()) {
                debug!("System {} already at latest target", system.hostname);
                continue;
            }

            if let Err(e) =
                update_desired_target(&self.pool, &system.hostname, Some(latest_target_for_host))
                    .await
            {
                error!(
                    "Failed to set desired_target for {} -> {}: {:#}",
                    system.hostname, latest_target_for_host, e
                );
            } else {
                info!(
                    "📋 Updated desired target for {}: {:?} -> {}",
                    system.hostname,
                    system.desired_target.as_deref(),
                    latest_target_for_host
                );
                updated_count += 1;
            }
        }

        Ok(updated_count)
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
