//! Shared same-import implementation reconciliation.
//!
//! When multiple requirements within a single import have identical normalized
//! technical enforcement, Crystal Forge detects this and allows them to share
//! a single policy.
//!
//! This reduces duplicate policies when a compliance framework has multiple
//! rules requiring the same system configuration.

use serde_json::{Map, Value};
use uuid::Uuid;

use crate::compliance::xccdf::exact_technical_match::RequirementTechnicalIdentity;

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
        use sha2::{Sha256, Digest};

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

/// A group of imported requirements sharing identical normalized technical enforcement.
#[derive(Debug, Clone)]
pub struct SharedImplementationGroup {
    /// Unique stable identifier for this shared enforcement group.
    pub group_id: SharedImplementationId,
    /// The technical enforcement this group implements.
    pub technical_identity: RequirementTechnicalIdentity,
    /// Requirement canonical keys that share this enforcement.
    pub requirement_keys: Vec<String>,
    /// Whether an existing accepted policy can satisfy all requirements in this group.
    pub existing_policy_candidate: Option<(Uuid, String, u8)>, // (policy_id, name, confidence)
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
        groups.entry(group_id.clone()).or_insert_with(Vec::new).push(req_key);
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
                existing_policy_candidate: None, // Will be filled by caller
                action: SharedImplementationAction::ReviewIndividually, // Default; will be refined
            })
        })
        .collect()
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

/// Validate that a shared implementation group is still valid at commit time.
///
/// Revalidates that:
/// 1. All requirements still have the same technical identity
/// 2. Group identity is deterministic and stable
///
/// Returns:
/// - Ok if valid
/// - Err with IMPORT_SHARED_IMPLEMENTATION_STALE if requirements changed
pub fn validate_shared_group_at_commit(
    group: &SharedImplementationGroup,
    authoritative_identities: Vec<(&str, &RequirementTechnicalIdentity)>,
) -> Result<(), String> {
    // Verify all requirements in the group are still present in authoritative data
    for req_key in &group.requirement_keys {
        if !authoritative_identities.iter().any(|(k, _)| k == req_key) {
            return Err(format!(
                "IMPORT_SHARED_IMPLEMENTATION_STALE: requirement {} no longer present in import",
                req_key
            ));
        }
    }

    // Verify all requirements still have identical technical enforcement
    let expected_id = group.group_id.clone();
    for (req_key, identity) in &authoritative_identities {
        if !group.requirement_keys.contains(&req_key.to_string()) {
            continue; // Not part of this group
        }

        let current_id = SharedImplementationId::from_technical_identity(identity);
        if current_id != expected_id {
            return Err(format!(
                "IMPORT_SHARED_IMPLEMENTATION_STALE: requirement {} enforcement changed",
                req_key
            ));
        }
    }

    Ok(())
}

/// Parameters for creating a shared policy for a group of requirements.
///
/// Used during the import commit phase to create a single policy that
/// satisfies multiple requirements with identical technical enforcement.
#[derive(Debug, Clone)]
pub struct SharedPolicyCreationParams {
    /// Stable policy ID (generated from group ID to ensure determinism)
    pub policy_id: Uuid,
    /// Shared policy version ID
    pub policy_version_id: Uuid,
    /// User-readable name derived from requirements and technical identity
    pub name: String,
    /// Description of the shared enforcement
    pub description: String,
    /// Policy config (Nix options normalized from technical identity)
    pub config: Value,
    /// Compliance metadata (JSON encoding group membership)
    pub compliance_metadata: Value,
    /// Requirements (keys) that share this policy
    pub requirement_keys: Vec<String>,
}

/// Generate policy IDs for shared implementations.
///
/// Note: IDs are generated randomly at preview time; the stable group identity
/// persists in the group_id field which is stored in the database.
/// At commit time, the group_id is re-validated against authoritative source bytes.
pub fn generate_shared_policy_ids(_group: &SharedImplementationGroup) -> (Uuid, Uuid) {
    // Generate fresh UUIDs for each shared policy
    // Stability comes from persistent storage of group_id and revalidation at commit
    let policy_id = Uuid::new_v4();
    let version_id = Uuid::new_v4();
    
    (policy_id, version_id)
}

/// A shared policy to be created at commit time, along with its associated requirements.
///
/// Used internally during commit_foreign_import to track which requirements
/// should be mapped to the same policy lineage.
#[derive(Debug, Clone)]
pub struct SharedPolicyCommitRecord {
    /// Policy lineage ID
    pub policy_id: Uuid,
    /// Policy version ID
    pub policy_version_id: Uuid,
    /// Requirement keys (rule IDs) that map to this policy
    pub requirement_keys: Vec<String>,
    /// Group identity (for revalidation)
    pub group_id: SharedImplementationId,
}

/// Identify which policy_records belong to shared groups that need to be created.
///
/// Called at commit time (after detecting shared groups from authoritative source)
/// to partition the policy_records into:
/// - Records that share implementation (will map to one new shared policy)
/// - Records that stand alone (will each get their own policy)
///
/// Shared groups only trigger when:
/// - Multiple records have identical technical identity
/// - No existing accepted policy covers all members
///
/// Returns (shared_groups_to_create, individual_policy_record_indices)
pub fn partition_shared_and_individual_policies(
    groups: &[SharedImplementationGroup],
    policy_records: &[crate::compliance::xccdf::import_models::ImportedPolicyRecord],
    rule_to_record_idx: &std::collections::HashMap<String, usize>,
) -> (Vec<SharedPolicyCommitRecord>, Vec<usize>) {
    let mut shared_records: Vec<SharedPolicyCommitRecord> = Vec::new();
    let mut shared_rule_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for group in groups {
        if group.existing_policy_candidate.is_some() {
            // Existing policy covers all members; don't create new shared policy
            continue;
        }

        // Create one shared policy for this group
        let (policy_id, version_id) = generate_shared_policy_ids(group);
        shared_records.push(SharedPolicyCommitRecord {
            policy_id,
            policy_version_id: version_id,
            requirement_keys: group.requirement_keys.clone(),
            group_id: group.group_id.clone(),
        });

        for req_key in &group.requirement_keys {
            shared_rule_ids.insert(req_key.clone());
        }
    }

    // Individual records are those NOT in any shared group
    let individual_indices: Vec<usize> = policy_records
        .iter()
        .enumerate()
        .filter_map(|(idx, rec)| {
            if shared_rule_ids.contains(&rec.source_rule_id) {
                None
            } else {
                Some(idx)
            }
        })
        .collect();

    (shared_records, individual_indices)
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
        assert_eq!(groups.len(), 1, "should create one group for identical enforcement");
        assert_eq!(groups[0].requirement_keys.len(), 3, "should contain all three requirements");
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
        assert_eq!(groups.len(), 0, "different values should not create a shared group");
    }

    #[test]
    fn different_option_sets_not_grouped() {
        let mut options_a = Map::new();
        options_a.insert("services.openssh.enable".to_string(), Value::String("false".to_string()));
        options_a.insert("services.openssh.settings.PermitRootLogin".to_string(), Value::String("no".to_string()));

        let mut options_b = Map::new();
        options_b.insert("services.openssh.enable".to_string(), Value::String("false".to_string()));

        let identity_a = RequirementTechnicalIdentity {
            enforced_options: options_a,
        };
        let identity_b = RequirementTechnicalIdentity {
            enforced_options: options_b,
        };

        let requirements = vec![("V-111".to_string(), identity_a), ("V-222".to_string(), identity_b)];

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
            ("V-111".to_string(), RequirementTechnicalIdentity {
                enforced_options: Map::new(),
            }),
            ("V-222".to_string(), RequirementTechnicalIdentity {
                enforced_options: Map::new(),
            }),
        ];

        let groups = detect_shared_implementations(requirements);
        assert_eq!(groups.len(), 0, "empty enforcement should not create a group");
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
        
        let group = SharedImplementationGroup {
            group_id: SharedImplementationId { technical_hash: "test".to_string() },
            technical_identity: identity_from_options(&[("services.openssh.enable", "false")]),
            requirement_keys: vec!["V-111".to_string(), "V-222".to_string(), "V-333".to_string()],
            existing_policy_candidate: None,
            action: SharedImplementationAction::ReviewIndividually,
        };

        let (remaining_group, breakout) = remove_from_shared_group(group, "V-222");
        assert_eq!(breakout, "V-222");
        assert!(remaining_group.is_some(), "group should remain with 2 requirements");
        assert_eq!(remaining_group.unwrap().requirement_keys.len(), 2);
    }

    #[test]
    fn test_remove_last_from_shared_group() {
        use super::remove_from_shared_group;
        
        let group = SharedImplementationGroup {
            group_id: SharedImplementationId { technical_hash: "test".to_string() },
            technical_identity: identity_from_options(&[("services.openssh.enable", "false")]),
            requirement_keys: vec!["V-111".to_string(), "V-222".to_string()],
            existing_policy_candidate: None,
            action: SharedImplementationAction::ReviewIndividually,
        };

        let (remaining_group, _breakout) = remove_from_shared_group(group, "V-111");
        assert!(remaining_group.is_none(), "group should be removed when only 1 requirement remains");
    }

    #[test]
    fn test_recommend_action_without_candidate() {
        use super::recommend_action;

        let group = SharedImplementationGroup {
            group_id: SharedImplementationId { technical_hash: "test".to_string() },
            technical_identity: identity_from_options(&[("services.openssh.enable", "false")]),
            requirement_keys: vec!["V-111".to_string(), "V-222".to_string()],
            existing_policy_candidate: None,
            action: SharedImplementationAction::ReviewIndividually,
        };

        assert_eq!(
            recommend_action(&group),
            SharedImplementationAction::CreateShared,
            "should recommend CreateShared when no existing policy"
        );
    }

    #[test]
    fn test_recommend_action_with_candidate() {
        use super::recommend_action;

        let group = SharedImplementationGroup {
            group_id: SharedImplementationId { technical_hash: "test".to_string() },
            technical_identity: identity_from_options(&[("services.openssh.enable", "false")]),
            requirement_keys: vec!["V-111".to_string(), "V-222".to_string()],
            existing_policy_candidate: Some((Uuid::new_v4(), "existing-policy".to_string(), 90)),
            action: SharedImplementationAction::ReviewIndividually,
        };

        assert_eq!(
            recommend_action(&group),
            SharedImplementationAction::ReuseExisting,
            "should recommend ReuseExisting when existing policy candidate found"
        );
    }

    #[test]
    fn test_validate_shared_group_at_commit_valid() {
        use super::validate_shared_group_at_commit;

        let identity = identity_from_options(&[("services.openssh.enable", "false")]);
        let group = SharedImplementationGroup {
            group_id: SharedImplementationId::from_technical_identity(&identity),
            technical_identity: identity.clone(),
            requirement_keys: vec!["V-111".to_string(), "V-222".to_string()],
            existing_policy_candidate: None,
            action: SharedImplementationAction::CreateShared,
        };

        let authoritative = vec![
            ("V-111" as &str, &identity),
            ("V-222" as &str, &identity),
        ];

        assert!(
            validate_shared_group_at_commit(&group, authoritative).is_ok(),
            "should validate group with unchanged enforcement"
        );
    }

    #[test]
    fn test_validate_shared_group_at_commit_stale_enforcement() {
        use super::validate_shared_group_at_commit;

        let identity = identity_from_options(&[("services.openssh.enable", "false")]);
        let group = SharedImplementationGroup {
            group_id: SharedImplementationId::from_technical_identity(&identity),
            technical_identity: identity.clone(),
            requirement_keys: vec!["V-111".to_string(), "V-222".to_string()],
            existing_policy_candidate: None,
            action: SharedImplementationAction::CreateShared,
        };

        // Changed enforcement for one requirement
        let changed_identity = identity_from_options(&[("services.openssh.enable", "true")]);
        let authoritative = vec![
            ("V-111" as &str, &identity),
            ("V-222" as &str, &changed_identity),
        ];

        let result = validate_shared_group_at_commit(&group, authoritative);
        assert!(result.is_err(), "should reject group with changed enforcement");
        assert!(
            result.unwrap_err().contains("IMPORT_SHARED_IMPLEMENTATION_STALE"),
            "error should indicate stale group"
        );
    }

    #[test]
    fn test_validate_shared_group_at_commit_missing_requirement() {
        use super::validate_shared_group_at_commit;

        let identity = identity_from_options(&[("services.openssh.enable", "false")]);
        let group = SharedImplementationGroup {
            group_id: SharedImplementationId::from_technical_identity(&identity),
            technical_identity: identity.clone(),
            requirement_keys: vec!["V-111".to_string(), "V-222".to_string()],
            existing_policy_candidate: None,
            action: SharedImplementationAction::CreateShared,
        };

        // Only one requirement present
        let authoritative = vec![("V-111" as &str, &identity)];

        let result = validate_shared_group_at_commit(&group, authoritative);
        assert!(result.is_err(), "should reject group when requirement missing");
        assert!(
            result.unwrap_err().contains("no longer present"),
            "error should indicate missing requirement"
        );
    }

    #[test]
    fn test_generate_shared_policy_ids_fresh() {
        use super::generate_shared_policy_ids;

        let identity = identity_from_options(&[("services.openssh.enable", "false")]);
        let group = SharedImplementationGroup {
            group_id: SharedImplementationId::from_technical_identity(&identity),
            technical_identity: identity,
            requirement_keys: vec!["V-111".to_string(), "V-222".to_string()],
            existing_policy_candidate: None,
            action: SharedImplementationAction::CreateShared,
        };

        let (policy_id1, version_id1) = generate_shared_policy_ids(&group);
        let (policy_id2, version_id2) = generate_shared_policy_ids(&group);

        // IDs are fresh (non-deterministic); determinism comes from persistent group_id
        assert_ne!(policy_id1, policy_id2, "policy IDs should be fresh each call");
        assert_ne!(version_id1, version_id2, "version IDs should be fresh each call");
        assert_ne!(policy_id1, version_id1, "policy and version IDs should differ");
    }

    #[test]
    fn test_partition_shared_and_individual_with_no_groups() {
        use super::partition_shared_and_individual_policies;

        let groups = vec![];
        let policy_records = vec![]; // Empty for this test
        let rule_map = std::collections::HashMap::new();

        let (shared, individual) = partition_shared_and_individual_policies(&groups, &policy_records, &rule_map);

        assert_eq!(shared.len(), 0, "no groups should produce no shared records");
        assert_eq!(individual.len(), 0, "no records should produce no individual records");
    }

    #[test]
    fn test_partition_shared_with_existing_candidate() {
        use super::partition_shared_and_individual_policies;

        let identity = identity_from_options(&[("services.openssh.enable", "false")]);
        let group = SharedImplementationGroup {
            group_id: SharedImplementationId::from_technical_identity(&identity),
            technical_identity: identity,
            requirement_keys: vec!["V-111".to_string(), "V-222".to_string()],
            existing_policy_candidate: Some((Uuid::new_v4(), "existing".to_string(), 90)),
            action: SharedImplementationAction::ReuseExisting,
        };

        let groups = vec![group];
        let policy_records = vec![];
        let rule_map = std::collections::HashMap::new();

        let (shared, _individual) = partition_shared_and_individual_policies(&groups, &policy_records, &rule_map);

        assert_eq!(
            shared.len(),
            0,
            "group with existing candidate should not create new shared policy"
        );
    }

    #[test]
    fn test_partition_shared_without_candidate() {
        use super::partition_shared_and_individual_policies;

        let identity = identity_from_options(&[("services.openssh.enable", "false")]);
        let group = SharedImplementationGroup {
            group_id: SharedImplementationId::from_technical_identity(&identity),
            technical_identity: identity,
            requirement_keys: vec!["V-111".to_string(), "V-222".to_string()],
            existing_policy_candidate: None,
            action: SharedImplementationAction::CreateShared,
        };

        let groups = vec![group];
        let policy_records = vec![];
        let rule_map = std::collections::HashMap::new();

        let (shared, _individual) = partition_shared_and_individual_policies(&groups, &policy_records, &rule_map);

        assert_eq!(
            shared.len(),
            1,
            "group without existing candidate should create new shared policy"
        );
        assert_eq!(
            shared[0].requirement_keys.len(),
            2,
            "shared policy should have both requirements"
        );
    }
}
