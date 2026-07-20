use crate::config::CrystalForgeConfig;
use crate::models::deployment_policies::{
    ApprovalConfig, CanaryConfig, CveThresholdConfig, DeploymentPolicyRecord, TimeWindowConfig,
};
use crate::models::systems::DeploymentPolicy;
use crate::queries::deployment::{get_systems_with_auto_latest_policy, update_desired_target};
use crate::queries::deployment_policies::get_deployment_policy_by_id;
use crate::queries::derivations::get_latest_deployable_targets_for_flake_hosts;
use crate::queries::environments::get_system_effective_policy_ids;
use crate::server::load_cve_policies;
use crate::services::approval_policy::{self, DeploymentContext};
use crate::services::canary_rollout::{self, RolloutContext};
use crate::services::cve_policy_gate::check_cve_policies;
use crate::services::cve_threshold_policy;
use crate::services::time_window_policy;
use anyhow::{Context, Result};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
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

#[derive(Debug)]
enum AdvancedGateDecision {
    Allow,
    Warn(String),
    Block(String),
    Pending(String),
}

fn map_time_window_decision(result: time_window_policy::TimeWindowResult) -> AdvancedGateDecision {
    if !result.deployment_allowed {
        return AdvancedGateDecision::Block(
            result
                .reason
                .unwrap_or_else(|| "Blocked by time window policy".to_string()),
        );
    }

    if let Some(reason) = result.reason {
        AdvancedGateDecision::Warn(reason)
    } else {
        AdvancedGateDecision::Allow
    }
}

fn map_approval_decision(result: approval_policy::ApprovalResult) -> AdvancedGateDecision {
    if result.deployment_allowed {
        AdvancedGateDecision::Allow
    } else {
        AdvancedGateDecision::Pending(result.reason.unwrap_or_else(|| {
            format!(
                "Approvals pending ({}/{})",
                result.approvals_received, result.approvals_required
            )
        }))
    }
}

fn map_canary_decision_for_system(
    result: canary_rollout::CanaryResult,
    system_id: uuid::Uuid,
) -> AdvancedGateDecision {
    if !result.deployment_allowed {
        return AdvancedGateDecision::Pending(
            result
                .reason
                .unwrap_or_else(|| "Canary rollout observation in progress".to_string()),
        );
    }

    if result.systems_to_deploy.contains(&system_id) {
        AdvancedGateDecision::Allow
    } else {
        AdvancedGateDecision::Pending(
            result
                .reason
                .unwrap_or_else(|| "System not selected for current canary phase".to_string()),
        )
    }
}

fn map_cve_threshold_decision(
    result: cve_threshold_policy::CveThresholdResult,
) -> AdvancedGateDecision {
    if !result.deployment_allowed {
        let reason = if result.warnings.is_empty() {
            "CVE threshold policy blocked deployment".to_string()
        } else {
            result.warnings.join("; ")
        };
        AdvancedGateDecision::Block(reason)
    } else if !result.warnings.is_empty() {
        AdvancedGateDecision::Warn(result.warnings.join("; "))
    } else {
        AdvancedGateDecision::Allow
    }
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
        if systems.is_empty() {
            return Ok(0);
        }

        // Collect flake configuration names we’re responsible for
        let config_names: Vec<String> = systems
            .iter()
            .map(|s| s.configuration_name().to_string())
            .collect();

        // Fetch per-host latest deployable targets for the latest commit
        let per_host =
            get_latest_deployable_targets_for_flake_hosts(&self.pool, flake_id, &config_names)
                .await?;
        let latest_by_host: HashMap<_, _> = per_host
            .into_iter()
            .map(|h| (h.hostname.clone(), h))
            .collect();

        // Build effective policy map for all systems in this flake batch.
        let mut effective_policy_ids_by_system: HashMap<uuid::Uuid, Vec<uuid::Uuid>> =
            HashMap::new();
        let mut all_policy_ids: HashSet<uuid::Uuid> = HashSet::new();
        let mut failed_policy_lookup_systems: HashSet<uuid::Uuid> = HashSet::new();
        for system in &systems {
            let policy_ids = match get_system_effective_policy_ids(&self.pool, system.id).await {
                Ok(ids) => ids,
                Err(err) => {
                    warn!(
                        "Failed to load effective deployment policies for {} ({}): {:#}; skipping deployment update",
                        system.hostname, system.id, err
                    );
                    failed_policy_lookup_systems.insert(system.id);
                    continue;
                }
            };
            for policy_id in &policy_ids {
                all_policy_ids.insert(*policy_id);
            }
            effective_policy_ids_by_system.insert(system.id, policy_ids);
        }

        let mut policies_by_id: HashMap<uuid::Uuid, DeploymentPolicyRecord> = HashMap::new();
        let mut failed_policy_loads: HashSet<uuid::Uuid> = HashSet::new();
        for policy_id in all_policy_ids {
            match get_deployment_policy_by_id(&self.pool, &policy_id).await {
                Ok(Some(policy)) => {
                    policies_by_id.insert(policy.id, policy);
                }
                Ok(None) => {
                    failed_policy_loads.insert(policy_id);
                }
                Err(err) => {
                    warn!("Failed to load deployment policy {}: {:#}", policy_id, err);
                    failed_policy_loads.insert(policy_id);
                }
            }
        }

        let mut updated_count = 0;

        for system in &systems {
            if failed_policy_lookup_systems.contains(&system.id) {
                warn!(
                    "Skipping deployment update for {} because effective policy lookup failed",
                    system.hostname
                );
                continue;
            }

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

            let Some(latest_target_for_host) = latest_by_host.get(system.configuration_name())
            else {
                debug!(
                    "No deployable nixos derivation on latest commit for host {} (config {})",
                    system.hostname,
                    system.configuration_name()
                );
                continue;
            };

            let Some(store_path) = latest_target_for_host.store_path.as_ref() else {
                debug!(
                    "No store path for host {}; skipping desired_target update",
                    system.hostname
                );
                continue;
            };

            if system.desired_target.as_deref() == Some(store_path.as_str()) {
                debug!("System {} already at latest target", system.hostname);
                continue;
            }

            let decision = self
                .evaluate_advanced_policy_gates(
                    system,
                    latest_target_for_host,
                    &systems,
                    &effective_policy_ids_by_system,
                    &policies_by_id,
                    &failed_policy_loads,
                )
                .await;

            match decision {
                AdvancedGateDecision::Allow => {
                    debug!(
                        "✅ Advanced policy gates passed for {} -> {}",
                        system.hostname, store_path
                    );
                }
                AdvancedGateDecision::Warn(reason) => {
                    warn!(
                        "⚠️ Advanced policy warning for {} -> {}: {}",
                        system.hostname, store_path, reason
                    );
                }
                AdvancedGateDecision::Pending(reason) => {
                    info!(
                        "⏳ Advanced policy pending for {} -> {}: {}",
                        system.hostname, store_path, reason
                    );
                    continue;
                }
                AdvancedGateDecision::Block(reason) => {
                    warn!(
                        "🛑 Advanced policy blocked deployment for {} -> {}: {}",
                        system.hostname, store_path, reason
                    );
                    continue;
                }
            }

            // Preserve legacy CVE gate behavior for require_cve_check policies.
            let cve_policies = load_cve_policies(&self.pool).await;
            if !cve_policies.is_empty() {
                match check_cve_policies(
                    &self.pool,
                    latest_target_for_host.derivation_id,
                    &cve_policies,
                )
                .await
                {
                    Ok(gate) if !gate.deployment_allowed => {
                        warn!(
                            "🛑 Legacy CVE gate blocked deployment for {} -> {}: {}",
                            system.hostname,
                            store_path,
                            gate.block_reason.as_deref().unwrap_or("policy violation")
                        );
                        continue;
                    }
                    Ok(_) => {}
                    Err(err) => {
                        warn!(
                            "Legacy CVE gate evaluation failed for {} -> {}: {:#}; skipping deployment update",
                            system.hostname, store_path, err
                        );
                        continue;
                    }
                }
            }

            if let Err(e) =
                update_desired_target(&self.pool, &system.hostname, Some(store_path)).await
            {
                error!(
                    "Failed to set desired_target for {} -> {}: {:#}",
                    system.hostname, store_path, e
                );
            } else {
                info!(
                    "📋 Updated desired target for {}: {:?} -> {}",
                    system.hostname,
                    system.desired_target.as_deref(),
                    store_path
                );
                updated_count += 1;
            }
        }

        Ok(updated_count)
    }

    async fn evaluate_advanced_policy_gates(
        &self,
        system: &crate::models::systems::System,
        target: &crate::queries::derivations::HostLatestTarget,
        all_systems_for_flake: &[crate::models::systems::System],
        effective_policy_ids_by_system: &HashMap<uuid::Uuid, Vec<uuid::Uuid>>,
        policies_by_id: &HashMap<uuid::Uuid, DeploymentPolicyRecord>,
        failed_policy_loads: &HashSet<uuid::Uuid>,
    ) -> AdvancedGateDecision {
        let Some(policy_ids) = effective_policy_ids_by_system.get(&system.id) else {
            return AdvancedGateDecision::Allow;
        };

        for policy_id in policy_ids {
            if failed_policy_loads.contains(policy_id) {
                return AdvancedGateDecision::Block(format!(
                    "Failed to load enabled deployment policy {}",
                    policy_id
                ));
            }

            let Some(policy) = policies_by_id.get(policy_id) else {
                return AdvancedGateDecision::Block(format!(
                    "Deployment policy {} was not found",
                    policy_id
                ));
            };

            if !policy.enabled {
                continue;
            }

            match policy.policy_type.as_str() {
                "time_window" => {
                    let config =
                        match serde_json::from_value::<TimeWindowConfig>(policy.config.clone()) {
                            Ok(config) => config,
                            Err(err) => {
                                return AdvancedGateDecision::Block(format!(
                                    "Invalid time_window policy config for policy {}: {}",
                                    policy.id, err
                                ));
                            }
                        };
                    let decision =
                        map_time_window_decision(time_window_policy::check_time_window(&config));
                    if !matches!(decision, AdvancedGateDecision::Allow) {
                        return decision;
                    }
                }
                "require_approvals" => {
                    let config =
                        match serde_json::from_value::<ApprovalConfig>(policy.config.clone()) {
                            Ok(config) => config,
                            Err(err) => {
                                return AdvancedGateDecision::Block(format!(
                                    "Invalid require_approvals policy config for policy {}: {}",
                                    policy.id, err
                                ));
                            }
                        };
                    match approval_policy::check_approvals(
                        &self.pool,
                        DeploymentContext::Commit,
                        &target.commit_hash,
                        policy.id,
                        &config,
                    )
                    .await
                    {
                        Ok(result) => {
                            let decision = map_approval_decision(result);
                            if !matches!(decision, AdvancedGateDecision::Allow) {
                                return decision;
                            }
                        }
                        Err(err) => {
                            return AdvancedGateDecision::Block(format!(
                                "Approval policy evaluation failed: {}",
                                err
                            ));
                        }
                    }
                }
                "canary_rollout" => {
                    let config = match serde_json::from_value::<CanaryConfig>(policy.config.clone())
                    {
                        Ok(config) => config,
                        Err(err) => {
                            return AdvancedGateDecision::Block(format!(
                                "Invalid canary_rollout policy config for policy {}: {}",
                                policy.id, err
                            ));
                        }
                    };

                    let rollout_group: Vec<uuid::Uuid> = all_systems_for_flake
                        .iter()
                        .filter(|candidate| {
                            effective_policy_ids_by_system
                                .get(&candidate.id)
                                .map(|ids| ids.contains(policy_id))
                                .unwrap_or(false)
                        })
                        .map(|s| s.id)
                        .collect();

                    if rollout_group.is_empty() {
                        continue;
                    }

                    match canary_rollout::check_rollout(
                        &self.pool,
                        RolloutContext::Commit,
                        &target.commit_hash,
                        policy.id,
                        &config,
                        &rollout_group,
                    )
                    .await
                    {
                        Ok(result) => {
                            let decision = map_canary_decision_for_system(result, system.id);
                            if !matches!(decision, AdvancedGateDecision::Allow) {
                                return decision;
                            }
                        }
                        Err(err) => {
                            return AdvancedGateDecision::Block(format!(
                                "Canary rollout policy evaluation failed: {}",
                                err
                            ));
                        }
                    }
                }
                "cve_threshold" => {
                    let config =
                        match serde_json::from_value::<CveThresholdConfig>(policy.config.clone()) {
                            Ok(config) => config,
                            Err(err) => {
                                return AdvancedGateDecision::Block(format!(
                                    "Invalid cve_threshold policy config for policy {}: {}",
                                    policy.id, err
                                ));
                            }
                        };
                    match cve_threshold_policy::check_cve_thresholds(
                        &self.pool,
                        target.derivation_id,
                        &config,
                    )
                    .await
                    {
                        Ok(result) => {
                            let decision = map_cve_threshold_decision(result);
                            if !matches!(decision, AdvancedGateDecision::Allow) {
                                return decision;
                            }
                        }
                        Err(err) => {
                            return AdvancedGateDecision::Block(format!(
                                "CVE threshold policy evaluation failed: {}",
                                err
                            ));
                        }
                    }
                }
                _ => {}
            }
        }

        AdvancedGateDecision::Allow
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_window_block_maps_to_block() {
        let decision = map_time_window_decision(time_window_policy::TimeWindowResult {
            deployment_allowed: false,
            reason: Some("outside window".to_string()),
        });

        match decision {
            AdvancedGateDecision::Block(reason) => assert_eq!(reason, "outside window"),
            _ => panic!("expected block decision"),
        }
    }

    #[test]
    fn approval_incomplete_maps_to_pending() {
        let decision = map_approval_decision(approval_policy::ApprovalResult {
            deployment_allowed: false,
            approvals_received: 1,
            approvals_required: 2,
            reason: Some("Only 1/2 approvals received".to_string()),
        });

        match decision {
            AdvancedGateDecision::Pending(reason) => {
                assert!(reason.contains("1/2"));
            }
            _ => panic!("expected pending decision"),
        }
    }

    #[test]
    fn canary_unselected_system_maps_to_pending() {
        let selected = uuid::Uuid::new_v4();
        let unselected = uuid::Uuid::new_v4();

        let decision = map_canary_decision_for_system(
            canary_rollout::CanaryResult {
                deployment_allowed: true,
                systems_to_deploy: vec![selected],
                reason: Some("phase 1".to_string()),
                rollout_state: None,
            },
            unselected,
        );

        match decision {
            AdvancedGateDecision::Pending(reason) => assert!(reason.contains("phase")),
            _ => panic!("expected pending decision"),
        }
    }

    #[test]
    fn cve_threshold_violation_maps_to_block() {
        let decision = map_cve_threshold_decision(cve_threshold_policy::CveThresholdResult {
            deployment_allowed: false,
            violations: vec![],
            warnings: vec!["BLOCK: critical threshold exceeded".to_string()],
        });

        match decision {
            AdvancedGateDecision::Block(reason) => assert!(reason.contains("critical")),
            _ => panic!("expected block decision"),
        }
    }
}
