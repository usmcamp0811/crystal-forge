//! Rust-authoritative canonical digest helpers.
//!
//! This module is the single canonical implementation of `cf-model-json-1`
//! digests for policies, bundles, and assignment effective-sets. SQL triggers
//! set `semantic_digest = 'pending'` as a sentinel; every Rust write path
//! must call the appropriate helper *within the same transaction* and fail if
//! the digest cannot be computed or stored.
//!
//! # Canonical field sets
//!
//! ## Policy version
//!
//! ```text
//! canonicalization_version, compliance_metadata, config, dependencies,
//! description, execution_phase, implementation_state, name, policy_type
//! ```
//!
//! ## Bundle version
//!
//! ```text
//! canonicalization_version, description, framework, framework_version,
//! layer, name, owner, policy_version_ids (ordered by policy_order)
//! ```
//!
//! ## Assignment effective-set
//!
//! ```text
//! canonicalization_version, additions (sorted UUIDs),
//! effective_policy_version_ids (resolved ordered set),
//! enforcement_mode, exclusions (sorted UUIDs),
//! value_overrides (sorted by policy_version_id, then value_path)
//! ```

use serde_json::{Value, json};
use uuid::Uuid;

use super::canonical::semantic_digest;

// ── Typed canonical DTOs ─────────────────────────────────────────────────────

/// All semantic fields for a policy version digest.
///
/// The digest must change when any field that affects activation, enforcement,
/// or exported meaning changes. Fields like timestamps, trust state, local DB
/// IDs, and assignment state are excluded.
///
/// `opaque_xml` preserves imported semantics that Crystal Forge cannot model;
/// two opaque policies with different XML but identical modeled fields must
/// produce different digests. We hash the normalized opaque content rather than
/// including raw bytes in the digest DTO.
#[derive(Debug, Clone)]
pub struct PolicyVersionCanonical {
    pub name: String,
    pub description: Option<String>,
    pub policy_type: String,
    pub implementation_state: String,
    pub execution_phase: String,
    pub config: Value,
    pub compliance_metadata: Value,
    pub dependencies: Value,
    /// SHA-256 hex of the normalised opaque XML, or `null` when absent.
    /// Included so that different preserved XML always produces a different digest.
    pub opaque_xml_digest: Option<String>,
    /// Whether the policy lineage is currently enabled. This is part of the
    /// version model's default activation state for interchange.
    pub enabled_by_default: Option<bool>,
}

impl PolicyVersionCanonical {
    pub fn to_digest_value(&self) -> Value {
        json!({
            "canonicalization_version": "cf-model-json-1",
            "compliance_metadata": self.compliance_metadata,
            "config": self.config,
            "dependencies": self.dependencies,
            "description": self.description.as_deref().unwrap_or(""),
            "enabled_by_default": self.enabled_by_default,
            "execution_phase": self.execution_phase,
            "implementation_state": self.implementation_state,
            "name": self.name,
            "opaque_xml_digest": self.opaque_xml_digest,
            "policy_type": self.policy_type,
        })
    }

    pub fn compute_digest(&self) -> String {
        semantic_digest(&self.to_digest_value())
    }

    /// Compute the sha-256 hex digest of the trimmed opaque XML, or return `None`.
    pub fn digest_opaque_xml(xml: Option<&str>) -> Option<String> {
        use sha2::{Digest as ShaDigest, Sha256};
        xml.map(|s| hex::encode(Sha256::digest(s.trim().as_bytes())))
    }
}

#[derive(Debug, Clone)]
pub struct PolicyMappingCanonical {
    pub requirement_version_id: Uuid,
    pub relationship: String,
    pub coverage: String,
    pub rationale: Option<String>,
    pub provenance: String,
    pub trust_state: String,
}

pub fn compute_mapping_digest(mappings: &mut [PolicyMappingCanonical]) -> String {
    mappings.sort_by_key(|mapping| mapping.requirement_version_id);
    let entries: Vec<Value> = mappings
        .iter()
        .map(|mapping| {
            json!({
                "coverage": mapping.coverage,
                "provenance": mapping.provenance,
                "rationale": mapping.rationale,
                "relationship": mapping.relationship,
                "requirement_version_id": mapping.requirement_version_id.to_string(),
                "trust_state": mapping.trust_state,
            })
        })
        .collect();
    semantic_digest(&json!(entries))
}

/// A single exact membership entry with both version identity and selection state.
#[derive(Debug, Clone)]
pub struct BundleMembershipEntry {
    pub policy_version_id: Uuid,
    pub selected: bool,
}

/// All semantic fields for a bundle version digest.
#[derive(Debug, Clone)]
pub struct BundleVersionCanonical {
    pub name: String,
    pub framework: String,
    pub framework_version: Option<String>,
    pub description: Option<String>,
    pub layer: String,
    pub owner: String,
    /// Ordered membership entries (by policy_order).
    pub members: Vec<BundleMembershipEntry>,
}

impl BundleVersionCanonical {
    pub fn to_digest_value(&self) -> Value {
        let members: Vec<Value> = self
            .members
            .iter()
            .map(|m| {
                json!({
                    "policy_version_id": m.policy_version_id.to_string(),
                    "selected": m.selected,
                })
            })
            .collect();
        json!({
            "canonicalization_version": "cf-model-json-1",
            "description": self.description.as_deref().unwrap_or(""),
            "framework": self.framework,
            "framework_version": self.framework_version.as_deref().unwrap_or(""),
            "layer": self.layer,
            "members": members,
            "name": self.name,
            "owner": self.owner,
        })
    }

    pub fn compute_digest(&self) -> String {
        semantic_digest(&self.to_digest_value())
    }
}

#[derive(Debug, Clone)]
pub struct BundleRequirementCanonical {
    pub requirement_version_id: Uuid,
    pub selected: bool,
}

pub fn compute_requirement_membership_digest(
    memberships: &mut [BundleRequirementCanonical],
) -> String {
    memberships.sort_by_key(|membership| membership.requirement_version_id);
    semantic_digest(&json!(
        memberships
            .iter()
            .map(|membership| json!({
                "requirement_version_id": membership.requirement_version_id.to_string(),
                "selected": membership.selected,
            }))
            .collect::<Vec<_>>()
    ))
}

/// All semantic fields for an assignment effective-set digest.
///
/// Does NOT simply copy the bundle digest. Captures the specific overlay that
/// makes this assignment distinct: exclusions, additions, value overrides, and
/// enforcement mode, combined with the ordered resolved effective policy set.
#[derive(Debug, Clone)]
pub struct AssignmentEffectiveSetCanonical {
    pub enforcement_mode: String,
    /// Sorted list of excluded policy version IDs.
    pub exclusions: Vec<Uuid>,
    /// Added policy version IDs in declared assignment order.
    pub additions: Vec<Uuid>,
    /// Value overrides sorted by (policy_version_id, value_path).
    pub value_overrides: Vec<(Uuid, String, Value)>,
    /// Final resolved effective policy version IDs in evaluation order.
    pub effective_policy_version_ids: Vec<Uuid>,
}

impl AssignmentEffectiveSetCanonical {
    pub fn to_digest_value(&self) -> Value {
        let mut exclusions: Vec<String> = self.exclusions.iter().map(|id| id.to_string()).collect();
        exclusions.sort();
        let additions: Vec<String> = self.additions.iter().map(|id| id.to_string()).collect();
        let mut overrides = self
            .value_overrides
            .iter()
            .map(|(pid, path, val)| (pid.to_string(), path.clone(), val.clone()))
            .collect::<Vec<_>>();
        overrides.sort_by(|left, right| (&left.0, &left.1).cmp(&(&right.0, &right.1)));
        let overrides: Vec<Value> = overrides
            .into_iter()
            .map(|(pid, path, val)| {
                json!({ "policy_version_id": pid, "value_path": path, "value": val })
            })
            .collect();
        let effective: Vec<String> = self
            .effective_policy_version_ids
            .iter()
            .map(|id| id.to_string())
            .collect();
        json!({
            "additions": additions,
            "canonicalization_version": "cf-model-json-1",
            "effective_policy_version_ids": effective,
            "enforcement_mode": self.enforcement_mode,
            "exclusions": exclusions,
            "value_overrides": overrides,
        })
    }

    pub fn compute_digest(&self) -> String {
        semantic_digest(&self.to_digest_value())
    }
}

/// Canonical digest input for a system's combined resolved set. Unlike the
/// assignment overlay digest, this includes every bundle source and direct
/// policy contribution that participated in resolution.
#[derive(Debug, Clone)]
pub struct CombinedEffectiveSetCanonical {
    pub bundle_version_ids_ordered: Vec<Uuid>,
    pub addition_policy_version_ids: Vec<Uuid>,
    pub direct_policy_version_ids: Vec<Uuid>,
    pub effective_policy_version_ids: Vec<Uuid>,
    pub policy_modes: Vec<(Uuid, String)>,
    pub effective_configs: Vec<(Uuid, Value)>,
}

impl CombinedEffectiveSetCanonical {
    pub fn to_digest_value(&self) -> Value {
        let mut additions: Vec<String> = self
            .addition_policy_version_ids
            .iter()
            .map(ToString::to_string)
            .collect();
        additions.sort();
        let mut direct: Vec<String> = self
            .direct_policy_version_ids
            .iter()
            .map(ToString::to_string)
            .collect();
        direct.sort();
        let modes: Vec<Value> = self
            .policy_modes
            .iter()
            .map(|(id, mode)| json!({ "policy_version_id": id.to_string(), "mode": mode }))
            .collect();
        let configs: Vec<Value> = self
            .effective_configs
            .iter()
            .map(|(id, config)| json!({ "policy_version_id": id.to_string(), "config": config }))
            .collect();
        json!({
            "canonicalization_version": "cf-model-json-1",
            "bundle_version_ids_ordered": self.bundle_version_ids_ordered,
            "addition_policy_version_ids": additions,
            "direct_policy_version_ids": direct,
            "effective_policy_version_ids": self.effective_policy_version_ids,
            "policy_modes": modes,
            "effective_configs": configs,
        })
    }

    pub fn compute_digest(&self) -> String {
        semantic_digest(&self.to_digest_value())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn base_policy() -> PolicyVersionCanonical {
        PolicyVersionCanonical {
            name: "firewall".into(),
            description: Some("Firewall enabled".into()),
            policy_type: "custom_check".into(),
            implementation_state: "native".into(),
            execution_phase: "nix-evaluation".into(),
            config: json!({"expr": "cfg.config.networking.firewall.enable"}),
            compliance_metadata: json!({}),
            dependencies: json!([]),
            opaque_xml_digest: None,
            enabled_by_default: Some(true),
        }
    }

    fn bundle(policy_ids: Vec<Uuid>) -> BundleVersionCanonical {
        BundleVersionCanonical {
            name: "Test Bundle".into(),
            framework: "STIG".into(),
            framework_version: Some("V1R1".into()),
            description: Some("Description".into()),
            layer: "os".into(),
            owner: "Team".into(),
            members: policy_ids
                .into_iter()
                .map(|id| BundleMembershipEntry {
                    policy_version_id: id,
                    selected: true,
                })
                .collect(),
        }
    }

    fn mapping(requirement_version_id: Uuid) -> PolicyMappingCanonical {
        PolicyMappingCanonical {
            requirement_version_id,
            relationship: "supports".into(),
            coverage: "partial".into(),
            rationale: Some("rationale".into()),
            provenance: "manual".into(),
            trust_state: "trusted".into(),
        }
    }

    #[test]
    fn policy_digest_is_deterministic() {
        let a = base_policy().compute_digest();
        let b = base_policy().compute_digest();
        assert_eq!(a, b);
    }

    #[test]
    fn policy_digest_changes_when_implementation_state_changes() {
        let native = base_policy().compute_digest();
        let mut unbound = base_policy();
        unbound.implementation_state = "unbound".into();
        assert_ne!(native, unbound.compute_digest());
    }

    #[test]
    fn policy_digest_changes_when_execution_phase_changes() {
        let nix = base_policy().compute_digest();
        let mut post = base_policy();
        post.execution_phase = "post-build".into();
        assert_ne!(nix, post.compute_digest());
    }

    #[test]
    fn policy_digest_changes_when_compliance_metadata_changes() {
        let a = base_policy().compute_digest();
        let mut b = base_policy();
        b.compliance_metadata = json!({"stig_id": "V-123456"});
        assert_ne!(a, b.compute_digest());
    }

    #[test]
    fn policy_digest_changes_for_each_classification_metadata_key() {
        let baseline = base_policy().compute_digest();
        let classification_edits = [
            ("category", json!("security")),
            ("framework", json!("DISA STIG")),
            ("severity", json!("high")),
            ("control_family", json!("AC")),
            ("cmmc_level", json!(2)),
            ("cis_section", json!("4.1")),
            ("rationale", json!("Required by the source control.")),
        ];

        for (key, value) in classification_edits {
            let mut policy = base_policy();
            policy.compliance_metadata = json!({key: value});
            assert_ne!(
                baseline,
                policy.compute_digest(),
                "{key} must affect the digest"
            );
        }
    }

    #[test]
    fn policy_digest_changes_when_dependencies_change() {
        let a = base_policy().compute_digest();
        let mut b = base_policy();
        b.dependencies = json!([{"nix_option": "services.example.enable"}]);
        assert_ne!(a, b.compute_digest());
    }

    #[test]
    fn policy_digest_changes_when_opaque_xml_changes() {
        let mut with_xml = base_policy();
        with_xml.opaque_xml_digest =
            PolicyVersionCanonical::digest_opaque_xml(Some("<check>A</check>"));

        let mut with_different_xml = base_policy();
        with_different_xml.opaque_xml_digest =
            PolicyVersionCanonical::digest_opaque_xml(Some("<check>B</check>"));

        let no_xml = base_policy(); // opaque_xml_digest = None

        assert_ne!(no_xml.compute_digest(), with_xml.compute_digest());
        assert_ne!(
            with_xml.compute_digest(),
            with_different_xml.compute_digest()
        );
    }

    #[test]
    fn policy_digest_changes_when_config_changes() {
        let a = base_policy().compute_digest();
        let mut b = base_policy();
        b.config = json!({"expr": "false"});
        assert_ne!(a, b.compute_digest());
    }

    #[test]
    fn policy_digest_changes_when_name_changes() {
        let a = base_policy().compute_digest();
        let mut b = base_policy();
        b.name = "other-name".into();
        assert_ne!(a, b.compute_digest());
    }

    #[test]
    fn bundle_digest_is_deterministic() {
        let ids = vec![
            Uuid::parse_str("11111111-0000-0000-0000-000000000001").unwrap(),
            Uuid::parse_str("11111111-0000-0000-0000-000000000002").unwrap(),
        ];
        assert_eq!(
            bundle(ids.clone()).compute_digest(),
            bundle(ids).compute_digest()
        );
    }

    #[test]
    fn bundle_digest_changes_on_policy_order() {
        let id1 = Uuid::parse_str("11111111-0000-0000-0000-000000000001").unwrap();
        let id2 = Uuid::parse_str("11111111-0000-0000-0000-000000000002").unwrap();
        assert_ne!(
            bundle(vec![id1, id2]).compute_digest(),
            bundle(vec![id2, id1]).compute_digest()
        );
    }

    #[test]
    fn bundle_digest_changes_on_framework_version() {
        let mut b2 = bundle(vec![]);
        b2.framework_version = Some("V1R2".into());
        assert_ne!(bundle(vec![]).compute_digest(), b2.compute_digest());
    }

    #[test]
    fn bundle_digest_changes_on_description() {
        let mut b2 = bundle(vec![]);
        b2.description = Some("Different".into());
        assert_ne!(bundle(vec![]).compute_digest(), b2.compute_digest());
    }

    #[test]
    fn assignment_digest_differs_for_different_exclusions() {
        let id = Uuid::parse_str("11111111-0000-0000-0000-000000000001").unwrap();
        let a = AssignmentEffectiveSetCanonical {
            enforcement_mode: "enforce".into(),
            exclusions: vec![],
            additions: vec![],
            value_overrides: vec![],
            effective_policy_version_ids: vec![id],
        };
        let b = AssignmentEffectiveSetCanonical {
            enforcement_mode: "enforce".into(),
            exclusions: vec![id],
            additions: vec![],
            value_overrides: vec![],
            effective_policy_version_ids: vec![],
        };
        assert_ne!(a.compute_digest(), b.compute_digest());
    }

    #[test]
    fn policy_mapping_digest_changes_for_each_semantic_field() {
        let requirement_id = Uuid::from_u128(1);
        let base = {
            let mut mappings = vec![mapping(requirement_id)];
            compute_mapping_digest(&mut mappings)
        };

        for (field, value) in [
            ("relationship", "implements"),
            ("coverage", "full"),
            ("provenance", "inherited"),
            ("trust_state", "suggested"),
        ] {
            let mut changed = mapping(requirement_id);
            match field {
                "relationship" => changed.relationship = value.into(),
                "coverage" => changed.coverage = value.into(),
                "provenance" => changed.provenance = value.into(),
                "trust_state" => changed.trust_state = value.into(),
                _ => unreachable!(),
            }
            assert_ne!(base, compute_mapping_digest(&mut vec![changed]));
        }

        let mut changed = mapping(requirement_id);
        changed.rationale = Some("different rationale".into());
        assert_ne!(base, compute_mapping_digest(&mut vec![changed]));
    }

    #[test]
    fn policy_mapping_digest_is_stable_across_insertion_order() {
        let first = Uuid::from_u128(1);
        let second = Uuid::from_u128(2);
        let mut left = vec![mapping(first), mapping(second)];
        let mut right = vec![mapping(second), mapping(first)];
        assert_eq!(
            compute_mapping_digest(&mut left),
            compute_mapping_digest(&mut right)
        );
    }

    #[test]
    fn semantic_digest_remains_independent_of_policy_mappings() {
        let semantic = base_policy().compute_digest();
        let mut changed = mapping(Uuid::from_u128(2));
        changed.coverage = "full".into();
        assert_eq!(semantic, base_policy().compute_digest());
        assert_ne!(
            compute_mapping_digest(&mut vec![mapping(Uuid::from_u128(1))]),
            compute_mapping_digest(&mut vec![changed])
        );
    }

    #[test]
    fn bundle_requirement_digest_is_order_independent_and_membership_sensitive() {
        let first = BundleRequirementCanonical {
            requirement_version_id: Uuid::from_u128(1),
            selected: true,
        };
        let second = BundleRequirementCanonical {
            requirement_version_id: Uuid::from_u128(2),
            selected: false,
        };
        let mut left = vec![first.clone(), second.clone()];
        let mut right = vec![second, first];
        assert_eq!(
            compute_requirement_membership_digest(&mut left),
            compute_requirement_membership_digest(&mut right)
        );
        right[0].selected = !right[0].selected;
        assert_ne!(
            compute_requirement_membership_digest(&mut left),
            compute_requirement_membership_digest(&mut right)
        );
    }

    #[test]
    fn assignment_digest_differs_for_enforcement_mode() {
        let a = AssignmentEffectiveSetCanonical {
            enforcement_mode: "enforce".into(),
            exclusions: vec![],
            additions: vec![],
            value_overrides: vec![],
            effective_policy_version_ids: vec![],
        };
        let b = AssignmentEffectiveSetCanonical {
            enforcement_mode: "report_only".into(),
            exclusions: vec![],
            additions: vec![],
            value_overrides: vec![],
            effective_policy_version_ids: vec![],
        };
        assert_ne!(a.compute_digest(), b.compute_digest());
    }

    #[test]
    fn assignment_override_digest_is_order_independent() {
        let first = Uuid::parse_str("11111111-0000-0000-0000-000000000001").unwrap();
        let second = Uuid::parse_str("11111111-0000-0000-0000-000000000002").unwrap();
        let a = AssignmentEffectiveSetCanonical {
            enforcement_mode: "enforce".into(),
            exclusions: vec![],
            additions: vec![],
            value_overrides: vec![
                (second, "strict".into(), json!(false)),
                (first, "count".into(), json!(1)),
            ],
            effective_policy_version_ids: vec![first, second],
        };
        let b = AssignmentEffectiveSetCanonical {
            value_overrides: vec![
                (first, "count".into(), json!(1)),
                (second, "strict".into(), json!(false)),
            ],
            ..a.clone()
        };
        assert_eq!(a.compute_digest(), b.compute_digest());
    }

    #[test]
    fn policy_digest_changes_when_enabled_by_default_changes() {
        assert_ne!(base_policy().compute_digest(), {
            let mut p = base_policy();
            p.enabled_by_default = Some(false);
            p.compute_digest()
        });
    }

    #[test]
    fn bundle_digest_changes_when_selected_changes() {
        let id = Uuid::parse_str("11111111-0000-0000-0000-000000000001").unwrap();
        let mut b = bundle(vec![id]);
        b.members[0].selected = false;
        assert_ne!(bundle(vec![id]).compute_digest(), b.compute_digest());
    }
}
