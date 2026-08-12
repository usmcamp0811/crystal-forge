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
}
