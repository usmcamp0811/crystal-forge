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

use crate::compliance::digest::{AssignmentEffectiveSetCanonical, CombinedEffectiveSetCanonical};

// ── Typed domain models ───────────────────────────────────────────────────────

/// The scope an assignment targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentTarget {
    /// Targets every applicable system in one environment.
    Environment {
        /// Identifies the target environment.
        environment_id: Uuid,
    },
    /// Targets one exact system.
    System {
        /// Identifies the target system.
        system_id: Uuid,
    },
}

impl AssignmentTarget {
    /// Returns the persistence discriminator for this target.
    pub fn scope_type(&self) -> &'static str {
        match self {
            Self::Environment { .. } => "environment",
            Self::System { .. } => "system",
        }
    }
    /// Returns the environment or system ID represented by this target.
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
    /// Applies policy outcomes to enforcement decisions.
    Enforce,
    /// Records evidence without blocking deployment.
    ReportOnly,
}

impl AssignmentMode {
    /// Returns the persistence representation of this mode.
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
    /// Identifies the exact immutable policy version to modify.
    pub policy_version_id: Uuid,
    /// JSON-path within the policy config that this override sets.
    pub value_path: String,
    /// Contains the replacement JSON value.
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
    /// Describes how the contributing policy entered its effective set.
    pub source: EffectivePolicySource,
    /// Records the precedence of the contributing source.
    pub specificity: PolicySpecificity,
    /// Assignment that contributed this policy, when it came from a bundle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignment_id: Option<Uuid>,
    /// Bundle lineage that contributed this policy, when it came from a bundle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<Uuid>,
    /// Exact assigned bundle version, when it came from a bundle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_version_id: Option<Uuid>,
    /// Records the assignment scope discriminator when one exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_type: Option<String>,
    /// Records the assignment enforcement mode for this source.
    pub enforcement_mode: String,
    /// Indicates whether this source supplies the winning effective semantics.
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
    /// Contains a stable machine-readable conflict code.
    pub code: String,
    /// Explains the conflict for operators and API clients.
    pub message: String,
}

/// Result of resolving one assignment: either a complete set or a conflict.
#[derive(Debug)]
pub enum ResolutionOutcome {
    /// Contains the complete deterministic effective policy set.
    Resolved(EffectivePolicySet),
    /// Contains all semantic conflicts that prevent a complete set.
    Conflict(Vec<ResolutionConflict>),
}

// ── Input to the resolver ─────────────────────────────────────────────────────

/// All inputs needed to resolve one assignment's effective set.
#[derive(Debug)]
pub struct EffectivePolicyResolutionInput {
    /// Identifies the scope for which the set is resolved.
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
            } else if specificity == existing_spec {
                let entry = &mut staging[existing_idx];
                if entry.effective_config != candidate.effective_config
                    || entry.effective_mode != candidate.effective_mode
                {
                    per_lineage.insert(lineage_id, (existing_ver, existing_spec, existing_idx));
                    return MergeOutcome::Conflict(ResolutionConflict {
                        code: "EFFECTIVE_POLICY_OVERLAY_CONFLICT".into(),
                        message: format!(
                            "Policy lineage {lineage_id}: the same version {version_id} has different overlays at equal specificity {specificity:?}"
                        ),
                    });
                }
                // Equal-specificity assignments with identical resolved semantics
                // are both authoritative contexts for evidence navigation.
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

/// Implementation states that represent an unresolved requirement that Crystal
/// Forge cannot execute: `unbound` (requirement exists but has no
/// implementation) or `opaque` (the rule is preserved but its check semantics
/// cannot be modeled).
fn is_unresolved_implementation_state(implementation_state: &str) -> bool {
    matches!(implementation_state, "unbound" | "opaque")
}

/// Validate the effective set of one assignment for enforceability.
///
/// An enforce-mode assignment must not contain unresolved `unbound` or `opaque`
/// policies. Callers must apply exclusions **before** invoking this helper so
/// an excluded unresolved baseline policy is not rejected. Report-only
/// assignments may contain unresolved policies (they evaluate as `not_checked`
/// and never block deployment).
///
/// `effective_impl_states` maps each effective policy version to its
/// implementation state. Returns one typed `UNRESOLVED_ENFORCEMENT_POLICY`
/// conflict per unresolved policy so the UI can identify every policy that
/// blocks enforcement.
fn enforce_mode_unresolved_conflicts(
    mode: &AssignmentMode,
    effective_impl_states: &[(Uuid, String)],
) -> Vec<ResolutionConflict> {
    if *mode != AssignmentMode::Enforce {
        return Vec::new();
    }
    effective_impl_states
        .iter()
        .filter(|(_, implementation_state)| {
            is_unresolved_implementation_state(implementation_state)
        })
        .map(|(pv_id, implementation_state)| ResolutionConflict {
            code: "UNRESOLVED_ENFORCEMENT_POLICY".to_string(),
            message: format!(
                "Policy version {} has implementation state '{}'; enforce-mode assignments cannot contain unresolved policies (exclude the policy or use report-only mode)",
                pv_id, implementation_state
            ),
        })
        .collect()
}

// ── Authoritative resolver ────────────────────────────────────────────────────

/// Resolves the effective policy set for one assignment.
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
    let bundle_row = sqlx::query_as::<_, (String, String, String)>(
        r#"SELECT publication_state, semantic_digest, trust_state
           FROM compliance_bundle_versions
           WHERE id = $1"#,
    )
    .bind(input.bundle_version_id)
    .fetch_optional(&mut **tx)
    .await
    .context("load bundle version")?;

    let (bundle_state, bundle_digest, bundle_trust_state) = match bundle_row {
        Some((state, digest, trust_state)) => (state, digest, trust_state),
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

    if bundle_trust_state != "trusted" {
        return Ok(ResolutionOutcome::Conflict(vec![ResolutionConflict {
            code: "ASSIGNMENT_BUNDLE_UNTRUSTED".to_string(),
            message: format!("Bundle version {} is not trusted", input.bundle_version_id),
        }]));
    }

    // Load ordered baseline membership with policy lineage info
    let baseline_rows = sqlx::query_as::<
        _,
        (
            Uuid,
            Uuid,
            String,
            String,
            String,
            serde_json::Value,
            String,
            String,
        ),
    >(
        r#"SELECT cbvp.policy_version_id,
                  pv.policy_id,
                  pv.policy_type,
                  pv.publication_state,
                  pv.semantic_digest,
                  pv.config,
                  pv.implementation_state,
                  pv.trust_state
           FROM compliance_bundle_version_policies cbvp
           JOIN deployment_policy_versions pv ON pv.id = cbvp.policy_version_id
           WHERE cbvp.bundle_version_id = $1
             AND cbvp.selected = TRUE
           ORDER BY cbvp.policy_order"#,
    )
    .bind(input.bundle_version_id)
    .fetch_all(&mut **tx)
    .await
    .context("load baseline membership")?;

    // Validate every baseline policy is accepted
    for (pv_id, _, _, pv_state, _, _, implementation_state, trust_state) in &baseline_rows {
        if pv_state != "accepted" && pv_state != "deprecated" {
            return Ok(ResolutionOutcome::Conflict(vec![ResolutionConflict {
                code: "ASSIGNMENT_POLICY_NOT_ACCEPTED".to_string(),
                message: format!(
                    "Baseline policy version {} is in '{}' state; cannot assign bundle with draft member",
                    pv_id, pv_state
                ),
            }]));
        }
        if matches!(
            implementation_state.as_str(),
            "native" | "external" | "manual"
        ) && trust_state != "trusted"
        {
            return Ok(ResolutionOutcome::Conflict(vec![ResolutionConflict {
                code: "ASSIGNMENT_POLICY_NOT_TRUSTED".to_string(),
                message: format!("Baseline policy version {} is not trusted", pv_id),
            }]));
        }
    }

    // ── Step 2: Validate exclusions ───────────────────────────────────────────
    let baseline_version_ids: std::collections::HashSet<Uuid> = baseline_rows
        .iter()
        .map(|(id, _, _, _, _, _, _, _)| *id)
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
    // Implementation states are tracked alongside the effective set so the
    // enforceability check below runs against the post-exclusion set only.
    let surviving_baseline: Vec<&(
        Uuid,
        Uuid,
        String,
        String,
        String,
        serde_json::Value,
        String,
        String,
    )> = baseline_rows
        .iter()
        .filter(|(pv_id, _, _, _, _, _, _, _)| !exclusions_set.contains(pv_id))
        .collect();

    let mut effective_impl_states: Vec<(Uuid, String)> = surviving_baseline
        .iter()
        .map(|(pv_id, _, _, _, _, _, implementation_state, _)| {
            (*pv_id, implementation_state.clone())
        })
        .collect();

    let mut effective: Vec<EffectivePolicy> = surviving_baseline
        .iter()
        .enumerate()
        .map(
            |(idx, (pv_id, lin_id, ptype, _, _, config, _, _))| EffectivePolicy {
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
        let add_row = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                String,
                String,
                serde_json::Value,
                String,
                String,
            ),
        >(
            r#"SELECT policy_id, policy_type, publication_state, semantic_digest, config
                      , implementation_state, trust_state
               FROM deployment_policy_versions
               WHERE id = $1"#,
        )
        .bind(add_id)
        .fetch_optional(&mut **tx)
        .await
        .context("load addition policy version")?;

        let (
            add_lineage,
            add_type,
            add_state,
            add_digest,
            add_config,
            implementation_state,
            trust_state,
        ) = match add_row {
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

        if matches!(
            implementation_state.as_str(),
            "native" | "external" | "manual"
        ) && trust_state != "trusted"
        {
            return Ok(ResolutionOutcome::Conflict(vec![ResolutionConflict {
                code: "ASSIGNMENT_POLICY_UNTRUSTED".to_string(),
                message: format!("Addition policy version {} is not trusted", add_id),
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
        effective_impl_states.push((*add_id, implementation_state.clone()));
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

    // ── Step 4b: Validate that the effective set is enforceable ───────────────
    // Enforce-mode assignments reject unresolved `unbound`/`opaque` policies.
    // Exclusions were already applied when building `effective`, so an excluded
    // unresolved baseline policy never reaches this check. Report-only
    // assignments accept unresolved policies (they evaluate as `not_checked`).
    let unresolved =
        enforce_mode_unresolved_conflicts(&input.assignment_mode, &effective_impl_states);
    if !unresolved.is_empty() {
        return Ok(ResolutionOutcome::Conflict(unresolved));
    }

    // ── Step 5: Validate and apply overrides ──────────────────────────────────
    let effective_version_ids: std::collections::HashSet<Uuid> =
        effective.iter().map(|p| p.policy_version_id).collect();

    let mut ordered_overrides = input.overrides.clone();
    ordered_overrides.sort_by(|left, right| {
        (left.policy_version_id, &left.value_path)
            .cmp(&(right.policy_version_id, &right.value_path))
    });
    for ovr in &ordered_overrides {
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

/// Resolves all effective policies for a system by combining:
///   - Environment bundle assignments
///   - System bundle assignments
///   - Direct environment policy additions
///   - Direct system policy additions
///
/// Returns one combined EffectivePolicySet covering all active assignments.
/// Conflicts between assignments (duplicate policy lineages) are returned as
/// typed ResolutionConflicts.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot load a consistent policy snapshot.
/// It also returns an error when persisted policy configuration cannot decode.
pub async fn resolve_system_effective_policies(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<ResolutionOutcome> {
    resolve_system_effective_policies_with_options(pool, system_id, false).await
}

/// Resolves the exact effective policy set through a caller-owned transaction.
/// Callers that authorize a target use this to keep policy resolution, target
/// validation, assessment merges, and the guarded target write atomic.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot load policy inputs through the
/// transaction or persisted policy configuration cannot be decoded.
pub async fn resolve_system_effective_policies_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    system_id: Uuid,
) -> Result<ResolutionOutcome> {
    resolve_system_effective_policies_with_options_in_tx(tx, system_id, false).await
}

/// Resolves complete policy semantics for a deterministic system batch through
/// the caller's transaction. This is the authoritative API for decisions that
/// must remain in the same serializable closure as their locks and writes.
///
/// Duplicate input IDs are resolved once. Missing system IDs are omitted.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot load batch inputs through the
/// transaction or persisted policy configuration cannot be decoded.
pub async fn resolve_systems_effective_policies_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    system_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, ResolutionOutcome>> {
    let mut ids = system_ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let keyed = resolve_systems_effective_policies_batch_in_tx(tx, &ids, None, false).await?;
    Ok(keyed
        .into_iter()
        .filter_map(|((bundle_version_id, system_id), outcome)| {
            bundle_version_id.is_none().then_some((system_id, outcome))
        })
        .collect())
}

/// Resolves only policy semantics relevant to Nix evaluation while retaining
/// the complete resolver for compliance and deployment consumers.
/// Conflicts between two non-Nix policy versions at the same specificity are
/// ignored here because they cannot affect nix-eval-jobs.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot load a consistent policy snapshot.
/// It also returns an error when persisted evaluation configuration cannot
/// decode.
pub async fn resolve_system_effective_policies_for_evaluation(
    pool: &PgPool,
    system_id: Uuid,
) -> Result<ResolutionOutcome> {
    resolve_system_effective_policies_with_options(pool, system_id, true).await
}

/// Batch variant of `resolve_system_effective_policies_for_evaluation`.
///
/// Resolves effective policies for all provided `system_ids` in a single
/// REPEATABLE READ transaction using approximately 8–10 bulk queries,
/// regardless of the number of systems or assignments.
///
/// This eliminates the `O(S × (4 + 5N + A))` round-trip pattern in
/// `load_policies_by_configuration_for_eval`, where each system previously
/// required its own full resolver invocation (and thus its own transaction
/// plus per-assignment SQL loops).
///
/// The same merge/precedence semantics as `resolve_system_effective_policies_with_options`
/// are used; only the data-loading phase is batched.
///
/// # Returns
///
/// A `HashMap` from `system_id` to `ResolutionOutcome`.  Systems for which
/// resolution produced a conflict are included in the map as
/// `ResolutionOutcome::Conflict(_)`.  System IDs not found in the DB are
/// omitted silently (the caller should treat absence as "no assignments").
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot create or query the repeatable-read
/// snapshot, or when persisted policy configuration cannot be decoded.
pub async fn resolve_systems_effective_policies_for_evaluation_batch(
    pool: &PgPool,
    system_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, ResolutionOutcome>> {
    let keyed = resolve_systems_effective_policies_batch(pool, system_ids, None, true).await?;
    Ok(keyed
        .into_iter()
        .filter_map(|((bundle_version_id, system_id), outcome)| {
            if bundle_version_id.is_none() {
                Some((system_id, outcome))
            } else {
                None
            }
        })
        .collect())
}

/// Resolves compliance and deployment semantics for many systems in one
/// repeatable-read snapshot.
///
/// Missing system IDs are omitted from the returned map.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot create or query the repeatable-read
/// snapshot, or when persisted policy configuration cannot be decoded.
pub async fn resolve_systems_effective_policies_for_deployment_batch(
    pool: &PgPool,
    system_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, ResolutionOutcome>> {
    let keyed = resolve_systems_effective_policies_batch(pool, system_ids, None, false).await?;
    Ok(keyed
        .into_iter()
        .filter_map(|((bundle_version_id, system_id), outcome)| {
            bundle_version_id.is_none().then_some((system_id, outcome))
        })
        .collect())
}

/// Resolves one exact bundle version for all provided systems in one batch.
/// Explicit assignments for the requested version are authoritative; systems
/// in the bundle scope without one receive the unmodified baseline. Direct
/// environment and system policies still use the normal precedence rules.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot create or query the repeatable-read
/// snapshot, or when persisted policy configuration cannot be decoded.
pub async fn resolve_systems_effective_policies_for_bundle_version_batch(
    pool: &PgPool,
    system_ids: &[Uuid],
    bundle_version_id: Uuid,
) -> Result<std::collections::HashMap<Uuid, ResolutionOutcome>> {
    let keyed = resolve_systems_effective_policies_for_bundle_versions_batch(
        pool,
        &[(bundle_version_id, system_ids.to_vec())],
    )
    .await?;
    Ok(keyed
        .into_iter()
        .filter_map(|((version_id, system_id), outcome)| {
            if version_id == bundle_version_id {
                Some((system_id, outcome))
            } else {
                None
            }
        })
        .collect())
}

/// Resolves exact bundle versions for explicit system sets in one batch.
///
/// Unlike the single-version compatibility API, the requested bundle-version IDs
/// are first-class inputs: metadata and baseline membership are loaded for every
/// requested version even when there are no explicit assignments. Results are
/// keyed by `(bundle_version_id, system_id)` so callers can aggregate many
/// versions without accidentally merging assignments from another revision.
///
/// Empty requests return an empty map. Duplicate system IDs within a requested
/// version are resolved once.
///
/// # Errors
///
/// Returns an error when PostgreSQL cannot create or query the repeatable-read
/// snapshot, or when persisted policy configuration cannot be decoded.
pub async fn resolve_systems_effective_policies_for_bundle_versions_batch(
    pool: &PgPool,
    requests: &[(Uuid, Vec<Uuid>)],
) -> Result<std::collections::HashMap<(Uuid, Uuid), ResolutionOutcome>> {
    let mut requests_by_version = std::collections::HashMap::<Uuid, Vec<Uuid>>::new();
    for (bundle_version_id, system_ids) in requests {
        let entry = requests_by_version.entry(*bundle_version_id).or_default();
        entry.extend(system_ids.iter().copied());
    }
    for system_ids in requests_by_version.values_mut() {
        system_ids.sort_unstable();
        system_ids.dedup();
    }

    let all_system_ids: Vec<Uuid> = requests_by_version
        .values()
        .flatten()
        .copied()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    if all_system_ids.is_empty() || requests_by_version.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let keyed = resolve_systems_effective_policies_batch(
        pool,
        &all_system_ids,
        Some(&requests_by_version),
        false,
    )
    .await?;
    Ok(keyed
        .into_iter()
        .filter_map(|((bundle_version_id, system_id), outcome)| {
            bundle_version_id.map(|version_id| ((version_id, system_id), outcome))
        })
        .collect())
}

async fn resolve_systems_effective_policies_batch(
    pool: &PgPool,
    system_ids: &[Uuid],
    bundle_version_requests: Option<&std::collections::HashMap<Uuid, Vec<Uuid>>>,
    ignore_non_evaluation_conflicts: bool,
) -> Result<std::collections::HashMap<(Option<Uuid>, Uuid), ResolutionOutcome>> {
    if system_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let mut tx = pool
        .begin()
        .await
        .context("begin batch resolution transaction")?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
        .execute(&mut *tx)
        .await
        .context("set repeatable read (batch)")?;
    let result = resolve_systems_effective_policies_batch_in_tx(
        &mut tx,
        system_ids,
        bundle_version_requests,
        ignore_non_evaluation_conflicts,
    )
    .await?;
    tx.commit()
        .await
        .context("commit batch resolution snapshot")?;
    Ok(result)
}

async fn resolve_systems_effective_policies_batch_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    system_ids: &[Uuid],
    bundle_version_requests: Option<&std::collections::HashMap<Uuid, Vec<Uuid>>>,
    ignore_non_evaluation_conflicts: bool,
) -> Result<std::collections::HashMap<(Option<Uuid>, Uuid), ResolutionOutcome>> {
    if system_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    // ── Q1: Load system → environment_id for all systems ─────────────────────
    let system_envs: Vec<(Uuid, Option<Uuid>)> =
        sqlx::query_as("SELECT id, environment_id FROM systems WHERE id = ANY($1)")
            .bind(system_ids)
            .fetch_all(&mut **tx)
            .await
            .context("batch load system environments")?;

    // Build lookup maps
    let sys_env: std::collections::HashMap<Uuid, Option<Uuid>> = system_envs.into_iter().collect();
    let all_env_ids: Vec<Uuid> = sys_env
        .values()
        .filter_map(|eid| *eid)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let bundle_version_filter_ids: Option<Vec<Uuid>> = bundle_version_requests.map(|requests| {
        requests
            .keys()
            .copied()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect()
    });

    let current_published_versions: Vec<Uuid> = if let Some(version_ids) =
        &bundle_version_filter_ids
    {
        sqlx::query_scalar(
            "SELECT current_published_version_id FROM compliance_bundles WHERE current_published_version_id = ANY($1)",
        )
        .bind(version_ids)
        .fetch_all(&mut **tx)
        .await
        .context("load current published bundle versions for virtual baseline resolution")?
    } else {
        Vec::new()
    };

    // ── Q2: Load all active bundle assignments for all systems/environments ───
    #[allow(clippy::type_complexity)]
    let raw_assignments: Vec<(
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        String,
        String,
        String,
        Option<Uuid>,
        Option<Uuid>,
    )> = sqlx::query_as(
        r#"SELECT a.id, a.current_version_id, a.bundle_id, av.bundle_version_id,
                      a.scope_type, a.enforcement_mode, a.assignment_overlay_digest,
                      a.environment_id, a.system_id
               FROM compliance_bundle_assignments a
               JOIN compliance_bundle_assignment_versions av ON av.id = a.current_version_id
               WHERE a.active AND a.current_version_id IS NOT NULL
                 AND ($3::uuid[] IS NULL OR av.bundle_version_id = ANY($3))
                 AND (
                   (a.scope_type = 'environment' AND a.environment_id = ANY($1))
                   OR (a.scope_type = 'system' AND a.system_id = ANY($2))
                 )
               ORDER BY
                 CASE a.scope_type WHEN 'environment' THEN 1 WHEN 'system' THEN 2 ELSE 3 END,
                 a.bundle_id,
                 a.id"#,
    )
    .bind(&all_env_ids)
    .bind(system_ids)
    .bind(bundle_version_filter_ids.as_deref())
    .fetch_all(&mut **tx)
    .await
    .context("batch load bundle assignments")?;

    // Collect all assignment_version_ids and bundle_version_ids
    let all_assignment_version_ids: Vec<Uuid> = raw_assignments
        .iter()
        .map(|(_, av, _, _, _, _, _, _, _)| *av)
        .collect();
    let mut all_bundle_version_ids: Vec<Uuid> = raw_assignments
        .iter()
        .map(|(_, _, _, bv, _, _, _, _, _)| *bv)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    if let Some(requests) = bundle_version_requests {
        all_bundle_version_ids.extend(requests.keys().copied());
        all_bundle_version_ids.sort_unstable();
        all_bundle_version_ids.dedup();
    }

    // ── Q3: Bulk load exclusions for all assignment versions ──────────────────
    let raw_exclusions: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT assignment_version_id, policy_version_id
         FROM compliance_assignment_exclusions
         WHERE assignment_version_id = ANY($1)",
    )
    .bind(&all_assignment_version_ids)
    .fetch_all(&mut **tx)
    .await
    .context("batch load exclusions")?;

    let mut exclusions_by_av: std::collections::HashMap<Uuid, Vec<Uuid>> =
        std::collections::HashMap::new();
    for (av_id, pv_id) in raw_exclusions {
        exclusions_by_av.entry(av_id).or_default().push(pv_id);
    }

    // ── Q4: Bulk load additions for all assignment versions ───────────────────
    let raw_additions: Vec<(Uuid, Uuid, i32)> = sqlx::query_as(
        "SELECT assignment_version_id, policy_version_id, addition_order
         FROM compliance_assignment_additions
         WHERE assignment_version_id = ANY($1)
         ORDER BY assignment_version_id, addition_order",
    )
    .bind(&all_assignment_version_ids)
    .fetch_all(&mut **tx)
    .await
    .context("batch load additions")?;

    let mut additions_by_av: std::collections::HashMap<Uuid, Vec<Uuid>> =
        std::collections::HashMap::new();
    for (av_id, pv_id, _) in raw_additions {
        additions_by_av.entry(av_id).or_default().push(pv_id);
    }

    // ── Q5: Bulk load overrides for all assignment versions ───────────────────
    let raw_overrides: Vec<(Uuid, Uuid, String, serde_json::Value)> = sqlx::query_as(
        "SELECT assignment_version_id, policy_version_id, value_path, value
         FROM compliance_assignment_value_overrides
         WHERE assignment_version_id = ANY($1)",
    )
    .bind(&all_assignment_version_ids)
    .fetch_all(&mut **tx)
    .await
    .context("batch load overrides")?;

    let mut overrides_by_av: std::collections::HashMap<Uuid, Vec<PolicyOverride>> =
        std::collections::HashMap::new();
    for (av_id, pv_id, path, val) in raw_overrides {
        overrides_by_av
            .entry(av_id)
            .or_default()
            .push(PolicyOverride {
                policy_version_id: pv_id,
                value_path: path,
                value: val,
            });
    }
    for overrides in overrides_by_av.values_mut() {
        overrides.sort_by(|left, right| {
            (left.policy_version_id, &left.value_path)
                .cmp(&(right.policy_version_id, &right.value_path))
        });
    }

    // ── Q6: Bulk load bundle version states ───────────────────────────────────
    let raw_bundle_versions: Vec<(Uuid, String, String, String)> = sqlx::query_as(
        "SELECT id, publication_state, semantic_digest, trust_state
         FROM compliance_bundle_versions
         WHERE id = ANY($1)",
    )
    .bind(&all_bundle_version_ids)
    .fetch_all(&mut **tx)
    .await
    .context("batch load bundle versions")?;

    let bundle_version_info: std::collections::HashMap<Uuid, (String, String, String)> =
        raw_bundle_versions
            .into_iter()
            .map(|(id, state, digest, trust_state)| (id, (state, digest, trust_state)))
            .collect();

    // ── Q7: Bulk load baseline memberships for all bundle versions ────────────
    let raw_baselines: Vec<(
        Uuid,
        Uuid,
        Uuid,
        String,
        String,
        String,
        String,
        i32,
        serde_json::Value,
    )> = sqlx::query_as(
        r#"SELECT cbvp.bundle_version_id, cbvp.policy_version_id,
                      pv.policy_id, pv.policy_type, pv.publication_state,
                      pv.implementation_state, pv.trust_state, cbvp.policy_order, pv.config
               FROM compliance_bundle_version_policies cbvp
               JOIN deployment_policy_versions pv ON pv.id = cbvp.policy_version_id
               WHERE cbvp.bundle_version_id = ANY($1)
                 AND cbvp.selected = TRUE
               ORDER BY cbvp.bundle_version_id, cbvp.policy_order"#,
    )
    .bind(&all_bundle_version_ids)
    .fetch_all(&mut **tx)
    .await
    .context("batch load bundle baselines")?;

    // Group by bundle_version_id
    let mut baselines_by_bv: std::collections::HashMap<
        Uuid,
        Vec<(
            Uuid,
            Uuid,
            String,
            String,
            String,
            String,
            i32,
            serde_json::Value,
        )>,
    > = std::collections::HashMap::new();
    for (bv_id, pv_id, lin_id, ptype, pv_state, implementation_state, trust_state, order, config) in
        raw_baselines
    {
        baselines_by_bv.entry(bv_id).or_default().push((
            pv_id,
            lin_id,
            ptype,
            pv_state,
            implementation_state,
            trust_state,
            order,
            config,
        ));
    }

    // ── Q8: Bulk load all addition policy versions ────────────────────────────
    let all_addition_pv_ids: Vec<Uuid> = additions_by_av
        .values()
        .flatten()
        .copied()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let mut addition_pv_info: std::collections::HashMap<
        Uuid,
        (Uuid, String, serde_json::Value, String, String, String),
    > = std::collections::HashMap::new();
    if !all_addition_pv_ids.is_empty() {
        let raw_addition_pvs: Vec<(
            Uuid,
            Uuid,
            String,
            serde_json::Value,
            String,
            String,
            String,
        )> = sqlx::query_as(
            "SELECT id, policy_id, policy_type, config, implementation_state,
                        publication_state, trust_state
                 FROM deployment_policy_versions
                 WHERE id = ANY($1)",
        )
        .bind(&all_addition_pv_ids)
        .fetch_all(&mut **tx)
        .await
        .context("batch load addition policy versions")?;

        for (pv_id, lin_id, ptype, config, implementation_state, publication_state, trust_state) in
            raw_addition_pvs
        {
            addition_pv_info.insert(
                pv_id,
                (
                    lin_id,
                    ptype,
                    config,
                    implementation_state,
                    publication_state,
                    trust_state,
                ),
            );
        }
    }

    // ── Q9: Bulk load legacy direct environment policies ──────────────────────
    let mut env_direct_policies: std::collections::HashMap<
        Uuid,
        Vec<(Uuid, Uuid, String, serde_json::Value)>,
    > = std::collections::HashMap::new();
    if !all_env_ids.is_empty() {
        let raw_env_direct: Vec<(Uuid, Uuid, Uuid, String, serde_json::Value)> = sqlx::query_as(
            r#"SELECT ep.environment_id, pv.id, pv.policy_id, pv.policy_type, pv.config
               FROM environment_policies ep
               JOIN deployment_policies dp ON dp.id = ep.policy_id
               JOIN deployment_policy_versions pv
                 ON pv.id = COALESCE(dp.current_published_version_id, dp.current_draft_version_id)
               WHERE ep.environment_id = ANY($1)
                  AND dp.enabled = TRUE
                  AND (pv.implementation_state NOT IN ('native', 'external', 'manual')
                       OR pv.trust_state = 'trusted')
                 AND (
                   pv.publication_state = 'accepted'
                   OR (dp.current_published_version_id IS NULL
                       AND pv.publication_state IN ('incomplete', 'draft', 'interim'))
                   )
               ORDER BY ep.environment_id, pv.id"#,
        )
        .bind(&all_env_ids)
        .fetch_all(&mut **tx)
        .await
        .context("batch load environment direct policies")?;

        for (env_id, pv_id, lin_id, ptype, config) in raw_env_direct {
            env_direct_policies
                .entry(env_id)
                .or_default()
                .push((pv_id, lin_id, ptype, config));
        }
    }

    // ── Q10: Bulk load legacy direct system policies ──────────────────────────
    let mut sys_direct_policies: std::collections::HashMap<
        Uuid,
        Vec<(Uuid, Uuid, String, serde_json::Value)>,
    > = std::collections::HashMap::new();
    {
        let raw_sys_direct: Vec<(Uuid, Uuid, Uuid, String, serde_json::Value)> = sqlx::query_as(
            r#"SELECT sp.system_id, pv.id, pv.policy_id, pv.policy_type, pv.config
               FROM system_policies sp
               JOIN deployment_policies dp ON dp.id = sp.policy_id
               JOIN deployment_policy_versions pv
                 ON pv.id = COALESCE(dp.current_published_version_id, dp.current_draft_version_id)
               WHERE sp.system_id = ANY($1)
                  AND dp.enabled = TRUE
                  AND (pv.implementation_state NOT IN ('native', 'external', 'manual')
                       OR pv.trust_state = 'trusted')
                 AND (
                   pv.publication_state = 'accepted'
                   OR (dp.current_published_version_id IS NULL
                       AND pv.publication_state IN ('incomplete', 'draft', 'interim'))
                   )
               ORDER BY sp.system_id, pv.id"#,
        )
        .bind(system_ids)
        .fetch_all(&mut **tx)
        .await
        .context("batch load system direct policies")?;

        for (sys_id, pv_id, lin_id, ptype, config) in raw_sys_direct {
            sys_direct_policies
                .entry(sys_id)
                .or_default()
                .push((pv_id, lin_id, ptype, config));
        }
    }

    // ── Per-system merge phase (pure Rust, no DB access) ──────────────────────
    //
    // This phase applies exactly the same precedence/merge rules as the
    // per-assignment resolver, now entirely in memory.

    type AssignmentEntry = (
        Uuid,
        Uuid,
        Uuid,
        Uuid,
        String,
        String,
        Option<Uuid>,
        Option<Uuid>,
    );

    // Organise assignments per system for quick lookup.
    // Each entry: (assignment_id, assignment_version_id, bundle_id, bundle_version_id,
    //              scope_type, enforcement_mode, environment_id, system_id)
    // in scope-ordered position (already sorted by the SQL ORDER BY above).
    let mut assignments_for_system: std::collections::HashMap<Uuid, Vec<AssignmentEntry>> =
        std::collections::HashMap::new();
    let mut assignments_for_request: std::collections::HashMap<(Uuid, Uuid), Vec<AssignmentEntry>> =
        std::collections::HashMap::new();

    for row in &raw_assignments {
        let (
            a_id,
            av_id,
            b_id,
            bv_id,
            scope_type,
            enforcement_mode,
            _digest,
            env_id_opt,
            sys_id_opt,
        ) = row;
        let assignment_entry = (
            *a_id,
            *av_id,
            *b_id,
            *bv_id,
            scope_type.clone(),
            enforcement_mode.clone(),
            *env_id_opt,
            *sys_id_opt,
        );
        // Environment-scope assignments apply to every system in that environment.
        if scope_type == "environment" {
            if let Some(env_id) = env_id_opt {
                for (&sys_id, &ref sys_env_id) in &sys_env {
                    if sys_env_id.as_ref() == Some(env_id) {
                        assignments_for_system
                            .entry(sys_id)
                            .or_default()
                            .push(assignment_entry.clone());
                        if bundle_version_requests.is_some() {
                            assignments_for_request
                                .entry((*bv_id, sys_id))
                                .or_default()
                                .push(assignment_entry.clone());
                        }
                    }
                }
            }
        }
        // System-scope assignments apply only to the targeted system.
        if scope_type == "system" {
            if let Some(sys_id) = sys_id_opt {
                assignments_for_system
                    .entry(*sys_id)
                    .or_default()
                    .push(assignment_entry.clone());
                if bundle_version_requests.is_some() {
                    assignments_for_request
                        .entry((*bv_id, *sys_id))
                        .or_default()
                        .push(assignment_entry);
                }
            }
        }
    }

    let virtual_baseline_versions: std::collections::HashSet<Uuid> =
        current_published_versions.iter().copied().collect();

    let mut work_items: Vec<(Option<Uuid>, Uuid, Vec<AssignmentEntry>)> = Vec::new();
    if let Some(requests) = bundle_version_requests {
        for (&bundle_version_id, requested_systems) in requests {
            for &sys_id in requested_systems {
                let mut assignments = assignments_for_request
                    .get(&(bundle_version_id, sys_id))
                    .cloned()
                    .unwrap_or_default();
                // Preserve the versioned endpoint's applicability surface for
                // systems that have no explicit assignment for this exact
                // current-published version. A virtual baseline keeps the
                // operation set-based and sends it through the same merge path.
                if assignments.is_empty() && virtual_baseline_versions.contains(&bundle_version_id)
                {
                    assignments.push((
                        Uuid::nil(),
                        Uuid::nil(),
                        Uuid::nil(),
                        bundle_version_id,
                        "virtual_baseline".to_string(),
                        "enforce".to_string(),
                        None,
                        Some(sys_id),
                    ));
                }
                work_items.push((Some(bundle_version_id), sys_id, assignments));
            }
        }
    } else {
        for &sys_id in system_ids {
            work_items.push((
                None,
                sys_id,
                assignments_for_system
                    .get(&sys_id)
                    .cloned()
                    .unwrap_or_default(),
            ));
        }
    }

    let mut results = std::collections::HashMap::new();

    'system: for (requested_bundle_version_id, sys_id, sys_assignments) in work_items {
        let result_key = (requested_bundle_version_id, sys_id);
        let env_id = sys_env.get(&sys_id).and_then(|e| *e);

        let mut per_lineage: std::collections::HashMap<Uuid, (Uuid, PolicySpecificity, usize)> =
            std::collections::HashMap::new();
        let mut staging: Vec<EffectivePolicy> = Vec::new();
        let mut all_warnings: Vec<String> = Vec::new();
        let mut primary_bundle_version_id: Option<Uuid> = None;
        let mut bundle_version_ids_ordered: Vec<Uuid> = Vec::new();
        let mut has_assignments = false;

        // Process bundle assignments (already in scope order).
        for (assignment_id, av_id, bundle_id, bv_id, scope_type, enforcement_mode, _, _) in
            &sys_assignments
        {
            has_assignments = true;

            let specificity = match scope_type.as_str() {
                "virtual_baseline" => PolicySpecificity::BundleBaseline,
                "environment" => PolicySpecificity::Environment,
                _ => PolicySpecificity::System,
            };
            let mode = if enforcement_mode == "report_only" {
                AssignmentMode::ReportOnly
            } else {
                AssignmentMode::Enforce
            };

            // Validate bundle version state
            let Some((bv_state, bv_digest, bv_trust_state)) = bundle_version_info.get(bv_id) else {
                results.insert(
                    result_key,
                    ResolutionOutcome::Conflict(vec![ResolutionConflict {
                        code: "ASSIGNMENT_BUNDLE_NOT_FOUND".to_string(),
                        message: format!("Bundle version {} does not exist", bv_id),
                    }]),
                );
                continue 'system;
            };

            if bv_state != "accepted" {
                results.insert(
                    result_key, ResolutionOutcome::Conflict(vec![ResolutionConflict {
                    code: "ASSIGNMENT_BUNDLE_NOT_ACCEPTED".to_string(),
                    message: format!(
                        "Bundle version {} is in '{}' state; only 'accepted' versions can be assigned",
                        bv_id, bv_state
                    ),
                }]));
                continue 'system;
            }
            if bv_digest == "pending" {
                results.insert(
                    result_key,
                    ResolutionOutcome::Conflict(vec![ResolutionConflict {
                        code: "ASSIGNMENT_BUNDLE_DIGEST_PENDING".to_string(),
                        message: format!("Bundle version {} has pending digest", bv_id),
                    }]),
                );
                continue 'system;
            }
            if bv_trust_state != "trusted" {
                results.insert(
                    result_key,
                    ResolutionOutcome::Conflict(vec![ResolutionConflict {
                        code: "ASSIGNMENT_BUNDLE_UNTRUSTED".to_string(),
                        message: format!("Bundle version {} is not trusted", bv_id),
                    }]),
                );
                continue 'system;
            }

            let empty_baseline = vec![];
            let baseline = baselines_by_bv.get(bv_id).unwrap_or(&empty_baseline);
            if let Some((pv_id, _, _, _pv_state, _implementation_state, _trust_state, _, _)) =
                baseline
                    .iter()
                    .find(|(_, _, _, state, implementation, trust, _, _)| {
                        (state != "accepted" && state != "deprecated")
                            || (matches!(implementation.as_str(), "native" | "external" | "manual")
                                && trust != "trusted")
                    })
            {
                results.insert(
                    result_key,
                    ResolutionOutcome::Conflict(vec![ResolutionConflict {
                        code: "ASSIGNMENT_POLICY_NOT_READY".to_string(),
                        message: format!("Policy version {} is not ready for assignment", pv_id),
                    }]),
                );
                continue 'system;
            }
            let exclusions = exclusions_by_av.get(av_id).cloned().unwrap_or_default();
            let additions = additions_by_av.get(av_id).cloned().unwrap_or_default();
            let overrides_for_av = overrides_by_av.get(av_id).cloned().unwrap_or_default();

            // Build exclusion set
            let exclusions_set: std::collections::HashSet<Uuid> =
                exclusions.iter().copied().collect();
            let baseline_version_ids: std::collections::HashSet<Uuid> = baseline
                .iter()
                .map(|(pv_id, _, _, _, _, _, _, _)| *pv_id)
                .collect();

            // Validate exclusions
            for excl in &exclusions {
                if !baseline_version_ids.contains(excl) {
                    results.insert(
                        result_key,
                        ResolutionOutcome::Conflict(vec![ResolutionConflict {
                            code: "ASSIGNMENT_EXCLUSION_NOT_IN_BUNDLE".to_string(),
                            message: format!(
                                "Exclusion {} is not a member of bundle version {}",
                                excl, bv_id
                            ),
                        }]),
                    );
                    continue 'system;
                }
            }

            // Build surviving baseline
            let surviving_baseline: Vec<&(
                Uuid,
                Uuid,
                String,
                String,
                String,
                String,
                i32,
                serde_json::Value,
            )> = baseline
                .iter()
                .filter(|(pv_id, _, _, _, _, _, _, _)| !exclusions_set.contains(pv_id))
                .collect();

            let mut assignment_effective_impl_states: Vec<(Uuid, String)> = surviving_baseline
                .iter()
                .map(|(pv_id, _, _, _, implementation_state, _, _, _)| {
                    (*pv_id, implementation_state.clone())
                })
                .collect();

            let mut assignment_effective: Vec<EffectivePolicy> = surviving_baseline
                .iter()
                .enumerate()
                .map(
                    |(idx, (pv_id, lin_id, ptype, _, _, _, _, config))| EffectivePolicy {
                        policy_version_id: *pv_id,
                        policy_lineage_id: *lin_id,
                        policy_type: ptype.clone(),
                        source: EffectivePolicySource::Baseline,
                        specificity,
                        baseline_order: Some(idx as i32),
                        addition_order: None,
                        overrides: Vec::new(),
                        effective_config: config.clone(),
                        assignment_mode: mode.clone(),
                        effective_mode: mode.clone(),
                        provenance: Vec::new(),
                    },
                )
                .collect();

            // Validate and apply additions
            let mut seen_lineages: std::collections::HashMap<Uuid, Uuid> = assignment_effective
                .iter()
                .map(|p| (p.policy_lineage_id, p.policy_version_id))
                .collect();

            for (add_order, add_pv_id) in additions.iter().enumerate() {
                if exclusions_set.contains(add_pv_id) {
                    results.insert(
                        result_key,
                        ResolutionOutcome::Conflict(vec![ResolutionConflict {
                            code: "ASSIGNMENT_ADDITION_EXCLUDED".to_string(),
                            message: format!(
                                "Policy version {} is both excluded and added",
                                add_pv_id
                            ),
                        }]),
                    );
                    continue 'system;
                }
                if baseline_version_ids.contains(add_pv_id) {
                    results.insert(
                        result_key,
                        ResolutionOutcome::Conflict(vec![ResolutionConflict {
                            code: "ASSIGNMENT_ADDITION_DUPLICATE".to_string(),
                            message: format!(
                                "Policy version {} is already in the baseline",
                                add_pv_id
                            ),
                        }]),
                    );
                    continue 'system;
                }

                let Some((
                    lin_id,
                    ptype,
                    config,
                    implementation_state,
                    publication_state,
                    trust_state,
                )) = addition_pv_info.get(add_pv_id)
                else {
                    results.insert(
                        result_key,
                        ResolutionOutcome::Conflict(vec![ResolutionConflict {
                            code: "ASSIGNMENT_ADDITION_NOT_FOUND".to_string(),
                            message: format!("Addition policy version {} not found", add_pv_id),
                        }]),
                    );
                    continue 'system;
                };

                if publication_state != "accepted" {
                    results.insert(
                        result_key,
                        ResolutionOutcome::Conflict(vec![ResolutionConflict {
                            code: "ASSIGNMENT_POLICY_NOT_ACCEPTED".to_string(),
                            message: format!(
                                "Addition policy version {} is in '{}' state; must be 'accepted'",
                                add_pv_id, publication_state
                            ),
                        }]),
                    );
                    continue 'system;
                }
                if matches!(
                    implementation_state.as_str(),
                    "native" | "external" | "manual"
                ) && trust_state != "trusted"
                {
                    results.insert(
                        result_key,
                        ResolutionOutcome::Conflict(vec![ResolutionConflict {
                            code: "ASSIGNMENT_POLICY_UNTRUSTED".to_string(),
                            message: format!(
                                "Addition policy version {} is not trusted",
                                add_pv_id
                            ),
                        }]),
                    );
                    continue 'system;
                }

                if let Some(existing_vid) = seen_lineages.get(lin_id) {
                    if existing_vid != add_pv_id {
                        results.insert(
                            result_key,
                            ResolutionOutcome::Conflict(vec![ResolutionConflict {
                                code: "ASSIGNMENT_ADDITION_LINEAGE_CONFLICT".to_string(),
                                message: format!(
                                    "Addition {} conflicts with existing version {} for lineage {}",
                                    add_pv_id, existing_vid, lin_id
                                ),
                            }]),
                        );
                        continue 'system;
                    }
                } else {
                    seen_lineages.insert(*lin_id, *add_pv_id);
                    assignment_effective_impl_states
                        .push((*add_pv_id, implementation_state.clone()));
                    assignment_effective.push(EffectivePolicy {
                        policy_version_id: *add_pv_id,
                        policy_lineage_id: *lin_id,
                        policy_type: ptype.clone(),
                        source: EffectivePolicySource::Addition,
                        specificity,
                        baseline_order: None,
                        addition_order: Some(add_order as i32),
                        overrides: Vec::new(),
                        effective_config: config.clone(),
                        assignment_mode: mode.clone(),
                        effective_mode: mode.clone(),
                        provenance: Vec::new(),
                    });
                }
            }

            // Enforce-mode assignments reject unresolved `unbound`/`opaque`
            // policies. Exclusions were applied when building
            // `assignment_effective`, so an excluded unresolved baseline policy
            // never reaches this check. Report-only assignments accept them.
            let unresolved =
                enforce_mode_unresolved_conflicts(&mode, &assignment_effective_impl_states);
            if !unresolved.is_empty() {
                results.insert(result_key, ResolutionOutcome::Conflict(unresolved));
                continue 'system;
            }

            // Apply overrides
            let effective_version_ids: std::collections::HashSet<Uuid> = assignment_effective
                .iter()
                .map(|policy| policy.policy_version_id)
                .collect();
            for o in &overrides_for_av {
                if exclusions_set.contains(&o.policy_version_id)
                    || !effective_version_ids.contains(&o.policy_version_id)
                {
                    results.insert(
                        result_key,
                        ResolutionOutcome::Conflict(vec![ResolutionConflict {
                            code: "ASSIGNMENT_OVERRIDE_TARGET_MISSING".to_string(),
                            message: format!(
                                "Override targets policy version {} which is not in the effective set",
                                o.policy_version_id
                            ),
                        }]),
                    );
                    continue 'system;
                }
                for pol in assignment_effective.iter_mut() {
                    if pol.policy_version_id == o.policy_version_id {
                        if let Err(error) = validate_typed_override(
                            &pol.policy_type,
                            &pol.effective_config,
                            &o.value_path,
                            &o.value,
                        ) {
                            results.insert(
                                result_key,
                                ResolutionOutcome::Conflict(vec![ResolutionConflict {
                                    code: "ASSIGNMENT_OVERRIDE_FIELD_INVALID".to_string(),
                                    message: error,
                                }]),
                            );
                            continue 'system;
                        }
                        if let Err(error) = apply_json_path_override(
                            &mut pol.effective_config,
                            &o.value_path,
                            &o.value,
                        ) {
                            results.insert(
                                result_key,
                                ResolutionOutcome::Conflict(vec![ResolutionConflict {
                                    code: "ASSIGNMENT_OVERRIDE_VALUE_INVALID".to_string(),
                                    message: format!(
                                        "Override at path '{}' on policy {} failed: {}",
                                        o.value_path, o.policy_version_id, error
                                    ),
                                }]),
                            );
                            continue 'system;
                        }
                        if !pol.overrides.iter().any(|x| x.value_path == o.value_path) {
                            pol.overrides.push(o.clone());
                        }
                    }
                }
            }

            if primary_bundle_version_id.is_none() {
                primary_bundle_version_id = Some(*bv_id);
            }
            bundle_version_ids_ordered.push(*bv_id);

            // Merge assignment policies into staging using the authoritative merge
            for mut pol in assignment_effective {
                pol.specificity = specificity;
                let prov = ProvenanceEntry {
                    source: pol.source.clone(),
                    specificity,
                    assignment_id: Some(*assignment_id),
                    bundle_id: Some(*bundle_id),
                    bundle_version_id: Some(*bv_id),
                    scope_type: Some(scope_type.clone()),
                    enforcement_mode: enforcement_mode.clone(),
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
                        results.insert(result_key, ResolutionOutcome::Conflict(vec![conflict]));
                        continue 'system;
                    }
                    _ => {}
                }
            }
        }

        // Direct policies always participate in the effective set. When no
        // bundle assignment exists this is also the legacy-only fallback.
        {
            // Apply environment direct policies
            if let Some(eid) = env_id {
                if let Some(env_pols) = env_direct_policies.get(&eid) {
                    for (pv_id, lin_id, ptype, config) in env_pols {
                        let candidate = EffectivePolicy {
                            policy_version_id: *pv_id,
                            policy_lineage_id: *lin_id,
                            policy_type: ptype.clone(),
                            source: EffectivePolicySource::LegacyDirect,
                            specificity: PolicySpecificity::Environment,
                            baseline_order: None,
                            addition_order: None,
                            overrides: Vec::new(),
                            effective_config: config.clone(),
                            assignment_mode: AssignmentMode::Enforce,
                            effective_mode: AssignmentMode::Enforce,
                            provenance: Vec::new(),
                        };
                        let provenance = ProvenanceEntry {
                            source: EffectivePolicySource::LegacyDirect,
                            specificity: PolicySpecificity::Environment,
                            assignment_id: None,
                            bundle_id: None,
                            bundle_version_id: None,
                            scope_type: Some("environment".to_string()),
                            enforcement_mode: "enforce".to_string(),
                            authoritative: true,
                        };
                        if let MergeOutcome::Conflict(conflict) = merge_effective_policy_candidate(
                            candidate,
                            PolicySpecificity::Environment,
                            provenance,
                            &mut staging,
                            &mut per_lineage,
                            &mut all_warnings,
                            ignore_non_evaluation_conflicts,
                        ) {
                            results.insert(result_key, ResolutionOutcome::Conflict(vec![conflict]));
                            continue 'system;
                        }
                    }
                }
            }

            // Apply system direct policies (override environment for same lineage)
            if let Some(sys_pols) = sys_direct_policies.get(&sys_id) {
                for (pv_id, lin_id, ptype, config) in sys_pols {
                    let candidate = EffectivePolicy {
                        policy_version_id: *pv_id,
                        policy_lineage_id: *lin_id,
                        policy_type: ptype.clone(),
                        source: EffectivePolicySource::LegacyDirect,
                        specificity: PolicySpecificity::System,
                        baseline_order: None,
                        addition_order: None,
                        overrides: Vec::new(),
                        effective_config: config.clone(),
                        assignment_mode: AssignmentMode::Enforce,
                        effective_mode: AssignmentMode::Enforce,
                        provenance: Vec::new(),
                    };
                    let provenance = ProvenanceEntry {
                        source: EffectivePolicySource::LegacyDirect,
                        specificity: PolicySpecificity::System,
                        assignment_id: None,
                        bundle_id: None,
                        bundle_version_id: None,
                        scope_type: Some("system".to_string()),
                        enforcement_mode: "enforce".to_string(),
                        authoritative: true,
                    };
                    if let MergeOutcome::Conflict(conflict) = merge_effective_policy_candidate(
                        candidate,
                        PolicySpecificity::System,
                        provenance,
                        &mut staging,
                        &mut per_lineage,
                        &mut all_warnings,
                        ignore_non_evaluation_conflicts,
                    ) {
                        results.insert(result_key, ResolutionOutcome::Conflict(vec![conflict]));
                        continue 'system;
                    }
                }
            }

            if !has_assignments && !staging.is_empty() {
                all_warnings.push(
                    "Legacy direct-policy resolution used (no bundle assignments)".to_string(),
                );
            }
        }

        let all_policies = staging;
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

        results.insert(
            result_key,
            ResolutionOutcome::Resolved(EffectivePolicySet {
                bundle_version_id: primary_bundle_version_id.unwrap_or_default(),
                assignment_id: None,
                target: AssignmentTarget::System { system_id: sys_id },
                policies: all_policies,
                effective_set_digest: canonical.compute_digest(),
                warnings: all_warnings,
            }),
        );
    }

    Ok(results)
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

    let outcome = resolve_system_effective_policies_with_options_in_tx(
        &mut tx,
        system_id,
        ignore_non_evaluation_conflicts,
    )
    .await?;
    tx.commit().await.context("commit resolution transaction")?;
    Ok(outcome)
}

async fn resolve_system_effective_policies_with_options_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    system_id: Uuid,
    ignore_non_evaluation_conflicts: bool,
) -> Result<ResolutionOutcome> {
    // Load the system's environment inside the snapshot.
    let env_id: Option<Option<Uuid>> =
        sqlx::query_scalar("SELECT environment_id FROM systems WHERE id = $1")
            .bind(system_id)
            .fetch_optional(&mut **tx)
            .await
            .context("load system environment")?;

    let env_id = env_id.flatten();

    // Load all active bundle assignments with explicit semantic ordering.
    // Use the authoritative assignment snapshot: current_version_id -> compliance_bundle_assignment_versions -> bundle_version_id
    let assignments: Vec<_> =
        sqlx::query_as::<_, (Uuid, Uuid, Uuid, Uuid, String, String, String)>(
            r#"SELECT a.id, a.current_version_id, a.bundle_id, av.bundle_version_id,
                  a.scope_type, a.enforcement_mode, a.assignment_overlay_digest
            FROM compliance_bundle_assignments a
            JOIN compliance_bundle_assignment_versions av ON av.id = a.current_version_id
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
        .fetch_all(&mut **tx)
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
                  AND (pv.implementation_state NOT IN ('native', 'external', 'manual')
                       OR pv.trust_state = 'trusted')
                 AND (
                     pv.publication_state = 'accepted'
                     OR (
                         dp.current_published_version_id IS NULL
                         AND pv.publication_state IN ('incomplete', 'draft', 'interim')
                     )
                  )
               ORDER BY pv.id"#,
        )
        .bind(eid)
        .fetch_all(&mut **tx)
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
                  AND (pv.implementation_state NOT IN ('native', 'external', 'manual')
                       OR pv.trust_state = 'trusted')
                 AND (
                     pv.publication_state = 'accepted'
                     OR (
                         dp.current_published_version_id IS NULL
                         AND pv.publication_state IN ('incomplete', 'draft', 'interim')
                     )
                  )
               ORDER BY pv.id"#,
        )
        .bind(system_id)
        .fetch_all(&mut **tx)
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
        assignment_id,
        assignment_version_id,
        bundle_id,
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
        .fetch_all(&mut **tx)
        .await
        .context("load assignment exclusions")?;

        let additions: Vec<Uuid> = sqlx::query_scalar(
             "SELECT policy_version_id FROM compliance_assignment_additions WHERE assignment_version_id = $1 ORDER BY addition_order",
        )
        .bind(assignment_version_id)
        .fetch_all(&mut **tx)
        .await
        .context("load assignment additions")?;

        let overrides: Vec<PolicyOverride> = sqlx::query_as::<_, (Uuid, String, serde_json::Value)>(
            "SELECT policy_version_id, value_path, value FROM compliance_assignment_value_overrides WHERE assignment_version_id = $1",
        )
        .bind(assignment_version_id)
        .fetch_all(&mut **tx)
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

        let outcome = resolve_effective_policy_set_with_options(tx, &input).await?;

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
                        assignment_id: Some(*assignment_id),
                        bundle_id: Some(*bundle_id),
                        bundle_version_id: Some(*bundle_version_id),
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
                            return Ok(ResolutionOutcome::Conflict(vec![conflict]));
                        }
                        _ => {}
                    }
                }
            }
            ResolutionOutcome::Conflict(conflicts) => {
                return Ok(ResolutionOutcome::Conflict(conflicts));
            }
        }
    }

    // Inject direct policies (always resolved through the authoritative merge).
    for (pol, specificity) in direct_candidates {
        let prov = ProvenanceEntry {
            source: EffectivePolicySource::LegacyDirect,
            specificity,
            assignment_id: None,
            bundle_id: None,
            bundle_version_id: None,
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
                return Ok(ResolutionOutcome::Conflict(vec![conflict]));
            }
            _ => {}
        }
    }

    // The final all_policies preserves insertion order (first seen wins position).
    let all_policies = staging;

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
                  AND (pv.implementation_state NOT IN ('native', 'external', 'manual')
                       OR pv.trust_state = 'trusted')
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
                  AND (pv.implementation_state NOT IN ('native', 'external', 'manual')
                       OR pv.trust_state = 'trusted')
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

        assert!(
            validate_typed_override(
                "require_cve_check",
                &config,
                "max_critical",
                &serde_json::json!(3)
            )
            .is_ok()
        );
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

    fn candidate(
        lineage_id: Uuid,
        version_id: Uuid,
        source: EffectivePolicySource,
    ) -> EffectivePolicy {
        EffectivePolicy {
            policy_version_id: version_id,
            policy_lineage_id: lineage_id,
            policy_type: "require_cf_agent".to_string(),
            source,
            specificity: PolicySpecificity::BundleBaseline,
            baseline_order: None,
            addition_order: None,
            overrides: vec![],
            effective_config: serde_json::json!({}),
            assignment_mode: AssignmentMode::Enforce,
            effective_mode: AssignmentMode::Enforce,
            provenance: vec![],
        }
    }

    #[test]
    fn exact_policy_version_selection_prefers_higher_specificity() {
        let lineage_id = Uuid::from_u128(1);
        let baseline_version = Uuid::from_u128(2);
        let environment_version = Uuid::from_u128(3);
        let system_version = Uuid::from_u128(4);
        let mut staging = Vec::new();
        let mut per_lineage = std::collections::HashMap::new();
        let mut warnings = Vec::new();

        assert!(matches!(
            merge_effective_policy_candidate(
                candidate(
                    lineage_id,
                    baseline_version,
                    EffectivePolicySource::Baseline,
                ),
                PolicySpecificity::BundleBaseline,
                ProvenanceEntry {
                    source: EffectivePolicySource::Baseline,
                    specificity: PolicySpecificity::BundleBaseline,
                    assignment_id: None,
                    bundle_id: None,
                    bundle_version_id: None,
                    scope_type: Some("environment".to_string()),
                    enforcement_mode: "enforce".to_string(),
                    authoritative: true,
                },
                &mut staging,
                &mut per_lineage,
                &mut warnings,
                false,
            ),
            MergeOutcome::Inserted { .. }
        ));

        assert!(matches!(
            merge_effective_policy_candidate(
                candidate(
                    lineage_id,
                    environment_version,
                    EffectivePolicySource::Addition,
                ),
                PolicySpecificity::Environment,
                ProvenanceEntry {
                    source: EffectivePolicySource::Addition,
                    specificity: PolicySpecificity::Environment,
                    assignment_id: None,
                    bundle_id: None,
                    bundle_version_id: None,
                    scope_type: Some("environment".to_string()),
                    enforcement_mode: "enforce".to_string(),
                    authoritative: true,
                },
                &mut staging,
                &mut per_lineage,
                &mut warnings,
                false,
            ),
            MergeOutcome::Replaced { .. }
        ));
        assert_eq!(staging[0].policy_version_id, environment_version);

        assert!(matches!(
            merge_effective_policy_candidate(
                candidate(
                    lineage_id,
                    system_version,
                    EffectivePolicySource::LegacyDirect,
                ),
                PolicySpecificity::System,
                ProvenanceEntry {
                    source: EffectivePolicySource::LegacyDirect,
                    specificity: PolicySpecificity::System,
                    assignment_id: None,
                    bundle_id: None,
                    bundle_version_id: None,
                    scope_type: Some("system".to_string()),
                    enforcement_mode: "enforce".to_string(),
                    authoritative: true,
                },
                &mut staging,
                &mut per_lineage,
                &mut warnings,
                false,
            ),
            MergeOutcome::Replaced { .. }
        ));
        assert_eq!(staging[0].policy_version_id, system_version);
    }

    #[test]
    fn exact_same_specificity_version_conflict_is_not_silently_resolved() {
        let lineage_id = Uuid::from_u128(11);
        let first_version = Uuid::from_u128(12);
        let second_version = Uuid::from_u128(13);
        let mut staging = Vec::new();
        let mut per_lineage = std::collections::HashMap::new();
        let mut warnings = Vec::new();

        let provenance = |scope_type: &str| ProvenanceEntry {
            source: EffectivePolicySource::Addition,
            specificity: PolicySpecificity::Environment,
            assignment_id: None,
            bundle_id: None,
            bundle_version_id: None,
            scope_type: Some(scope_type.to_string()),
            enforcement_mode: "enforce".to_string(),
            authoritative: true,
        };

        assert!(matches!(
            merge_effective_policy_candidate(
                candidate(lineage_id, first_version, EffectivePolicySource::Addition),
                PolicySpecificity::Environment,
                provenance("environment"),
                &mut staging,
                &mut per_lineage,
                &mut warnings,
                false,
            ),
            MergeOutcome::Inserted { .. }
        ));

        let result = merge_effective_policy_candidate(
            candidate(lineage_id, second_version, EffectivePolicySource::Addition),
            PolicySpecificity::Environment,
            provenance("environment"),
            &mut staging,
            &mut per_lineage,
            &mut warnings,
            false,
        );
        assert!(
            matches!(result, MergeOutcome::Conflict(conflict) if conflict.code == "EFFECTIVE_POLICY_VERSION_CONFLICT")
        );
        assert_eq!(staging[0].policy_version_id, first_version);
    }

    #[tokio::test]
    #[ignore = "requires live database connection"]
    async fn published_bundle_virtual_baseline_resolves_without_explicit_assignment() {
        let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL required");
        let pool = sqlx::postgres::PgPool::connect(&database_url)
            .await
            .expect("connect test database");
        let suffix = Uuid::new_v4().simple().to_string();
        let environment_id: Uuid =
            sqlx::query_scalar("INSERT INTO environments (name) VALUES ($1) RETURNING id")
                .bind(format!("resolver-env-{suffix}"))
                .fetch_one(&pool)
                .await
                .expect("insert environment");
        let system_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO systems (id, hostname, environment_id, public_key, derivation) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(system_id)
        .bind(format!("resolver-system-{suffix}"))
        .bind(environment_id)
        .bind("ssh-ed25519 test")
        .bind("/nix/store/test-system")
        .execute(&pool)
        .await
        .expect("insert system");
        let policy_id: Uuid = sqlx::query_scalar(
            "INSERT INTO deployment_policies (name, policy_type, config) VALUES ($1, 'custom_check', $2) RETURNING id",
        )
        .bind(format!("resolver-policy-{suffix}"))
        .bind(serde_json::json!({ "expression": "config.security.auditd.enable", "strict": true }))
        .fetch_one(&pool)
        .await
        .expect("insert policy");
        let policy_version_id: Uuid =
            sqlx::query_scalar("SELECT id FROM deployment_policy_versions WHERE policy_id = $1")
                .bind(policy_id)
                .fetch_one(&pool)
                .await
                .expect("load policy version");
        let mut publish_policy_tx = pool.begin().await.expect("begin policy publication");
        sqlx::query("UPDATE deployment_policies SET current_draft_version_id = NULL WHERE id = $1 AND current_draft_version_id = $2")
            .bind(policy_id)
            .bind(policy_version_id)
            .execute(&mut *publish_policy_tx)
            .await
            .expect("clear policy draft pointer");
        sqlx::query(
            "UPDATE deployment_policy_versions SET publication_state = 'accepted', semantic_digest = $2, trust_state = 'trusted', implementation_state = 'native' WHERE id = $1",
        )
        .bind(policy_version_id)
        .bind(format!("policy-digest-{suffix}"))
        .execute(&mut *publish_policy_tx)
        .await
        .expect("publish policy version");
        sqlx::query(
            "UPDATE deployment_policies SET current_published_version_id = $1 WHERE id = $2",
        )
        .bind(policy_version_id)
        .bind(policy_id)
        .execute(&mut *publish_policy_tx)
        .await
        .expect("set policy published pointer");
        publish_policy_tx
            .commit()
            .await
            .expect("commit policy publication");

        let bundle_id: Uuid = sqlx::query_scalar(
            "INSERT INTO compliance_bundles (name, framework, version, layer, owner) VALUES ($1, 'CF-TEST', '1.0', 'os', 'Tests') RETURNING id",
        )
        .bind(format!("resolver-bundle-{suffix}"))
        .fetch_one(&pool)
        .await
        .expect("insert bundle");
        let bundle_version_id: Uuid =
            sqlx::query_scalar("SELECT id FROM compliance_bundle_versions WHERE bundle_id = $1")
                .bind(bundle_id)
                .fetch_one(&pool)
                .await
                .expect("load bundle version");
        sqlx::query(
            "INSERT INTO compliance_bundle_version_policies (bundle_version_id, policy_version_id, policy_order, selected) VALUES ($1, $2, 0, TRUE)",
        )
        .bind(bundle_version_id)
        .bind(policy_version_id)
        .execute(&pool)
        .await
        .expect("insert bundle membership");
        let mut publish_bundle_tx = pool.begin().await.expect("begin bundle publication");
        sqlx::query("UPDATE compliance_bundles SET current_draft_version_id = NULL WHERE id = $1 AND current_draft_version_id = $2")
            .bind(bundle_id)
            .bind(bundle_version_id)
            .execute(&mut *publish_bundle_tx)
            .await
            .expect("clear bundle draft pointer");
        sqlx::query(
            "UPDATE compliance_bundle_versions SET publication_state = 'accepted', semantic_digest = $2, trust_state = 'trusted' WHERE id = $1",
        )
        .bind(bundle_version_id)
        .bind(format!("bundle-digest-{suffix}"))
        .execute(&mut *publish_bundle_tx)
        .await
        .expect("publish bundle version");
        sqlx::query(
            "UPDATE compliance_bundles SET current_published_version_id = $1 WHERE id = $2",
        )
        .bind(bundle_version_id)
        .bind(bundle_id)
        .execute(&mut *publish_bundle_tx)
        .await
        .expect("set bundle published pointer");
        publish_bundle_tx
            .commit()
            .await
            .expect("commit bundle publication");
        sqlx::query(
            "INSERT INTO compliance_bundle_environments (bundle_id, environment_id) VALUES ($1, $2)",
        )
        .bind(bundle_id)
        .bind(environment_id)
        .execute(&pool)
        .await
        .expect("scope bundle to environment");

        let resolved = resolve_systems_effective_policies_for_bundle_versions_batch(
            &pool,
            &[(bundle_version_id, vec![system_id])],
        )
        .await
        .expect("resolve virtual baseline");
        assert_eq!(resolved.len(), 1);
        let outcome = resolved
            .get(&(bundle_version_id, system_id))
            .expect("requested system/version result");
        let ResolutionOutcome::Resolved(set) = outcome else {
            panic!("expected resolved virtual baseline, got {outcome:?}");
        };
        assert_eq!(set.bundle_version_id, bundle_version_id);
        let effective_policy = set
            .policies
            .iter()
            .find(|policy| policy.policy_version_id == policy_version_id)
            .expect("baseline policy must be present without an explicit assignment");
        assert_eq!(
            effective_policy.effective_config,
            serde_json::json!({ "expression": "config.security.auditd.enable", "strict": true })
        );
        let persisted_assignment_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM compliance_bundle_assignments WHERE bundle_version_id = $1 AND scope_type = 'system'",
        )
        .bind(bundle_version_id)
        .fetch_one(&pool)
        .await
        .expect("count persisted assignments");
        assert_eq!(persisted_assignment_count, 0);

        // Published compliance versions are immutable; leave the uniquely named
        // live-DB fixture rows in place instead of mutating them during cleanup.
    }
}
