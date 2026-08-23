//! Canonical Crystal Forge policy interchange documents (JSON and TOML).
//!
//! This is the single parser for the `urn:crystal-forge:policy-set:1` policy-set
//! document and for the bare single-policy export object. It is shared by the
//! server's policy import endpoints and by offline tools such as the
//! `cf-nixos-module` generator, so that both apply identical normalization,
//! compatibility, and digest-verification rules.
//!
//! # Compatibility inputs
//!
//! A bare `expression` field (the pre-versioning simplified shape) implies
//! `policy_type = "custom_check"` and synthesizes an equivalent `config`.
//!
//! # Digest verification
//!
//! When a document supplies `semantic_digest`, it is recomputed from the
//! normalized fields with [`PolicyVersionCanonical`] and a mismatch is an error.
//! Documents that omit the digest are accepted and the computed digest is
//! returned.

use std::collections::HashSet;

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::digest::PolicyVersionCanonical;

/// A policy version normalized from an interchange document.
#[derive(Debug, Clone)]
pub struct NormalizedPolicyImport {
    pub lineage_id: Uuid,
    pub version_id: Uuid,
    pub version: String,
    pub publication_state: String,
    pub name: String,
    pub description: Option<String>,
    pub policy_type: String,
    pub implementation_state: String,
    pub execution_phase: String,
    pub config: serde_json::Value,
    pub compliance_metadata: serde_json::Value,
    pub dependencies: serde_json::Value,
    pub opaque_xml: Option<String>,
    pub enabled_by_default: Option<bool>,
    pub semantic_digest: String,
}

/// Serialization format of a policy interchange document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyDocumentFormat {
    Json,
    Toml,
}

impl PolicyDocumentFormat {
    /// Resolve the format from a filename extension, defaulting to JSON when no
    /// filename is available.
    pub fn from_filename(filename: Option<&str>) -> Result<Self, String> {
        let extension = filename
            .and_then(|name| name.rsplit('.').next())
            .unwrap_or("json")
            .to_ascii_lowercase();
        match extension.as_str() {
            "toml" => Ok(Self::Toml),
            "json" => Ok(Self::Json),
            _ => Err("Policy interchange format must be JSON or TOML".to_string()),
        }
    }
}

/// Deterministic portable UUID for compatibility documents that carry no
/// explicit `lineage_id` / `version_id`.
///
/// Derived from the source digest and the policy's ordinal so that re-importing
/// or regenerating from identical bytes yields identical identities.
pub fn generate_compatibility_policy_uuid(
    source_sha256: &str,
    ordinal: usize,
    field: &str, // "lineage" or "version"
) -> Uuid {
    let seed = format!(
        "crystal-forge:policy-compat-{}:1:{}:{}",
        field, source_sha256, ordinal
    );
    let hash = Sha256::digest(seed.as_bytes());

    // Convert first 16 bytes of SHA-256 to UUID.
    // This is deterministic and collision-resistant.
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    Uuid::from_bytes(bytes)
}

/// Decode raw document bytes into the individual raw policy objects.
///
/// Accepts a `{"policies": [...]}` policy-set document or a single bare policy
/// object (detected by the presence of `policy_type`).
pub fn split_policy_document(
    bytes: &[u8],
    filename: Option<&str>,
) -> Result<Vec<serde_json::Value>, String> {
    let document = match PolicyDocumentFormat::from_filename(filename)? {
        PolicyDocumentFormat::Toml => std::str::from_utf8(bytes)
            .ok()
            .and_then(|text| toml::from_str::<toml::Value>(text).ok())
            .and_then(|value| serde_json::to_value(value).ok())
            .ok_or_else(|| "Policy TOML is invalid".to_string())?,
        PolicyDocumentFormat::Json => serde_json::from_slice::<serde_json::Value>(bytes)
            .map_err(|_| "Policy JSON is invalid".to_string())?,
    };
    let raw_policies = match document.get("policies") {
        Some(serde_json::Value::Array(policies)) => policies.clone(),
        Some(_) => return Err("The policies field must be an array".to_string()),
        None if document.get("policy_type").is_some() => vec![document],
        None => return Err("Policy interchange document must contain policies".to_string()),
    };
    if raw_policies.is_empty() {
        return Err("Policy interchange document contains no policies".to_string());
    }
    Ok(raw_policies)
}

/// Parse and normalize a policy interchange document.
///
/// When `source_sha256` is supplied, policies without explicit portable
/// identities receive deterministic compatibility UUIDs derived from it.
/// When it is `None`, such policies receive random UUIDs, which is only
/// appropriate for non-durable preview flows.
pub fn parse_policy_document(
    bytes: &[u8],
    filename: Option<&str>,
    source_sha256: Option<&str>,
) -> Result<Vec<NormalizedPolicyImport>, String> {
    split_policy_document(bytes, filename)?
        .into_iter()
        .enumerate()
        .map(|(idx, raw)| normalize_policy_import(raw, source_sha256, None, idx))
        .collect()
}

/// Reject documents that declare the same immutable version identity twice.
pub fn validate_policy_interchange_document(
    policies: &[NormalizedPolicyImport],
) -> Result<(), String> {
    let mut seen_versions = HashSet::new();
    for policy in policies {
        if !seen_versions.insert(policy.version_id) {
            return Err(format!(
                "Duplicate version ID {} in import document",
                policy.version_id
            ));
        }
    }
    Ok(())
}

/// Normalize one raw policy object and verify its semantic digest when present.
pub fn normalize_policy_import(
    raw: serde_json::Value,
    source_sha256: Option<&str>,
    compat_seed: Option<&str>,
    ordinal: usize,
) -> Result<NormalizedPolicyImport, String> {
    let object = raw
        .as_object()
        .ok_or_else(|| "Each imported policy must be an object".to_string())?;
    let compatibility_expression = object.get("expression").and_then(serde_json::Value::as_str);

    // Portable IDs: if explicit, use them; if missing, generate deterministically.
    let lineage_id =
        if let Some(lid_str) = object.get("lineage_id").and_then(serde_json::Value::as_str) {
            Uuid::parse_str(lid_str).map_err(|_| "lineage_id is not a UUID".to_string())?
        } else if let (Some(source_sha), _) = (source_sha256, compat_seed) {
            generate_compatibility_policy_uuid(source_sha, ordinal, "lineage")
        } else {
            Uuid::new_v4()
        };

    let version_id =
        if let Some(vid_str) = object.get("version_id").and_then(serde_json::Value::as_str) {
            Uuid::parse_str(vid_str).map_err(|_| "version_id is not a UUID".to_string())?
        } else if let (Some(source_sha), _) = (source_sha256, compat_seed) {
            generate_compatibility_policy_uuid(source_sha, ordinal, "version")
        } else {
            Uuid::new_v4()
        };
    let name = object
        .get("name")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Imported policy is missing name".to_string())?
        .to_string();
    let policy_type = object
        .get("policy_type")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .or_else(|| compatibility_expression.map(|_| "custom_check".to_string()))
        .ok_or_else(|| "Imported policy is missing policy_type".to_string())?;
    let config = object.get("config").cloned().unwrap_or_else(|| {
        compatibility_expression
            .map(|expression| {
                serde_json::json!({
                    "expression": expression,
                    "strict": object
                        .get("strict")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(true),
                })
            })
            .unwrap_or_else(|| serde_json::json!({}))
    });
    let version = object
        .get("version")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("0.1.0")
        .to_string();
    // Server-side policy import remains compatible with older documents that
    // omitted lifecycle state; those documents are drafts. Offline module
    // generation rejects that state because an export must prove immutability.
    let publication_state = object
        .get("publication_state")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("draft")
        .to_string();
    let description = object
        .get("description")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let implementation_state = object
        .get("implementation_state")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("native")
        .to_string();
    let execution_phase = object
        .get("execution_phase")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("nix-evaluation")
        .to_string();
    let compliance_metadata = object
        .get("compliance_metadata")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let dependencies = object
        .get("dependencies")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));
    let opaque_xml = object
        .get("opaque_xml")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let enabled_by_default = object
        .get("enabled_by_default")
        .and_then(serde_json::Value::as_bool);
    let canonical = PolicyVersionCanonical {
        name: name.clone(),
        description: description.clone(),
        policy_type: policy_type.clone(),
        implementation_state: implementation_state.clone(),
        execution_phase: execution_phase.clone(),
        config: config.clone(),
        compliance_metadata: compliance_metadata.clone(),
        dependencies: dependencies.clone(),
        opaque_xml_digest: PolicyVersionCanonical::digest_opaque_xml(opaque_xml.as_deref()),
        enabled_by_default,
    };
    let computed_digest = canonical.compute_digest();
    if let Some(expected) = object
        .get("semantic_digest")
        .and_then(serde_json::Value::as_str)
    {
        if expected != computed_digest {
            return Err("semantic_digest does not match the imported policy fields".to_string());
        }
    }
    Ok(NormalizedPolicyImport {
        lineage_id,
        version_id,
        version,
        publication_state,
        name,
        description,
        policy_type,
        implementation_state,
        execution_phase,
        config,
        compliance_metadata,
        dependencies,
        opaque_xml,
        enabled_by_default,
        semantic_digest: computed_digest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy_set_json() -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "schema": "urn:crystal-forge:policy-set:1",
            "version": "1",
            "policies": [
                {
                    "lineage_id": "11111111-1111-1111-1111-111111111111",
                    "version_id": "22222222-2222-2222-2222-222222222222",
                    "publication_state": "accepted",
                    "name": "firewall-enabled",
                    "policy_type": "custom_check",
                    "config": {"expression": "config.networking.firewall.enable == true"},
                }
            ],
        }))
        .expect("serialize fixture")
    }

    #[test]
    fn parses_policy_set_document() {
        let parsed = parse_policy_document(&policy_set_json(), Some("policy-set.json"), None)
            .expect("document parses");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "firewall-enabled");
        assert_eq!(parsed[0].policy_type, "custom_check");
        // Unspecified fields fall back to the canonical defaults.
        assert_eq!(parsed[0].implementation_state, "native");
        assert_eq!(parsed[0].execution_phase, "nix-evaluation");
        assert_eq!(parsed[0].version, "0.1.0");
    }

    #[test]
    fn parses_bare_single_policy_object() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "name": "solo",
            "publication_state": "accepted",
            "policy_type": "custom_check",
            "config": {"expression": "config.a.b == true"},
        }))
        .expect("serialize");
        let parsed =
            parse_policy_document(&bytes, Some("policy.json"), None).expect("document parses");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "solo");
    }

    #[test]
    fn compatibility_expression_becomes_custom_check() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "policies": [{
                "name": "legacy",
                "publication_state": "accepted",
                "expression": "config.services.openssh.enable == true",
                "strict": false,
            }],
        }))
        .expect("serialize");
        let parsed =
            parse_policy_document(&bytes, Some("legacy.json"), None).expect("document parses");
        assert_eq!(parsed[0].policy_type, "custom_check");
        assert_eq!(
            parsed[0].config["expression"],
            "config.services.openssh.enable == true"
        );
        assert_eq!(parsed[0].config["strict"], false);
    }

    #[test]
    fn mismatched_semantic_digest_is_rejected() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "policies": [{
                "name": "tampered",
                "publication_state": "accepted",
                "policy_type": "custom_check",
                "config": {"expression": "config.a.b == true"},
                "semantic_digest": "0".repeat(64),
            }],
        }))
        .expect("serialize");
        let error = parse_policy_document(&bytes, Some("p.json"), None)
            .expect_err("tampered digest must be rejected");
        assert!(
            error.contains("semantic_digest"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn matching_semantic_digest_is_accepted() {
        let parsed = parse_policy_document(&policy_set_json(), Some("p.json"), None)
            .expect("baseline parses");
        let digest = parsed[0].semantic_digest.clone();
        let bytes = serde_json::to_vec(&serde_json::json!({
            "policies": [{
                "lineage_id": "11111111-1111-1111-1111-111111111111",
                "version_id": "22222222-2222-2222-2222-222222222222",
                "publication_state": "accepted",
                "name": "firewall-enabled",
                "policy_type": "custom_check",
                "config": {"expression": "config.networking.firewall.enable == true"},
                "semantic_digest": digest,
            }],
        }))
        .expect("serialize");
        parse_policy_document(&bytes, Some("p.json"), None).expect("matching digest is accepted");
    }

    #[test]
    fn compatibility_uuids_are_deterministic_for_identical_sources() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "policies": [{
                "name": "no-ids",
                "publication_state": "accepted",
                "policy_type": "custom_check",
                "config": {"expression": "config.a.b == true"},
            }],
        }))
        .expect("serialize");
        let sha = "a".repeat(64);
        let first = parse_policy_document(&bytes, Some("p.json"), Some(&sha)).expect("parse");
        let second = parse_policy_document(&bytes, Some("p.json"), Some(&sha)).expect("parse");
        assert_eq!(first[0].version_id, second[0].version_id);
        assert_eq!(first[0].lineage_id, second[0].lineage_id);
        assert_ne!(first[0].version_id, first[0].lineage_id);
    }

    #[test]
    fn duplicate_version_identities_are_rejected() {
        let shared = "22222222-2222-2222-2222-222222222222";
        let bytes = serde_json::to_vec(&serde_json::json!({
            "policies": [
                {"version_id": shared, "name": "a", "publication_state": "accepted", "policy_type": "custom_check", "config": {}},
                {"version_id": shared, "name": "b", "publication_state": "accepted", "policy_type": "custom_check", "config": {}},
            ],
        }))
        .expect("serialize");
        let parsed = parse_policy_document(&bytes, Some("p.json"), None).expect("parse");
        let error = validate_policy_interchange_document(&parsed)
            .expect_err("duplicate identity must be rejected");
        assert!(error.contains("Duplicate version ID"), "got: {error}");
    }

    #[test]
    fn unsupported_extension_is_rejected() {
        let error = parse_policy_document(b"{}", Some("policies.xml"), None)
            .expect_err("xml is not a policy document format");
        assert!(error.contains("JSON or TOML"), "got: {error}");
    }

    #[test]
    fn toml_documents_parse_to_the_same_model() {
        let toml_text = r#"
[[policies]]
name = "firewall-enabled"
publication_state = "accepted"
policy_type = "custom_check"

[policies.config]
expression = "config.networking.firewall.enable == true"
"#;
        let parsed = parse_policy_document(toml_text.as_bytes(), Some("policy-set.toml"), None)
            .expect("toml parses");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "firewall-enabled");
        assert_eq!(
            parsed[0].config["expression"],
            "config.networking.firewall.enable == true"
        );
    }
}
