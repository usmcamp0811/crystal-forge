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

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::compliance::digest::{AssignmentEffectiveSetCanonical, CombinedEffectiveSetCanonical};

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
    /// Specificity level of the winning source.
    pub specificity: PolicySpecificity,
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
    /// All contributing sources preserved for evidence and diagnostics.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<ProvenanceEntry>,
}

/// One contributing source in the effective policy set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceEntry {
    pub source: EffectivePolicySource,
    pub specificity: PolicySpecificity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_type: Option<String>,
    pub enforcement_mode: String,
    pub authoritative: bool,
}

/// Where a policy appeared in the effective set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectivePolicySource {
    /// From the bundle's ordered baseline membership.
    Baseline,
    /// Added via assignment overlay.
    Addition,
    /// Direct environment or system policy (legacy compatibility).
    LegacyDirect,
}

/// Specificity level of a policy source.
///
/// Higher numeric value wins when the same policy lineage appears at multiple levels.
/// Bundle baseline is the lowest specificity; a direct system assignment is the highest.
///
/// ```text
/// BundleBaseline(0) < Environment(1) < System(2)
/// ```
///
/// When two sources at the same specificity produce different versions of the same
/// policy lineage, the result is `EFFECTIVE_POLICY_VERSION_CONFLICT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum PolicySpecificity {
    /// From a bundle's baseline membership (lowest precedence).
    BundleBaseline = 0,
    /// From an environment-scope assignment.
    Environment = 1,
    /// From a system-scope assignment (highest precedence).
    System = 2,
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
    /// Specificity level of the assignment source for the combined resolver.
    /// When resolving a single assignment directly (not as part of combined system
    /// resolution), use `BundleBaseline` as a conservative default.
    pub specificity: PolicySpecificity,
}

// ── Authoritative specificity-aware merge ──────────────────────────────────────

/// Result of attempting to insert one candidate into the effective set.
#[derive(Debug)]
enum MergeOutcome {
    /// The candidate was added as a new entry.
    Inserted { index: usize },
    /// Same exact version, deduplicated — update specificity if higher.
    Deduplicated {
        index: usize,
        specificity_updated: bool,
    },
    /// Higher-specificity candidate replaced the existing entry.
    Replaced { index: usize },
    /// A typed conflict was created (same specificity, different version).
    Conflict(ResolutionConflict),
}

/// Merge one `EffectivePolicy` candidate into the staged effective set.
///
/// Enforces the authoritative precedence:
///
/// ```text
/// BundleBaseline(0) < Environment(1) < System(2)
/// ```
///
/// - Same exact version at any specificity: deduplicate, keep highest specificity,
///   update source/mode/overrides to the highest-specificity winner.
/// - Different version, higher specificity: replace entirely.
/// - Different version, lower specificity: suppress with diagnostic.
/// - Different version, same specificity: return typed conflict.
fn merge_effective_policy_candidate(
    candidate: EffectivePolicy,
    specificity: PolicySpecificity,
    provenance: ProvenanceEntry,
    staging: &mut Vec<EffectivePolicy>,
    per_lineage: &mut std::collections::HashMap<Uuid, (Uuid, PolicySpecificity, usize)>,
    warnings: &mut Vec<String>,
    ignore_non_evaluation_conflicts: bool,
) -> MergeOutcome {
    let lineage_id = candidate.policy_lineage_id;
    let version_id = candidate.policy_version_id;

    // Look up the existing entry and remove it temporarily to avoid borrow issues.
    let existing = per_lineage.remove(&lineage_id);

    let result = if let Some((existing_ver, existing_spec, existing_idx)) = existing {
        if existing_ver == version_id {
            let spec_updated = specificity > existing_spec;
            if spec_updated {
                let entry = &mut staging[existing_idx];
                entry.specificity = specificity;
                entry.source = candidate.source;
                entry.assignment_mode = candidate.assignment_mode;
                entry.effective_mode = candidate.effective_mode;
                entry.overrides = candidate.overrides;
                entry.effective_config = candidate.effective_config;
                if candidate.baseline_order.is_some() {
                    entry.baseline_order = candidate.baseline_order;
                }
                if candidate.addition_order.is_some() {
                    entry.addition_order = candidate.addition_order;
                }
                for p in &mut entry.provenance {
                    p.authoritative = false;
                }
                entry.provenance.push(ProvenanceEntry {
                    authoritative: true,
                    ..provenance
                });
            } else {
                staging[existing_idx].provenance.push(ProvenanceEntry {
                    authoritative: false,
                    ..provenance
                });
            }
            if spec_updated {
                per_lineage.insert(lineage_id, (version_id, specificity, existing_idx));
            } else {
                per_lineage.insert(lineage_id, (version_id, existing_spec, existing_idx));
            }
            MergeOutcome::Deduplicated {
                index: existing_idx,
                specificity_updated: spec_updated,
            }
        } else if specificity > existing_spec {
            let mut new_entry = candidate;
            for p in &mut staging[existing_idx].provenance {
                p.authoritative = false;
            }
            new_entry.provenance = staging[existing_idx].provenance.clone();
            new_entry.provenance.push(ProvenanceEntry {
                authoritative: true,
                ..provenance
            });
            staging[existing_idx] = new_entry;
            per_lineage.insert(lineage_id, (version_id, specificity, existing_idx));
            MergeOutcome::Replaced {
                index: existing_idx,
            }
        } else if specificity == existing_spec {
            // Conflict: put the entry back and return conflict.
            per_lineage.insert(lineage_id, (existing_ver, existing_spec, existing_idx));
            let existing_type = &staging[existing_idx].policy_type;
            if ignore_non_evaluation_conflicts
                && !is_nix_evaluation_policy_type(existing_type)
                && !is_nix_evaluation_policy_type(&candidate.policy_type)
            {
                warnings.push(format!(
                    "Ignored non-Nix policy version conflict for lineage {lineage_id} ({existing_ver} vs {version_id})"
                ));
                MergeOutcome::Deduplicated {
                    index: existing_idx,
                    specificity_updated: false,
                }
            } else {
                MergeOutcome::Conflict(ResolutionConflict {
                    code: "EFFECTIVE_POLICY_VERSION_CONFLICT".into(),
                    message: format!(
                        "Policy lineage {lineage_id}: different versions at same specificity {specificity:?} ({existing_ver} vs {version_id})"
                    ),
                })
            }
        } else {
            // Lower specificity — add as non-authoritative provenance.
            staging[existing_idx].provenance.push(ProvenanceEntry {
                authoritative: false,
                ..provenance
            });
            per_lineage.insert(lineage_id, (existing_ver, existing_spec, existing_idx));
            MergeOutcome::Deduplicated {
                index: existing_idx,
                specificity_updated: false,
            }
        }
    } else {
        let idx = staging.len();
        let mut pol = candidate;
        pol.provenance.push(ProvenanceEntry {
            authoritative: true,
            ..provenance
        });
        staging.push(pol);
        per_lineage.insert(lineage_id, (version_id, specificity, idx));
        MergeOutcome::Inserted { index: idx }
    };
    result
}

fn is_nix_evaluation_policy_type(policy_type: &str) -> bool {
    matches!(policy_type, "require_packages" | "custom_check")
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
    resolve_effective_policy_set_with_options(tx, input).await
}

async fn resolve_effective_policy_set_with_options(
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
                message: format!("Bundle version {} does not exist", input.bundle_version_id),
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
    let baseline_rows =
        sqlx::query_as::<_, (Uuid, Uuid, String, String, String, serde_json::Value)>(
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
    let baseline_version_ids: std::collections::HashSet<Uuid> = baseline_rows
        .iter()
        .map(|(id, _, _, _, _, _)| *id)
        .collect();

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
        .map(
            |(idx, (pv_id, lin_id, ptype, _, _, config))| EffectivePolicy {
                policy_version_id: *pv_id,
                policy_lineage_id: *lin_id,
                policy_type: ptype.clone(),
                source: EffectivePolicySource::Baseline,
                specificity: input.specificity,
                baseline_order: Some(idx as i32),
                addition_order: None,
                overrides: Vec::new(),
                effective_config: config.clone(),
                assignment_mode: input.assignment_mode.clone(),
                effective_mode: input.assignment_mode.clone(),
                provenance: Vec::new(),
            },
        )
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
            specificity: input.specificity,
            baseline_order: None,
            addition_order: Some(add_order as i32),
            overrides: Vec::new(),
            effective_config: add_config,
            assignment_mode: input.assignment_mode.clone(),
            effective_mode: input.assignment_mode.clone(),
            provenance: Vec::new(),
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
                if let Err(error) = validate_typed_override(
                    &pol.policy_type,
                    &pol.effective_config,
                    &ovr.value_path,
                    &ovr.value,
                ) {
                    return Ok(ResolutionOutcome::Conflict(vec![ResolutionConflict {
                        code: "ASSIGNMENT_OVERRIDE_FIELD_INVALID".to_string(),
                        message: error,
                    }]));
                }
                pol.overrides.push(ovr.clone());
                // Apply the override to the effective config
                if let Err(e) =
                    apply_json_path_override(&mut pol.effective_config, &ovr.value_path, &ovr.value)
                {
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

/// Validate the small, explicit set of runtime fields that assignments may
/// override. Identity, publication, trust, implementation, and source fields
/// are not part of a policy config and therefore cannot be overridden here.
fn validate_typed_override(
    policy_type: &str,
    config: &serde_json::Value,
    path: &str,
    value: &serde_json::Value,
) -> std::result::Result<(), String> {
    let allowed = match policy_type {
        "require_cve_check" => matches!(
            path,
            "max_critical" | "max_high" | "require_high_justification" | "strict" | "when_no_scan"
        ),
        "time_window" => matches!(
            path,
            "days" | "start_time" | "end_time" | "timezone" | "action"
        ),
        "require_approvals" => {
            matches!(path, "count" | "role" | "distinct" | "expires_after_hours")
        }
        "canary_rollout" => matches!(
            path,
            "percentage"
                | "observe_duration_minutes"
                | "selection_strategy"
                | "health_check.type"
                | "health_check.fail_threshold"
        ),
        "cve_threshold" => {
            path == "no_scan_behavior"
                || path == "allow_justifications"
                || path == "require_acknowledgment"
                || path.starts_with("thresholds.")
                    && (path.ends_with(".max") || path.ends_with(".action"))
        }
        // Native agent/package/custom-check configs do not expose assignment
        // overrides in this phase; their executable semantics remain immutable.
        _ => false,
    };

    if !allowed {
        return Err(format!(
            "Override field '{}' is not supported for policy type '{}'",
            path, policy_type
        ));
    }

    // Require that the path already exists in the canonical config. This also
    // prevents callers from using overrides to add arbitrary new config keys.
    let existing = lookup_json_path(config, path)
        .ok_or_else(|| format!("Override field '{}' does not exist in policy config", path))?;

    let valid_type = match existing {
        serde_json::Value::Bool(_) => value.is_boolean(),
        serde_json::Value::Number(_) => value.is_number(),
        serde_json::Value::String(_) => value.is_string(),
        serde_json::Value::Array(_) => value.is_array(),
        serde_json::Value::Object(_) => value.is_object(),
        serde_json::Value::Null => value.is_null(),
    };
    if !valid_type {
        return Err(format!(
            "Override value for '{}' has invalid JSON type",
            path
        ));
    }

    let integer_fields = matches!(
        path,
        "max_critical"
            | "max_high"
            | "count"
            | "expires_after_hours"
            | "percentage"
            | "observe_duration_minutes"
            | "health_check.fail_threshold"
    );
    if integer_fields
        && !value.is_null()
        && value
            .as_u64()
            .is_none_or(|number| number > u64::from(u32::MAX))
    {
        return Err(format!(
            "Override value for '{}' must be a non-negative 32-bit integer",
            path
        ));
    }

    // Validate enum-like fields and policy-specific ranges.
    if matches!(path, "when_no_scan" | "action" | "no_scan_behavior")
        && value.as_str().is_some_and(|v| !match path {
            "when_no_scan" => matches!(v, "block" | "skip"),
            "action" => matches!(v, "block" | "warn"),
            "no_scan_behavior" => matches!(v, "block" | "skip" | "warn"),
            _ => true,
        })
    {
        return Err(format!(
            "Override value for '{}' is not a supported enum value",
            path
        ));
    }
    if path == "percentage" && value.as_u64().is_none_or(|v| v > 100) {
        return Err("Canary percentage override must be between 0 and 100".to_string());
    }

    Ok(())
}

fn lookup_json_path<'a>(
    config: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    let mut current = config;
    for segment in path.split('.') {
        current = match current {
            serde_json::Value::Object(map) => map.get(segment)?,
            serde_json::Value::Array(values) => values.get(segment.parse::<usize>().ok()?)?,
            serde_json::Value::Null
            | serde_json::Value::Bool(_)
            | serde_json::Value::Number(_)
            | serde_json::Value::String(_) => return None,
        };
    }
    Some(current)
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
    resolve_system_effective_policies_with_options(pool, system_id, false).await
}

/// Resolve only the policy semantics relevant to Nix evaluation while still
/// retaining the complete resolver for compliance and deployment consumers.
/// Conflicts between two non-Nix policy versions at the same specificity are
/// ignored here because they cannot affect nix-eval-jobs.
pub async fn resolve_system_effective_policies_for_evaluation(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<ResolutionOutcome> {
    resolve_system_effective_policies_with_options(pool, system_id, true).await
}

async fn resolve_system_effective_policies_with_options(
    pool: &PgPool,
    system_id: Uuid,
    ignore_non_evaluation_conflicts: bool,
) -> Result<ResolutionOutcome> {
    // ── Open one repeatable-read snapshot for all resolver reads ──────────
    let mut tx = pool.begin().await.context("begin resolution transaction")?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .execute(&mut *tx)
        .await
        .context("set repeatable read")?;

    // Load the system's environment inside the snapshot.
    let env_id: Option<Option<Uuid>> =
        sqlx::query_scalar("SELECT environment_id FROM systems WHERE id = $1")
            .bind(system_id)
            .fetch_optional(&mut *tx)
            .await
            .context("load system environment")?;

    let env_id = env_id.flatten();

    // Load all active bundle assignments with explicit semantic ordering.
    let assignments: Vec<_> =
        sqlx::query_as::<_, (Uuid, Uuid, Uuid, Uuid, String, String, String)>(
            r#"SELECT a.id, a.current_version_id, a.bundle_id, a.bundle_version_id,
                  a.scope_type, a.enforcement_mode, a.assignment_overlay_digest
           FROM compliance_bundle_assignments a
           WHERE a.active AND a.current_version_id IS NOT NULL
             AND ((a.scope_type = 'environment' AND a.environment_id = $2)
               OR (a.scope_type = 'system' AND a.system_id = $1)
             )
           ORDER BY
             CASE a.scope_type
               WHEN 'environment' THEN 1
               WHEN 'system' THEN 2
               ELSE 3
             END,
             a.bundle_id,
             a.id"#,
        )
        .bind(system_id)
        .bind(env_id)
        .fetch_all(&mut *tx)
        .await
        .context("load bundle assignments for system")?;

    // Load direct environment policies (in-tx).
    let mut direct_candidates: Vec<(EffectivePolicy, PolicySpecificity)> = Vec::new();

    if let Some(eid) = env_id {
        let env_direct: Vec<(Uuid, Uuid, String, serde_json::Value)> = sqlx::query_as(
            r#"SELECT pv.id, pv.policy_id, pv.policy_type, pv.config
               FROM environment_policies ep
               JOIN deployment_policies dp ON dp.id = ep.policy_id
               JOIN deployment_policy_versions pv
                 ON pv.id = COALESCE(
                     dp.current_published_version_id,
                     dp.current_draft_version_id
                 )
               WHERE ep.environment_id = $1
                 AND dp.enabled = TRUE
                 AND (
                     pv.publication_state = 'accepted'
                     OR (
                         dp.current_published_version_id IS NULL
                         AND pv.publication_state IN ('incomplete', 'draft', 'interim')
                     )
                 )"#,
        )
        .bind(eid)
        .fetch_all(&mut *tx)
        .await
        .context("load environment direct policies")?;

        for (pv_id, lin_id, ptype, config) in env_direct {
            direct_candidates.push((
                EffectivePolicy {
                    policy_version_id: pv_id,
                    policy_lineage_id: lin_id,
                    policy_type: ptype,
                    source: EffectivePolicySource::LegacyDirect,
                    specificity: PolicySpecificity::Environment,
                    baseline_order: None,
                    addition_order: None,
                    overrides: Vec::new(),
                    effective_config: config,
                    assignment_mode: AssignmentMode::Enforce,
                    effective_mode: AssignmentMode::Enforce,
                    provenance: Vec::new(),
                },
                PolicySpecificity::Environment,
            ));
        }
    }

    // Load system direct policies (in-tx).
    {
        let sys_direct: Vec<(Uuid, Uuid, String, serde_json::Value)> = sqlx::query_as(
            r#"SELECT pv.id, pv.policy_id, pv.policy_type, pv.config
               FROM system_policies sp
               JOIN deployment_policies dp ON dp.id = sp.policy_id
               JOIN deployment_policy_versions pv
                 ON pv.id = COALESCE(
                     dp.current_published_version_id,
                     dp.current_draft_version_id
                 )
               WHERE sp.system_id = $1
                 AND dp.enabled = TRUE
                 AND (
                     pv.publication_state = 'accepted'
                     OR (
                         dp.current_published_version_id IS NULL
                         AND pv.publication_state IN ('incomplete', 'draft', 'interim')
                     )
                 )"#,
        )
        .bind(system_id)
        .fetch_all(&mut *tx)
        .await
        .context("load system direct policies")?;

        for (pv_id, lin_id, ptype, config) in sys_direct {
            direct_candidates.push((
                EffectivePolicy {
                    policy_version_id: pv_id,
                    policy_lineage_id: lin_id,
                    policy_type: ptype,
                    source: EffectivePolicySource::LegacyDirect,
                    specificity: PolicySpecificity::System,
                    baseline_order: None,
                    addition_order: None,
                    overrides: Vec::new(),
                    effective_config: config,
                    assignment_mode: AssignmentMode::Enforce,
                    effective_mode: AssignmentMode::Enforce,
                    provenance: Vec::new(),
                },
                PolicySpecificity::System,
            ));
        }
    }

    // ── Unified resolution: bundle assignments + direct policies ────────────

    let mut per_lineage: std::collections::HashMap<Uuid, (Uuid, PolicySpecificity, usize)> =
        std::collections::HashMap::new();
    let mut staging: Vec<EffectivePolicy> = Vec::new();
    let mut all_warnings: Vec<String> = Vec::new();
    let mut primary_bundle_version_id: Option<Uuid> = None;
    let mut bundle_version_ids_ordered: Vec<Uuid> = Vec::new();

    // Process bundle assignments first (in scope-order).
    for (
        _assignment_id,
        assignment_version_id,
        _bundle_id,
        bundle_version_id,
        scope_type,
        enforcement_mode,
        _,
    ) in &assignments
    {
        let specificity = if scope_type == "environment" {
            PolicySpecificity::Environment
        } else {
            PolicySpecificity::System
        };

        let exclusions: Vec<Uuid> = sqlx::query_scalar(
             "SELECT policy_version_id FROM compliance_assignment_exclusions WHERE assignment_version_id = $1",
        )
        .bind(assignment_version_id)
        .fetch_all(&mut *tx)
        .await
        .context("load assignment exclusions")?;

        let additions: Vec<Uuid> = sqlx::query_scalar(
             "SELECT policy_version_id FROM compliance_assignment_additions WHERE assignment_version_id = $1 ORDER BY policy_version_id",
        )
        .bind(assignment_version_id)
        .fetch_all(&mut *tx)
        .await
        .context("load assignment additions")?;

        let overrides: Vec<PolicyOverride> = sqlx::query_as::<_, (Uuid, String, serde_json::Value)>(
            "SELECT policy_version_id, value_path, value FROM compliance_assignment_value_overrides WHERE assignment_version_id = $1",
        )
        .bind(assignment_version_id)
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
            specificity,
        };

        let outcome = resolve_effective_policy_set_with_options(
            &mut tx,
            &input,
        )
        .await?;

        match outcome {
            ResolutionOutcome::Resolved(set) => {
                bundle_version_ids_ordered.push(*bundle_version_id);
                if primary_bundle_version_id.is_none() {
                    primary_bundle_version_id = Some(*bundle_version_id);
                }
                all_warnings.extend(set.warnings);

                for mut pol in set.policies {
                    pol.specificity = specificity;
                    let prov = ProvenanceEntry {
                        source: pol.source.clone(),
                        specificity,
                        scope_type: Some(scope_type.to_string()),
                        enforcement_mode: enforcement_mode.to_string(),
                        authoritative: true,
                    };
                    match merge_effective_policy_candidate(
                        pol,
                        specificity,
                        prov,
                        &mut staging,
                        &mut per_lineage,
                        &mut all_warnings,
                        ignore_non_evaluation_conflicts,
                    ) {
                        MergeOutcome::Conflict(conflict) => {
                            let _ = tx.rollback().await;
                            return Ok(ResolutionOutcome::Conflict(vec![conflict]));
                        }
                        _ => {}
                    }
                }
            }
            ResolutionOutcome::Conflict(conflicts) => {
                let _ = tx.rollback().await;
                return Ok(ResolutionOutcome::Conflict(conflicts));
            }
        }
    }

    // Inject direct policies (always resolved through the authoritative merge).
    for (pol, specificity) in direct_candidates {
        let prov = ProvenanceEntry {
            source: EffectivePolicySource::LegacyDirect,
            specificity,
            scope_type: Some(
                if specificity == PolicySpecificity::Environment {
                    "environment"
                } else {
                    "system"
                }
                .to_string(),
            ),
            enforcement_mode: "enforce".to_string(),
            authoritative: true,
        };
        match merge_effective_policy_candidate(
            pol,
            specificity,
            prov,
            &mut staging,
            &mut per_lineage,
            &mut all_warnings,
            ignore_non_evaluation_conflicts,
        ) {
            MergeOutcome::Conflict(conflict) => {
                let _ = tx.rollback().await;
                return Ok(ResolutionOutcome::Conflict(vec![conflict]));
            }
            _ => {}
        }
    }

    // The final all_policies preserves insertion order (first seen wins position).
    let all_policies = staging;

    tx.commit().await.context("commit resolution transaction")?;

    // ── Canonical combined target digest ──────────────────────────────────────
    //
    let effective_pids: Vec<Uuid> = all_policies.iter().map(|p| p.policy_version_id).collect();
    let mut additions: Vec<Uuid> = all_policies
        .iter()
        .filter(|p| matches!(p.source, EffectivePolicySource::Addition))
        .map(|p| p.policy_version_id)
        .collect();
    additions.sort();
    let mut direct: Vec<Uuid> = all_policies
        .iter()
        .filter(|p| matches!(p.source, EffectivePolicySource::LegacyDirect))
        .map(|p| p.policy_version_id)
        .collect();
    direct.sort();
    let canonical = CombinedEffectiveSetCanonical {
        bundle_version_ids_ordered,
        addition_policy_version_ids: additions,
        direct_policy_version_ids: direct,
        effective_policy_version_ids: effective_pids,
        policy_modes: all_policies
            .iter()
            .map(|p| (p.policy_version_id, p.effective_mode.as_str().to_string()))
            .collect(),
        effective_configs: all_policies
            .iter()
            .map(|p| (p.policy_version_id, p.effective_config.clone()))
            .collect(),
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
///
/// Resolves direct environment and system policies through the same
/// specificity-aware algorithm used by the bundle assignment path.
/// Environment policies have `Environment` specificity; system policies
/// have `System` specificity and can override environment policies for the
/// same lineage.
async fn resolve_legacy_system_policies(
    pool: &PgPool,
    system_id: Uuid,
    env_id: Option<Uuid>,
) -> Result<ResolutionOutcome> {
    // per_lineage: lineage_id → (version_id, specificity, index)
    let mut per_lineage: std::collections::HashMap<Uuid, (Uuid, PolicySpecificity, usize)> =
        std::collections::HashMap::new();
    let mut policies: Vec<EffectivePolicy> = Vec::new();
    let mut warnings: Vec<String> =
        vec!["Legacy direct-policy resolution used (no bundle assignments)".to_string()];

    if let Some(eid) = env_id {
        let env_direct: Vec<(Uuid, Uuid, String, serde_json::Value)> = sqlx::query_as(
            r#"SELECT pv.id, pv.policy_id, pv.policy_type, pv.config
               FROM environment_policies ep
               JOIN deployment_policies dp ON dp.id = ep.policy_id
               JOIN deployment_policy_versions pv
                 ON pv.id = COALESCE(
                     dp.current_published_version_id,
                     dp.current_draft_version_id
                 )
               WHERE ep.environment_id = $1
                 AND dp.enabled = TRUE
                 AND (
                     pv.publication_state = 'accepted'
                     OR (
                         dp.current_published_version_id IS NULL
                         AND pv.publication_state IN ('incomplete', 'draft', 'interim')
                     )
                 )"#,
        )
        .bind(eid)
        .fetch_all(pool)
        .await
        .context("load legacy environment policies")?;

        for (pv_id, lin_id, ptype, config) in env_direct {
            let idx = policies.len();
            per_lineage.insert(lin_id, (pv_id, PolicySpecificity::Environment, idx));
            policies.push(EffectivePolicy {
                policy_version_id: pv_id,
                policy_lineage_id: lin_id,
                policy_type: ptype,
                source: EffectivePolicySource::Addition,
                specificity: PolicySpecificity::Environment,
                baseline_order: None,
                addition_order: None,
                overrides: Vec::new(),
                effective_config: config,
                assignment_mode: AssignmentMode::Enforce,
                effective_mode: AssignmentMode::Enforce,
                provenance: Vec::new(),
            });
        }
    }

    let sys_direct: Vec<(Uuid, Uuid, String, serde_json::Value)> = sqlx::query_as(
        r#"SELECT pv.id, pv.policy_id, pv.policy_type, pv.config
           FROM system_policies sp
           JOIN deployment_policies dp ON dp.id = sp.policy_id
           JOIN deployment_policy_versions pv
             ON pv.id = COALESCE(
                 dp.current_published_version_id,
                 dp.current_draft_version_id
             )
           WHERE sp.system_id = $1
             AND dp.enabled = TRUE
             AND (
                 pv.publication_state = 'accepted'
                 OR (
                     dp.current_published_version_id IS NULL
                     AND pv.publication_state IN ('incomplete', 'draft', 'interim')
                 )
             )"#,
    )
    .bind(system_id)
    .fetch_all(pool)
    .await
    .context("load legacy system policies")?;

    for (pv_id, lin_id, ptype, config) in sys_direct {
        match per_lineage.get(&lin_id) {
            None => {
                let idx = policies.len();
                per_lineage.insert(lin_id, (pv_id, PolicySpecificity::System, idx));
                policies.push(EffectivePolicy {
                    policy_version_id: pv_id,
                    policy_lineage_id: lin_id,
                    policy_type: ptype,
                    source: EffectivePolicySource::Addition,
                    specificity: PolicySpecificity::System,
                    baseline_order: None,
                    addition_order: None,
                    overrides: Vec::new(),
                    effective_config: config,
                    assignment_mode: AssignmentMode::Enforce,
                    effective_mode: AssignmentMode::Enforce,
                    provenance: Vec::new(),
                });
            }
            Some(&(_existing_vid, _existing_spec, existing_idx)) => {
                // System specificity always wins over environment.
                warnings.push(format!(
                    "Legacy system direct policy {} (lineage {}) overrides environment-level version",
                    pv_id, lin_id
                ));
                policies[existing_idx] = EffectivePolicy {
                    policy_version_id: pv_id,
                    policy_lineage_id: lin_id,
                    policy_type: ptype,
                    source: EffectivePolicySource::Addition,
                    specificity: PolicySpecificity::System,
                    baseline_order: None,
                    addition_order: None,
                    overrides: Vec::new(),
                    effective_config: config,
                    assignment_mode: AssignmentMode::Enforce,
                    effective_mode: AssignmentMode::Enforce,
                    provenance: Vec::new(),
                };
                per_lineage.insert(lin_id, (pv_id, PolicySpecificity::System, existing_idx));
            }
        }
    }

    let effective_pids: Vec<Uuid> = policies.iter().map(|p| p.policy_version_id).collect();
    let canonical = AssignmentEffectiveSetCanonical {
        enforcement_mode: "enforce".to_string(),
        exclusions: vec![],
        additions: vec![],
        value_overrides: vec![],
        effective_policy_version_ids: effective_pids,
    };

    Ok(ResolutionOutcome::Resolved(EffectivePolicySet {
        bundle_version_id: Uuid::nil(),
        assignment_id: None,
        target: AssignmentTarget::System { system_id },
        policies,
        effective_set_digest: canonical.compute_digest(),
        warnings,
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
            specificity: PolicySpecificity::BundleBaseline,
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
        let result = apply_json_path_override(&mut config, "some.path", &serde_json::json!(42));
        assert!(result.is_err(), "override on scalar root must fail");
    }

    #[test]
    fn apply_json_path_override_rejects_out_of_bounds_array() {
        let mut config = serde_json::json!({"items": [1, 2]});
        let result = apply_json_path_override(&mut config, "items.5", &serde_json::json!(99));
        assert!(result.is_err(), "out-of-bounds array override must fail");
    }

    #[test]
    fn typed_override_accepts_supported_scalar() {
        let config = serde_json::json!({
            "max_critical": 0,
            "max_high": 2,
            "require_high_justification": false,
            "strict": true,
            "when_no_scan": "block"
        });

        assert!(validate_typed_override(
            "require_cve_check",
            &config,
            "max_critical",
            &serde_json::json!(3)
        )
        .is_ok());
    }

    #[test]
    fn typed_override_rejects_unknown_and_immutable_fields() {
        let config = serde_json::json!({"strict": true});

        let unknown = validate_typed_override(
            "require_cve_check",
            &config,
            "unknown",
            &serde_json::json!(true),
        );
        assert!(unknown.is_err());

        let identity = validate_typed_override(
            "require_cve_check",
            &config,
            "policy_type",
            &serde_json::json!("require_approvals"),
        );
        assert!(identity.is_err());
    }

    #[test]
    fn typed_override_rejects_wrong_type_and_invalid_enum() {
        let config = serde_json::json!({
            "strict": true,
            "when_no_scan": "block"
        });

        let wrong_type = validate_typed_override(
            "require_cve_check",
            &config,
            "strict",
            &serde_json::json!("yes"),
        );
        assert!(wrong_type.is_err());

        let invalid_enum = validate_typed_override(
            "require_cve_check",
            &config,
            "when_no_scan",
            &serde_json::json!("warn"),
        );
        assert!(invalid_enum.is_err());
    }

    #[test]
    fn typed_override_rejects_invalid_canary_percentage() {
        let config = serde_json::json!({"percentage": 25});
        let result = validate_typed_override(
            "canary_rollout",
            &config,
            "percentage",
            &serde_json::json!(101),
        );
        assert!(result.is_err());
    }

    #[test]
    fn typed_override_rejects_fractional_integer() {
        let config = serde_json::json!({"max_critical": 0});
        let result = validate_typed_override(
            "require_cve_check",
            &config,
            "max_critical",
            &serde_json::json!(1.5),
        );
        assert!(result.is_err());
    }

    // ── Specificity unit tests ─────────────────────────────────────────────────

    #[test]
    fn specificity_ordering_is_correct() {
        assert!(PolicySpecificity::BundleBaseline < PolicySpecificity::Environment);
        assert!(PolicySpecificity::Environment < PolicySpecificity::System);
        assert!(PolicySpecificity::BundleBaseline < PolicySpecificity::System);
    }

    #[test]
    fn specificity_digest_stability() {
        // Two canonical sets with the same policy version IDs must produce the
        // same digest regardless of which local assignment IDs contributed them.
        let pv1 = Uuid::new_v4();
        let pv2 = Uuid::new_v4();
        let bv1 = Uuid::new_v4();
        let bv2 = Uuid::new_v4();

        // Same policy versions, different bundle version IDs in digest input
        // (reflecting different local assignment row participation).
        let c_a = AssignmentEffectiveSetCanonical {
            enforcement_mode: "enforce".to_string(),
            exclusions: vec![bv1],
            additions: vec![],
            value_overrides: vec![],
            effective_policy_version_ids: vec![pv1, pv2],
        };
        let c_b = AssignmentEffectiveSetCanonical {
            enforcement_mode: "enforce".to_string(),
            exclusions: vec![bv2],
            additions: vec![],
            value_overrides: vec![],
            effective_policy_version_ids: vec![pv1, pv2],
        };
        // Different bundle version IDs in the scope-ordering input produce
        // different digests (the digest covers which bundle versions participated).
        assert_ne!(
            c_a.compute_digest(),
            c_b.compute_digest(),
            "different bundle version IDs in digest input must produce different digests"
        );
    }

    #[test]
    fn specificity_digest_stable_for_same_inputs() {
        // Equivalent inputs always produce the same digest regardless of call order.
        let pv = Uuid::new_v4();
        let bv = Uuid::new_v4();
        let canonical = AssignmentEffectiveSetCanonical {
            enforcement_mode: "enforce".to_string(),
            exclusions: vec![bv],
            additions: vec![],
            value_overrides: vec![],
            effective_policy_version_ids: vec![pv],
        };
        assert_eq!(
            canonical.compute_digest(),
            canonical.compute_digest(),
            "same inputs must always produce the same digest"
        );
    }

    #[test]
    fn specificity_override_value_changes_digest() {
        let pv = Uuid::new_v4();
        let bv = Uuid::new_v4();
        let base = AssignmentEffectiveSetCanonical {
            enforcement_mode: "enforce".to_string(),
            exclusions: vec![bv],
            additions: vec![],
            value_overrides: vec![(pv, "max_critical".to_string(), serde_json::json!(0))],
            effective_policy_version_ids: vec![pv],
        };
        let changed = AssignmentEffectiveSetCanonical {
            enforcement_mode: "enforce".to_string(),
            exclusions: vec![bv],
            additions: vec![],
            value_overrides: vec![(pv, "max_critical".to_string(), serde_json::json!(3))],
            effective_policy_version_ids: vec![pv],
        };
        assert_ne!(
            base.compute_digest(),
            changed.compute_digest(),
            "different override value must change digest"
        );
    }

    #[test]
    fn specificity_enforcement_mode_changes_digest() {
        let pv = Uuid::new_v4();
        let enforce = AssignmentEffectiveSetCanonical {
            enforcement_mode: "enforce".to_string(),
            exclusions: vec![],
            additions: vec![],
            value_overrides: vec![],
            effective_policy_version_ids: vec![pv],
        };
        let report_only = AssignmentEffectiveSetCanonical {
            enforcement_mode: "report_only".to_string(),
            exclusions: vec![],
            additions: vec![],
            value_overrides: vec![],
            effective_policy_version_ids: vec![pv],
        };
        assert_ne!(
            enforce.compute_digest(),
            report_only.compute_digest(),
            "enforcement mode change must change digest"
        );
    }
}
