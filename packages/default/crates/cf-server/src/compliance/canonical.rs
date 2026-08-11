//! Canonical semantic JSON and SHA-256 digests for portable compliance objects.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::interchange::{CANONICALIZATION_VERSION, DIGEST_ALGORITHM};

/// Returns recursively key-sorted JSON suitable for a semantic digest.
///
/// Arrays are intentionally retained in input order. Callers must normalize
/// set-like arrays before calling this function; ordered policy rules and bundle
/// membership are semantically significant and must not be reordered here.
pub fn canonicalize_json(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), canonicalize_json(value)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(canonicalize_json).collect()),
        value => value.clone(),
    }
}

/// Produces the frozen `cf-model-json-1` SHA-256 semantic digest.
pub fn semantic_digest(value: &Value) -> String {
    let canonical = canonicalize_json(value);
    hex::encode(Sha256::digest(canonical.to_string().as_bytes()))
}

/// Identifies the digest contract persisted alongside a semantic digest.
pub const fn digest_contract() -> (&'static str, &'static str) {
    (DIGEST_ALGORITHM, CANONICALIZATION_VERSION)
}

/// Publication states shared by policy and bundle versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationState {
    Incomplete,
    Draft,
    Interim,
    Accepted,
    Deprecated,
}

impl PublicationState {
    pub const fn is_immutable(self) -> bool {
        matches!(self, Self::Accepted | Self::Deprecated)
    }
}

/// Whether Crystal Forge can activate an imported requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImplementationState {
    Native,
    Manual,
    External,
    Unbound,
    Opaque,
}

impl ImplementationState {
    pub const fn can_activate(self) -> bool {
        matches!(self, Self::Native | Self::Manual | Self::External)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn object_key_order_does_not_change_the_digest() {
        assert_eq!(
            semantic_digest(&json!({"policy": {"name": "firewall", "strict": true}})),
            semantic_digest(&json!({"policy": {"strict": true, "name": "firewall"}}))
        );
    }

    #[test]
    fn ordered_arrays_change_the_digest() {
        assert_ne!(
            semantic_digest(&json!({"rules": ["one", "two"]})),
            semantic_digest(&json!({"rules": ["two", "one"]}))
        );
    }

    #[test]
    fn publication_and_implementation_state_preserve_activation_rules() {
        assert!(PublicationState::Accepted.is_immutable());
        assert!(PublicationState::Deprecated.is_immutable());
        assert!(!PublicationState::Draft.is_immutable());
        assert!(!PublicationState::Incomplete.is_immutable());
        assert!(!PublicationState::Interim.is_immutable());
        assert!(ImplementationState::Native.can_activate());
        assert!(ImplementationState::Manual.can_activate());
        assert!(ImplementationState::External.can_activate());
        assert!(!ImplementationState::Unbound.can_activate());
        assert!(!ImplementationState::Opaque.can_activate());
    }

    // ── Bundle digest field coverage ──────────────────────────────────────────

    fn policy_canonical(name: &str, policy_type: &str, config: Value) -> Value {
        json!({
            "canonicalization_version": "cf-model-json-1",
            "config": config,
            "description": "",
            "execution_phase": "nix-evaluation",
            "implementation_state": "native",
            "name": name,
            "policy_type": policy_type,
        })
    }

    fn bundle_canonical(
        name: &str,
        framework: &str,
        framework_version: &str,
        description: &str,
        layer: &str,
        owner: &str,
        policy_ids: Vec<&str>,
    ) -> Value {
        json!({
            "canonicalization_version": "cf-model-json-1",
            "description": description,
            "framework": framework,
            "framework_version": framework_version,
            "layer": layer,
            "name": name,
            "owner": owner,
            "policy_version_ids": policy_ids,
        })
    }

    #[test]
    fn bundle_digest_changes_when_framework_version_changes() {
        let a = bundle_canonical("B", "STIG", "V1R1", "", "os", "Me", vec![]);
        let b = bundle_canonical("B", "STIG", "V1R2", "", "os", "Me", vec![]);
        assert_ne!(semantic_digest(&a), semantic_digest(&b));
    }

    #[test]
    fn bundle_digest_changes_when_description_changes() {
        let a = bundle_canonical("B", "STIG", "V1R1", "Desc A", "os", "Me", vec![]);
        let b = bundle_canonical("B", "STIG", "V1R1", "Desc B", "os", "Me", vec![]);
        assert_ne!(semantic_digest(&a), semantic_digest(&b));
    }

    #[test]
    fn bundle_digest_changes_when_membership_order_changes() {
        let a = bundle_canonical("B", "STIG", "V1R1", "", "os", "Me", vec!["id1", "id2"]);
        let b = bundle_canonical("B", "STIG", "V1R1", "", "os", "Me", vec!["id2", "id1"]);
        assert_ne!(semantic_digest(&a), semantic_digest(&b));
    }

    #[test]
    fn identical_bundles_produce_identical_digests() {
        let a = bundle_canonical("B", "STIG", "V1R1", "Desc", "os", "Me", vec!["id1"]);
        let b = bundle_canonical("B", "STIG", "V1R1", "Desc", "os", "Me", vec!["id1"]);
        assert_eq!(semantic_digest(&a), semantic_digest(&b));
    }

    #[test]
    fn policy_digest_changes_on_every_semantic_field() {
        let base = policy_canonical("firewall", "custom_check", json!({"expr": "true"}));
        let changed_name = policy_canonical("firewall2", "custom_check", json!({"expr": "true"}));
        let changed_type =
            policy_canonical("firewall", "require_packages", json!({"expr": "true"}));
        let changed_config = policy_canonical("firewall", "custom_check", json!({"expr": "false"}));

        assert_ne!(semantic_digest(&base), semantic_digest(&changed_name));
        assert_ne!(semantic_digest(&base), semantic_digest(&changed_type));
        assert_ne!(semantic_digest(&base), semantic_digest(&changed_config));
    }

    #[test]
    fn digest_key_order_does_not_affect_value() {
        // Object key order in the input must not change the digest.
        let a = json!({
            "canonicalization_version": "cf-model-json-1",
            "config": {"expr": "true"},
            "description": "",
            "execution_phase": "nix-evaluation",
            "implementation_state": "native",
            "name": "test",
            "policy_type": "custom_check",
        });
        let b = json!({
            "policy_type": "custom_check",
            "name": "test",
            "description": "",
            "implementation_state": "native",
            "execution_phase": "nix-evaluation",
            "config": {"expr": "true"},
            "canonicalization_version": "cf-model-json-1",
        });
        assert_eq!(semantic_digest(&a), semantic_digest(&b));
    }
}
