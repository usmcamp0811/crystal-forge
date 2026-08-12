//! Shared same-import implementation reconciliation.
//!
//! When multiple requirements within a single import have identical normalized
//! technical enforcement, Crystal Forge detects this and allows them to share
//! a single policy.
//!
//! This reduces duplicate policies when a compliance framework has multiple
//! rules requiring the same system configuration.
//!
//! A shared group is a *reconciliation recommendation about implementation
//! reuse*. It never merges the requirements themselves: requirements remain
//! distinct authoritative compliance objects, each with its own mapping
//! semantics, and the group only describes which policy satisfies them.

use serde_json::{Map, Value};
use std::collections::HashMap;
use uuid::Uuid;

use crate::compliance::requirement_model::PolicyCandidate;
use crate::compliance::xccdf::exact_technical_match::RequirementTechnicalIdentity;
use crate::compliance::xccdf::import_models::{ImportedPolicyRecord, MapExistingProof};

// ── Validation errors ─────────────────────────────────────────────────────────

/// Typed error result from shared-group validation.
///
/// All validation failures during commit produce this error with code
/// IMPORT_SHARED_IMPLEMENTATION_STALE and a descriptive message.
#[derive(Debug, Clone)]
pub struct SharedValidationError {
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for SharedValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for SharedValidationError {}

/// A shared-group decision that has passed all authoritative validation checks.
/// This is the trust boundary: nothing below this type should inspect raw
/// client SharedGroupDecision values.
///
/// All fields are server-derived or verified server-side:
/// - policy_id, policy_version_id: generated UUIDs for this shared policy
/// - group_id: derived from authoritative technical identity
/// - requirement_keys: validated client-selected rule IDs that have passed authoritative validation
/// - technical_identity: authoritative enforcement inferred from parsed rules
#[derive(Debug, Clone)]
pub struct ValidatedSharedCreation {
    pub policy_id: Uuid,
    pub policy_version_id: Uuid,
    pub group_id: SharedImplementationId,
    pub requirement_keys: Vec<String>,
    pub technical_identity: RequirementTechnicalIdentity,
}

/// Stable identity for a shared technical implementation group.
///
/// Derived deterministically from the normalized technical enforcement.
/// Used to detect shared groups at preview time and validate them at commit.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SharedImplementationId {
    /// Hash of canonical technical identity.
    /// Use to identify the group stably across preview/commit cycle.
    pub technical_hash: String,
}

impl SharedImplementationId {
    /// Derive a stable group ID from a technical identity.
    ///
    /// Uses SHA-256 hash of canonical JSON representation of enforced options.
    pub fn from_technical_identity(identity: &RequirementTechnicalIdentity) -> Self {
        use sha2::{Digest, Sha256};

        // Create a deterministic string representation for hashing.
        let canonical_json = serde_json::to_string(&identity.enforced_options)
            .unwrap_or_else(|_| "error".to_string());
        let mut hasher = Sha256::new();
        hasher.update(canonical_json.as_bytes());
        let result = hasher.finalize();
        let technical_hash = format!("{:x}", result);

        SharedImplementationId { technical_hash }
    }
}

/// An exact immutable policy version that can satisfy every member of a
/// shared implementation group.
///
/// The reusable object is a policy *version*, not a lineage: two members may
/// only share a candidate when the exact same `policy_version_id` is eligible
/// for each of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedPolicyCandidate {
    pub policy_id: Uuid,
    pub policy_version_id: Uuid,
    pub policy_name: String,
    pub confidence: u8,
}

/// A group of imported requirements sharing identical normalized technical enforcement.
#[derive(Debug, Clone)]
pub struct SharedImplementationGroup {
    /// Unique stable identifier for this shared enforcement group.
    pub group_id: SharedImplementationId,
    /// The technical enforcement this group implements.
    pub technical_identity: RequirementTechnicalIdentity,
    /// Requirement canonical keys that share this enforcement.
    pub requirement_keys: Vec<String>,
    /// An existing policy version that is a valid candidate for *every* member.
    /// `None` when no single exact policy version is common to all members.
    pub existing_policy_candidate: Option<SharedPolicyCandidate>,
    /// Per-member reuse evidence for `existing_policy_candidate`.
    /// Each member may reach the common candidate through a different proof;
    /// the group does not collapse those into one group-wide proof.
    pub member_proofs: HashMap<String, MapExistingProof>,
    /// Recommended action for this group.
    pub action: SharedImplementationAction,
}

/// Recommended action for a shared implementation group during import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedImplementationAction {
    /// Reuse an existing accepted policy.
    ReuseExisting,
    /// Create one new policy for all requirements in the group.
    CreateShared,
    /// Review each requirement independently (user override).
    ReviewIndividually,
}

/// Detect and group imported requirements that share technical implementation.
///
/// This function takes a set of requirements, each with an inferred technical
/// identity, and groups them by exact normalized technical enforcement.
///
/// Returns a Vec of `SharedImplementationGroup` where each group contains
/// requirements with identical enforcement.
///
/// # Arguments
/// - `requirements_with_identity`: Vec of (canonical_key, technical_identity, user_action)
///
/// # Returns
/// Map of SharedImplementationId -> SharedImplementationGroup
pub fn detect_shared_implementations(
    requirements_with_identity: Vec<(String, RequirementTechnicalIdentity)>,
) -> Vec<SharedImplementationGroup> {
    use std::collections::HashMap;

    // Group requirements by technical identity hash.
    let mut groups: HashMap<SharedImplementationId, Vec<String>> = HashMap::new();
    let mut identity_map: HashMap<SharedImplementationId, RequirementTechnicalIdentity> =
        HashMap::new();

    for (req_key, identity) in requirements_with_identity {
        // Skip groups with no technical enforcement.
        if identity.enforced_options.is_empty() {
            continue;
        }

        let group_id = SharedImplementationId::from_technical_identity(&identity);
        groups
            .entry(group_id.clone())
            .or_insert_with(Vec::new)
            .push(req_key);
        identity_map.insert(group_id, identity);
    }

    // Convert to SharedImplementationGroup, filtering single-requirement groups.
    groups
        .into_iter()
        .filter_map(|(group_id, requirement_keys)| {
            // Only return groups with 2+ requirements.
            // Single requirements should go through normal candidate discovery.
            if requirement_keys.len() < 2 {
                return None;
            }

            let technical_identity = identity_map.get(&group_id)?.clone();

            Some(SharedImplementationGroup {
                group_id,
                technical_identity,
                requirement_keys,
                existing_policy_candidate: None, // Filled by caller via candidate intersection
                member_proofs: HashMap::new(),   // Filled by caller per member
                action: SharedImplementationAction::ReviewIndividually, // Default; will be refined
            })
        })
        .collect()
}

/// Compute the common existing-policy candidate for a shared group.
///
/// A shared group may recommend one existing policy only if the **same exact
/// policy version** is a valid candidate for every participating requirement.
///
/// Candidate sets are keyed by `policy_version_id` per requirement; the
/// common candidate is the version present in every member's set. When no
/// single version is common to all members, returns `None` (the group should
/// fall back to `CreateShared` even if individual members have unrelated
/// candidates).
///
/// # Arguments
/// - `member_candidates`: candidate list for each member, keyed by rule ID.
///   Rules absent from the map are treated as having no candidates.
/// - `member_rule_ids`: the rules belonging to the group.
pub fn common_shared_candidate(
    member_candidates: &HashMap<String, Vec<PolicyCandidate>>,
    member_rule_ids: &[String],
) -> Option<SharedPolicyCandidate> {
    // Build per-requirement candidate sets keyed by exact policy version.
    let mut version_sets: Vec<HashMap<Uuid, &PolicyCandidate>> = Vec::new();
    for rule_id in member_rule_ids {
        let candidates = member_candidates.get(rule_id);
        let set: HashMap<Uuid, &PolicyCandidate> = match candidates {
            Some(list) => list.iter().map(|c| (c.policy_version_id, c)).collect(),
            None => HashMap::new(),
        };
        version_sets.push(set);
    }

    // Intersection across all members.
    let mut common: HashMap<Uuid, &PolicyCandidate> = HashMap::new();
    for (version_id, candidate) in &version_sets[0] {
        if version_sets.iter().all(|set| set.contains_key(version_id)) {
            common.insert(*version_id, *candidate);
        }
    }

    // Deterministic pick: highest total confidence across members, then lowest
    // version UUID as tie-breaker.
    common
        .into_iter()
        .max_by(|(v1, c1), (v2, c2)| {
            let conf1 = total_confidence(&version_sets, v1);
            let conf2 = total_confidence(&version_sets, v2);
            conf1
                .cmp(&conf2)
                .then_with(|| c2.policy_version_id.cmp(&c1.policy_version_id))
        })
        .map(|(version_id, candidate)| SharedPolicyCandidate {
            policy_id: candidate.policy_id,
            policy_version_id: version_id,
            policy_name: candidate.policy_name.clone(),
            confidence: candidate.confidence,
        })
}

fn total_confidence(version_sets: &[HashMap<Uuid, &PolicyCandidate>], version_id: &Uuid) -> u32 {
    version_sets
        .iter()
        .filter_map(|set| set.get(version_id))
        .map(|c| c.confidence as u32)
        .sum()
}

/// Filter requirements and group ID to remove breakouts.
///
/// When a user decides to review a requirement individually instead of with
/// its shared group, this function updates the group membership.
///
/// Returns:
/// - Updated group (may be removed if only 1 requirement remains)
/// - The breakout requirement key
pub fn remove_from_shared_group(
    mut group: SharedImplementationGroup,
    breakout_key: &str,
) -> (Option<SharedImplementationGroup>, String) {
    group.requirement_keys.retain(|k| k != breakout_key);

    let result_group = if group.requirement_keys.len() >= 2 {
        Some(group)
    } else {
        None
    };

    (result_group, breakout_key.to_string())
}

/// Determine the recommended action for a shared implementation group.
///
/// Logic:
/// 1. If an existing accepted policy matches, recommend ReuseExisting
/// 2. Otherwise, recommend CreateShared
/// 3. User can override to ReviewIndividually
pub fn recommend_action(group: &SharedImplementationGroup) -> SharedImplementationAction {
    if group.existing_policy_candidate.is_some() {
        SharedImplementationAction::ReuseExisting
    } else {
        SharedImplementationAction::CreateShared
    }
}

/// Parameters for creating a shared policy for a group of requirements.
///
/// Used during the import commit phase to create a single policy that
/// satisfies multiple requirements with identical technical enforcement.
///
/// The policy is built from *technical behavior*, never from the first member
/// requirement's framework metadata. Requirement identity and per-requirement
/// mapping semantics live in `policy_requirement_mappings`; the reusable policy
/// itself stays framework-neutral.
#[derive(Debug, Clone)]
pub struct SharedPolicyCreationParams {
    /// New policy lineage ID.
    pub policy_id: Uuid,
    /// New policy version ID.
    pub policy_version_id: Uuid,
    /// Human-readable name derived from the technical enforcement.
    pub name: String,
    /// Description of the shared enforcement.
    pub description: String,
    /// Policy config built from the normalized technical identity.
    pub config: Value,
    /// Requirements (keys) that share this policy.
    pub requirement_keys: Vec<String>,
}

/// Generate policy IDs for shared implementations.
///
/// IDs are freshly generated per commit. Determinism/idempotency is provided
/// by the transaction boundary, source-artifact identity, requirement
/// reconciliation, and persisted mappings — not by the policy UUID itself.
pub fn generate_shared_policy_ids(_group: &SharedImplementationGroup) -> (Uuid, Uuid) {
    (Uuid::new_v4(), Uuid::new_v4())
}

// ── Import policy resolution plan ─────────────────────────────────────────────

/// Explicit resolution outcome for exactly one imported requirement.
///
/// Every imported requirement that needs a policy has exactly one of these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyResolution {
    /// Reuse an existing policy version (MapExisting action).
    ReuseExisting { selected_policy_version_id: Uuid },
    /// Create one new shared policy for a group of requirements.
    CreateShared { group_id: SharedImplementationId },
    /// Create an individual policy for a single requirement record.
    CreateIndividual { record_index: usize },
}

/// A new shared policy to create for one shared-implementation decision.
///
/// Created by the resolution planner from ValidatedSharedCreation.
/// Contains the authoritative server-generated IDs and technical identity.
#[derive(Debug, Clone)]
pub struct SharedCreation {
    pub policy_id: Uuid,
    pub policy_version_id: Uuid,
    /// Rule IDs that map to this shared policy (validated client-selected IDs).
    pub requirement_keys: Vec<String>,
    /// Group identity derived from authoritative technical enforcement.
    pub group_id: SharedImplementationId,
    /// Authoritative technical enforcement for this shared policy.
    pub technical_identity: RequirementTechnicalIdentity,
}

/// A group of requirements reusing one existing policy version.
#[derive(Debug, Clone)]
pub struct SharedReuse {
    pub selected_policy_version_id: Uuid,
    pub requirement_keys: Vec<String>,
}

/// Complete resolution plan for the foreign import commit.
///
/// Every non-excluded requirement record has exactly one entry in
/// `rule_resolutions`; `rule_to_policy_version` is derived from this plan
/// after policy creation/reuse resolution.
#[derive(Debug, Clone, Default)]
pub struct ImportPolicyResolutionPlan {
    pub shared_creations: Vec<SharedCreation>,
    pub shared_reuses: Vec<SharedReuse>,
    pub individual_creations: Vec<usize>,
    /// rule_id -> selected published version for MapExisting actions.
    pub individual_reuses: HashMap<String, Uuid>,
    /// Every imported requirement that needs a policy has exactly one entry.
    pub rule_resolutions: HashMap<String, PolicyResolution>,
}

/// Build the import policy resolution plan from validated shared creations and policy records.
///
/// The plan is pure (no I/O) so it can be unit tested with real records.
/// The planner receives **only** ValidatedSharedCreation objects - no raw client input.
///
/// Rules:
/// - Every ValidatedSharedCreation becomes one SharedCreation in the plan (no UUID generation).
/// - Every record with `mapped_policy_version_id` (MapExisting) resolves to
///   `ReuseExisting` with that exact version.
/// - Every other record resolves to an individual creation.
pub fn build_import_policy_resolution_plan(
    validated_shared_creations: &[ValidatedSharedCreation],
    policy_records: &[ImportedPolicyRecord],
) -> Result<ImportPolicyResolutionPlan, String> {
    let mut plan = ImportPolicyResolutionPlan::default();

    // 1. Process validated shared creations first.
    // These have already passed all authoritative validation.
    for validated in validated_shared_creations {
        // Reuse the authoritative server-generated UUIDs and identity from validation.
        for rule_id in &validated.requirement_keys {
            plan.rule_resolutions.insert(
                rule_id.clone(),
                PolicyResolution::CreateShared {
                    group_id: validated.group_id.clone(),
                },
            );
        }
        plan.shared_creations.push(SharedCreation {
            policy_id: validated.policy_id,
            policy_version_id: validated.policy_version_id,
            requirement_keys: validated.requirement_keys.clone(),
            group_id: validated.group_id.clone(),
            technical_identity: validated.technical_identity.clone(),
        });
    }

    // 2. MapExisting records -> reuse.
    for (idx, rec) in policy_records.iter().enumerate() {
        if let Some(version_id) = rec.mapped_policy_version_id {
            if plan.rule_resolutions.contains_key(&rec.source_rule_id) {
                // Already covered by a CreateShared decision; the contradiction
                // was rejected above, so this should not happen.
                continue;
            }
            plan.individual_reuses
                .insert(rec.source_rule_id.clone(), version_id);
            plan.rule_resolutions.insert(
                rec.source_rule_id.clone(),
                PolicyResolution::ReuseExisting {
                    selected_policy_version_id: version_id,
                },
            );
        }
    }

    // 3. Everything else -> individual creation.
    for (idx, rec) in policy_records.iter().enumerate() {
        if plan.rule_resolutions.contains_key(&rec.source_rule_id) {
            continue;
        }
        plan.individual_creations.push(idx);
        plan.rule_resolutions.insert(
            rec.source_rule_id.clone(),
            PolicyResolution::CreateIndividual { record_index: idx },
        );
    }

    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compliance::xccdf::inference::NixosLiteralValue;

    fn identity_from_options(options: &[(&str, &str)]) -> RequirementTechnicalIdentity {
        let mut map = Map::new();
        for (key, val) in options {
            map.insert(key.to_string(), Value::String(val.to_string()));
        }
        RequirementTechnicalIdentity {
            enforced_options: map,
        }
    }

    fn make_group(
        keys: &[&str],
        candidate: Option<SharedPolicyCandidate>,
    ) -> SharedImplementationGroup {
        let identity = identity_from_options(&[("services.openssh.enable", "false")]);
        SharedImplementationGroup {
            group_id: SharedImplementationId::from_technical_identity(&identity),
            technical_identity: identity,
            requirement_keys: keys.iter().map(|k| k.to_string()).collect(),
            existing_policy_candidate: candidate,
            member_proofs: HashMap::new(),
            action: SharedImplementationAction::ReviewIndividually,
        }
    }

    fn make_record(
        rule_id: &str,
        implementation_state: &str,
        mapped_version: Option<Uuid>,
    ) -> ImportedPolicyRecord {
        ImportedPolicyRecord {
            policy_id: Uuid::new_v4(),
            policy_version_id: Uuid::new_v4(),
            source_rule_id: rule_id.to_string(),
            source_rule_order: 0,
            implementation_state: implementation_state.to_string(),
            policy_type: "imported_xccdf".to_string(),
            version: None,
            execution_phase: "not-applicable".to_string(),
            config: serde_json::json!({}),
            dependencies: serde_json::json!([]),
            enabled_by_default: false,
            portable: false,
            semantic_digest: None,
            selected: true,
            policy_order: 0,
            name: rule_id.to_string(),
            description: None,
            compliance_metadata: serde_json::json!({}),
            opaque_xml: None,
            mapped_policy_version_id: mapped_version,
            mapped_policy_proof: None,
            mapping_semantics: None,
            evidence_requirements: Vec::new(),
        }
    }

    #[test]
    fn detect_exact_shared_group() {
        let requirements = vec![
            (
                "V-111".to_string(),
                identity_from_options(&[("services.openssh.enable", "false")]),
            ),
            (
                "V-222".to_string(),
                identity_from_options(&[("services.openssh.enable", "false")]),
            ),
            (
                "V-333".to_string(),
                identity_from_options(&[("services.openssh.enable", "false")]),
            ),
        ];

        let groups = detect_shared_implementations(requirements);
        assert_eq!(
            groups.len(),
            1,
            "should create one group for identical enforcement"
        );
        assert_eq!(
            groups[0].requirement_keys.len(),
            3,
            "should contain all three requirements"
        );
        assert_eq!(
            groups[0].requirement_keys,
            vec!["V-111", "V-222", "V-333"],
            "requirements should be preserved"
        );
    }

    #[test]
    fn different_values_not_grouped() {
        let requirements = vec![
            (
                "V-111".to_string(),
                identity_from_options(&[("services.openssh.enable", "true")]),
            ),
            (
                "V-222".to_string(),
                identity_from_options(&[("services.openssh.enable", "false")]),
            ),
        ];

        let groups = detect_shared_implementations(requirements);
        assert_eq!(
            groups.len(),
            0,
            "different values should not create a shared group"
        );
    }

    #[test]
    fn different_option_sets_not_grouped() {
        let mut options_a = Map::new();
        options_a.insert(
            "services.openssh.enable".to_string(),
            Value::String("false".to_string()),
        );
        options_a.insert(
            "services.openssh.settings.PermitRootLogin".to_string(),
            Value::String("no".to_string()),
        );

        let mut options_b = Map::new();
        options_b.insert(
            "services.openssh.enable".to_string(),
            Value::String("false".to_string()),
        );

        let identity_a = RequirementTechnicalIdentity {
            enforced_options: options_a,
        };
        let identity_b = RequirementTechnicalIdentity {
            enforced_options: options_b,
        };

        let requirements = vec![
            ("V-111".to_string(), identity_a),
            ("V-222".to_string(), identity_b),
        ];

        let groups = detect_shared_implementations(requirements);
        assert_eq!(
            groups.len(),
            0,
            "different option sets should not create a shared group"
        );
    }

    #[test]
    fn single_requirement_not_grouped() {
        let requirements = vec![(
            "V-111".to_string(),
            identity_from_options(&[("services.openssh.enable", "false")]),
        )];

        let groups = detect_shared_implementations(requirements);
        assert_eq!(
            groups.len(),
            0,
            "single requirement should not create a shared group"
        );
    }

    #[test]
    fn empty_enforcement_not_grouped() {
        let requirements = vec![
            (
                "V-111".to_string(),
                RequirementTechnicalIdentity {
                    enforced_options: Map::new(),
                },
            ),
            (
                "V-222".to_string(),
                RequirementTechnicalIdentity {
                    enforced_options: Map::new(),
                },
            ),
        ];

        let groups = detect_shared_implementations(requirements);
        assert_eq!(
            groups.len(),
            0,
            "empty enforcement should not create a group"
        );
    }

    #[test]
    fn group_id_stable() {
        let identity = identity_from_options(&[("services.openssh.enable", "false")]);
        let id1 = SharedImplementationId::from_technical_identity(&identity);
        let id2 = SharedImplementationId::from_technical_identity(&identity);

        assert_eq!(id1, id2, "group IDs should be deterministic");
    }

    #[test]
    fn test_remove_from_shared_group() {
        use super::remove_from_shared_group;

        let group = make_group(&["V-111", "V-222", "V-333"], None);
        let (remaining_group, breakout) = remove_from_shared_group(group, "V-222");
        assert_eq!(breakout, "V-222");
        assert!(
            remaining_group.is_some(),
            "group should remain with 2 requirements"
        );
        assert_eq!(remaining_group.unwrap().requirement_keys.len(), 2);
    }

    #[test]
    fn test_remove_last_from_shared_group() {
        use super::remove_from_shared_group;

        let group = make_group(&["V-111", "V-222"], None);
        let (remaining_group, _breakout) = remove_from_shared_group(group, "V-111");
        assert!(
            remaining_group.is_none(),
            "group should be removed when only 1 requirement remains"
        );
    }

    #[test]
    fn test_recommend_action_without_candidate() {
        use super::recommend_action;

        let group = make_group(&["V-111", "V-222"], None);
        assert_eq!(
            recommend_action(&group),
            SharedImplementationAction::CreateShared,
            "should recommend CreateShared when no existing policy"
        );
    }

    #[test]
    fn test_recommend_action_with_candidate() {
        use super::recommend_action;

        let candidate = SharedPolicyCandidate {
            policy_id: Uuid::new_v4(),
            policy_version_id: Uuid::new_v4(),
            policy_name: "existing-policy".to_string(),
            confidence: 90,
        };
        let group = make_group(&["V-111", "V-222"], Some(candidate));
        assert_eq!(
            recommend_action(&group),
            SharedImplementationAction::ReuseExisting,
            "should recommend ReuseExisting when existing policy candidate found"
        );
    }

    #[test]
    fn test_generate_shared_policy_ids_fresh() {
        use super::generate_shared_policy_ids;

        let group = make_group(&["V-111", "V-222"], None);
        let (policy_id1, version_id1) = generate_shared_policy_ids(&group);
        let (policy_id2, version_id2) = generate_shared_policy_ids(&group);

        // IDs are fresh (non-deterministic); determinism comes from the import
        // transaction and persisted mappings, not the UUID itself.
        assert_ne!(
            policy_id1, policy_id2,
            "policy IDs should be fresh each call"
        );
        assert_ne!(
            version_id1, version_id2,
            "version IDs should be fresh each call"
        );
        assert_ne!(
            policy_id1, version_id1,
            "policy and version IDs should differ"
        );
    }

    // ── Common candidate intersection ─────────────────────────────────────────

    fn candidate(version: Uuid, confidence: u8) -> PolicyCandidate {
        PolicyCandidate {
            policy_id: Uuid::new_v4(),
            policy_version_id: version,
            policy_name: format!("policy-{}", &version.simple().to_string()[..8]),
            match_type:
                crate::compliance::requirement_model::PolicyCandidateMatchType::ExactTechnicalMatch,
            confidence,
            match_reasons: vec!["exact technical match".to_string()],
        }
    }

    #[test]
    fn test_common_candidate_intersection() {
        use super::common_shared_candidate;

        let p17 = Uuid::new_v4();
        let p20 = Uuid::new_v4();
        let p44 = Uuid::new_v4();
        let mut members: HashMap<String, Vec<PolicyCandidate>> = HashMap::new();
        members.insert(
            "V-111".to_string(),
            vec![candidate(p17, 90), candidate(p20, 70)],
        );
        members.insert("V-222".to_string(), vec![candidate(p17, 90)]);
        members.insert(
            "V-333".to_string(),
            vec![candidate(p17, 90), candidate(p44, 80)],
        );

        let common = common_shared_candidate(
            &members,
            &[
                "V-111".to_string(),
                "V-222".to_string(),
                "V-333".to_string(),
            ],
        );
        let common = common.expect("P17 is common to all members");
        assert_eq!(
            common.policy_version_id, p17,
            "only the version present in every member's set may be common"
        );
    }

    #[test]
    fn test_common_candidate_none_when_not_common() {
        use super::common_shared_candidate;

        let p17 = Uuid::new_v4();
        let p44 = Uuid::new_v4();
        let mut members: HashMap<String, Vec<PolicyCandidate>> = HashMap::new();
        members.insert("V-111".to_string(), vec![candidate(p17, 90)]);
        members.insert("V-222".to_string(), vec![candidate(p44, 80)]);

        let common = common_shared_candidate(&members, &["V-111".to_string(), "V-222".to_string()]);
        assert!(
            common.is_none(),
            "no exact version is common to both members"
        );
    }

    #[test]
    fn test_common_candidate_not_collapsed_by_lineage() {
        use super::common_shared_candidate;

        // Same lineage, different versions: must NOT be treated as a common candidate.
        let p17v2 = Uuid::new_v4();
        let p17v3 = Uuid::new_v4();
        let mut members: HashMap<String, Vec<PolicyCandidate>> = HashMap::new();
        members.insert("V-111".to_string(), vec![candidate(p17v2, 95)]);
        members.insert("V-222".to_string(), vec![candidate(p17v3, 95)]);

        let common = common_shared_candidate(&members, &["V-111".to_string(), "V-222".to_string()]);
        assert!(
            common.is_none(),
            "different versions of one lineage are not a common candidate"
        );
    }

    // ── Resolution planner (item 18) ─────────────────────────────────────────

    fn make_validated_shared_creation(rule_ids: &[&str]) -> ValidatedSharedCreation {
        use serde_json::{Map, json};

        // Create a simple technical identity with enforced options
        let mut enforced_options = Map::new();
        enforced_options.insert(
            "services.openssh.settings.PermitRootLogin".to_string(),
            json!("no"),
        );
        let identity = RequirementTechnicalIdentity { enforced_options };
        let group_id = SharedImplementationId::from_technical_identity(&identity);

        ValidatedSharedCreation {
            policy_id: Uuid::new_v4(),
            policy_version_id: Uuid::new_v4(),
            group_id,
            requirement_keys: rule_ids.iter().map(|r| r.to_string()).collect(),
            technical_identity: identity,
        }
    }

    #[test]
    fn test_planner_existing_candidate_group_reuses() {
        use super::build_import_policy_resolution_plan;

        // A and B both reuse the same existing policy version: no new policies.
        let version = Uuid::new_v4();
        let records = vec![
            make_record("A", "mapped", Some(version)),
            make_record("B", "mapped", Some(version)),
        ];

        let plan = build_import_policy_resolution_plan(&[], &records).unwrap();

        assert_eq!(
            plan.shared_creations.len(),
            0,
            "no shared creations for pure reuse"
        );
        assert_eq!(
            plan.individual_creations.len(),
            0,
            "no individual creations for reuse"
        );
        assert_eq!(plan.individual_reuses.len(), 2);
        assert_eq!(plan.individual_reuses["A"], version);
        assert_eq!(plan.individual_reuses["B"], version);
        assert!(matches!(
            plan.rule_resolutions["A"],
            PolicyResolution::ReuseExisting { selected_policy_version_id } if selected_policy_version_id == version
        ));
        assert!(matches!(
            plan.rule_resolutions["B"],
            PolicyResolution::ReuseExisting { selected_policy_version_id } if selected_policy_version_id == version
        ));
    }

    #[test]
    fn test_planner_shared_new_group() {
        use super::build_import_policy_resolution_plan;

        let records = vec![
            make_record("A", "native", None),
            make_record("B", "native", None),
            make_record("C", "native", None),
        ];
        let validated = vec![make_validated_shared_creation(&["A", "B", "C"])];

        let plan = build_import_policy_resolution_plan(&validated, &records).unwrap();

        assert_eq!(
            plan.shared_creations.len(),
            1,
            "exactly one shared creation"
        );
        assert_eq!(
            plan.shared_creations[0].requirement_keys,
            vec!["A", "B", "C"]
        );
        assert_eq!(
            plan.individual_creations.len(),
            0,
            "no individual creations for A/B/C"
        );
        assert!(matches!(
            plan.rule_resolutions["A"],
            PolicyResolution::CreateShared { .. }
        ));
        assert!(matches!(
            plan.rule_resolutions["B"],
            PolicyResolution::CreateShared { .. }
        ));
        assert!(matches!(
            plan.rule_resolutions["C"],
            PolicyResolution::CreateShared { .. }
        ));
    }

    #[test]
    fn test_planner_breakout() {
        use super::build_import_policy_resolution_plan;

        // A/B share; C is broken out to an individual policy.
        let records = vec![
            make_record("A", "native", None),
            make_record("B", "native", None),
            make_record("C", "native", None),
        ];
        let validated = vec![make_validated_shared_creation(&["A", "B"])];

        let plan = build_import_policy_resolution_plan(&validated, &records).unwrap();

        assert_eq!(plan.shared_creations.len(), 1);
        assert_eq!(plan.shared_creations[0].requirement_keys, vec!["A", "B"]);
        assert_eq!(
            plan.individual_creations,
            vec![2],
            "C gets its own individual creation"
        );
        assert!(matches!(
            plan.rule_resolutions["C"],
            PolicyResolution::CreateIndividual { record_index: 2 }
        ));
    }

    #[test]
    fn test_planner_mixed_unrelated_record() {
        use super::build_import_policy_resolution_plan;

        let records = vec![
            make_record("A", "native", None),
            make_record("B", "native", None),
            make_record("C", "native", None),
        ];
        let validated = vec![make_validated_shared_creation(&["A", "B"])];

        let plan = build_import_policy_resolution_plan(&validated, &records).unwrap();

        assert_eq!(plan.shared_creations.len(), 1, "A/B shared");
        assert_eq!(
            plan.individual_creations.len(),
            1,
            "C unrelated -> individual"
        );
    }

    #[test]
    fn test_planner_preserves_validated_ids_and_identity() {
        use super::build_import_policy_resolution_plan;

        // Proof that policy_id, policy_version_id, group_id, requirement_keys,
        // and technical_identity survive unchanged through the planner.
        use serde_json::{Map, json};

        let mut enforced_options = Map::new();
        enforced_options.insert(
            "services.openssh.settings.PermitRootLogin".to_string(),
            json!("no"),
        );
        let identity = RequirementTechnicalIdentity { enforced_options };
        let group_id = SharedImplementationId::from_technical_identity(&identity);
        let policy_id = Uuid::new_v4();
        let policy_version_id = Uuid::new_v4();

        let validated = vec![ValidatedSharedCreation {
            policy_id,
            policy_version_id,
            group_id: group_id.clone(),
            requirement_keys: vec!["A".to_string(), "B".to_string()],
            technical_identity: identity.clone(),
        }];

        let records = vec![
            make_record("A", "native", None),
            make_record("B", "native", None),
        ];

        let plan = build_import_policy_resolution_plan(&validated, &records).unwrap();

        assert_eq!(plan.shared_creations.len(), 1);
        let shared = &plan.shared_creations[0];
        assert_eq!(shared.policy_id, policy_id, "policy_id must be preserved");
        assert_eq!(
            shared.policy_version_id, policy_version_id,
            "policy_version_id must be preserved"
        );
        assert_eq!(shared.group_id, group_id, "group_id must be preserved");
        assert_eq!(
            shared.requirement_keys, validated[0].requirement_keys,
            "requirement_keys must be preserved"
        );
        assert_eq!(
            shared.technical_identity, identity,
            "technical_identity must be preserved"
        );
    }

    #[test]
    fn test_planner_individual_without_decisions() {
        use super::build_import_policy_resolution_plan;

        let records = vec![
            make_record("A", "native", None),
            make_record("B", "manual", None),
            make_record("C", "opaque", None),
        ];

        let plan = build_import_policy_resolution_plan(&[], &records).unwrap();

        assert_eq!(
            plan.shared_creations.len(),
            0,
            "no decisions -> no shared creation"
        );
        assert_eq!(
            plan.individual_creations.len(),
            3,
            "each record gets its own policy"
        );
    }
}
