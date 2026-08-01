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
        assert!(!PublicationState::Draft.is_immutable());
        assert!(ImplementationState::Native.can_activate());
        assert!(!ImplementationState::Unbound.can_activate());
        assert!(!ImplementationState::Opaque.can_activate());
    }
}
