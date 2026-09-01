//! Coordinates automatic and agent-side system deployment.
//!
//! The server-side manager resolves effective policies for each auto-latest
//! system, evaluates legacy advanced gates, and delegates the final desired
//! target write to atomic composite authorization.

use crate::compliance::resolver::{
    AssignmentMode, EffectivePolicy, ResolutionOutcome,
    resolve_systems_effective_policies_for_deployment_batch,
};
use crate::config::CrystalForgeConfig;
use crate::models::deployment_policies::{
    ApprovalConfig, CanaryConfig, CveThresholdConfig, DeploymentPolicyRecord, TimeWindowConfig,
};
use crate::models::systems::DeploymentPolicy;
use crate::queries::deployment::get_systems_with_auto_latest_policy;
use crate::queries::deployment_policies::get_deployment_policies_by_versions;
use crate::queries::derivations::get_latest_deployable_targets_for_flake_hosts;
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
/// Manages automatic target selection for systems with `auto_latest` policy.
///
/// Manual and pinned targets remain under administrator control.
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
    /// Creates a deployment policy manager for one server configuration and pool.
    pub fn new(config: CrystalForgeConfig, pool: PgPool) -> Self {
        Self { config, pool }
    }

    /// Runs the automatic deployment policy management loop.
    ///
    /// Each polling pass logs and contains its own failure so later passes can
    /// continue. Manual and pinned systems are not changed.
    ///
    /// # Errors
    ///
    /// The loop currently runs until cancellation and does not return a
    /// recoverable error during normal operation.
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
        let mut effective_policies_by_system: HashMap<uuid::Uuid, Vec<EffectivePolicy>> =
            HashMap::new();
        let mut all_policy_version_ids: HashSet<uuid::Uuid> = HashSet::new();
        let mut failed_policy_lookup_systems: HashSet<uuid::Uuid> = HashSet::new();
        let system_ids = systems.iter().map(|system| system.id).collect::<Vec<_>>();
        let mut resolved_by_system =
            resolve_systems_effective_policies_for_deployment_batch(&self.pool, &system_ids)
                .await
                .context("Failed to batch-resolve effective deployment policies")?;
        for system in &systems {
            let policy_ids = match resolved_by_system.remove(&system.id) {
                Some(ResolutionOutcome::Resolved(set)) => set
                    .policies
                    .into_iter()
                    // Report-only policies are evaluated by compliance paths but
                    // must never block or alter deployment configuration.
                    .filter(|policy| matches!(policy.effective_mode, AssignmentMode::Enforce))
                    .collect::<Vec<EffectivePolicy>>(),
                Some(ResolutionOutcome::Conflict(conflicts)) => {
                    warn!(
                        "Effective policy conflict for {} ({}): {}; skipping deployment update",
                        system.hostname,
                        system.id,
                        conflicts
                            .iter()
                            .map(|conflict| format!("{}: {}", conflict.code, conflict.message))
                            .collect::<Vec<_>>()
                            .join("; ")
                    );
                    failed_policy_lookup_systems.insert(system.id);
                    continue;
                }
                None => {
                    warn!(
                        "Effective deployment policy batch omitted {} ({}); skipping deployment update",
                        system.hostname, system.id
                    );
                    failed_policy_lookup_systems.insert(system.id);
                    continue;
                }
            };
            for policy in &policy_ids {
                // Composite policy records are decoded and freshness-checked by
                // the final serializable authorization transaction. Do not load
                // them a second time for the legacy advanced-gate pass.
                if policy.policy_type != "composite" {
                    all_policy_version_ids.insert(policy.policy_version_id);
                }
            }
            effective_policies_by_system.insert(system.id, policy_ids);
        }

        let all_policy_version_ids = all_policy_version_ids.into_iter().collect::<Vec<_>>();
        let policies_by_id =
            get_deployment_policies_by_versions(&self.pool, &all_policy_version_ids)
                .await
                .context("Failed to load effective deployment policy versions")?;
        let failed_policy_loads = all_policy_version_ids
            .into_iter()
            .filter(|version_id| !policies_by_id.contains_key(version_id))
            .collect::<HashSet<_>>();
        let cve_policies = load_cve_policies(&self.pool).await;

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
                    &effective_policies_by_system,
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

            match crate::services::composite_enforcement::authorize_and_set_system_target(
                &self.pool,
                system.id,
                store_path,
                "auto_desired_target",
            )
            .await
            {
                Ok(authorization) if authorization.allowed() => {
                    info!(
                        "📋 Updated desired target for {}: {:?} -> {}",
                        system.hostname,
                        system.desired_target.as_deref(),
                        store_path
                    );
                    updated_count += 1;
                }
                Ok(authorization) => warn!(
                    "🛑 Composite policy blocked atomic target update for {} -> {}: {}",
                    system.hostname, store_path, authorization.detail
                ),
                Err(e) => error!(
                    "Failed atomic composite authorization/target update for {} -> {}: {:#}",
                    system.hostname, store_path, e
                ),
            }
        }

        Ok(updated_count)
    }

    async fn evaluate_advanced_policy_gates(
        &self,
        system: &crate::models::systems::System,
        target: &crate::queries::derivations::HostLatestTarget,
        all_systems_for_flake: &[crate::models::systems::System],
        effective_policies_by_system: &HashMap<uuid::Uuid, Vec<EffectivePolicy>>,
        policies_by_id: &HashMap<uuid::Uuid, DeploymentPolicyRecord>,
        failed_policy_loads: &HashSet<uuid::Uuid>,
    ) -> AdvancedGateDecision {
        let Some(effective_policies) = effective_policies_by_system.get(&system.id) else {
            return AdvancedGateDecision::Allow;
        };

        for effective_policy in effective_policies {
            if effective_policy.policy_type == "composite" {
                continue;
            }
            let policy_id = &effective_policy.policy_version_id;
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

            // The resolver has already applied assignment value overrides.  The
            // legacy record is used only for policy type/metadata and as a
            // defensive fallback for un-overridden policies.
            let effective_config = if effective_policy.effective_config.is_null() {
                policy.config.clone()
            } else {
                effective_policy.effective_config.clone()
            };

            match policy.policy_type.as_str() {
                "time_window" => {
                    let config = match serde_json::from_value::<TimeWindowConfig>(
                        effective_config.clone(),
                    ) {
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
                        match serde_json::from_value::<ApprovalConfig>(effective_config.clone()) {
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
                    let config =
                        match serde_json::from_value::<CanaryConfig>(effective_config.clone()) {
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
                            effective_policies_by_system
                                .get(&candidate.id)
                                .map(|policies| {
                                    policies
                                        .iter()
                                        .any(|candidate| candidate.policy_version_id == *policy_id)
                                })
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
                    let config = match serde_json::from_value::<CveThresholdConfig>(
                        effective_config.clone(),
                    ) {
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
                // Composite policies are authorized once for the complete set
                // in the atomic desired-target update below.
                "composite" => {}
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

/// Spawns the deployment policy manager as a background task.
///
/// The returned task logs a terminal manager error instead of propagating it
/// through the join result.
///
/// # Errors
///
/// This function currently performs no fallible setup before spawning.
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

    #[test]
    fn auto_latest_uses_batch_policy_and_global_cve_queries() {
        let source = include_str!("mod.rs");
        let production_source = source
            .split("#[cfg(test)]")
            .next()
            .expect("deployment module has production source");
        assert_eq!(
            production_source
                .matches("resolve_systems_effective_policies_for_deployment_batch(&self.pool")
                .count(),
            1
        );
        assert_eq!(
            production_source
                .matches("load_cve_policies(&self.pool).await")
                .count(),
            1
        );
    }
}
