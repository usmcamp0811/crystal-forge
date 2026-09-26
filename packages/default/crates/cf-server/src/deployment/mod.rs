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
    ApprovalConfig, CanaryConfig, CveCheckConfig, CveThresholdConfig, DeploymentPolicyRecord,
    TimeWindowConfig,
};
use crate::models::systems::DeploymentPolicy;
use crate::queries::deployment::get_systems_with_auto_latest_policy;
use crate::queries::deployment_policies::get_deployment_policies_by_versions;
use crate::queries::derivations::get_latest_deployable_targets_for_flake_hosts;
use crate::services::approval_policy::{self, DeploymentContext};
use crate::services::canary_rollout::{self, RolloutContext};
use crate::services::cve_policy_gate::{CveGateResult, check_cve_policies};
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

/// Maps one `require_cve_check` evaluation onto an advanced-gate decision.
///
/// The caller must invoke [`check_cve_policies`] with exactly one config drawn
/// from the system's resolved effective policy set. A non-blocking violation
/// (strict = false, or `when_no_scan = skip`) surfaces as a warning rather than
/// silently passing, so operators can see it without it affecting delivery.
fn map_cve_check_decision(result: CveGateResult) -> AdvancedGateDecision {
    if !result.deployment_allowed {
        let reason = result
            .block_reason
            .unwrap_or_else(|| "require_cve_check policy blocked deployment".to_string());
        return AdvancedGateDecision::Block(reason);
    }
    let warnings: Vec<String> = result
        .outcomes
        .iter()
        .filter(|outcome| !outcome.passed)
        .filter_map(|outcome| outcome.reason.clone())
        .collect();
    if warnings.is_empty() {
        AdvancedGateDecision::Allow
    } else {
        AdvancedGateDecision::Warn(warnings.join("; "))
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

        // Collect effective configuration names for this flake.
        let config_names: Vec<String> = systems
            .iter()
            .map(|s| s.configuration_name().to_string())
            .collect();

        // Fetch the newest deployable artifact per configuration across commits.
        let per_host =
            get_latest_deployable_targets_for_flake_hosts(&self.pool, flake_id, &config_names)
                .await?;
        let latest_by_host: HashMap<_, _> = per_host
            .into_iter()
            .map(|h| (h.hostname.clone(), h))
            .collect();

        // Read existing immutable delivery identity once for the whole flake.
        // The final serializable transaction rechecks this hint under locks.
        let pending_identity: HashMap<
            uuid::Uuid,
            (
                String,
                Option<i32>,
                Option<i32>,
                Option<uuid::Uuid>,
                Option<uuid::Uuid>,
                i32,
                i64,
            ),
        > = sqlx::query_as(
            r#"SELECT input.system_id, pending.target_store_path,
                       pending.requested_commit_id, pending.requested_derivation_id,
                       pending.evaluation_snapshot_id, selected.id, derivation.commit_id,
                       live_pending.row_count
               FROM UNNEST($1::uuid[], $2::integer[]) AS input(system_id, derivation_id)
               JOIN systems system ON system.id = input.system_id
               JOIN derivations derivation ON derivation.id = input.derivation_id
               LEFT JOIN LATERAL (
                   SELECT snapshot.id
                   FROM evaluation_snapshot_selections selection
                   JOIN evaluation_snapshots snapshot ON snapshot.id = selection.current_snapshot_id
                   WHERE selection.commit_id = derivation.commit_id
                     AND selection.configuration_name = derivation.derivation_name
                     AND snapshot.lifecycle = 'available' AND snapshot.integrity_version = 1
                   LIMIT 1
                ) selected ON TRUE
                CROSS JOIN LATERAL (
                    SELECT COUNT(*) AS row_count
                    FROM (
                        SELECT 1 FROM pending_system_deployments
                        WHERE system_id = input.system_id AND status = 'pending'
                          AND expires_at > NOW()
                        LIMIT 2
                    ) live
                ) live_pending
                JOIN LATERAL (
                   SELECT target_store_path, requested_commit_id, requested_derivation_id,
                          evaluation_snapshot_id
                   FROM pending_system_deployments
                   WHERE system_id = input.system_id AND status = 'pending'
                     AND expires_at > NOW() AND target_store_path = system.desired_target
                   ORDER BY issued_at DESC, id DESC LIMIT 1
               ) pending ON TRUE"#,
        )
        .bind(&systems.iter().map(|system| system.id).collect::<Vec<_>>())
        .bind(
            &systems
                .iter()
                .map(|system| {
                    latest_by_host
                        .get(system.configuration_name())
                        .map(|target| target.derivation_id)
                        .unwrap_or(-1)
                })
                .collect::<Vec<_>>(),
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(
            |(id, path, commit, derivation, artifact, selected, selected_commit, row_count)| {
                (
                    id,
                    (
                        path,
                        commit,
                        derivation,
                        artifact,
                        selected,
                        selected_commit,
                        row_count,
                    ),
                )
            },
        )
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
                    "No deployable NixOS derivation exists for host {} (config {}) across flake {}",
                    system.hostname,
                    system.configuration_name(),
                    flake_id
                );
                continue;
            };

            if latest_target_for_host.newer_raw_commit_exists {
                debug!(
                    "Newer raw flake commits are not yet deployable for host {} (config {}); selected {}",
                    system.hostname,
                    system.configuration_name(),
                    latest_target_for_host.commit_hash
                );
            }

            let Some(store_path) = latest_target_for_host.store_path.as_ref() else {
                debug!(
                    "No store path for host {}; skipping desired_target update",
                    system.hostname
                );
                continue;
            };

            // A single claimable row is required; other live rows (including
            // another path) must pass runtime gates and final locked cleanup.
            if system.desired_target.as_deref() == Some(store_path.as_str())
                && pending_identity.get(&system.id).is_some_and(
                    |(path, commit, derivation, artifact, selected, selected_commit, row_count)| {
                        *row_count == 1
                            && path == store_path
                            && *derivation == Some(latest_target_for_host.derivation_id)
                            && *artifact == *selected
                            && *commit == Some(*selected_commit)
                    },
                )
            {
                debug!(
                    "System {} already has newest deployable target",
                    system.hostname
                );
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

            match crate::services::composite_enforcement::authorize_and_set_system_target_for_derivation(
                &self.pool,
                system.id,
                store_path,
                "auto_desired_target",
                latest_target_for_host.derivation_id,
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
                "require_cve_check" => {
                    let config =
                        match serde_json::from_value::<CveCheckConfig>(effective_config.clone()) {
                            Ok(config) => config,
                            Err(err) => {
                                return AdvancedGateDecision::Block(format!(
                                    "Invalid require_cve_check policy config for policy {}: {}",
                                    policy.id, err
                                ));
                            }
                        };
                    // SECURITY: `effective_policies_by_system` already dropped
                    // every report_only policy before this loop ran (see the
                    // filter above), so a require_cve_check reaching this arm
                    // is always an effective Enforce assignment. Evaluate only
                    // this system's resolved policy, never the fleet-global
                    // enabled set, and scope it to the selected artifact.
                    let policies = vec![
                        crate::models::deployment_policies::DeploymentPolicy::RequireCveCheck {
                            config,
                        },
                    ];
                    match check_cve_policies(&self.pool, target.derivation_id, &policies).await {
                        Ok(gate) => {
                            let decision = map_cve_check_decision(gate);
                            if let AdvancedGateDecision::Block(ref reason) = decision {
                                warn!(
                                    hostname = %system.hostname,
                                    policy_lineage_id = %policy.id,
                                    policy_version_id = %policy_id,
                                    effective_mode = ?effective_policy.effective_mode,
                                    derivation_id = target.derivation_id,
                                    reason = %reason,
                                    "🛑 require_cve_check (enforce) blocked auto_latest deployment"
                                );
                            }
                            if !matches!(decision, AdvancedGateDecision::Allow) {
                                return decision;
                            }
                        }
                        Err(err) => {
                            return AdvancedGateDecision::Block(format!(
                                "require_cve_check policy evaluation failed: {}",
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
    use uuid::Uuid;

    // ── Shared fixtures for require_cve_check regressions ───────────────────
    //
    // TASK-437 correction: require_cve_check must be evaluated only through a
    // system's resolved effective policy set, never through a fleet-global
    // `enabled = true` query. Only an effective Enforce assignment may block
    // `auto_latest` delivery; report_only always advances while still
    // producing a FAIL compliance outcome elsewhere in the compliance
    // pipeline. These helpers build one deployable artifact and publish a
    // `require_cve_check` policy through a compliance bundle, matching the
    // real sledge shape from the owner's report.

    /// Creates one flake, one commit, and one eligible cache-published NixOS
    /// derivation. Returns `(flake_id, derivation_id, store_path,
    /// configuration_name)`. Systems that share `configuration_name` all
    /// resolve to this same artifact.
    async fn cve_gate_artifact(pool: &PgPool, suffix: &str) -> (i32, i32, String, String) {
        let config_name = format!("cve-gate-config-{suffix}");
        let flake_id: i32 =
            sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id")
                .bind(format!("cve-gate-{suffix}"))
                .bind(format!("https://example.invalid/{suffix}"))
                .fetch_one(pool)
                .await
                .unwrap();
        let commit_id: i32 = sqlx::query_scalar(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) \
             VALUES ($1, $2, '2026-01-01') RETURNING id",
        )
        .bind(flake_id)
        .bind(format!("commit-{suffix}"))
        .fetch_one(pool)
        .await
        .unwrap();
        let store_path = format!("/nix/store/{suffix}-nixos-system-cve-gate");
        let derivation_id: i32 = sqlx::query_scalar(
            "INSERT INTO derivations (commit_id, derivation_type, derivation_name, status_id, \
             attempt_count, store_path, cf_agent_enabled, policy_requirements_met) \
             VALUES ($1, 'nixos', $2, 11, 0, $3, true, true) RETURNING id",
        )
        .bind(commit_id)
        .bind(&config_name)
        .bind(&store_path)
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO cache_push_jobs (derivation_id, status, store_path) \
             VALUES ($1, 'completed', $2)",
        )
        .bind(derivation_id)
        .bind(&store_path)
        .execute(pool)
        .await
        .unwrap();
        (flake_id, derivation_id, store_path, config_name)
    }

    /// Inserts one completed schema-1-shaped CVE scan for a derivation.
    async fn cve_gate_scan(pool: &PgPool, derivation_id: i32, critical: i32, high: i32) {
        sqlx::query(
            "INSERT INTO cve_scans (derivation_id, scanner_name, status, critical_count, \
             high_count, medium_count, low_count, completed_at) \
             VALUES ($1, 'vulnix', 'completed', $2, $3, 0, 0, NOW())",
        )
        .bind(derivation_id)
        .bind(critical)
        .bind(high)
        .execute(pool)
        .await
        .unwrap();
    }

    /// Creates one `auto_latest` system, optionally inside `environment_id`.
    async fn cve_gate_system(
        pool: &PgPool,
        hostname: &str,
        flake_id: i32,
        configuration_name: &str,
        environment_id: Option<Uuid>,
    ) -> Uuid {
        use base64::{Engine, engine::general_purpose::STANDARD};
        use ed25519_dalek::SigningKey;
        sqlx::query_scalar(
            "INSERT INTO systems (hostname, public_key, derivation, flake_id, \
             system_configuration_name, deployment_policy, environment_id) \
             VALUES ($1, $2, 'test-derivation', $3, $4, 'auto_latest', $5) RETURNING id",
        )
        .bind(hostname)
        .bind(
            STANDARD.encode(
                SigningKey::generate(&mut rand::thread_rng())
                    .verifying_key()
                    .to_bytes(),
            ),
        )
        .bind(flake_id)
        .bind(configuration_name)
        .bind(environment_id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// Publishes a `require_cve_check` deployment policy version: accepted,
    /// trusted, and the lineage's published pointer. Returns `(policy_id,
    /// policy_version_id)`.
    async fn publish_cve_check_policy(
        pool: &PgPool,
        suffix: &str,
        config: &CveCheckConfig,
    ) -> (Uuid, Uuid) {
        let policy_id: Uuid = sqlx::query_scalar(
            "INSERT INTO deployment_policies (name, policy_type, config, enabled) \
             VALUES ($1, 'require_cve_check', $2, true) RETURNING id",
        )
        .bind(format!("test rollout {suffix}"))
        .bind(serde_json::to_value(config).unwrap())
        .fetch_one(pool)
        .await
        .unwrap();
        let version_id: Uuid = sqlx::query_scalar(
            "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
        )
        .bind(policy_id)
        .fetch_one(pool)
        .await
        .unwrap();
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("UPDATE deployment_policies SET current_draft_version_id = NULL WHERE id = $1")
            .bind(policy_id)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE deployment_policy_versions SET publication_state = 'accepted', \
             trust_state = 'trusted' WHERE id = $1",
        )
        .bind(version_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE deployment_policies SET current_published_version_id = $1 WHERE id = $2",
        )
        .bind(version_id)
        .bind(policy_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
        (policy_id, version_id)
    }

    /// Publishes a bundle containing exactly one policy version: accepted and
    /// trusted. Returns `(bundle_id, bundle_version_id)`.
    async fn publish_cve_gate_bundle(
        pool: &PgPool,
        suffix: &str,
        policy_version_id: Uuid,
    ) -> (Uuid, Uuid) {
        let bundle_id: Uuid = sqlx::query_scalar(
            "INSERT INTO compliance_bundles (name, framework, version, layer) \
             VALUES ($1, 'test', '1.0', 'fleet') RETURNING id",
        )
        .bind(format!("cve-gate-bundle-{suffix}"))
        .fetch_one(pool)
        .await
        .unwrap();
        let bundle_version_id: Uuid = sqlx::query_scalar(
            "SELECT current_draft_version_id FROM compliance_bundles WHERE id = $1",
        )
        .bind(bundle_id)
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO compliance_bundle_version_policies \
             (bundle_version_id, policy_version_id, policy_order) VALUES ($1, $2, 0)",
        )
        .bind(bundle_version_id)
        .bind(policy_version_id)
        .execute(pool)
        .await
        .unwrap();
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("UPDATE compliance_bundles SET current_draft_version_id = NULL WHERE id = $1")
            .bind(bundle_id)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE compliance_bundle_versions SET publication_state = 'accepted', \
             trust_state = 'trusted', semantic_digest = $1 WHERE id = $2",
        )
        .bind(format!("cve-gate-{bundle_version_id}"))
        .bind(bundle_version_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2",
        )
        .bind(bundle_version_id)
        .bind(bundle_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
        (bundle_id, bundle_version_id)
    }

    /// Assigns a published bundle to one system (`system_id = Some(..)`) or
    /// one environment (`environment_id = Some(..)`) with `mode` (`"enforce"`
    /// or `"report_only"`). Returns `(assignment_id, assignment_version_id)`.
    async fn assign_cve_gate_bundle(
        pool: &PgPool,
        bundle_id: Uuid,
        bundle_version_id: Uuid,
        environment_id: Option<Uuid>,
        system_id: Option<Uuid>,
        mode: &str,
    ) -> (Uuid, Uuid) {
        let scope_type = if system_id.is_some() {
            "system"
        } else {
            "environment"
        };
        let assignment_id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO compliance_bundle_assignments
                 (bundle_id, bundle_version_id, scope_type, environment_id, system_id,
                  active, enforcement_mode, assignment_overlay_digest)
               VALUES ($1, $2, $3, $4, $5, true, $6, 'cve-gate-overlay') RETURNING id"#,
        )
        .bind(bundle_id)
        .bind(bundle_version_id)
        .bind(scope_type)
        .bind(environment_id)
        .bind(system_id)
        .bind(mode)
        .fetch_one(pool)
        .await
        .unwrap();
        let assignment_version_id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO compliance_bundle_assignment_versions
                 (assignment_id, version_number, bundle_version_id, enforcement_mode,
                  assignment_overlay_digest)
               VALUES ($1, 1, $2, $3, 'cve-gate-overlay') RETURNING id"#,
        )
        .bind(assignment_id)
        .bind(bundle_version_id)
        .bind(mode)
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE compliance_bundle_assignments SET current_version_id = $1 WHERE id = $2",
        )
        .bind(assignment_version_id)
        .bind(assignment_id)
        .execute(pool)
        .await
        .unwrap();
        (assignment_id, assignment_version_id)
    }

    /// Overrides one top-level `require_cve_check` config field within an
    /// assignment version. The resolver only allows a small, explicit set of
    /// fields for this policy type and requires the path to already exist.
    async fn override_cve_gate_field(
        pool: &PgPool,
        assignment_id: Uuid,
        assignment_version_id: Uuid,
        policy_version_id: Uuid,
        value_path: &str,
        value: serde_json::Value,
    ) {
        sqlx::query(
            "INSERT INTO compliance_assignment_value_overrides \
             (assignment_id, assignment_version_id, policy_version_id, value_path, value) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(assignment_id)
        .bind(assignment_version_id)
        .bind(policy_version_id)
        .bind(value_path)
        .bind(value)
        .execute(pool)
        .await
        .unwrap();
    }

    /// Reads back a system's `desired_target`/`desired_target_set_at` and any
    /// live pending deployment target.
    async fn cve_gate_system_state(
        pool: &PgPool,
        system_id: Uuid,
    ) -> (
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<String>,
    ) {
        sqlx::query_as(
            "SELECT desired_target, desired_target_set_at, \
             (SELECT target_store_path FROM pending_system_deployments \
              WHERE system_id = $1 AND status = 'pending') FROM systems WHERE id = $1",
        )
        .bind(system_id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    // A: A globally `enabled = true` require_cve_check policy that is not
    // assigned to any bundle, environment, or system must never gate this
    // system's deployment. `enabled` names a usable lineage, not fleet-global
    // applicability.
    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires a verified disposable DATABASE_URL"]
    async fn cve_gate_enabled_but_unassigned_policy_does_not_block(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let (flake_id, derivation_id, store_path, config_name) =
            cve_gate_artifact(&pool, &suffix).await;
        cve_gate_scan(&pool, derivation_id, 5, 0).await;
        publish_cve_check_policy(
            &pool,
            &suffix,
            &CveCheckConfig {
                max_critical: 0,
                strict: true,
                ..Default::default()
            },
        )
        .await;
        let system_id = cve_gate_system(
            &pool,
            &format!("host-{suffix}"),
            flake_id,
            &config_name,
            None,
        )
        .await;

        let manager = DeploymentPolicyManager::new(CrystalForgeConfig::default(), pool.clone());
        let stats = manager.update_auto_latest_policies().await.unwrap();
        assert_eq!((stats.systems_checked, stats.systems_updated), (1, 1));

        let (desired, set_at, _pending) = cve_gate_system_state(&pool, system_id).await;
        assert_eq!(desired.as_deref(), Some(store_path.as_str()));
        assert!(set_at.is_some());
    }

    // B: An effective report_only require_cve_check with a failing scan
    // (248 > 50) must not block; desired_target and a pending
    // auto_desired_target row must both advance.
    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires a verified disposable DATABASE_URL"]
    async fn cve_gate_report_only_fail_does_not_block(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let (flake_id, derivation_id, store_path, config_name) =
            cve_gate_artifact(&pool, &suffix).await;
        cve_gate_scan(&pool, derivation_id, 248, 1274).await;
        let (_policy_id, version_id) = publish_cve_check_policy(
            &pool,
            &suffix,
            &CveCheckConfig {
                max_critical: 50,
                strict: true,
                ..Default::default()
            },
        )
        .await;
        let (bundle_id, bundle_version_id) =
            publish_cve_gate_bundle(&pool, &suffix, version_id).await;
        let system_id = cve_gate_system(
            &pool,
            &format!("host-{suffix}"),
            flake_id,
            &config_name,
            None,
        )
        .await;
        assign_cve_gate_bundle(
            &pool,
            bundle_id,
            bundle_version_id,
            None,
            Some(system_id),
            "report_only",
        )
        .await;

        let manager = DeploymentPolicyManager::new(CrystalForgeConfig::default(), pool.clone());
        let stats = manager.update_auto_latest_policies().await.unwrap();
        assert_eq!((stats.systems_checked, stats.systems_updated), (1, 1));

        let (desired, set_at, pending) = cve_gate_system_state(&pool, system_id).await;
        assert_eq!(desired.as_deref(), Some(store_path.as_str()));
        assert!(set_at.is_some());
        assert_eq!(pending.as_deref(), Some(store_path.as_str()));
    }

    // C: The identical policy and scan under an effective enforce assignment
    // must block; desired_target must not advance.
    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires a verified disposable DATABASE_URL"]
    async fn cve_gate_enforce_fail_blocks(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let (flake_id, derivation_id, _store_path, config_name) =
            cve_gate_artifact(&pool, &suffix).await;
        cve_gate_scan(&pool, derivation_id, 248, 1274).await;
        let (_policy_id, version_id) = publish_cve_check_policy(
            &pool,
            &suffix,
            &CveCheckConfig {
                max_critical: 50,
                strict: true,
                ..Default::default()
            },
        )
        .await;
        let (bundle_id, bundle_version_id) =
            publish_cve_gate_bundle(&pool, &suffix, version_id).await;
        let system_id = cve_gate_system(
            &pool,
            &format!("host-{suffix}"),
            flake_id,
            &config_name,
            None,
        )
        .await;
        assign_cve_gate_bundle(
            &pool,
            bundle_id,
            bundle_version_id,
            None,
            Some(system_id),
            "enforce",
        )
        .await;

        let manager = DeploymentPolicyManager::new(CrystalForgeConfig::default(), pool.clone());
        let stats = manager.update_auto_latest_policies().await.unwrap();
        assert_eq!((stats.systems_checked, stats.systems_updated), (1, 0));

        let (desired, set_at, pending) = cve_gate_system_state(&pool, system_id).await;
        assert_eq!(desired, None);
        assert_eq!(set_at, None);
        assert_eq!(pending, None);
    }

    // D: The same policy lineage/version assigned to two environments with
    // different modes must produce different outcomes for their systems.
    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires a verified disposable DATABASE_URL"]
    async fn cve_gate_same_lineage_different_environment_modes(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let (flake_id, derivation_id, store_path, config_name) =
            cve_gate_artifact(&pool, &suffix).await;
        cve_gate_scan(&pool, derivation_id, 248, 1274).await;
        let (_policy_id, version_id) = publish_cve_check_policy(
            &pool,
            &suffix,
            &CveCheckConfig {
                max_critical: 50,
                strict: true,
                ..Default::default()
            },
        )
        .await;
        let (bundle_id, bundle_version_id) =
            publish_cve_gate_bundle(&pool, &suffix, version_id).await;

        let dev_env: Uuid =
            sqlx::query_scalar("INSERT INTO environments (name) VALUES ($1) RETURNING id")
                .bind(format!("dev-{suffix}"))
                .fetch_one(&pool)
                .await
                .unwrap();
        let prod_env: Uuid =
            sqlx::query_scalar("INSERT INTO environments (name) VALUES ($1) RETURNING id")
                .bind(format!("prod-{suffix}"))
                .fetch_one(&pool)
                .await
                .unwrap();
        assign_cve_gate_bundle(
            &pool,
            bundle_id,
            bundle_version_id,
            Some(dev_env),
            None,
            "report_only",
        )
        .await;
        assign_cve_gate_bundle(
            &pool,
            bundle_id,
            bundle_version_id,
            Some(prod_env),
            None,
            "enforce",
        )
        .await;

        let dev_system = cve_gate_system(
            &pool,
            &format!("dev-host-{suffix}"),
            flake_id,
            &config_name,
            Some(dev_env),
        )
        .await;
        let prod_system = cve_gate_system(
            &pool,
            &format!("prod-host-{suffix}"),
            flake_id,
            &config_name,
            Some(prod_env),
        )
        .await;

        let manager = DeploymentPolicyManager::new(CrystalForgeConfig::default(), pool.clone());
        let stats = manager.update_auto_latest_policies().await.unwrap();
        assert_eq!((stats.systems_checked, stats.systems_updated), (2, 1));

        let (dev_desired, ..) = cve_gate_system_state(&pool, dev_system).await;
        assert_eq!(
            dev_desired.as_deref(),
            Some(store_path.as_str()),
            "dev (report_only) must deploy"
        );
        let (prod_desired, prod_set_at, prod_pending) =
            cve_gate_system_state(&pool, prod_system).await;
        assert_eq!(prod_desired, None, "prod (enforce) must block");
        assert_eq!(prod_set_at, None);
        assert_eq!(prod_pending, None);
    }

    // E: An assignment-level effective_config override changes the threshold
    // actually enforced, independent of the published policy's own config.
    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires a verified disposable DATABASE_URL"]
    async fn cve_gate_per_environment_effective_config_override(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let (flake_id, derivation_id, store_path, config_name) =
            cve_gate_artifact(&pool, &suffix).await;
        cve_gate_scan(&pool, derivation_id, 248, 0).await;
        let (_policy_id, version_id) = publish_cve_check_policy(
            &pool,
            &suffix,
            &CveCheckConfig {
                max_critical: 50,
                strict: true,
                ..Default::default()
            },
        )
        .await;
        let (bundle_id, bundle_version_id) =
            publish_cve_gate_bundle(&pool, &suffix, version_id).await;

        let env_a: Uuid =
            sqlx::query_scalar("INSERT INTO environments (name) VALUES ($1) RETURNING id")
                .bind(format!("env-a-{suffix}"))
                .fetch_one(&pool)
                .await
                .unwrap();
        let env_b: Uuid =
            sqlx::query_scalar("INSERT INTO environments (name) VALUES ($1) RETURNING id")
                .bind(format!("env-b-{suffix}"))
                .fetch_one(&pool)
                .await
                .unwrap();
        let (assignment_a, assignment_version_a) = assign_cve_gate_bundle(
            &pool,
            bundle_id,
            bundle_version_id,
            Some(env_a),
            None,
            "enforce",
        )
        .await;
        override_cve_gate_field(
            &pool,
            assignment_a,
            assignment_version_a,
            version_id,
            "max_critical",
            serde_json::json!(300),
        )
        .await;
        assign_cve_gate_bundle(
            &pool,
            bundle_id,
            bundle_version_id,
            Some(env_b),
            None,
            "enforce",
        )
        .await;

        let system_a = cve_gate_system(
            &pool,
            &format!("host-a-{suffix}"),
            flake_id,
            &config_name,
            Some(env_a),
        )
        .await;
        let system_b = cve_gate_system(
            &pool,
            &format!("host-b-{suffix}"),
            flake_id,
            &config_name,
            Some(env_b),
        )
        .await;

        let manager = DeploymentPolicyManager::new(CrystalForgeConfig::default(), pool.clone());
        let stats = manager.update_auto_latest_policies().await.unwrap();
        assert_eq!((stats.systems_checked, stats.systems_updated), (2, 1));

        let (a_desired, ..) = cve_gate_system_state(&pool, system_a).await;
        assert_eq!(
            a_desired.as_deref(),
            Some(store_path.as_str()),
            "override max_critical=300 must allow"
        );
        let (b_desired, ..) = cve_gate_system_state(&pool, system_b).await;
        assert_eq!(
            b_desired, None,
            "unmodified max_critical=50 must still block"
        );
    }

    // F: when_no_scan=block only blocks under an effective enforce
    // assignment; report_only never blocks regardless of outcome.
    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires a verified disposable DATABASE_URL"]
    async fn cve_gate_when_no_scan_block_only_blocks_under_enforce(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let (flake_id, _derivation_id, store_path, config_name) =
            cve_gate_artifact(&pool, &suffix).await;
        // Deliberately no cve_scans row for this derivation.
        let (_policy_id, version_id) = publish_cve_check_policy(
            &pool,
            &suffix,
            &CveCheckConfig {
                strict: true,
                ..Default::default()
            },
        )
        .await;
        let (bundle_id, bundle_version_id) =
            publish_cve_gate_bundle(&pool, &suffix, version_id).await;

        let report_system = cve_gate_system(
            &pool,
            &format!("report-{suffix}"),
            flake_id,
            &config_name,
            None,
        )
        .await;
        assign_cve_gate_bundle(
            &pool,
            bundle_id,
            bundle_version_id,
            None,
            Some(report_system),
            "report_only",
        )
        .await;

        let manager = DeploymentPolicyManager::new(CrystalForgeConfig::default(), pool.clone());
        let stats = manager.update_auto_latest_policies().await.unwrap();
        assert_eq!((stats.systems_checked, stats.systems_updated), (1, 1));
        let (desired, ..) = cve_gate_system_state(&pool, report_system).await;
        assert_eq!(
            desired.as_deref(),
            Some(store_path.as_str()),
            "report_only when_no_scan=block must not block"
        );

        let enforce_system = cve_gate_system(
            &pool,
            &format!("enforce-{suffix}"),
            flake_id,
            &config_name,
            None,
        )
        .await;
        assign_cve_gate_bundle(
            &pool,
            bundle_id,
            bundle_version_id,
            None,
            Some(enforce_system),
            "enforce",
        )
        .await;
        let stats = manager.update_auto_latest_policies().await.unwrap();
        assert_eq!(stats.systems_checked, 2);
        let (enforce_desired, ..) = cve_gate_system_state(&pool, enforce_system).await;
        assert_eq!(
            enforce_desired, None,
            "enforce when_no_scan=block must block"
        );
    }

    // G: A non-strict violation under an effective enforce assignment warns
    // but does not block; desired_target still advances.
    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires a verified disposable DATABASE_URL"]
    async fn cve_gate_enforce_non_strict_warns_and_advances(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let (flake_id, derivation_id, store_path, config_name) =
            cve_gate_artifact(&pool, &suffix).await;
        cve_gate_scan(&pool, derivation_id, 248, 0).await;
        let (_policy_id, version_id) = publish_cve_check_policy(
            &pool,
            &suffix,
            &CveCheckConfig {
                max_critical: 50,
                strict: false,
                ..Default::default()
            },
        )
        .await;
        let (bundle_id, bundle_version_id) =
            publish_cve_gate_bundle(&pool, &suffix, version_id).await;
        let system_id = cve_gate_system(
            &pool,
            &format!("host-{suffix}"),
            flake_id,
            &config_name,
            None,
        )
        .await;
        assign_cve_gate_bundle(
            &pool,
            bundle_id,
            bundle_version_id,
            None,
            Some(system_id),
            "enforce",
        )
        .await;

        let manager = DeploymentPolicyManager::new(CrystalForgeConfig::default(), pool.clone());
        let stats = manager.update_auto_latest_policies().await.unwrap();
        assert_eq!((stats.systems_checked, stats.systems_updated), (1, 1));
        let (desired, ..) = cve_gate_system_state(&pool, system_id).await;
        assert_eq!(
            desired.as_deref(),
            Some(store_path.as_str()),
            "non-strict violation must warn, not block"
        );
    }

    // H: A system-scope assignment of the same policy lineage/version
    // overrides an environment-scope default of the same lineage/version.
    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires a verified disposable DATABASE_URL"]
    async fn cve_gate_system_assignment_overrides_environment_default(pool: PgPool) {
        use crate::compliance::resolver::resolve_system_effective_policies;

        let suffix = Uuid::new_v4().simple().to_string();
        let (flake_id, derivation_id, store_path, config_name) =
            cve_gate_artifact(&pool, &suffix).await;
        cve_gate_scan(&pool, derivation_id, 248, 0).await;
        let (_policy_id, version_id) = publish_cve_check_policy(
            &pool,
            &suffix,
            &CveCheckConfig {
                max_critical: 50,
                strict: true,
                ..Default::default()
            },
        )
        .await;
        let (bundle_id, bundle_version_id) =
            publish_cve_gate_bundle(&pool, &suffix, version_id).await;

        let environment_id: Uuid =
            sqlx::query_scalar("INSERT INTO environments (name) VALUES ($1) RETURNING id")
                .bind(format!("env-{suffix}"))
                .fetch_one(&pool)
                .await
                .unwrap();
        // Environment default: enforce (would block, 248 > 50).
        assign_cve_gate_bundle(
            &pool,
            bundle_id,
            bundle_version_id,
            Some(environment_id),
            None,
            "enforce",
        )
        .await;
        let system_id = cve_gate_system(
            &pool,
            &format!("host-{suffix}"),
            flake_id,
            &config_name,
            Some(environment_id),
        )
        .await;
        // System-level assignment of the SAME bundle/policy version:
        // report_only. System specificity outranks environment specificity
        // for the same policy lineage/version (see
        // `compliance::resolver::merge_effective_policy_candidate`).
        assign_cve_gate_bundle(
            &pool,
            bundle_id,
            bundle_version_id,
            None,
            Some(system_id),
            "report_only",
        )
        .await;

        let resolved = match resolve_system_effective_policies(&pool, system_id)
            .await
            .unwrap()
        {
            ResolutionOutcome::Resolved(set) => set,
            ResolutionOutcome::Conflict(conflicts) => {
                panic!("expected resolved set: {conflicts:?}")
            }
        };
        assert_eq!(resolved.policies.len(), 1);
        assert_eq!(
            resolved.policies[0].effective_mode,
            AssignmentMode::ReportOnly,
            "system assignment must win over the environment default"
        );

        let manager = DeploymentPolicyManager::new(CrystalForgeConfig::default(), pool.clone());
        let stats = manager.update_auto_latest_policies().await.unwrap();
        assert_eq!((stats.systems_checked, stats.systems_updated), (1, 1));
        let (desired, ..) = cve_gate_system_state(&pool, system_id).await;
        assert_eq!(desired.as_deref(), Some(store_path.as_str()));
    }

    // Real sledge acceptance case (owner report): the exact same effective
    // assignment allows delivery under report_only while a FAIL compliance
    // outcome still exists, then blocks it once switched to enforce.
    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires a verified disposable DATABASE_URL"]
    async fn cve_gate_sledge_shaped_report_only_then_enforce(pool: PgPool) {
        let suffix = Uuid::new_v4().simple().to_string();
        let (flake_id, derivation_id, store_path, config_name) =
            cve_gate_artifact(&pool, &suffix).await;
        cve_gate_scan(&pool, derivation_id, 248, 1274).await;
        let (_policy_id, version_id) = publish_cve_check_policy(
            &pool,
            &suffix,
            &CveCheckConfig {
                max_critical: 50,
                strict: true,
                ..Default::default()
            },
        )
        .await;
        let (bundle_id, bundle_version_id) =
            publish_cve_gate_bundle(&pool, &suffix, version_id).await;
        let system_id = cve_gate_system(
            &pool,
            &format!("sledge-{suffix}"),
            flake_id,
            &config_name,
            None,
        )
        .await;
        let (assignment_id, _assignment_version_id) = assign_cve_gate_bundle(
            &pool,
            bundle_id,
            bundle_version_id,
            None,
            Some(system_id),
            "report_only",
        )
        .await;

        let manager = DeploymentPolicyManager::new(CrystalForgeConfig::default(), pool.clone());
        let stats = manager.update_auto_latest_policies().await.unwrap();
        assert_eq!((stats.systems_checked, stats.systems_updated), (1, 1));
        let (desired, set_at, pending) = cve_gate_system_state(&pool, system_id).await;
        assert_eq!(desired.as_deref(), Some(store_path.as_str()));
        assert!(set_at.is_some());
        assert_eq!(pending.as_deref(), Some(store_path.as_str()));

        // Switch the exact same assignment to enforce with a new immutable
        // version, then clear the delivered target so the next pass proves
        // whether it advances again.
        sqlx::query(
            "UPDATE systems SET desired_target = NULL, desired_target_set_at = NULL WHERE id = $1",
        )
        .bind(system_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE pending_system_deployments SET status = 'superseded', completed_at = NOW() \
             WHERE system_id = $1 AND status = 'pending'",
        )
        .bind(system_id)
        .execute(&pool)
        .await
        .unwrap();
        let enforced_version_id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO compliance_bundle_assignment_versions
                 (assignment_id, version_number, bundle_version_id, enforcement_mode,
                  assignment_overlay_digest)
               VALUES ($1, 2, $2, 'enforce', 'cve-gate-overlay') RETURNING id"#,
        )
        .bind(assignment_id)
        .bind(bundle_version_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE compliance_bundle_assignments SET current_version_id = $1 WHERE id = $2",
        )
        .bind(enforced_version_id)
        .bind(assignment_id)
        .execute(&pool)
        .await
        .unwrap();

        let stats = manager.update_auto_latest_policies().await.unwrap();
        assert_eq!((stats.systems_checked, stats.systems_updated), (1, 0));
        let (desired, set_at, pending) = cve_gate_system_state(&pool, system_id).await;
        assert_eq!(
            desired, None,
            "enforce must block delivery of the failing target"
        );
        assert_eq!(set_at, None);
        assert_eq!(pending, None);
    }

    // SQLx creates and drops databases; run only against the disposable test cluster.
    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires a verified disposable DATABASE_URL"]
    async fn auto_latest_waits_for_exact_cache_output_and_error_free_derivation(pool: PgPool) {
        use base64::{Engine, engine::general_purpose::STANDARD};
        use ed25519_dalek::SigningKey;
        use uuid::Uuid;

        let manager = DeploymentPolicyManager::new(CrystalForgeConfig::default(), pool.clone());
        for case in ["null_cache", "wrong_cache", "derivation_error"] {
            let suffix = Uuid::new_v4().simple().to_string();
            let host = format!("{case}-{suffix}");
            let path_a = format!("/nix/store/{suffix}-a");
            let path_b = format!("/nix/store/{suffix}-b");
            let flake: i32 = sqlx::query_scalar(
                "INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id",
            )
            .bind(&host)
            .bind(format!("https://example.invalid/{suffix}"))
            .fetch_one(&pool)
            .await
            .unwrap();
            let system: Uuid = sqlx::query_scalar(
                "INSERT INTO systems (hostname, public_key, derivation, flake_id, deployment_policy)
                 VALUES ($1, $2, 'test', $3, 'auto_latest') RETURNING id",
            )
            .bind(&host)
            .bind(
                STANDARD.encode(
                    SigningKey::generate(&mut rand::thread_rng())
                        .verifying_key()
                        .to_bytes(),
                ),
            )
            .bind(flake)
            .fetch_one(&pool)
            .await
            .unwrap();
            let mut identities = Vec::new();
            for (index, path) in [(1, &path_a), (2, &path_b)] {
                let commit: i32 = sqlx::query_scalar(
                    "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp)
                     VALUES ($1, $2, '2026-01-01'::timestamptz + $3 * INTERVAL '1 day') RETURNING id",
                )
                .bind(flake)
                .bind(format!("{index}-{suffix}"))
                .bind(index)
                .fetch_one(&pool)
                .await
                .unwrap();
                let derivation: i32 = sqlx::query_scalar(
                    "INSERT INTO derivations (commit_id, derivation_type, derivation_name,
                     status_id, attempt_count, store_path, cf_agent_enabled, policy_requirements_met,
                     error_message)
                     VALUES ($1, 'nixos', $2, 11, 0, $3, true, true, $4) RETURNING id",
                )
                .bind(commit)
                .bind(&host)
                .bind(path)
                .bind((index == 2 && case == "derivation_error").then_some("build error"))
                .fetch_one(&pool)
                .await
                .unwrap();
                let cache_path = if index == 1 || case == "derivation_error" {
                    Some(path.as_str())
                } else if case == "wrong_cache" {
                    Some(path_a.as_str())
                } else {
                    None
                };
                sqlx::query(
                    "INSERT INTO cache_push_jobs (derivation_id, status, store_path)
                     VALUES ($1, 'completed', $2)",
                )
                .bind(derivation)
                .bind(cache_path)
                .execute(&pool)
                .await
                .unwrap();
                identities.push((commit, derivation));

                if index == 1 {
                    assert_eq!(
                        manager
                            .update_auto_latest_policies()
                            .await
                            .unwrap()
                            .systems_updated,
                        1,
                        "{case}: A must be selected"
                    );
                    sqlx::query(
                        "INSERT INTO system_states (hostname, change_reason, store_path)
                         VALUES ($1, 'startup', $2)",
                    )
                    .bind(&host)
                    .bind(&path_a)
                    .execute(&pool)
                    .await
                    .unwrap();
                }
            }

            let (desired_a, set_at_a, pending_a, commit_a, derivation_a): (
                Option<String>,
                Option<chrono::DateTime<chrono::Utc>>,
                Uuid,
                Option<i32>,
                Option<i32>,
            ) = sqlx::query_as(
                "SELECT s.desired_target, s.desired_target_set_at, p.id,
                 p.requested_commit_id, p.requested_derivation_id
                 FROM systems s JOIN pending_system_deployments p ON p.system_id = s.id
                 WHERE s.id = $1 AND p.status = 'pending'",
            )
            .bind(system)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(desired_a.as_deref(), Some(path_a.as_str()), "{case}");
            assert_eq!(
                (commit_a, derivation_a),
                (Some(identities[0].0), Some(identities[0].1))
            );
            let before: (String, Option<String>) = sqlx::query_as(
                "SELECT deployment_status, latest_commit_hash
                 FROM view_system_deployment_status WHERE hostname = $1",
            )
            .bind(&host)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(
                before,
                ("up_to_date".into(), Some(format!("1-{suffix}"))),
                "{case}"
            );
            assert_eq!(
                manager
                    .update_auto_latest_policies()
                    .await
                    .unwrap()
                    .systems_updated,
                0,
                "{case}: incomplete B must not replace A"
            );
            let unchanged: (Option<String>, Option<chrono::DateTime<chrono::Utc>>, Uuid, i64) =
                sqlx::query_as(
                    "SELECT desired_target, desired_target_set_at,
                     (SELECT id FROM pending_system_deployments WHERE system_id = $1 AND status = 'pending'),
                     (SELECT count(*) FROM pending_system_deployments WHERE system_id = $1)
                     FROM systems WHERE id = $1",
                )
                .bind(system)
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(
                unchanged,
                (Some(path_a.clone()), set_at_a, pending_a, 1),
                "{case}"
            );

            if case == "derivation_error" {
                sqlx::query("UPDATE derivations SET error_message = NULL WHERE id = $1")
                    .bind(identities[1].1)
                    .execute(&pool)
                    .await
                    .unwrap();
            } else {
                sqlx::query("UPDATE cache_push_jobs SET store_path = $2 WHERE derivation_id = $1")
                    .bind(identities[1].1)
                    .bind(&path_b)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            assert_eq!(
                manager
                    .update_auto_latest_policies()
                    .await
                    .unwrap()
                    .systems_updated,
                1,
                "{case}: exactly published B must advance on the next pass"
            );
            let advanced: (Option<String>, String, Option<i32>, Option<i32>) = sqlx::query_as(
                "SELECT s.desired_target, p.target_store_path,
                 p.requested_commit_id, p.requested_derivation_id
                 FROM systems s JOIN pending_system_deployments p ON p.system_id = s.id
                 WHERE s.id = $1 AND p.status = 'pending'",
            )
            .bind(system)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(
                advanced,
                (
                    Some(path_b.clone()),
                    path_b.clone(),
                    Some(identities[1].0),
                    Some(identities[1].1)
                ),
                "{case}"
            );
            let after: (String, Option<String>) = sqlx::query_as(
                "SELECT deployment_status, latest_commit_hash
                 FROM view_system_deployment_status WHERE hostname = $1",
            )
            .bind(&host)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(
                after,
                ("behind".into(), Some(format!("2-{suffix}"))),
                "{case}"
            );
            sqlx::query(
                "INSERT INTO system_states (hostname, change_reason, store_path)
                 VALUES ($1, 'startup', $2)",
            )
            .bind(&host)
            .bind(&path_b)
            .execute(&pool)
            .await
            .unwrap();
            let installed: String = sqlx::query_scalar(
                "SELECT deployment_status FROM view_system_deployment_status WHERE hostname = $1",
            )
            .bind(&host)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(installed, "up_to_date", "{case}");
        }
    }

    // SQLx creates and drops a database; use only the verified disposable PG.
    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires a verified disposable DATABASE_URL"]
    async fn auto_latest_same_path_rebinds_exact_lineage_without_redelivery(pool: PgPool) {
        use crate::models::deployment_policies::CreateDeploymentPolicyRequest;
        use crate::queries::deployment_policies::create_deployment_policy;
        use base64::{Engine, engine::general_purpose::STANDARD};
        use ed25519_dalek::SigningKey;
        use sqlx::Row;
        use uuid::Uuid;

        let suffix = Uuid::new_v4().simple().to_string();
        let host = format!("same-path-{suffix}");
        let path = format!("/nix/store/{suffix}-system");
        let flake: i32 =
            sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id")
                .bind(&host)
                .bind(format!("https://example.invalid/{suffix}"))
                .fetch_one(&pool)
                .await
                .unwrap();
        let system: Uuid = sqlx::query_scalar(
            "INSERT INTO systems (hostname, public_key, derivation, flake_id, deployment_policy)
             VALUES ($1, $3, 'test', $2, 'auto_latest') RETURNING id",
        )
        .bind(&host)
        .bind(flake)
        .bind(
            STANDARD.encode(
                SigningKey::generate(&mut rand::thread_rng())
                    .verifying_key()
                    .to_bytes(),
            ),
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let manager = DeploymentPolicyManager::new(CrystalForgeConfig::default(), pool.clone());
        let mut identities = Vec::new();
        for index in 1..=2 {
            let commit: i32 = sqlx::query_scalar(
                "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp)
                 VALUES ($1, $2, '2026-01-01'::timestamptz + $3 * INTERVAL '1 day') RETURNING id",
            )
            .bind(flake)
            .bind(format!("{index}-{suffix}"))
            .bind(index)
            .fetch_one(&pool)
            .await
            .unwrap();
            let derivation: i32 = sqlx::query_scalar(
                "INSERT INTO derivations (commit_id, derivation_type, derivation_name,
                 status_id, attempt_count, store_path, cf_agent_enabled, policy_requirements_met)
                 VALUES ($1, 'nixos', $2, 11, 0, $3, true, true) RETURNING id",
            )
            .bind(commit)
            .bind(&host)
            .bind(&path)
            .fetch_one(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO cache_push_jobs (derivation_id, status, store_path)
                 VALUES ($1, 'completed', $2)",
            )
            .bind(derivation)
            .bind(&path)
            .execute(&pool)
            .await
            .unwrap();
            let artifact: Uuid = sqlx::query_scalar(
                "INSERT INTO evaluation_snapshots (commit_id, configuration_name, lifecycle)
                 VALUES ($1, $2, 'available') RETURNING id",
            )
            .bind(commit)
            .bind(&host)
            .fetch_one(&pool)
            .await
            .unwrap();
            sqlx::query("UPDATE evaluation_snapshots SET integrity_version = 1 WHERE id = $1")
                .bind(artifact)
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query(
                "INSERT INTO evaluation_snapshot_selections
                 (commit_id, configuration_name, current_snapshot_id) VALUES ($1, $2, $3)",
            )
            .bind(commit)
            .bind(&host)
            .bind(artifact)
            .execute(&pool)
            .await
            .unwrap();
            let scan: Uuid = sqlx::query_scalar(
                "INSERT INTO cve_scans (derivation_id, scanner_name, status)
                 VALUES ($1, 'same-path-regression', 'in_progress') RETURNING id",
            )
            .bind(derivation)
            .fetch_one(&pool)
            .await
            .unwrap();
            sqlx::query(
                "UPDATE cve_scans SET status = 'completed', completed_at = NOW(),
                 evidence_schema_version = 1 WHERE id = $1",
            )
            .bind(scan)
            .execute(&pool)
            .await
            .unwrap();
            identities.push((commit, derivation));
            if index == 2 {
                sqlx::query("UPDATE derivations SET error_message = 'stale' WHERE id = $1")
                    .bind(identities[0].1)
                    .execute(&pool)
                    .await
                    .unwrap();
                assert!(
                    crate::services::composite_enforcement::authorize_and_claim_desired_target(
                        &pool, system, &path,
                    )
                    .await
                    .is_err(),
                    "delivery must not authorize B against A's pending row"
                );
            }
            assert_eq!(
                manager
                    .update_auto_latest_policies()
                    .await
                    .unwrap()
                    .systems_updated,
                1
            );
            let row = sqlx::query(
                "SELECT desired_target, desired_target_set_at FROM systems WHERE id = $1",
            )
            .bind(system)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(
                row.get::<Option<String>, _>("desired_target").as_deref(),
                Some(path.as_str())
            );
            let mut at = row
                .get::<Option<chrono::DateTime<chrono::Utc>>, _>("desired_target_set_at")
                .unwrap();
            let pending = sqlx::query(
                "SELECT id, requested_commit_id, requested_derivation_id,
                 evaluation_snapshot_id, delivered_at
                 FROM pending_system_deployments WHERE system_id = $1 AND status = 'pending'",
            )
            .bind(system)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(
                pending.get::<Option<i32>, _>("requested_commit_id"),
                Some(commit)
            );
            assert_eq!(
                pending.get::<Option<i32>, _>("requested_derivation_id"),
                Some(derivation)
            );
            assert_eq!(
                pending.get::<Option<Uuid>, _>("evaluation_snapshot_id"),
                Some(artifact)
            );
            let mut pending_id: Uuid = pending.get("id");
            assert!(
                pending
                    .get::<Option<chrono::DateTime<chrono::Utc>>, _>("delivered_at")
                    .is_none()
            );
            let scheduled: (Uuid, i32, String, Uuid) = sqlx::query_as(
                "SELECT deployment_id, derivation_id, commit_hash, scan_id
                 FROM view_active_scheduled_cve_scan_targets WHERE system_id = $1",
            )
            .bind(system)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(
                scheduled,
                (pending_id, derivation, format!("{index}-{suffix}"), scan),
                "scheduled CVE reader must follow the immutable pending identity"
            );
            if index == 2 {
                let old: (String, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
                    "SELECT status, delivered_at FROM pending_system_deployments
                     WHERE system_id = $1 AND requested_derivation_id = $2",
                )
                .bind(system)
                .bind(identities[0].1)
                .fetch_one(&pool)
                .await
                .unwrap();
                assert_eq!(old, ("superseded".into(), None));
                sqlx::query(
                    "INSERT INTO pending_system_deployments
                     (system_id, target_store_path, source, issued_at, expires_at,
                      requested_commit_id, requested_derivation_id)
                     VALUES ($1, $2, 'auto_desired_target', NOW() - INTERVAL '5 minutes',
                             NOW() + INTERVAL '2 hours', $3, $4)",
                )
                .bind(system)
                .bind(&path)
                .bind(identities[0].0)
                .bind(identities[0].1)
                .execute(&pool)
                .await
                .unwrap();
                assert_eq!(
                    manager
                        .update_auto_latest_policies()
                        .await
                        .unwrap()
                        .systems_updated,
                    1
                );
                let still_b: (Option<chrono::DateTime<chrono::Utc>>, Uuid, i64) = sqlx::query_as(
                    "SELECT desired_target_set_at,
                     (SELECT id FROM pending_system_deployments WHERE system_id = $1 AND status = 'pending'),
                     (SELECT count(*) FROM pending_system_deployments WHERE system_id = $1 AND status = 'pending')
                     FROM systems WHERE id = $1",
                )
                .bind(system)
                .fetch_one(&pool)
                .await
                .unwrap();
                assert_eq!(still_b, (Some(at), pending_id, 1));
                // A newer stale A must not be hidden by the still-live B row.
                sqlx::query(
                    "INSERT INTO pending_system_deployments
                     (system_id, target_store_path, source, expires_at,
                      requested_commit_id, requested_derivation_id)
                     VALUES ($1, $2, 'auto_desired_target', NOW() + INTERVAL '2 hours', $3, $4)",
                )
                .bind(system)
                .bind(&path)
                .bind(identities[0].0)
                .bind(identities[0].1)
                .execute(&pool)
                .await
                .unwrap();
                assert_eq!(
                    manager
                        .update_auto_latest_policies()
                        .await
                        .unwrap()
                        .systems_updated,
                    1
                );
                let repaired: (Option<chrono::DateTime<chrono::Utc>>, Uuid, Option<i32>, i64) =
                    sqlx::query_as(
                        "SELECT desired_target_set_at,
                         (SELECT id FROM pending_system_deployments WHERE system_id = $1 AND status = 'pending'),
                         (SELECT requested_derivation_id FROM pending_system_deployments WHERE system_id = $1 AND status = 'pending'),
                         (SELECT count(*) FROM pending_system_deployments WHERE system_id = $1 AND status = 'pending')
                         FROM systems WHERE id = $1",
                    )
                    .bind(system)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
                assert_ne!(repaired.0, Some(at));
                assert_ne!(repaired.1, pending_id);
                assert_eq!((repaired.2, repaired.3), (Some(derivation), 1));
                pending_id = repaired.1;
                at = repaired.0.unwrap();
                let claim =
                    crate::services::composite_enforcement::authorize_and_claim_desired_target(
                        &pool, system, &path,
                    )
                    .await
                    .unwrap();
                assert_eq!(claim.target.as_deref(), Some(path.as_str()));
                let delivered: Uuid = sqlx::query_scalar(
                    "SELECT id FROM pending_system_deployments WHERE system_id = $1
                     AND status = 'pending' AND delivered_at IS NOT NULL",
                )
                .bind(system)
                .fetch_one(&pool)
                .await
                .unwrap();
                assert_eq!(delivered, pending_id);
            }
            assert_eq!(
                manager
                    .update_auto_latest_policies()
                    .await
                    .unwrap()
                    .systems_updated,
                0
            );
            let unchanged: (Option<chrono::DateTime<chrono::Utc>>, Uuid, i64) = sqlx::query_as(
                "SELECT desired_target_set_at,
                 (SELECT id FROM pending_system_deployments WHERE system_id = $1 AND status = 'pending'),
                 (SELECT count(*) FROM pending_system_deployments WHERE system_id = $1)
                 FROM systems WHERE id = $1",
            )
            .bind(system)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(
                unchanged,
                (
                    Some(at),
                    pending_id,
                    i64::from(index) + 3 * i64::from(index == 2)
                )
            );
        }

        let commit_c: i32 = sqlx::query_scalar(
            "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp)
             VALUES ($1, $2, '2026-01-04') RETURNING id",
        )
        .bind(flake)
        .bind(format!("3-{suffix}"))
        .fetch_one(&pool)
        .await
        .unwrap();
        let derivation_c: i32 = sqlx::query_scalar(
            "INSERT INTO derivations (commit_id, derivation_type, derivation_name,
             status_id, attempt_count, store_path, cf_agent_enabled, policy_requirements_met)
             VALUES ($1, 'nixos', $2, 11, 0, $3, true, true) RETURNING id",
        )
        .bind(commit_c)
        .bind(&host)
        .bind(&path)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO cache_push_jobs (derivation_id, status, store_path) VALUES ($1, 'completed', $2)")
            .bind(derivation_c)
            .bind(&path)
            .execute(&pool)
            .await
            .unwrap();
        let b_state: (Option<chrono::DateTime<chrono::Utc>>, Uuid) = sqlx::query_as(
            "SELECT desired_target_set_at,
             (SELECT id FROM pending_system_deployments WHERE system_id = $1 AND status = 'pending')
             FROM systems WHERE id = $1",
        )
        .bind(system)
        .fetch_one(&pool)
        .await
        .unwrap();
        for (policy_type, config) in [
            (
                "require_approvals",
                serde_json::json!({"description": "wait", "count": 1, "role": "admin"}),
            ),
            (
                "time_window",
                serde_json::json!({"description": "closed", "days": [],
                "start_time": "00:00", "end_time": "23:59", "timezone": "UTC", "action": "block"}),
            ),
        ] {
            let policy = create_deployment_policy(
                &pool,
                &CreateDeploymentPolicyRequest {
                    name: format!("{policy_type}-{suffix}"),
                    policy_type: policy_type.into(),
                    config,
                    enabled: Some(true),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            sqlx::query(
                "UPDATE deployment_policy_versions SET trust_state = 'trusted'
                 WHERE id = (SELECT current_draft_version_id FROM deployment_policies WHERE id = $1)",
            )
            .bind(policy.id)
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query("INSERT INTO system_policies (system_id, policy_id) VALUES ($1, $2)")
                .bind(system)
                .bind(policy.id)
                .execute(&pool)
                .await
                .unwrap();
            assert_eq!(
                manager
                    .update_auto_latest_policies()
                    .await
                    .unwrap()
                    .systems_updated,
                0
            );
            let still_b: (Option<chrono::DateTime<chrono::Utc>>, Uuid, Option<i32>) = sqlx::query_as(
                "SELECT desired_target_set_at,
                 (SELECT id FROM pending_system_deployments WHERE system_id = $1 AND status = 'pending'),
                 (SELECT requested_derivation_id FROM pending_system_deployments
                  WHERE system_id = $1 AND status = 'pending') FROM systems WHERE id = $1",
            )
            .bind(system)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(still_b, (b_state.0, b_state.1, Some(identities[1].1)));
            sqlx::query("DELETE FROM system_policies WHERE system_id = $1 AND policy_id = $2")
                .bind(system)
                .bind(policy.id)
                .execute(&pool)
                .await
                .unwrap();
        }
        assert_eq!(
            manager
                .update_auto_latest_policies()
                .await
                .unwrap()
                .systems_updated,
            1
        );
        let rebound: (Option<i32>, Option<i32>) = sqlx::query_as(
            "SELECT requested_commit_id, requested_derivation_id
             FROM pending_system_deployments WHERE system_id = $1 AND status = 'pending'",
        )
        .bind(system)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(rebound, (Some(commit_c), Some(derivation_c)));
    }

    // This test creates SQLx databases. Run it only with DATABASE_URL pointing
    // at a separately verified disposable PostgreSQL cluster, never the preview.
    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires a verified disposable DATABASE_URL"]
    async fn sledge_auto_latest_sets_and_delivers_newest_deployable_target(pool: PgPool) {
        use crate::handlers::agent::heartbeat::{LogResponse, log};
        use crate::handlers::agent_request::CFState;
        use crate::models::deployment_policies::CreateDeploymentPolicyRequest;
        use crate::queries::deployment_policies::create_deployment_policy;
        use crate::queue::QueueNotifier;
        use crate::server::jobs::BackgroundJobRegistry;
        use crate::test_utils::builders::SystemStateBuilder;
        use axum::{Router, routing::post};
        use base64::{Engine, engine::general_purpose::STANDARD};
        use ed25519_dalek::{Signer, SigningKey};
        use sqlx::Row;
        use std::sync::Arc;
        use uuid::Uuid;

        let suffix = Uuid::new_v4().simple().to_string();
        let host = format!("sledge-{suffix}");
        let config_name = format!("nixos-sledge-{suffix}");
        let signing_key = SigningKey::generate(&mut rand::thread_rng());
        let flake_id: i32 =
            sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id")
                .bind(&host)
                .bind(format!("https://example.invalid/{suffix}"))
                .fetch_one(&pool)
                .await
                .unwrap();
        let system_id: Uuid = sqlx::query_scalar(
            "INSERT INTO systems (hostname, public_key, derivation, flake_id, \
             system_configuration_name, deployment_policy) \
             VALUES ($1, $2, 'test-derivation', $3, $4, 'auto_latest') RETURNING id",
        )
        .bind(&host)
        .bind(STANDARD.encode(signing_key.verifying_key().to_bytes()))
        .bind(flake_id)
        .bind(&config_name)
        .fetch_one(&pool)
        .await
        .unwrap();
        let path_a = format!("/nix/store/{suffix}-a");
        let path_b = format!("/nix/store/{suffix}-b");
        let mut b_commit = 0;
        let mut b_derivation = 0;
        for (index, path) in [(1, &path_a), (2, &path_b), (3, &path_b)] {
            let commit_id: i32 = sqlx::query_scalar(
                "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) \
                 VALUES ($1, $2, '2026-01-01'::timestamptz + $3 * INTERVAL '1 day') RETURNING id",
            )
            .bind(flake_id)
            .bind(format!("{index}-{suffix}"))
            .bind(index)
            .fetch_one(&pool)
            .await
            .unwrap();
            if index == 3 {
                // Raw HEAD C has no NixOS derivation for sledge.
                continue;
            }
            let derivation_id: i32 = sqlx::query_scalar(
                "INSERT INTO derivations (commit_id, derivation_type, derivation_name, \
                 status_id, attempt_count, store_path, cf_agent_enabled, policy_requirements_met) \
                 VALUES ($1, 'nixos', $2, 11, 0, $3, true, true) RETURNING id",
            )
            .bind(commit_id)
            .bind(&config_name)
            .bind(path)
            .fetch_one(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO cache_push_jobs (derivation_id, status, store_path) \
                 VALUES ($1, 'completed', $2)",
            )
            .bind(derivation_id)
            .bind(path)
            .execute(&pool)
            .await
            .unwrap();
            if index == 2 {
                b_commit = commit_id;
                b_derivation = derivation_id;
            }
        }
        let nullable_metadata: Option<String> =
            sqlx::query_scalar("SELECT derivation_target FROM derivations WHERE id = $1")
                .bind(b_derivation)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(nullable_metadata, None);
        sqlx::query(
            "INSERT INTO system_states (hostname, change_reason, store_path) VALUES ($1, 'startup', $2)",
        )
        .bind(&host)
        .bind(&path_a)
        .execute(&pool)
        .await
        .unwrap();

        let selected =
            get_latest_deployable_targets_for_flake_hosts(&pool, flake_id, &[config_name.clone()])
                .await
                .unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].store_path.as_deref(), Some(path_b.as_str()));
        assert!(selected[0].newer_raw_commit_exists);

        let manager = DeploymentPolicyManager::new(CrystalForgeConfig::default(), pool.clone());
        let first = manager.update_auto_latest_policies().await.unwrap();
        assert_eq!((first.systems_checked, first.systems_updated), (1, 1));
        let row =
            sqlx::query("SELECT desired_target, desired_target_set_at FROM systems WHERE id = $1")
                .bind(system_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            row.get::<Option<String>, _>("desired_target").as_deref(),
            Some(path_b.as_str())
        );
        let issued_at: chrono::DateTime<chrono::Utc> =
            row.get::<Option<_>, _>("desired_target_set_at").unwrap();
        let pending = sqlx::query(
            "SELECT id, source, target_store_path, requested_commit_id, requested_derivation_id, \
             delivered_at FROM pending_system_deployments WHERE system_id = $1 AND status = 'pending'",
        )
        .bind(system_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let pending_id: Uuid = pending.get("id");
        assert_eq!(pending.get::<String, _>("source"), "auto_desired_target");
        assert_eq!(pending.get::<String, _>("target_store_path"), path_b);
        assert_eq!(
            pending.get::<Option<i32>, _>("requested_commit_id"),
            Some(b_commit)
        );
        assert_eq!(
            pending.get::<Option<i32>, _>("requested_derivation_id"),
            Some(b_derivation)
        );
        assert!(
            pending
                .get::<Option<chrono::DateTime<chrono::Utc>>, _>("delivered_at")
                .is_none()
        );

        let second = manager.update_auto_latest_policies().await.unwrap();
        assert_eq!((second.systems_checked, second.systems_updated), (1, 0));
        let unchanged: (Option<chrono::DateTime<chrono::Utc>>, i64) = sqlx::query_as(
            "SELECT desired_target_set_at, (SELECT count(*) FROM pending_system_deployments \
             WHERE system_id = $1) FROM systems WHERE id = $1",
        )
        .bind(system_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(unchanged, (Some(issued_at), 1));

        let state = CFState::new(
            pool.clone(),
            crate::config::ServerConfig::default(),
            Arc::new(QueueNotifier::new()),
            BackgroundJobRegistry::new(),
        );
        let app = Router::new()
            .route("/current-system", post(log))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let payload = SystemStateBuilder::new()
            .hostname(&host)
            .store_path(&path_a)
            .build();
        let body = serde_json::to_vec(&payload).unwrap();
        let signature = STANDARD.encode(signing_key.sign(&body).to_bytes());
        let response = reqwest::Client::new()
            .post(format!("http://{address}/current-system"))
            .header("x-key-id", &host)
            .header("x-signature", signature)
            .body(body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let delivered: LogResponse = response.json().await.unwrap();
        assert_eq!(delivered.desired_target.as_deref(), Some(path_b.as_str()));
        let delivered_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM pending_system_deployments WHERE system_id = $1 \
             AND status = 'pending' AND delivered_at IS NOT NULL",
        )
        .bind(system_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(delivered_id, pending_id);
        server.abort();

        for (policy_type, config, expected) in [
            (
                "require_approvals",
                serde_json::json!({"description": "wait for approval", "count": 1, "role": "admin"}),
                "pending",
            ),
            (
                "time_window",
                serde_json::json!({"description": "closed window", "days": [],
                    "start_time": "00:00", "end_time": "23:59", "timezone": "UTC", "action": "block"}),
                "blocked",
            ),
        ] {
            let gated_host = format!("{policy_type}-{suffix}");
            let gated_id: Uuid = sqlx::query_scalar(
                "INSERT INTO systems (hostname, public_key, derivation, flake_id, \
                 system_configuration_name, deployment_policy) \
                 VALUES ($1, $4, 'test-derivation', $2, $3, 'auto_latest') RETURNING id",
            )
            .bind(&gated_host)
            .bind(flake_id)
            .bind(&config_name)
            .bind(
                STANDARD.encode(
                    SigningKey::generate(&mut rand::thread_rng())
                        .verifying_key()
                        .to_bytes(),
                ),
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            let policy = create_deployment_policy(
                &pool,
                &CreateDeploymentPolicyRequest {
                    name: format!("{policy_type}-{suffix}"),
                    policy_type: policy_type.into(),
                    config,
                    enabled: Some(true),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            sqlx::query(
                "UPDATE deployment_policy_versions SET trust_state = 'trusted' \
                 WHERE id = (SELECT current_draft_version_id FROM deployment_policies WHERE id = $1)",
            )
            .bind(policy.id)
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query("INSERT INTO system_policies (system_id, policy_id) VALUES ($1, $2)")
                .bind(gated_id)
                .bind(policy.id)
                .execute(&pool)
                .await
                .unwrap();
            assert_eq!(
                get_latest_deployable_targets_for_flake_hosts(
                    &pool,
                    flake_id,
                    &[config_name.clone()],
                )
                .await
                .unwrap()[0]
                    .store_path
                    .as_deref(),
                Some(path_b.as_str()),
                "{expected} runtime gate must not change artifact selection"
            );
            let gated_system = get_systems_with_auto_latest_policy(&pool)
                .await
                .unwrap()
                .into_iter()
                .find(|system| system.id == gated_id)
                .unwrap();
            let resolved =
                resolve_systems_effective_policies_for_deployment_batch(&pool, &[gated_id])
                    .await
                    .unwrap();
            let ResolutionOutcome::Resolved(effective) = &resolved[&gated_id] else {
                panic!("gated policy must resolve");
            };
            assert_eq!(effective.policies.len(), 1);
            let version_id = effective.policies[0].policy_version_id;
            let records = get_deployment_policies_by_versions(&pool, &[version_id])
                .await
                .unwrap();
            let decision = manager
                .evaluate_advanced_policy_gates(
                    &gated_system,
                    &selected[0],
                    std::slice::from_ref(&gated_system),
                    &HashMap::from([(gated_id, effective.policies.clone())]),
                    &records,
                    &HashSet::new(),
                )
                .await;
            assert!(
                matches!(
                    (&decision, expected),
                    (AdvancedGateDecision::Pending(_), "pending")
                        | (AdvancedGateDecision::Block(_), "blocked")
                ),
                "expected {expected} runtime gate, got {decision:?}"
            );
        }
        for policy in ["manual", "pinned"] {
            sqlx::query(
                "INSERT INTO systems (hostname, public_key, derivation, flake_id, \
                 system_configuration_name, deployment_policy) \
                 VALUES ($1, $5, 'test-derivation', $2, $3, $4)",
            )
            .bind(format!("{policy}-{suffix}"))
            .bind(flake_id)
            .bind(&config_name)
            .bind(policy)
            .bind(
                STANDARD.encode(
                    SigningKey::generate(&mut rand::thread_rng())
                        .verifying_key()
                        .to_bytes(),
                ),
            )
            .execute(&pool)
            .await
            .unwrap();
        }
        let gated_pass = manager.update_auto_latest_policies().await.unwrap();
        assert_eq!(
            (gated_pass.systems_checked, gated_pass.systems_updated),
            (3, 0)
        );
        let untouched: (i64, i64) = sqlx::query_as(
            "SELECT count(*), count(*) FILTER (WHERE desired_target IS NULL \
             AND desired_target_set_at IS NULL) FROM systems \
             WHERE hostname IN ($1, $2, $3, $4)",
        )
        .bind(format!("require_approvals-{suffix}"))
        .bind(format!("time_window-{suffix}"))
        .bind(format!("manual-{suffix}"))
        .bind(format!("pinned-{suffix}"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(untouched, (4, 4));
        let gated_pending: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pending_system_deployments \
             WHERE system_id IN (SELECT id FROM systems WHERE hostname IN ($1, $2))",
        )
        .bind(format!("require_approvals-{suffix}"))
        .bind(format!("time_window-{suffix}"))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(gated_pending, 0);
    }

    // Use only a separately verified disposable PostgreSQL cluster, never the preview.
    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires a verified disposable DATABASE_URL"]
    async fn auto_latest_report_only_composite_fail_does_not_block_target(pool: PgPool) {
        use crate::compliance::resolver::{ResolutionOutcome, resolve_system_effective_policies};
        use crate::models::deployment_policies::{
            AssignedPolicy, CreateDeploymentPolicyRequest, DeploymentPolicy, PolicyCheckResult,
            composite_rule_result_key, policy_results_json,
        };
        use crate::queries::deployment_policies::create_deployment_policy;
        use crate::services::composite_enforcement::{
            enforce_composite_authorization_digest, persist_evaluation_assessments_in_tx,
        };
        use base64::{Engine, engine::general_purpose::STANDARD};
        use ed25519_dalek::SigningKey;
        use uuid::Uuid;

        let suffix = Uuid::new_v4().simple().to_string();
        let config_name = format!("nixos-composite-{suffix}");
        let path_a = format!("/nix/store/{suffix}-a");
        let path_b = path_a.clone();
        let flake_id: i32 =
            sqlx::query_scalar("INSERT INTO flakes (name, repo_url) VALUES ($1, $2) RETURNING id")
                .bind(format!("composite-{suffix}"))
                .bind(format!("https://example.invalid/{suffix}"))
                .fetch_one(&pool)
                .await
                .unwrap();
        let mut b_derivation = 0;
        let mut a_identity = (0, 0);
        for (index, path) in [(1, &path_a), (2, &path_b)] {
            let commit_id: i32 = sqlx::query_scalar(
                "INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp) \
                 VALUES ($1, $2, '2026-01-01'::timestamptz + $3 * INTERVAL '1 day') RETURNING id",
            )
            .bind(flake_id)
            .bind(format!("{index}-{suffix}"))
            .bind(index)
            .fetch_one(&pool)
            .await
            .unwrap();
            let derivation_id: i32 = sqlx::query_scalar(
                "INSERT INTO derivations (commit_id, derivation_type, derivation_name, \
                 status_id, attempt_count, expected_store_path, store_path, cf_agent_enabled, \
                 policy_requirements_met) VALUES ($1, 'nixos', $2, 11, 0, $3, $3, true, true) RETURNING id",
            )
            .bind(commit_id)
            .bind(&config_name)
            .bind(path)
            .fetch_one(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO cache_push_jobs (derivation_id, status, store_path) \
                 VALUES ($1, 'completed', $2)",
            )
            .bind(derivation_id)
            .bind(path)
            .execute(&pool)
            .await
            .unwrap();
            if index == 2 {
                b_derivation = derivation_id;
            } else {
                a_identity = (commit_id, derivation_id);
            }
        }

        let rule_id = Uuid::new_v4();
        let config = serde_json::json!({
            "schema_version": 1,
            "mode": "all",
            "rules": [{"id": rule_id, "kind": "nixos_option", "config": {
                "path": "networking.firewall.enable", "operator": "==",
                "value_type": "boolean", "value": true
            }}]
        });
        let policy = create_deployment_policy(
            &pool,
            &CreateDeploymentPolicyRequest {
                name: format!("composite-{suffix}"),
                policy_type: "composite".into(),
                config: config.clone(),
                enabled: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let version_id: Uuid = sqlx::query_scalar(
            "SELECT current_draft_version_id FROM deployment_policies WHERE id = $1",
        )
        .bind(policy.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("UPDATE deployment_policies SET current_draft_version_id = NULL WHERE id = $1")
            .bind(policy.id)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE deployment_policy_versions SET publication_state = 'accepted', trust_state = 'trusted' WHERE id = $1",
        )
        .bind(version_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE deployment_policies SET current_published_version_id = $1 WHERE id = $2",
        )
        .bind(version_id)
        .bind(policy.id)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let bundle_id: Uuid = sqlx::query_scalar(
            "INSERT INTO compliance_bundles (name, framework, version, layer) \
             VALUES ($1, 'test', '1.0', 'fleet') RETURNING id",
        )
        .bind(format!("composite-{suffix}"))
        .fetch_one(&pool)
        .await
        .unwrap();
        let bundle_version_id: Uuid = sqlx::query_scalar(
            "SELECT current_draft_version_id FROM compliance_bundles WHERE id = $1",
        )
        .bind(bundle_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO compliance_bundle_version_policies \
             (bundle_version_id, policy_version_id, policy_order) VALUES ($1, $2, 0)",
        )
        .bind(bundle_version_id)
        .bind(version_id)
        .execute(&pool)
        .await
        .unwrap();
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("UPDATE compliance_bundles SET current_draft_version_id = NULL WHERE id = $1")
            .bind(bundle_id)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE compliance_bundle_versions SET publication_state = 'accepted', trust_state = 'trusted', semantic_digest = $1 WHERE id = $2",
        )
        .bind(format!("test-{bundle_version_id}"))
        .bind(bundle_version_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2",
        )
        .bind(bundle_version_id)
        .bind(bundle_id)
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let mut report_digest = None;
        let mut enforce_digest = None;
        let mut systems = Vec::new();
        for mode in ["report_only", "enforce"] {
            let host = format!("{mode}-{suffix}");
            let system_id: Uuid = sqlx::query_scalar(
                "INSERT INTO systems (hostname, public_key, derivation, flake_id, \
                 system_configuration_name, deployment_policy) \
                 VALUES ($1, $2, 'test-derivation', $3, $4, 'auto_latest') RETURNING id",
            )
            .bind(&host)
            .bind(
                STANDARD.encode(
                    SigningKey::generate(&mut rand::thread_rng())
                        .verifying_key()
                        .to_bytes(),
                ),
            )
            .bind(flake_id)
            .bind(&config_name)
            .fetch_one(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO system_states (hostname, change_reason, store_path) \
                 VALUES ($1, 'startup', $2)",
            )
            .bind(&host)
            .bind(&path_a)
            .execute(&pool)
            .await
            .unwrap();
            let assignment_id: Uuid = sqlx::query_scalar(
                "INSERT INTO compliance_bundle_assignments \
                 (bundle_id, bundle_version_id, system_id, scope_type, active, enforcement_mode, assignment_overlay_digest) \
                 VALUES ($1, $2, $3, 'system', true, $4, 'test-overlay') RETURNING id",
            )
            .bind(bundle_id)
            .bind(bundle_version_id)
            .bind(system_id)
            .bind(mode)
            .fetch_one(&pool)
            .await
            .unwrap();
            let snapshot_id: Uuid = sqlx::query_scalar(
                "INSERT INTO compliance_bundle_assignment_versions \
                 (assignment_id, version_number, bundle_version_id, enforcement_mode, assignment_overlay_digest) \
                 VALUES ($1, 1, $2, $3, 'test-overlay') RETURNING id",
            )
            .bind(assignment_id)
            .bind(bundle_version_id)
            .bind(mode)
            .fetch_one(&pool)
            .await
            .unwrap();
            sqlx::query(
                "UPDATE compliance_bundle_assignments SET current_version_id = $1 WHERE id = $2",
            )
            .bind(snapshot_id)
            .bind(assignment_id)
            .execute(&pool)
            .await
            .unwrap();

            let ResolutionOutcome::Resolved(resolved) =
                resolve_system_effective_policies(&pool, system_id)
                    .await
                    .unwrap()
            else {
                panic!("composite assignment must resolve");
            };
            assert_eq!(resolved.policies.len(), 1);
            assert_eq!(resolved.policies[0].effective_mode.as_str(), mode);
            let digest = enforce_composite_authorization_digest(&resolved);
            let parsed_config = serde_json::from_value(config.clone()).unwrap();
            let assigned = AssignedPolicy {
                policy_id: version_id,
                policy_name: "manager regression".into(),
                enforcement_mode: Default::default(),
                policy: DeploymentPolicy::Composite {
                    config: parsed_config,
                },
            };
            let check = PolicyCheckResult::from_assigned(
                host.clone(),
                &serde_json::json!({
                    "cfAgentEnabled": true,
                    composite_rule_result_key(&version_id, &rule_id): {"success": true, "value": false}
                }),
                std::slice::from_ref(&assigned),
            )
            .unwrap();
            let results = policy_results_json(&check, std::slice::from_ref(&assigned));
            let mut tx = pool.begin().await.unwrap();
            persist_evaluation_assessments_in_tx(
                &mut tx,
                system_id,
                b_derivation,
                &path_b,
                &results,
                &resolved,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
            let (outcome, assessment_digest): (String, String) = sqlx::query_as(
                "SELECT overall_outcome, effective_set_digest FROM composite_policy_assessments \
                 WHERE system_id = $1 AND policy_version_id = $2 AND target_store_path = $3",
            )
            .bind(system_id)
            .bind(version_id)
            .bind(&path_b)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(outcome, "fail", "{mode} finding must remain FAIL");
            let (rule_outcome, rule_evidence): (String, serde_json::Value) = sqlx::query_as(
                "SELECT result.outcome, result.evidence FROM composite_policy_rule_results result \
                 JOIN composite_policy_assessments assessment ON assessment.id = result.assessment_id \
                 WHERE assessment.system_id = $1 AND result.rule_id = $2",
            )
            .bind(system_id)
            .bind(rule_id)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(rule_outcome, "fail");
            assert_ne!(rule_evidence, serde_json::json!({}));
            sqlx::query("UPDATE systems SET desired_target = $2, desired_target_set_at = NOW() WHERE id = $1")
                .bind(system_id)
                .bind(&path_a)
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query(
                "INSERT INTO pending_system_deployments
                 (system_id, target_store_path, source, expires_at,
                  requested_commit_id, requested_derivation_id)
                 VALUES ($1, $2, 'auto_desired_target', NOW() + INTERVAL '2 hours', $3, $4)",
            )
            .bind(system_id)
            .bind(&path_a)
            .bind(a_identity.0)
            .bind(a_identity.1)
            .execute(&pool)
            .await
            .unwrap();
            if mode == "report_only" {
                assert_ne!(assessment_digest, digest);
                report_digest = Some(digest);
            } else {
                assert_eq!(assessment_digest, digest);
                enforce_digest = Some(digest);
            }
            systems.push((mode, system_id));
        }
        assert_ne!(report_digest, enforce_digest);

        let manager = DeploymentPolicyManager::new(CrystalForgeConfig::default(), pool.clone());
        let stats = manager.update_auto_latest_policies().await.unwrap();
        assert_eq!((stats.systems_checked, stats.systems_updated), (2, 1));
        for (mode, system_id) in systems {
            let (desired, set_at, pending): (
                Option<String>,
                Option<chrono::DateTime<chrono::Utc>>,
                Option<String>,
            ) = sqlx::query_as(
                "SELECT desired_target, desired_target_set_at, \
                 (SELECT target_store_path FROM pending_system_deployments \
                  WHERE system_id = $1 AND status = 'pending') FROM systems WHERE id = $1",
            )
            .bind(system_id)
            .fetch_one(&pool)
            .await
            .unwrap();
            if mode == "report_only" {
                assert_eq!(desired.as_deref(), Some(path_b.as_str()));
                assert!(set_at.is_some());
                assert_eq!(pending.as_deref(), Some(path_b.as_str()));
            } else {
                assert_eq!(
                    desired.as_deref(),
                    Some(path_a.as_str()),
                    "enforce FAIL must retain A"
                );
                assert!(set_at.is_some());
                assert_eq!(pending.as_deref(), Some(path_a.as_str()));
            }
            let bound: Option<i32> = sqlx::query_scalar(
                "SELECT requested_derivation_id FROM pending_system_deployments
                 WHERE system_id = $1 AND status = 'pending'",
            )
            .bind(system_id)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(
                bound,
                Some(if mode == "report_only" {
                    b_derivation
                } else {
                    a_identity.1
                })
            );
        }
    }

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
    fn cve_check_violation_maps_to_block() {
        use crate::models::deployment_policies::CveCheckOutcome;
        let decision = map_cve_check_decision(CveGateResult {
            outcomes: vec![CveCheckOutcome {
                policy_description: "require_cve_check(max_critical=50)".into(),
                passed: false,
                blocking: true,
                reason: Some("248 critical CVE(s) found (max allowed: 50)".into()),
            }],
            deployment_allowed: false,
            block_reason: Some("248 critical CVE(s) found (max allowed: 50)".into()),
        });
        match decision {
            AdvancedGateDecision::Block(reason) => assert!(reason.contains("248")),
            _ => panic!("expected block decision"),
        }
    }

    #[test]
    fn cve_check_non_blocking_violation_maps_to_warn() {
        use crate::models::deployment_policies::CveCheckOutcome;
        let decision = map_cve_check_decision(CveGateResult {
            outcomes: vec![CveCheckOutcome {
                policy_description: "require_cve_check(max_critical=0)".into(),
                passed: false,
                blocking: false,
                reason: Some("3 critical CVE(s) found (max allowed: 0)".into()),
            }],
            deployment_allowed: true,
            block_reason: None,
        });
        match decision {
            AdvancedGateDecision::Warn(reason) => assert!(reason.contains("critical")),
            _ => panic!("expected warn decision, non-strict violations must not block"),
        }
    }

    #[test]
    fn cve_check_pass_maps_to_allow() {
        let decision = map_cve_check_decision(CveGateResult {
            outcomes: vec![],
            deployment_allowed: true,
            block_reason: None,
        });
        assert!(matches!(decision, AdvancedGateDecision::Allow));
    }

    /// Guards the TASK-437 correction: `require_cve_check` must be evaluated
    /// only through each system's resolved effective policy set (one
    /// `"require_cve_check"` match arm inside `evaluate_advanced_policy_gates`),
    /// never through a retired fleet-global `load_cve_policies` query. A
    /// globally `enabled` policy lineage is not automatically applicable to
    /// every system; applicability comes only from the effective assignment
    /// resolver.
    #[test]
    fn auto_latest_evaluates_require_cve_check_through_effective_policy_batch() {
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
            production_source.matches("load_cve_policies").count(),
            0,
            "the fleet-global require_cve_check loader must not be used by auto_latest"
        );
        assert_eq!(
            production_source
                .matches("\"require_cve_check\" =>")
                .count(),
            1,
            "require_cve_check must be evaluated exactly once, from the effective policy match"
        );
    }
}
