//! Authoritative effective-policy resolver.
//!
//! This module is the **single** implementation of assignment-based effective
//! policy resolution. Every caller (evaluation, deployment gates, compliance
//! rollups, assignment previews) must use this resolver — not reimplement the
//! ordering, exclusion, addition, or precedence rules.
//!
//! # Schema assumptions (2022_correctness_3.sql + 0198_assignments.sql)
//!
//! - `compliance_bundle_assignments` stores one row per (bundle_version_id, scope, scope_id).
//!   The CHECK constraint ensures scope_type='environment' ⟹ environment_id IS NOT NULL AND system_id IS NULL,
//!   and scope_type='system' ⟹ system_id IS NOT NULL AND environment_id IS NULL.
//! - `compliance_assignment_exclusions` and `compliance_assignment_additions` are
//!   simple join tables; PK is (assignment_id, policy_version_id).
//! - `compliance_assignment_value_overrides` uses (assignment_id, policy_version_id, value_path) unique key.
//! - All child tables fire `invalidate_overlay_on_child_change` → sets
//!   `assignment_overlay_digest = 'pending'`. Rust write paths must call
//!   `write_assignment_effective_set_digest` in the same transaction.
//! - `assignment_overlay_digest` is computed from `AssignmentEffectiveSetCanonical`
//!   (see digest.rs lines 150-197).
//!
//! # Effective set algorithm
//!
//! ```text
//! baseline_membership (ordered by policy_order from compliance_bundle_version_policies)
//!   - exclusions (remove by policy_version_id)
//!   + additions (append in provided order after baseline)
//!   → validate at most one version per policy lineage
//!   → apply value overrides to matching policies
//!   → return ordered EffectivePolicySet
//! ```
//!
//! # Report-only behavior
//!
//! `enforcement_mode = 'report_only'` applies to the **entire assignment**.
//! Every effective policy inherits it. Report-only policies:
//! - Are included in evaluation and compliance evidence.
//! - Are excluded from deployment configuration generation.
//! - Do not block deployment.
//!
//! # Environment vs. system precedence
//!
//! Environment assignments apply to every system in the environment.
//! System assignments apply to one specific system.
//! When resolving policies for a system:
//!   1. Load active environment assignments (where the system belongs).
//!   2. Load active system assignments (directly on the system).
//!   3. Combine all effective sets deterministically (sorted by assignment ID).
//!   4. Reject duplicate policy-lineage conflicts across assignments.
//!   5. Environment direct policy additions apply after bundle baselines.
//!   6. System direct policy additions apply after everything.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::compliance::digest::AssignmentEffectiveSetCanonical;

// ── Typed domain models ───────────────────────────────────────────────────────

/// The scope an assignment targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentTarget {
    Environment { environment_id: Uuid },
    System { system_id: Uuid },
}

impl AssignmentTarget {
    pub fn scope_type(&self) -> &'static str {
        match self {
            Self::Environment { .. } => "environment",
            Self::System { .. } => "system",
        }
    }
    pub fn scope_id(&self) -> Uuid {
        match self {
            Self::Environment { environment_id } => *environment_id,
            Self::System { system_id } => *system_id,
        }
    }
}

/// Assignment enforcement mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentMode {
    Enforce,
    ReportOnly,
}

impl AssignmentMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Enforce => "enforce",
            Self::ReportOnly => "report_only",
        }
    }
}

/// A single value override targeting one policy in the effective set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyOverride {
    pub policy_version_id: Uuid,
    /// JSON-path within the policy config that this override sets.
    pub value_path: String,
    pub value: serde_json::Value,
}

/// A fully-resolved policy in the effective set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectivePolicy {
    /// Exact immutable policy version ID.
    pub policy_version_id: Uuid,
    /// Portable policy lineage ID.
    pub policy_lineage_id: Uuid,
    /// Policy type (e.g. "custom_check", "require_cve_check").
    pub policy_type: String,
    /// Where this policy came from in the effective set.
    pub source: EffectivePolicySource,
    /// Bundle membership order, when baseline.
    pub baseline_order: Option<i32>,
    /// Addition order, when added via assignment.
    pub addition_order: Option<i32>,
    /// Overrides applied to this policy (in insertion order).
    pub overrides: Vec<PolicyOverride>,
    /// Effective runtime config after override application.
    /// For native policies this is the resolved config JSON; for others it is
    /// the preserved source config.
    pub effective_config: serde_json::Value,
    /// Assignment-level enforcement mode (inherited by every member).
    pub assignment_mode: AssignmentMode,
    /// Whether this specific policy is report-only (overrides assignment mode).
    /// Currently equals assignment_mode; reserved for future per-policy overrides.
    pub effective_mode: AssignmentMode,
}

/// Where a policy appeared in the effective set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectivePolicySource {
    /// From the bundle's ordered baseline membership.
    Baseline,
    /// Added via assignment overlay.
    Addition,
}

/// The complete resolved effective policy set for one assignment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectivePolicySet {
    /// Exact immutable bundle version this set was resolved from.
    pub bundle_version_id: Uuid,
    /// Assignment ID this set corresponds to (None for preview).
    pub assignment_id: Option<Uuid>,
    /// The scope the assignment targets.
    pub target: AssignmentTarget,
    /// Ordered effective policies.
    pub policies: Vec<EffectivePolicy>,
    /// Canonical digest of the effective set.
    pub effective_set_digest: String,
    /// Warnings generated during resolution.
    pub warnings: Vec<String>,
}

/// A conflict that prevents resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionConflict {
    pub code: String,
    pub message: String,
}

/// Result of resolving one assignment: either a complete set or a conflict.
#[derive(Debug)]
pub enum ResolutionOutcome {
    Resolved(EffectivePolicySet),
    Conflict(Vec<ResolutionConflict>),
}

// ── Input to the resolver ─────────────────────────────────────────────────────

/// All inputs needed to resolve one assignment's effective set.
#[derive(Debug)]
pub struct EffectivePolicyResolutionInput {
    pub target: AssignmentTarget,
    /// Exact immutable bundle version ID (must be in 'accepted' state).
    pub bundle_version_id: Uuid,
    /// Exact policy version IDs to exclude from the baseline.
    pub exclusions: Vec<Uuid>,
    /// Exact policy version IDs to add beyond the baseline.
    /// Order is significant: appended in provided order.
    pub additions: Vec<Uuid>,
    /// Value overrides targeting policies in the effective set.
    pub overrides: Vec<PolicyOverride>,
    /// Assignment-level enforcement mode.
    pub assignment_mode: AssignmentMode,
}

// ── Authoritative resolver ────────────────────────────────────────────────────

/// Resolve the effective policy set for one assignment.
///
/// This is the **single authoritative implementation** of the effective-policy
/// algorithm. All callers (evaluation, deployment, compliance rollups,
/// assignment previews) must use this function.
///
/// # Algorithm (must match module-level docs)
///
/// 1. Load bundle version + ordered membership in one query.
/// 2. Validate bundle is in 'accepted' state.
/// 3. Apply exclusions (remove by policy_version_id).
/// 4. Append additions in provided order after surviving baseline.
/// 5. Validate at most one version per policy lineage across the final set.
/// 6. Validate every override targets a policy in the final set.
/// 7. Apply overrides to produce effective configs.
/// 8. Compute effective-set digest using AssignmentEffectiveSetCanonical.
/// 9. Return ordered EffectivePolicySet.
///
/// # Errors
///
/// Returns `Err` for database failures. Returns `Ok(ResolutionOutcome::Conflict)`
/// for semantic conflicts (duplicate lineage, invalid bundle state, etc.).
pub async fn resolve_effective_policy_set(
    tx: &mut Transaction<'_, Postgres>,
    input: &EffectivePolicyResolutionInput,
) -> Result<ResolutionOutcome> {
    // ── Step 1: Load bundle version and ordered membership ────────────────────
    let bundle_row = sqlx::query_as::<_, (String, String)>(
        r#"SELECT publication_state, semantic_digest
           FROM compliance_bundle_versions
           WHERE id = $1"#,
    )
    .bind(input.bundle_version_id)
    .fetch_optional(&mut **tx)
    .await
    .context("load bundle version")?;

    let (bundle_state, bundle_digest) = match bundle_row {
        Some((state, digest)) => (state, digest),
        None => {
            return Ok(ResolutionOutcome::Conflict(vec![ResolutionConflict {
                code: "ASSIGNMENT_BUNDLE_NOT_FOUND".to_string(),
                message: format!(
                    "Bundle version {} does not exist",
                    input.bundle_version_id
                ),
            }]));
        }
    };

    if bundle_state != "accepted" {
        return Ok(ResolutionOutcome::Conflict(vec![ResolutionConflict {
            code: "ASSIGNMENT_BUNDLE_NOT_ACCEPTED".to_string(),
            message: format!(
                "Bundle version {} is in '{}' state; only 'accepted' versions can be assigned",
                input.bundle_version_id, bundle_state
            ),
        }]));
    }

    if bundle_digest == "pending" {
        return Ok(ResolutionOutcome::Conflict(vec![ResolutionConflict {
            code: "ASSIGNMENT_BUNDLE_DIGEST_PENDING".to_string(),
            message: format!(
                "Bundle version {} has pending digest; recompute before assignment",
                input.bundle_version_id
            ),
        }]));
    }

    // Load ordered baseline membership with policy lineage info
    let baseline_rows = sqlx::query_as::<_, (Uuid, Uuid, String, String, String, serde_json::Value)>(
        r#"SELECT cbvp.policy_version_id,
                  pv.policy_id,
                  pv.policy_type,
                  pv.publication_state,
                  pv.semantic_digest,
                  pv.config
           FROM compliance_bundle_version_policies cbvp
           JOIN deployment_policy_versions pv ON pv.id = cbvp.policy_version_id
           WHERE cbvp.bundle_version_id = $1
           ORDER BY cbvp.policy_order"#,
    )
    .bind(input.bundle_version_id)
    .fetch_all(&mut **tx)
    .await
    .context("load baseline membership")?;

    // Validate every baseline policy is accepted
    for (pv_id, _, _, pv_state, _, _) in &baseline_rows {
        if pv_state != "accepted" && pv_state != "deprecated" {
            return Ok(ResolutionOutcome::Conflict(vec![ResolutionConflict {
                code: "ASSIGNMENT_POLICY_NOT_ACCEPTED".to_string(),
                message: format!(
                    "Baseline policy version {} is in '{}' state; cannot assign bundle with draft member",
                    pv_id, pv_state
                ),
            }]));
        }
    }

    // ── Step 2: Validate exclusions ───────────────────────────────────────────
    let baseline_version_ids: std::collections::HashSet<Uuid> =
        baseline_rows.iter().map(|(id, _, _, _, _, _)| *id).collect();

    for excl in &input.exclusions {
        if !baseline_version_ids.contains(excl) {
            return Ok(ResolutionOutcome::Conflict(vec![ResolutionConflict {
                code: "ASSIGNMENT_EXCLUSION_NOT_IN_BUNDLE".to_string(),
                message: format!(
                    "Exclusion {} is not a member of bundle version {}",
                    excl, input.bundle_version_id
                ),
            }]));
        }
    }

    let exclusions_set: std::collections::HashSet<Uuid> =
        input.exclusions.iter().copied().collect();

    // ── Step 3: Build surviving baseline after exclusions ─────────────────────
    let mut effective: Vec<EffectivePolicy> = baseline_rows
        .iter()
        .enumerate()
        .filter(|(_, (pv_id, _, _, _, _, _))| !exclusions_set.contains(pv_id))
        .map(|(idx, (pv_id, lin_id, ptype, _, _, config))| EffectivePolicy {
            policy_version_id: *pv_id,
            policy_lineage_id: *lin_id,
            policy_type: ptype.clone(),
            source: EffectivePolicySource::Baseline,
            baseline_order: Some(idx as i32),
            addition_order: None,
            overrides: Vec::new(),
            effective_config: config.clone(),
            assignment_mode: input.assignment_mode.clone(),
            effective_mode: input.assignment_mode.clone(),
        })
        .collect();

    // ── Step 4: Validate additions and append ─────────────────────────────────
    let mut seen_lineages: std::collections::HashMap<Uuid, Uuid> = effective
        .iter()
        .map(|p| (p.policy_lineage_id, p.policy_version_id))
        .collect();

    for (add_order, add_id) in input.additions.iter().enumerate() {
        // Reject additions that are also excluded
        if exclusions_set.contains(add_id) {
            return Ok(ResolutionOutcome::Conflict(vec![ResolutionConflict {
                code: "ASSIGNMENT_ADDITION_EXCLUDED".to_string(),
                message: format!(
                    "Policy version {} is both excluded and added; ambiguous",
                    add_id
                ),
            }]));
        }

        // Reject additions that duplicate a baseline version
        if baseline_version_ids.contains(add_id) {
            return Ok(ResolutionOutcome::Conflict(vec![ResolutionConflict {
                code: "ASSIGNMENT_ADDITION_DUPLICATE".to_string(),
                message: format!(
                    "Policy version {} is already in the baseline; use an override instead",
                    add_id
                ),
            }]));
        }

        // Load the addition's policy version
        let add_row = sqlx::query_as::<_, (Uuid, String, String, String, serde_json::Value)>(
            r#"SELECT policy_id, policy_type, publication_state, semantic_digest, config
               FROM deployment_policy_versions
               WHERE id = $1"#,
        )
        .bind(add_id)
        .fetch_optional(&mut **tx)
        .await
        .context("load addition policy version")?;

        let (add_lineage, add_type, add_state, add_digest, add_config) = match add_row {
            Some(row) => row,
            None => {
                return Ok(ResolutionOutcome::Conflict(vec![ResolutionConflict {
                    code: "ASSIGNMENT_POLICY_NOT_ACCEPTED".to_string(),
                    message: format!("Addition policy version {} does not exist", add_id),
                }]));
            }
        };

        if add_state != "accepted" {
            return Ok(ResolutionOutcome::Conflict(vec![ResolutionConflict {
                code: "ASSIGNMENT_POLICY_NOT_ACCEPTED".to_string(),
                message: format!(
                    "Addition policy version {} is in '{}' state; must be 'accepted'",
                    add_id, add_state
                ),
            }]));
        }

        // Reject duplicate lineage across the effective set
        if let Some(existing_vid) = seen_lineages.get(&add_lineage) {
            return Ok(ResolutionOutcome::Conflict(vec![ResolutionConflict {
                code: "EFFECTIVE_POLICY_VERSION_CONFLICT".to_string(),
                message: format!(
                    "Addition {} (lineage {}) conflicts with already-effective version {} of the same lineage",
                    add_id, add_lineage, existing_vid
                ),
            }]));
        }

        seen_lineages.insert(add_lineage, *add_id);
        effective.push(EffectivePolicy {
            policy_version_id: *add_id,
            policy_lineage_id: add_lineage,
            policy_type: add_type,
            source: EffectivePolicySource::Addition,
            baseline_order: None,
            addition_order: Some(add_order as i32),
            overrides: Vec::new(),
            effective_config: add_config,
            assignment_mode: input.assignment_mode.clone(),
            effective_mode: input.assignment_mode.clone(),
        });
    }

    // ── Step 5: Validate and apply overrides ──────────────────────────────────
    let effective_version_ids: std::collections::HashSet<Uuid> =
        effective.iter().map(|p| p.policy_version_id).collect();

    for ovr in &input.overrides {
        if exclusions_set.contains(&ovr.policy_version_id) {
            return Ok(ResolutionOutcome::Conflict(vec![ResolutionConflict {
                code: "ASSIGNMENT_OVERRIDE_TARGET_MISSING".to_string(),
                message: format!(
                    "Override targets excluded policy version {}",
                    ovr.policy_version_id
                ),
            }]));
        }
        if !effective_version_ids.contains(&ovr.policy_version_id) {
            return Ok(ResolutionOutcome::Conflict(vec![ResolutionConflict {
                code: "ASSIGNMENT_OVERRIDE_TARGET_MISSING".to_string(),
                message: format!(
                    "Override targets policy version {} which is not in the effective set",
                    ovr.policy_version_id
                ),
            }]));
        }

        // Apply the override to the matching effective policy's config
        for pol in effective.iter_mut() {
            if pol.policy_version_id == ovr.policy_version_id {
                pol.overrides.push(ovr.clone());
                // Apply the override to the effective config
                if let Err(e) = apply_json_path_override(
                    &mut pol.effective_config,
                    &ovr.value_path,
                    &ovr.value,
                ) {
                    return Ok(ResolutionOutcome::Conflict(vec![ResolutionConflict {
                        code: "ASSIGNMENT_OVERRIDE_VALUE_INVALID".to_string(),
                        message: format!(
                            "Override at path '{}' on policy {} failed: {}",
                            ovr.value_path, ovr.policy_version_id, e
                        ),
                    }]));
                }
            }
        }
    }

    // ── Step 6: Compute effective-set digest ──────────────────────────────────
    let effective_pids: Vec<Uuid> = effective.iter().map(|p| p.policy_version_id).collect();
    let canonical = AssignmentEffectiveSetCanonical {
        enforcement_mode: input.assignment_mode.as_str().to_string(),
        exclusions: input.exclusions.clone(),
        additions: input.additions.clone(),
        value_overrides: input
            .overrides
            .iter()
            .map(|o| (o.policy_version_id, o.value_path.clone(), o.value.clone()))
            .collect(),
        effective_policy_version_ids: effective_pids.clone(),
    };
    let effective_set_digest = canonical.compute_digest();

    Ok(ResolutionOutcome::Resolved(EffectivePolicySet {
        bundle_version_id: input.bundle_version_id,
        assignment_id: None,
        target: input.target.clone(),
        policies: effective,
        effective_set_digest,
        warnings: Vec::new(),
    }))
}

/// Apply a JSON-path override to a config value.
///
/// Path syntax: dot-separated keys, e.g. `"rules.0.expression"`.
/// Array indices are supported as numeric path segments.
/// The path must already exist (override replaces, does not create).
fn apply_json_path_override(
    config: &mut serde_json::Value,
    path: &str,
    value: &serde_json::Value,
) -> Result<()> {
    let segments: Vec<&str> = path.split('.').collect();
    if segments.is_empty() {
        bail!("Empty override path");
    }
    let mut current = config;
    for (i, seg) in segments.iter().enumerate() {
        if i == segments.len() - 1 {
            // Last segment: replace value
            match current {
                serde_json::Value::Object(map) => {
                    if !map.contains_key(*seg) {
                        bail!("Path segment '{}' does not exist in config object", seg);
                    }
                    map.insert(seg.to_string(), value.clone());
                }
                serde_json::Value::Array(arr) => {
                    let idx: usize = seg
                        .parse()
                        .with_context(|| format!("Array index '{}' is not a number", seg))?;
                    if idx >= arr.len() {
                        bail!("Array index {} out of bounds (len {})", idx, arr.len());
                    }
                    arr[idx] = value.clone();
                }
                _ => bail!(
                    "Cannot override path '{}': current node is not an object or array",
                    path
                ),
            }
        } else {
            // Intermediate segment: navigate deeper
            current = match current {
                serde_json::Value::Object(map) => map
                    .get_mut(*seg)
                    .with_context(|| format!("Path segment '{}' does not exist", seg))?,
                serde_json::Value::Array(arr) => {
                    let idx: usize = seg
                        .parse()
                        .with_context(|| format!("Array index '{}' is not a number", seg))?;
                    arr.get_mut(idx)
                        .with_context(|| format!("Array index {} out of bounds", idx))?
                }
                _ => bail!(
                    "Cannot navigate to path '{}': current node is a scalar",
                    path
                ),
            };
        }
    }
    Ok(())
}

// ── System-level combined resolution ─────────────────────────────────────────

/// Resolve all effective policies for a system by combining:
///   - Environment bundle assignments
///   - System bundle assignments
///   - Direct environment policy additions
///   - Direct system policy additions
///
/// Returns one combined EffectivePolicySet covering all active assignments.
/// Conflicts between assignments (duplicate policy lineages) are returned as
/// typed ResolutionConflicts.
pub async fn resolve_system_effective_policies(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<ResolutionOutcome> {
    // Load the system's environment
    let env_id: Option<Option<Uuid>> = sqlx::query_scalar(
        "SELECT environment_id FROM systems WHERE id = $1",
    )
    .bind(system_id)
    .fetch_optional(pool)
    .await
    .context("load system environment")?;

    let env_id = env_id.flatten();

    // Load all active bundle assignments for this system's scope:
    // - environment assignments where system belongs to that environment
    // - system assignments directly on this system
    let assignments = sqlx::query_as::<
        _,
        (
            Uuid,       // assignment_id
            Uuid,       // bundle_version_id
            String,     // scope_type
            String,     // enforcement_mode
            String,     // overlay_digest
        ),
    >(
        r#"SELECT id, bundle_version_id, scope_type, enforcement_mode, assignment_overlay_digest
           FROM compliance_bundle_assignments
           WHERE (scope_type = 'environment' AND environment_id = $2)
              OR (scope_type = 'system' AND system_id = $1)
           ORDER BY id"#,
    )
    .bind(system_id)
    .bind(env_id)
    .fetch_all(pool)
    .await
    .context("load bundle assignments for system")?;

    if assignments.is_empty() {
        // No assignments — fall back to legacy direct-policy resolution
        return resolve_legacy_system_policies(pool, system_id, env_id).await;
    }

    let mut all_policies: Vec<EffectivePolicy> = Vec::new();
    let mut seen_lineages: std::collections::HashMap<Uuid, Uuid> =
        std::collections::HashMap::new();
    let mut all_warnings: Vec<String> = Vec::new();
    let mut primary_bundle_version_id: Option<Uuid> = None;

    let mut tx = pool.begin().await.context("begin resolution transaction")?;

    for (assignment_id, bundle_version_id, scope_type, enforcement_mode, _) in &assignments {
        // Load exclusions and additions for this assignment
        let exclusions: Vec<Uuid> = sqlx::query_scalar(
            "SELECT policy_version_id FROM compliance_assignment_exclusions WHERE assignment_id = $1",
        )
        .bind(assignment_id)
        .fetch_all(&mut *tx)
        .await
        .context("load assignment exclusions")?;

        let additions: Vec<Uuid> = sqlx::query_scalar(
            "SELECT policy_version_id FROM compliance_assignment_additions WHERE assignment_id = $1 ORDER BY policy_version_id",
        )
        .bind(assignment_id)
        .fetch_all(&mut *tx)
        .await
        .context("load assignment additions")?;

        let overrides: Vec<PolicyOverride> = sqlx::query_as::<_, (Uuid, String, serde_json::Value)>(
            "SELECT policy_version_id, value_path, value FROM compliance_assignment_value_overrides WHERE assignment_id = $1",
        )
        .bind(assignment_id)
        .fetch_all(&mut *tx)
        .await
        .context("load assignment overrides")?
        .into_iter()
        .map(|(pvid, path, val)| PolicyOverride {
            policy_version_id: pvid,
            value_path: path,
            value: val,
        })
        .collect();

        let mode = if enforcement_mode == "report_only" {
            AssignmentMode::ReportOnly
        } else {
            AssignmentMode::Enforce
        };

        let target = if scope_type == "environment" {
            AssignmentTarget::Environment {
                environment_id: env_id.unwrap_or_default(),
            }
        } else {
            AssignmentTarget::System { system_id }
        };

        let input = EffectivePolicyResolutionInput {
            target,
            bundle_version_id: *bundle_version_id,
            exclusions,
            additions,
            overrides,
            assignment_mode: mode,
        };

        let outcome = resolve_effective_policy_set(&mut tx, &input).await?;

        match outcome {
            ResolutionOutcome::Resolved(set) => {
                // Check for duplicate lineages across assignments
                for pol in &set.policies {
                    if let Some(existing_vid) = seen_lineages.get(&pol.policy_lineage_id) {
                        let _ = tx.rollback().await;
                        return Ok(ResolutionOutcome::Conflict(vec![ResolutionConflict {
                            code: "EFFECTIVE_POLICY_VERSION_CONFLICT".to_string(),
                            message: format!(
                                "Policy lineage {} has conflicting versions across assignments: {} vs {}",
                                pol.policy_lineage_id, existing_vid, pol.policy_version_id
                            ),
                        }]));
                    }
                    seen_lineages.insert(pol.policy_lineage_id, pol.policy_version_id);
                }

                if primary_bundle_version_id.is_none() {
                    primary_bundle_version_id = Some(*bundle_version_id);
                }
                all_policies.extend(set.policies);
                all_warnings.extend(set.warnings);
            }
            ResolutionOutcome::Conflict(conflicts) => {
                let _ = tx.rollback().await;
                return Ok(ResolutionOutcome::Conflict(conflicts));
            }
        }
    }

    // Also add direct environment and system policies (legacy compatibility)
    if let Some(eid) = env_id {
        let env_direct: Vec<(Uuid, Uuid, String, serde_json::Value)> = sqlx::query_as(
            r#"SELECT pv.id, pv.policy_id, pv.policy_type, pv.config
               FROM environment_policies ep
               JOIN deployment_policies dp ON dp.id = ep.policy_id
               JOIN deployment_policy_versions pv ON pv.id = dp.current_published_version_id
               WHERE ep.environment_id = $1
                 AND pv.publication_state = 'accepted'"#,
        )
        .bind(eid)
        .fetch_all(&mut *tx)
        .await
        .context("load direct environment policies")?;

        for (pv_id, lin_id, ptype, config) in env_direct {
            if !seen_lineages.contains_key(&lin_id) {
                seen_lineages.insert(lin_id, pv_id);
                all_warnings.push(format!(
                    "Legacy direct environment policy {} (lineage {}) included; migrate to bundle assignment",
                    pv_id, lin_id
                ));
                all_policies.push(EffectivePolicy {
                    policy_version_id: pv_id,
                    policy_lineage_id: lin_id,
                    policy_type: ptype,
                    source: EffectivePolicySource::Addition,
                    baseline_order: None,
                    addition_order: None,
                    overrides: Vec::new(),
                    effective_config: config,
                    assignment_mode: AssignmentMode::Enforce,
                    effective_mode: AssignmentMode::Enforce,
                });
            }
        }
    }

    // System direct policies (highest specificity)
    let sys_direct: Vec<(Uuid, Uuid, String, serde_json::Value)> = sqlx::query_as(
        r#"SELECT pv.id, pv.policy_id, pv.policy_type, pv.config
           FROM system_policies sp
           JOIN deployment_policies dp ON dp.id = sp.policy_id
           JOIN deployment_policy_versions pv ON pv.id = dp.current_published_version_id
           WHERE sp.system_id = $1
             AND pv.publication_state = 'accepted'"#,
    )
    .bind(system_id)
    .fetch_all(&mut *tx)
    .await
    .context("load direct system policies")?;

    for (pv_id, lin_id, ptype, config) in sys_direct {
        // System direct overrides any earlier version of the same lineage
        if let Some(existing_vid) = seen_lineages.get(&lin_id) {
            // Replace the earlier entry with the system-level version
            all_policies.retain(|p| p.policy_lineage_id != lin_id);
            all_warnings.push(format!(
                "System direct policy {} (lineage {}) replaces version {} from bundle/environment scope",
                pv_id, lin_id, existing_vid
            ));
        }
        seen_lineages.insert(lin_id, pv_id);
        all_policies.push(EffectivePolicy {
            policy_version_id: pv_id,
            policy_lineage_id: lin_id,
            policy_type: ptype,
            source: EffectivePolicySource::Addition,
            baseline_order: None,
            addition_order: None,
            overrides: Vec::new(),
            effective_config: config,
            assignment_mode: AssignmentMode::Enforce,
            effective_mode: AssignmentMode::Enforce,
        });
    }

    tx.commit().await.context("commit resolution transaction")?;

    // Compute combined digest
    let effective_pids: Vec<Uuid> = all_policies.iter().map(|p| p.policy_version_id).collect();
    let canonical = AssignmentEffectiveSetCanonical {
        enforcement_mode: "enforce".to_string(), // Combined scope; individual policies have their own mode
        exclusions: vec![],
        additions: vec![],
        value_overrides: vec![],
        effective_policy_version_ids: effective_pids.clone(),
    };

    let target = AssignmentTarget::System { system_id };
    Ok(ResolutionOutcome::Resolved(EffectivePolicySet {
        bundle_version_id: primary_bundle_version_id.unwrap_or_default(),
        assignment_id: None,
        target,
        policies: all_policies,
        effective_set_digest: canonical.compute_digest(),
        warnings: all_warnings,
    }))
}

/// Legacy fallback when no bundle assignments exist.
/// Preserves the current behavior for installations with no compliance bundles.
async fn resolve_legacy_system_policies(
    pool: &PgPool,
    system_id: Uuid,
    env_id: Option<Uuid>,
) -> Result<ResolutionOutcome> {
    let mut policies: Vec<EffectivePolicy> = Vec::new();
    let mut seen_lineages: std::collections::HashSet<Uuid> = std::collections::HashSet::new();

    if let Some(eid) = env_id {
        let env_direct: Vec<(Uuid, Uuid, String, serde_json::Value)> = sqlx::query_as(
            r#"SELECT pv.id, pv.policy_id, pv.policy_type, pv.config
               FROM environment_policies ep
               JOIN deployment_policies dp ON dp.id = ep.policy_id
               JOIN deployment_policy_versions pv ON pv.id = dp.current_published_version_id
               WHERE ep.environment_id = $1
                 AND pv.publication_state = 'accepted'"#,
        )
        .bind(eid)
        .fetch_all(pool)
        .await
        .context("load legacy environment policies")?;

        for (pv_id, lin_id, ptype, config) in env_direct {
            if seen_lineages.insert(lin_id) {
                policies.push(EffectivePolicy {
                    policy_version_id: pv_id,
                    policy_lineage_id: lin_id,
                    policy_type: ptype,
                    source: EffectivePolicySource::Addition,
                    baseline_order: None,
                    addition_order: None,
                    overrides: Vec::new(),
                    effective_config: config,
                    assignment_mode: AssignmentMode::Enforce,
                    effective_mode: AssignmentMode::Enforce,
                });
            }
        }
    }

    let sys_direct: Vec<(Uuid, Uuid, String, serde_json::Value)> = sqlx::query_as(
        r#"SELECT pv.id, pv.policy_id, pv.policy_type, pv.config
           FROM system_policies sp
           JOIN deployment_policies dp ON dp.id = sp.policy_id
           JOIN deployment_policy_versions pv ON pv.id = dp.current_published_version_id
           WHERE sp.system_id = $1
             AND pv.publication_state = 'accepted'"#,
    )
    .bind(system_id)
    .fetch_all(pool)
    .await
    .context("load legacy system policies")?;

    for (pv_id, lin_id, ptype, config) in sys_direct {
        if seen_lineages.insert(lin_id) {
            policies.push(EffectivePolicy {
                policy_version_id: pv_id,
                policy_lineage_id: lin_id,
                policy_type: ptype,
                source: EffectivePolicySource::Addition,
                baseline_order: None,
                addition_order: None,
                overrides: Vec::new(),
                effective_config: config,
                assignment_mode: AssignmentMode::Enforce,
                effective_mode: AssignmentMode::Enforce,
            });
        }
    }

    let effective_pids: Vec<Uuid> = policies.iter().map(|p| p.policy_version_id).collect();
    let canonical = AssignmentEffectiveSetCanonical {
        enforcement_mode: "enforce".to_string(),
        exclusions: vec![],
        additions: vec![],
        value_overrides: vec![],
        effective_policy_version_ids: effective_pids.clone(),
    };

    Ok(ResolutionOutcome::Resolved(EffectivePolicySet {
        bundle_version_id: Uuid::nil(),
        assignment_id: None,
        target: AssignmentTarget::System { system_id },
        policies,
        effective_set_digest: canonical.compute_digest(),
        warnings: vec!["Legacy direct-policy resolution used (no bundle assignments)".to_string()],
    }))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_input(
        exclusions: Vec<Uuid>,
        additions: Vec<Uuid>,
        overrides: Vec<PolicyOverride>,
        mode: AssignmentMode,
    ) -> EffectivePolicyResolutionInput {
        EffectivePolicyResolutionInput {
            target: AssignmentTarget::Environment {
                environment_id: Uuid::new_v4(),
            },
            bundle_version_id: Uuid::new_v4(),
            exclusions,
            additions,
            overrides,
            assignment_mode: mode,
        }
    }

    #[test]
    fn digest_changes_when_mode_changes() {
        let ids = vec![Uuid::new_v4(), Uuid::new_v4()];
        let canonical_enforce = AssignmentEffectiveSetCanonical {
            enforcement_mode: "enforce".to_string(),
            exclusions: vec![],
            additions: vec![],
            value_overrides: vec![],
            effective_policy_version_ids: ids.clone(),
        };
        let canonical_report = AssignmentEffectiveSetCanonical {
            enforcement_mode: "report_only".to_string(),
            exclusions: vec![],
            additions: vec![],
            value_overrides: vec![],
            effective_policy_version_ids: ids,
        };
        assert_ne!(
            canonical_enforce.compute_digest(),
            canonical_report.compute_digest(),
            "mode change must change digest"
        );
    }

    #[test]
    fn digest_changes_when_policy_order_changes() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let canonical_a = AssignmentEffectiveSetCanonical {
            enforcement_mode: "enforce".to_string(),
            exclusions: vec![],
            additions: vec![],
            value_overrides: vec![],
            effective_policy_version_ids: vec![id1, id2],
        };
        let canonical_b = AssignmentEffectiveSetCanonical {
            enforcement_mode: "enforce".to_string(),
            exclusions: vec![],
            additions: vec![],
            value_overrides: vec![],
            effective_policy_version_ids: vec![id2, id1],
        };
        assert_ne!(
            canonical_a.compute_digest(),
            canonical_b.compute_digest(),
            "policy order must change digest"
        );
    }

    #[test]
    fn digest_changes_when_override_changes() {
        let pv_id = Uuid::new_v4();
        let canonical_a = AssignmentEffectiveSetCanonical {
            enforcement_mode: "enforce".to_string(),
            exclusions: vec![],
            additions: vec![],
            value_overrides: vec![(
                pv_id,
                "rules.0.expression".to_string(),
                serde_json::json!("cfg.config.services.nginx.enable"),
            )],
            effective_policy_version_ids: vec![pv_id],
        };
        let canonical_b = AssignmentEffectiveSetCanonical {
            enforcement_mode: "enforce".to_string(),
            exclusions: vec![],
            additions: vec![],
            value_overrides: vec![(
                pv_id,
                "rules.0.expression".to_string(),
                serde_json::json!("cfg.config.services.apache.enable"),
            )],
            effective_policy_version_ids: vec![pv_id],
        };
        assert_ne!(
            canonical_a.compute_digest(),
            canonical_b.compute_digest(),
            "override change must change digest"
        );
    }

    #[test]
    fn digest_ignores_local_only_metadata() {
        let pv_id = Uuid::new_v4();
        // Two canonicals with same semantic fields produce the same digest
        let canonical_a = AssignmentEffectiveSetCanonical {
            enforcement_mode: "enforce".to_string(),
            exclusions: vec![],
            additions: vec![],
            value_overrides: vec![],
            effective_policy_version_ids: vec![pv_id],
        };
        let canonical_b = AssignmentEffectiveSetCanonical {
            enforcement_mode: "enforce".to_string(),
            exclusions: vec![],
            additions: vec![],
            value_overrides: vec![],
            effective_policy_version_ids: vec![pv_id],
        };
        assert_eq!(
            canonical_a.compute_digest(),
            canonical_b.compute_digest(),
            "identical semantic fields must produce identical digest"
        );
    }

    #[test]
    fn apply_json_path_override_replaces_scalar() {
        let mut config = serde_json::json!({
            "rules": [
                { "expression": "cfg.config.old" }
            ]
        });
        let result = apply_json_path_override(
            &mut config,
            "rules.0.expression",
            &serde_json::json!("cfg.config.new"),
        );
        assert!(result.is_ok(), "override must succeed");
        assert_eq!(
            config["rules"][0]["expression"],
            serde_json::json!("cfg.config.new")
        );
    }

    #[test]
    fn apply_json_path_override_rejects_missing_path() {
        let mut config = serde_json::json!({"rules": []});
        let result = apply_json_path_override(
            &mut config,
            "rules.0.expression",
            &serde_json::json!("cfg.config.new"),
        );
        assert!(result.is_err(), "override of missing path must fail");
    }

    #[test]
    fn apply_json_path_override_rejects_scalar_parent() {
        let mut config = serde_json::json!("scalar");
        let result = apply_json_path_override(
            &mut config,
            "some.path",
            &serde_json::json!(42),
        );
        assert!(result.is_err(), "override on scalar root must fail");
    }

    #[test]
    fn apply_json_path_override_rejects_out_of_bounds_array() {
        let mut config = serde_json::json!({"items": [1, 2]});
        let result = apply_json_path_override(
            &mut config,
            "items.5",
            &serde_json::json!(99),
        );
        assert!(result.is_err(), "out-of-bounds array override must fail");
    }
}
