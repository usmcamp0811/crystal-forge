//! Resolve the effective policy set from all inputs.
//!
//! This is *effective-policy resolution*, not import reconciliation. Crystal
//! Forge resolves at most one version per policy lineage; two versions of the
//! same lineage must never silently coexist and produce a hybrid configuration.
//! The lineage/version decision is delegated to
//! [`cf_compliance::effective_set`], the same rule `cf-server`'s resolver uses.
//!
//! All offline inputs are peers: an exported artifact carries no environment or
//! system scope, so every candidate is resolved at
//! [`Specificity::BundleBaseline`]. Two different versions of one lineage are
//! therefore always an ambiguity that a human must resolve, never an implicit
//! "newest wins".

use std::collections::BTreeMap;

use cf_compliance::effective_set::{LineageDecision, Specificity, resolve_lineage_candidate};
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

/// A blocking disagreement between inputs.
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

impl SelectionConflict {
    fn new(code: &'static str, message: String) -> Self {
        Self { code, message }
    }
}

/// Combine every loaded input into one effective policy set.
///
/// Returns every conflict found rather than stopping at the first, so an
/// operator can fix them in one pass. Conflicts are never resolved
/// automatically.
pub fn select_policies(inputs: &[LoadedInput]) -> Result<Selection, Vec<SelectionConflict>> {
    let mut conflicts: Vec<SelectionConflict> = Vec::new();

    // ── Policies: resolve to at most one version per lineage ────────────────
    //
    // Keyed by lineage so a second version of the same lineage cannot slip in
    // under a different version ID.
    let mut by_lineage: BTreeMap<Uuid, ResolvedPolicy> = BTreeMap::new();
    let mut deduplicated: Vec<String> = Vec::new();

    for input in inputs {
        for policy in &input.policies {
            let existing = by_lineage
                .get(&policy.policy_id)
                .map(|selected| (selected.policy_version_id, Specificity::BundleBaseline));

            match resolve_lineage_candidate(
                policy.policy_id,
                existing,
                policy.policy_version_id,
                Specificity::BundleBaseline,
            ) {
                LineageDecision::Insert => {
                    by_lineage.insert(policy.policy_id, policy.clone());
                }
                LineageDecision::Deduplicate { .. } => {
                    // Same immutable version seen again. It must be identical:
                    // one immutable identity may not have two definitions.
                    let selected = by_lineage
                        .get_mut(&policy.policy_id)
                        .expect("lineage present for a deduplicate decision");

                    if selected.semantic_digest != policy.semantic_digest {
                        conflicts.push(SelectionConflict::new(
                            "CF_POLICY_VERSION_DIGEST_CONFLICT",
                            format!(
                                "policy version {} ('{}') is defined with semantic digest {} in {} \
                                 and with {} in {}. An immutable policy version must not have two \
                                 different definitions.",
                                policy.policy_version_id,
                                policy.name,
                                selected.semantic_digest,
                                selected.origin.primary_input_label(),
                                policy.semantic_digest,
                                policy.origin.primary_input_label(),
                            ),
                        ));
                        continue;
                    }

                    // Identical duplicate: record every origin so provenance
                    // does not depend on CLI argument order. The diagnostic is
                    // derived from the final merged state below, not from
                    // whichever occurrence happened to arrive second.
                    selected.origin.merge(&policy.origin);
                }
                LineageDecision::Replace | LineageDecision::Suppress => {
                    // Unreachable while every offline input is a peer, but
                    // handled explicitly so a future specificity source cannot
                    // silently take a branch this generator never reviewed.
                    conflicts.push(SelectionConflict::new(
                        "CF_POLICY_LINEAGE_SPECIFICITY_UNSUPPORTED",
                        format!(
                            "policy lineage {} resolved by specificity, which offline generation \
                             does not model",
                            policy.policy_id
                        ),
                    ));
                }
                LineageDecision::Conflict {
                    lineage_id,
                    existing_version_id,
                    candidate_version_id,
                } => {
                    let existing_policy = by_lineage.get(&lineage_id);
                    conflicts.push(SelectionConflict::new(
                        "CF_EFFECTIVE_POLICY_VERSION_CONFLICT",
                        format!(
                            "policy lineage {lineage_id} is selected at two different versions: {} \
                             (from {}) and {} (from {}). Crystal Forge resolves one version per \
                             lineage; generate from a single resolved policy set, or remove one of \
                             the inputs.",
                            existing_version_id,
                            existing_policy
                                .map(|p| p.origin.primary_input_label())
                                .unwrap_or_else(|| "<unknown>".to_string()),
                            candidate_version_id,
                            policy.origin.primary_input_label(),
                        ),
                    ));
                }
            }
        }
    }

    // ── Bundles: reconcile immutable bundle version identities ──────────────
    let mut by_bundle_version: BTreeMap<Uuid, ResolvedBundle> = BTreeMap::new();

    for input in inputs {
        let Some(bundle) = &input.bundle else {
            continue;
        };

        match by_bundle_version.get_mut(&bundle.bundle_version_id) {
            None => {
                by_bundle_version.insert(bundle.bundle_version_id, bundle.clone());
            }
            Some(existing) => {
                if existing.semantic_digest != bundle.semantic_digest
                    || existing.bundle_id != bundle.bundle_id
                    || existing.selected_policy_version_ids != bundle.selected_policy_version_ids
                {
                    conflicts.push(SelectionConflict::new(
                        "CF_BUNDLE_VERSION_IDENTITY_CONFLICT",
                        format!(
                            "bundle version {} is defined differently in {} and {}. An immutable \
                             bundle version must not have two different definitions.",
                            bundle.bundle_version_id,
                            existing.primary_input_label(),
                            bundle.primary_input_label(),
                        ),
                    ));
                    continue;
                }

                existing.merge_origin(bundle);
            }
        }
    }

    if !conflicts.is_empty() {
        // Deterministic diagnostics regardless of input order.
        conflicts.sort_by(|a, b| a.code.cmp(b.code).then(a.message.cmp(&b.message)));
        conflicts.dedup_by(|a, b| a.code == b.code && a.message == b.message);
        return Err(conflicts);
    }

    let mut policies: Vec<ResolvedPolicy> = by_lineage.into_values().collect();
    // Content-derived ordering, independent of input order.
    policies.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then(a.policy_version_id.cmp(&b.policy_version_id))
    });

    let bundles: Vec<ResolvedBundle> = by_bundle_version.into_values().collect();

    // Derive dedup diagnostics from the final merged state so their content is
    // a function of the input set, never of the input order.
    for policy in &policies {
        if policy.origin.input_labels.len() > 1 {
            deduplicated.push(format!(
                "{} ({}) appears in {}",
                policy.name,
                policy.policy_version_id,
                policy.origin.input_labels.join(", ")
            ));
        }
    }
    for bundle in &bundles {
        if bundle.input_labels.len() > 1 {
            deduplicated.push(format!(
                "bundle version {} appears in {}",
                bundle.bundle_version_id,
                bundle.input_labels.join(", ")
            ));
        }
    }

    deduplicated.sort();
    deduplicated.dedup();

    Ok(Selection {
        policies,
        bundles,
        deduplicated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PolicyOrigin;

    fn origin(label: &str) -> PolicyOrigin {
        PolicyOrigin::new(label.to_string(), format!("sha-{label}"), None)
    }

    fn policy(
        name: &str,
        lineage_id: &str,
        version_id: &str,
        digest: &str,
        label: &str,
    ) -> ResolvedPolicy {
        ResolvedPolicy {
            policy_id: Uuid::parse_str(lineage_id).expect("uuid"),
            policy_version_id: Uuid::parse_str(version_id).expect("uuid"),
            version: "1".into(),
            publication_state: "accepted".into(),
            name: name.into(),
            description: None,
            policy_type: "custom_check".into(),
            implementation_state: "native".into(),
            execution_phase: "nix-evaluation".into(),
            config: serde_json::json!({"expression": "config.a.b == true"}),
            compliance_metadata: serde_json::json!({}),
            semantic_digest: digest.into(),
            origin: origin(label),
        }
    }

    fn input(label: &str, policies: Vec<ResolvedPolicy>) -> LoadedInput {
        LoadedInput {
            label: label.into(),
            source_sha256: format!("sha-{label}"),
            selected_xml_sha256: None,
            bundle: None,
            policies,
        }
    }

    const L1: &str = "11111111-1111-1111-1111-111111111111";
    const L2: &str = "11111111-2222-2222-2222-222222222222";
    const V1: &str = "22222222-1111-1111-1111-111111111111";
    const V2: &str = "22222222-2222-2222-2222-222222222222";

    // ── 10.1 effective policy resolution ────────────────────────────────────

    #[test]
    fn same_lineage_same_version_same_digest_deduplicates() {
        let inputs = vec![
            input("a.json", vec![policy("p", L1, V1, "d", "a.json")]),
            input("b.json", vec![policy("p", L1, V1, "d", "b.json")]),
        ];
        let selection = select_policies(&inputs).expect("no conflict");
        assert_eq!(selection.policies.len(), 1);
        // Both origins retained.
        assert_eq!(
            selection.policies[0].origin.input_labels,
            vec!["a.json".to_string(), "b.json".to_string()]
        );
    }

    #[test]
    fn same_lineage_same_version_different_digest_is_an_identity_conflict() {
        let inputs = vec![
            input("a.json", vec![policy("p", L1, V1, "d1", "a.json")]),
            input("b.json", vec![policy("p", L1, V1, "d2", "b.json")]),
        ];
        let conflicts = select_policies(&inputs).expect_err("must conflict");
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].code, "CF_POLICY_VERSION_DIGEST_CONFLICT");
    }

    #[test]
    fn same_lineage_different_versions_is_an_effective_set_conflict() {
        let inputs = vec![
            input("a.json", vec![policy("p", L1, V1, "d1", "a.json")]),
            input("b.json", vec![policy("p", L1, V2, "d2", "b.json")]),
        ];
        let conflicts = select_policies(&inputs).expect_err("must conflict");
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].code, "CF_EFFECTIVE_POLICY_VERSION_CONFLICT");
        assert!(
            conflicts[0].message.contains(V1),
            "{}",
            conflicts[0].message
        );
        assert!(
            conflicts[0].message.contains(V2),
            "{}",
            conflicts[0].message
        );
    }

    /// The regression that matters most: two versions of one lineage touching
    /// disjoint options must not silently merge into a hybrid configuration.
    #[test]
    fn two_versions_of_one_lineage_never_produce_a_hybrid_configuration() {
        let mut v1 = policy("p", L1, V1, "d1", "a.json");
        v1.config = serde_json::json!({"expression": "config.services.a.enable == true"});
        let mut v2 = policy("p", L1, V2, "d2", "b.json");
        v2.config = serde_json::json!({"expression": "config.services.b.enable == true"});

        let conflicts = select_policies(&[input("a.json", vec![v1]), input("b.json", vec![v2])])
            .expect_err("disjoint options must still conflict");
        assert_eq!(conflicts[0].code, "CF_EFFECTIVE_POLICY_VERSION_CONFLICT");
    }

    #[test]
    fn different_lineages_coexist() {
        let inputs = vec![
            input("a.json", vec![policy("alpha", L1, V1, "d1", "a.json")]),
            input("b.json", vec![policy("beta", L2, V2, "d2", "b.json")]),
        ];
        let selection = select_policies(&inputs).expect("no conflict");
        assert_eq!(selection.policies.len(), 2);
    }

    // ── 10.5 deterministic provenance ───────────────────────────────────────

    #[test]
    fn provenance_is_independent_of_input_order() {
        let forward = select_policies(&[
            input("a.json", vec![policy("p", L1, V1, "d", "a.json")]),
            input("b.json", vec![policy("p", L1, V1, "d", "b.json")]),
        ])
        .expect("ok");
        let reverse = select_policies(&[
            input("b.json", vec![policy("p", L1, V1, "d", "b.json")]),
            input("a.json", vec![policy("p", L1, V1, "d", "a.json")]),
        ])
        .expect("ok");

        assert_eq!(
            forward.policies[0].origin.input_labels,
            reverse.policies[0].origin.input_labels
        );
        assert_eq!(
            forward.policies[0].origin.source_sha256s,
            reverse.policies[0].origin.source_sha256s
        );
        assert_eq!(forward.deduplicated, reverse.deduplicated);
    }

    #[test]
    fn selection_order_is_independent_of_input_order() {
        let forward = select_policies(&[
            input("a.json", vec![policy("zulu", L1, V1, "d1", "a.json")]),
            input("b.json", vec![policy("alpha", L2, V2, "d2", "b.json")]),
        ])
        .expect("ok");
        let reverse = select_policies(&[
            input("b.json", vec![policy("alpha", L2, V2, "d2", "b.json")]),
            input("a.json", vec![policy("zulu", L1, V1, "d1", "a.json")]),
        ])
        .expect("ok");

        let names = |s: &Selection| {
            s.policies
                .iter()
                .map(|p| p.name.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(&forward),
            vec!["alpha".to_string(), "zulu".to_string()]
        );
        assert_eq!(names(&forward), names(&reverse));
    }

    // ── 10.4 bundle identity reconciliation ─────────────────────────────────

    fn bundle(version_id: &str, digest: &str, label: &str, members: Vec<&str>) -> ResolvedBundle {
        ResolvedBundle {
            bundle_id: Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").expect("uuid"),
            bundle_version_id: Uuid::parse_str(version_id).expect("uuid"),
            name: "b".into(),
            version: "1".into(),
            framework: None,
            framework_version: None,
            publication_state: "accepted".into(),
            semantic_digest: Some(digest.into()),
            source_sha256s: vec![format!("sha-{label}")],
            input_labels: vec![label.to_string()],
            selected_policy_version_ids: members
                .into_iter()
                .map(|id| Uuid::parse_str(id).expect("uuid"))
                .collect(),
        }
    }

    fn bundle_input(label: &str, bundle: ResolvedBundle) -> LoadedInput {
        LoadedInput {
            label: label.into(),
            source_sha256: format!("sha-{label}"),
            selected_xml_sha256: None,
            bundle: Some(bundle),
            policies: Vec::new(),
        }
    }

    const BV: &str = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";

    #[test]
    fn identical_bundle_versions_deduplicate() {
        let selection = select_policies(&[
            bundle_input("a.xml", bundle(BV, "d", "a.xml", vec![V1])),
            bundle_input("b.xml", bundle(BV, "d", "b.xml", vec![V1])),
        ])
        .expect("no conflict");
        assert_eq!(selection.bundles.len(), 1);
        assert_eq!(
            selection.bundles[0].input_labels,
            vec!["a.xml".to_string(), "b.xml".to_string()]
        );
    }

    #[test]
    fn bundle_versions_with_different_digests_conflict() {
        let conflicts = select_policies(&[
            bundle_input("a.xml", bundle(BV, "d1", "a.xml", vec![V1])),
            bundle_input("b.xml", bundle(BV, "d2", "b.xml", vec![V1])),
        ])
        .expect_err("must conflict");
        assert_eq!(conflicts[0].code, "CF_BUNDLE_VERSION_IDENTITY_CONFLICT");
    }

    #[test]
    fn bundle_versions_with_different_membership_conflict() {
        let conflicts = select_policies(&[
            bundle_input("a.xml", bundle(BV, "d", "a.xml", vec![V1])),
            bundle_input("b.xml", bundle(BV, "d", "b.xml", vec![V1, V2])),
        ])
        .expect_err("must conflict");
        assert_eq!(conflicts[0].code, "CF_BUNDLE_VERSION_IDENTITY_CONFLICT");
    }

    #[test]
    fn empty_input_set_selects_nothing() {
        let selection = select_policies(&[]).expect("ok");
        assert!(selection.policies.is_empty());
        assert!(selection.bundles.is_empty());
    }
}
