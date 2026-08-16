//! Resolve the effective policy set from all inputs.
//!
//! Multiple inputs may overlap. Identity, deduplication, and conflict rules are
//! delegated to [`cf_compliance::xccdf::reconciliation`], the same planner the
//! server uses for CF-native import, by reconciling each new input against the
//! policies accumulated so far. This guarantees the generator cannot accept a
//! combination the server would reject, or reject one the server would accept.

use std::collections::BTreeMap;

use cf_compliance::xccdf::reconciliation::{
    ExistingPolicyIdentity, NativePolicyIdentity, ReconcileConflict, ReconcileDecision,
    plan_policy_reconciliation,
};
use uuid::Uuid;

use crate::input::LoadedInput;
use crate::model::{ResolvedBundle, ResolvedPolicy};

/// The deduplicated, conflict-free policy set plus the bundles that selected it.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// Effective policy versions, ordered deterministically.
    pub policies: Vec<ResolvedPolicy>,
    /// Bundle versions contributing to the selection, ordered deterministically.
    pub bundles: Vec<ResolvedBundle>,
    /// Human-readable notes about duplicates that were collapsed.
    pub deduplicated: Vec<String>,
}

/// A blocking disagreement between two inputs about one immutable identity.
#[derive(Debug, Clone)]
pub struct SelectionConflict {
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for SelectionConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

/// Combine every loaded input into one effective policy set.
///
/// Conflicting definitions for the same immutable policy/version identity are
/// returned as errors and never resolved automatically.
pub fn select_policies(inputs: &[LoadedInput]) -> Result<Selection, Vec<SelectionConflict>> {
    let mut accepted: BTreeMap<Uuid, ResolvedPolicy> = BTreeMap::new();
    let mut bundles: Vec<ResolvedBundle> = Vec::new();
    let mut deduplicated: Vec<String> = Vec::new();
    let mut conflicts: Vec<SelectionConflict> = Vec::new();

    for input in inputs {
        // Reconcile this input's policies against everything accepted so far.
        let existing: Vec<ExistingPolicyIdentity> = accepted
            .values()
            .map(|policy| ExistingPolicyIdentity {
                lineage_id: policy.policy_id,
                version_id: policy.policy_version_id,
                policy_type: policy.policy_type.clone(),
                semantic_digest: policy.semantic_digest.clone(),
            })
            .collect();

        let imported: Vec<NativePolicyIdentity> = input
            .policies
            .iter()
            .map(|policy| NativePolicyIdentity {
                lineage_id: policy.policy_id,
                version_id: policy.policy_version_id,
                policy_type: policy.policy_type.clone(),
                semantic_digest: policy.semantic_digest.clone(),
                source_rule_id: policy.name.clone(),
            })
            .collect();

        let plan = plan_policy_reconciliation(&imported, &existing);

        for conflict in &plan.conflicts {
            conflicts.push(describe_conflict(conflict, &input.label));
        }
        if !plan.conflicts.is_empty() {
            continue;
        }

        for (identity, decision) in plan.decisions {
            let Some(policy) = input
                .policies
                .iter()
                .find(|candidate| candidate.policy_version_id == identity.version_id)
            else {
                continue;
            };

            match decision {
                ReconcileDecision::ReuseExact { .. } => {
                    // Identical duplicate definitions are acceptable: keep the
                    // first occurrence so output does not depend on input order
                    // beyond the first definition.
                    deduplicated.push(format!(
                        "{} ({}) also appears in {}",
                        policy.name, policy.policy_version_id, input.label
                    ));
                }
                ReconcileDecision::CreateLineageAndVersion { .. }
                | ReconcileDecision::CreateVersionInExistingLineage { .. } => {
                    accepted.insert(policy.policy_version_id, policy.clone());
                }
            }
        }

        if let Some(bundle) = &input.bundle {
            bundles.push(bundle.clone());
        }
    }

    if !conflicts.is_empty() {
        return Err(conflicts);
    }

    let mut policies: Vec<ResolvedPolicy> = accepted.into_values().collect();
    // Stable, content-derived ordering so output never depends on input order.
    policies.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then(a.policy_version_id.cmp(&b.policy_version_id))
    });

    bundles.sort_by_key(|bundle| bundle.bundle_version_id);
    deduplicated.sort();
    deduplicated.dedup();

    Ok(Selection {
        policies,
        bundles,
        deduplicated,
    })
}

fn describe_conflict(conflict: &ReconcileConflict, label: &str) -> SelectionConflict {
    let message = match conflict {
        ReconcileConflict::VersionDigestMismatch {
            version_id,
            local_digest,
            imported_digest,
            source_rule_id,
            ..
        } => format!(
            "{label}: policy version {version_id} ('{source_rule_id}') was already defined with semantic digest {local_digest}, \
             but this input defines it with {imported_digest}. An immutable policy version must not have two different definitions."
        ),
        ReconcileConflict::VersionBelongsToDifferentLineage {
            lineage_id,
            version_id,
            actual_lineage_id,
        } => format!(
            "{label}: policy version {version_id} claims lineage {lineage_id}, but it was already defined under lineage {actual_lineage_id}."
        ),
        ReconcileConflict::PolicyTypeMismatch {
            lineage_id,
            version_id,
        } => format!(
            "{label}: policy version {version_id} in lineage {lineage_id} changes policy type between inputs."
        ),
        ReconcileConflict::LineageObjectTypeMismatch { lineage_id } => {
            format!("{label}: lineage {lineage_id} is defined with conflicting object types.")
        }
        ReconcileConflict::InvalidPortableIdentity { source_rule_id } => {
            format!("{label}: policy '{source_rule_id}' has an invalid portable identity.")
        }
    };

    SelectionConflict {
        code: conflict.code(),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PolicyOrigin;

    fn policy(name: &str, version_id: &str, digest: &str) -> ResolvedPolicy {
        ResolvedPolicy {
            policy_id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("uuid"),
            policy_version_id: Uuid::parse_str(version_id).expect("uuid"),
            version: "1".into(),
            name: name.into(),
            description: None,
            policy_type: "custom_check".into(),
            implementation_state: "native".into(),
            execution_phase: "nix-evaluation".into(),
            config: serde_json::json!({"expression": "config.a.b == true"}),
            compliance_metadata: serde_json::json!({}),
            semantic_digest: digest.into(),
            origin: PolicyOrigin {
                input_label: "in".into(),
                source_sha256: String::new(),
                bundle_version_id: None,
            },
        }
    }

    fn input(label: &str, policies: Vec<ResolvedPolicy>) -> LoadedInput {
        LoadedInput {
            label: label.into(),
            source_sha256: String::new(),
            bundle: None,
            policies,
        }
    }

    const V1: &str = "22222222-2222-2222-2222-222222222222";
    const V2: &str = "33333333-3333-3333-3333-333333333333";

    #[test]
    fn identical_duplicates_are_deduplicated() {
        let inputs = vec![
            input("a.json", vec![policy("p", V1, "digest-a")]),
            input("b.json", vec![policy("p", V1, "digest-a")]),
        ];
        let selection = select_policies(&inputs).expect("no conflict");
        assert_eq!(selection.policies.len(), 1);
        assert_eq!(selection.deduplicated.len(), 1);
    }

    #[test]
    fn conflicting_definitions_for_one_identity_are_rejected() {
        let inputs = vec![
            input("a.json", vec![policy("p", V1, "digest-a")]),
            input("b.json", vec![policy("p", V1, "digest-b")]),
        ];
        let conflicts = select_policies(&inputs).expect_err("must conflict");
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].code, "CF_NATIVE_VERSION_DIGEST_CONFLICT");
        assert!(conflicts[0].message.contains("b.json"));
    }

    #[test]
    fn multiple_inputs_combine() {
        let inputs = vec![
            input("a.json", vec![policy("alpha", V1, "digest-a")]),
            input("b.json", vec![policy("beta", V2, "digest-b")]),
        ];
        let selection = select_policies(&inputs).expect("no conflict");
        assert_eq!(selection.policies.len(), 2);
    }

    #[test]
    fn selection_order_is_independent_of_input_order() {
        let forward = vec![
            input("a.json", vec![policy("zulu", V1, "d1")]),
            input("b.json", vec![policy("alpha", V2, "d2")]),
        ];
        let reverse = vec![
            input("b.json", vec![policy("alpha", V2, "d2")]),
            input("a.json", vec![policy("zulu", V1, "d1")]),
        ];
        let a = select_policies(&forward).expect("ok");
        let b = select_policies(&reverse).expect("ok");
        let names_a: Vec<_> = a.policies.iter().map(|p| p.name.clone()).collect();
        let names_b: Vec<_> = b.policies.iter().map(|p| p.name.clone()).collect();
        assert_eq!(names_a, vec!["alpha".to_string(), "zulu".to_string()]);
        assert_eq!(names_a, names_b);
    }

    #[test]
    fn empty_input_set_selects_nothing() {
        let selection = select_policies(&[]).expect("ok");
        assert!(selection.policies.is_empty());
        assert!(selection.bundles.is_empty());
    }
}
