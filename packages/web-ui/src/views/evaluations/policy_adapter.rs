//! Adapts captured wire evidence to the pure matrix model.
//!
//! Version UUIDs define identity. Labels never establish bundle membership.
//! Unresolved applicability overrides a clean classification, including when
//! there are no columns to count. Bundle cells use per-system contributions,
//! not the configuration-wide or cross-system membership union.
//! Bundle assignment is independent of eligible policy contributions. Only
//! captured contexts establish assignment; older incomplete context metadata
//! cannot establish nonassignment from an absent context or an empty bundle.

use std::collections::{HashMap, HashSet};

use super::policy_matrix::{self as model, Bundle, ControlId, Outcome, RowHealth, Summary};
use crate::api::models::eval_policy_matrix::*;

/// Separates bundle context presence from eligible-control applicability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BundleApplicability {
    /// An explicit captured context applies, even with zero eligible controls.
    Assigned,
    /// Complete captured contexts explicitly exclude this bundle version.
    NotAssigned,
    /// Context metadata is incomplete; absence cannot prove nonassignment.
    Unknown,
}

/// Contains one captured system's normalized outcomes and common health summary.
#[derive(Clone, PartialEq)]
pub(super) struct MatrixRow {
    /// Keys expansion by system identity and configuration, not display name.
    pub(super) key: String,
    /// Retains captured names, resolution, and source memberships.
    pub(super) source: EvalMatrixRow,
    /// Keys distinct controls by exact policy-version identity.
    pub(super) outcomes: HashMap<String, Outcome>,
    /// Keys original matching-attempt detail text by exact control version.
    /// Unknown and nonassigned outcomes never retain details.
    pub(super) details: HashMap<ControlId, String>,
    /// Supplies the same counts to row ratios, bars, filters, and sorting.
    pub(super) common_summary: Summary,
    /// States whether absent bundle contexts prove nonassignment.
    pub(super) bundle_metadata_complete: bool,
}

impl MatrixRow {
    /// Classifies health without certifying unresolved applicability as clean.
    pub(super) fn health(&self) -> RowHealth {
        let health = model::row_health(&self.common_summary);
        if health == RowHealth::Clean && !self.resolved() {
            RowHealth::Unavailable
        } else {
            health
        }
    }

    /// Returns whether an empty applicability set is authoritative.
    pub(super) fn resolved(&self) -> bool {
        self.source.resolution == EvalMatrixResolution::Resolved
    }

    /// Returns context presence without inferring real assignments from policies.
    ///
    /// Synthetic unbundled groups have no assignment identity. Their presence
    /// follows known unbundled controls and complete policy resolution instead.
    pub(super) fn bundle_applicability(&self, bundle: &Bundle) -> BundleApplicability {
        let present = if bundle.synthetic {
            self.source
                .policies
                .iter()
                .any(|p| p.memberships.is_empty())
        } else {
            self.source
                .assigned_bundles
                .iter()
                .any(|context| context.bundle_version_id.to_string() == bundle.id)
        };
        if present {
            BundleApplicability::Assigned
        } else if (bundle.synthetic && self.resolved())
            || (!bundle.synthetic && self.bundle_metadata_complete)
        {
            BundleApplicability::NotAssigned
        } else {
            BundleApplicability::Unknown
        }
    }

    /// Aggregates only this system's authoritative bundle contributions.
    ///
    /// Known nonassignment excludes the entire bundle. Incomplete old context
    /// metadata retains known policy contributions without asserting assignment.
    /// The renderer must show that context uncertainty separately from counts.
    pub(super) fn bundle_summary(&self, bundle: &Bundle) -> Summary {
        if self.bundle_applicability(bundle) == BundleApplicability::NotAssigned {
            return Summary::default();
        }
        let outcomes = bundle
            .control_ids
            .iter()
            .map(|id| {
                let binding = self
                    .source
                    .policies
                    .iter()
                    .find(|p| p.policy_version_id.to_string() == *id);
                let belongs = binding.is_some_and(|p| {
                    if bundle.synthetic {
                        p.memberships.is_empty()
                    } else {
                        p.memberships
                            .iter()
                            .any(|m| m.bundle_version_id.to_string() == bundle.id)
                    }
                });
                let outcome = if belongs {
                    self.outcomes.get(id).copied().unwrap_or(Outcome::Unknown)
                } else if self.resolved() {
                    Outcome::NotAssigned
                } else {
                    Outcome::Unknown
                };
                (id.clone(), outcome)
            })
            .collect();
        model::bundle_summary(bundle, &outcomes)
    }
}

/// Contains deduplicated captured columns and presentation-only membership sets.
#[derive(Clone, PartialEq)]
pub(super) struct Matrix {
    /// Lists each exact policy version once in captured order.
    pub(super) controls: Vec<EvalPolicyColumn>,
    /// Lists real bundles plus an explicitly synthetic unbundled group if needed.
    pub(super) bundles: Vec<Bundle>,
    /// Retains planned systems with absent evidence.
    pub(super) rows: Vec<MatrixRow>,
}

/// Adapts captured provenance only; legacy callers must keep the flat renderer.
pub(super) fn captured(snapshot: &EvalPolicyMatrixSnapshot) -> Option<Matrix> {
    if snapshot.provenance != EvalMatrixProvenance::Captured {
        return None;
    }
    let mut seen = HashSet::new();
    let controls: Vec<_> = snapshot
        .columns
        .iter()
        .filter(|p| seen.insert(p.policy_version_id))
        .cloned()
        .collect();
    let ids: Vec<_> = controls
        .iter()
        .map(|p| p.policy_version_id.to_string())
        .collect();
    let mut bundle_ids = HashSet::new();
    let mut bundles: Vec<_> = snapshot
        .bundles
        .iter()
        .filter(|b| bundle_ids.insert(b.bundle_version_id))
        .map(|b| {
            let members: HashSet<_> = snapshot
                .rows
                .iter()
                .flat_map(|r| &r.policies)
                .filter(|p| {
                    p.memberships
                        .iter()
                        .any(|m| m.bundle_version_id == b.bundle_version_id)
                })
                .map(|p| p.policy_version_id.to_string())
                .collect();
            Bundle {
                id: b.bundle_version_id.to_string(),
                synthetic: false,
                control_ids: ids
                    .iter()
                    .filter(|id| members.contains(*id))
                    .cloned()
                    .collect(),
            }
        })
        .collect();
    let unbundled: Vec<_> = ids
        .iter()
        .filter(|id| {
            snapshot
                .rows
                .iter()
                .flat_map(|r| &r.policies)
                .any(|p| p.policy_version_id.to_string() == **id && p.memberships.is_empty())
                || !bundles.iter().any(|b| b.control_ids.contains(id))
        })
        .cloned()
        .collect();
    if !unbundled.is_empty() {
        bundles.push(Bundle {
            id: "unbundled".into(),
            synthetic: true,
            control_ids: unbundled,
        });
    }
    let rows = snapshot
        .rows
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let mut details = HashMap::new();
            let outcomes: HashMap<_, _> = controls
                .iter()
                .map(|p| {
                    let bound = source
                        .policies
                        .iter()
                        .find(|b| b.policy_version_id == p.policy_version_id);
                    let position = snapshot
                        .columns
                        .iter()
                        .position(|c| c.policy_version_id == p.policy_version_id);
                    let cell = position
                        .and_then(|i| source.cells.get(i))
                        .copied()
                        .unwrap_or(EvalMatrixCell::Unknown);
                    let outcome =
                        if source.resolution == EvalMatrixResolution::Resolved && bound.is_none() {
                            Outcome::NotAssigned
                        } else {
                            match cell {
                                EvalMatrixCell::Pass => Outcome::Pass,
                                EvalMatrixCell::Warn => Outcome::Warning,
                                EvalMatrixCell::Fail => Outcome::Fail,
                                // Bound controls cannot become unassigned because
                                // evidence is absent or contradictory.
                                EvalMatrixCell::Unknown | EvalMatrixCell::NotAssigned => {
                                    Outcome::Unknown
                                }
                            }
                        };
                    // Preserve the original column/UUID alignment. The server
                    // enforces attempt ownership; unavailable cells carry no
                    // evidence, even if a malformed response includes text.
                    if matches!(outcome, Outcome::Pass | Outcome::Warning | Outcome::Fail)
                        && bound.is_some_and(|binding| binding.executable)
                        && let Some(detail) = position
                            .and_then(|i| source.details.get(i))
                            .and_then(Option::as_ref)
                    {
                        details.insert(p.policy_version_id.to_string(), detail.clone());
                    }
                    (p.policy_version_id.to_string(), outcome)
                })
                .collect();
            MatrixRow {
                key: format!(
                    "{}:{}:{index}",
                    source
                        .system_id
                        .map(|id| id.to_string())
                        .unwrap_or_default(),
                    source.configuration_name
                ),
                source: source.clone(),
                common_summary: model::aggregate(&ids, &outcomes),
                bundle_metadata_complete: snapshot.empty_bundle_metadata_complete,
                outcomes,
                details,
            }
        })
        .collect();
    Some(Matrix {
        controls,
        bundles,
        rows,
    })
}

/// Returns whether an outcome needs attention, including unavailable evidence.
pub(super) fn nonpass(outcome: Outcome) -> bool {
    matches!(outcome, Outcome::Fail | Outcome::Warning | Outcome::Unknown)
}

#[cfg(test)]
#[path = "policy_detail_tests.rs"]
mod detail_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::EvalPolicyMatrixResponse;
    use serde_json::json;

    fn fixture() -> EvalPolicyMatrixSnapshot {
        serde_json::from_value(json!({
            "evaluation_attempt_id": null, "attempt_number": 2, "provenance": "captured",
            "mock_evidence": false,
            "columns": [
                {"policy_version_id": "00000000-0000-0000-0000-000000000001", "policy_id": "00000000-0000-0000-0000-000000000011", "name": "Actual policy", "version": "2"},
                {"policy_version_id": "00000000-0000-0000-0000-000000000002", "policy_id": "00000000-0000-0000-0000-000000000012", "name": "Missing result", "version": "3"}
            ],
            "bundles": [
                {"bundle_version_id": "00000000-0000-0000-0000-000000000021", "bundle_id": "00000000-0000-0000-0000-000000000031", "name": "A", "version": "1", "framework": "F", "framework_version": null},
                {"bundle_version_id": "00000000-0000-0000-0000-000000000022", "bundle_id": "00000000-0000-0000-0000-000000000032", "name": "B", "version": "2", "framework": "G", "framework_version": "4"}
            ], "empty_bundle_metadata_complete": true,
            "rows": [{"system_id": null, "system_name": "real-host", "configuration_name": "config", "resolution": "resolved", "evaluation_digest": null,
                "assigned_bundles": [
                    {"bundle_version_id": "00000000-0000-0000-0000-000000000021", "assignment_id": "00000000-0000-0000-0000-000000000041", "assignment_version_id": "00000000-0000-0000-0000-000000000051"},
                    {"bundle_version_id": "00000000-0000-0000-0000-000000000022", "assignment_id": "00000000-0000-0000-0000-000000000042", "assignment_version_id": "00000000-0000-0000-0000-000000000052"}
                ],
                "policies": [
                    {"policy_version_id": "00000000-0000-0000-0000-000000000001", "report_only": false, "executable": true, "memberships": [
                        {"bundle_version_id": "00000000-0000-0000-0000-000000000021", "assignment_id": "00000000-0000-0000-0000-000000000041", "assignment_version_id": "00000000-0000-0000-0000-000000000051"},
                        {"bundle_version_id": "00000000-0000-0000-0000-000000000022", "assignment_id": "00000000-0000-0000-0000-000000000042", "assignment_version_id": "00000000-0000-0000-0000-000000000052"}
                    ]},
                    {"policy_version_id": "00000000-0000-0000-0000-000000000002", "report_only": true, "executable": false, "memberships": []}
                ], "cells": ["pass"], "details": ["config.security.audit: captured check passed"]}]
        })).unwrap()
    }

    #[test]
    fn wire_overlap_and_missing_outcomes_keep_common_denominator() {
        let matrix = captured(&fixture()).unwrap();
        assert_eq!(matrix.controls.len(), 2);
        assert_eq!(model::shared_control_count(&matrix.bundles), 1);
        let row = &matrix.rows[0];
        assert_eq!(
            (
                row.common_summary.passed,
                row.common_summary.applicable,
                row.common_summary.unknown
            ),
            (1, 2, 1)
        );
        assert_eq!(row.health(), RowHealth::Unavailable);
        assert_eq!(row.bundle_summary(&matrix.bundles[0]).passed, 1);
        assert_eq!(row.bundle_summary(&matrix.bundles[1]).passed, 1);
    }

    #[test]
    fn per_system_membership_does_not_leak_across_bundle_union() {
        let mut snapshot = fixture();
        let mut other = snapshot.rows[0].clone();
        other.policies[0].memberships.remove(0);
        snapshot.rows.push(other);
        let matrix = captured(&snapshot).unwrap();
        assert_eq!(
            matrix.rows[1].bundle_summary(&matrix.bundles[0]).applicable,
            0
        );
        assert_eq!(
            matrix.rows[1].bundle_summary(&matrix.bundles[1]).applicable,
            1
        );
    }

    #[test]
    fn unresolved_empty_is_not_clean_or_authoritative_empty() {
        let mut snapshot = fixture();
        snapshot.columns.clear();
        snapshot.rows[0].resolution = EvalMatrixResolution::Unavailable;
        let matrix = captured(&snapshot).unwrap();
        assert!(!matrix.rows[0].resolved());
        assert_eq!(matrix.rows[0].health(), RowHealth::Unavailable);
    }

    #[test]
    fn unresolved_all_pass_evidence_still_cannot_certify_clean() {
        let mut snapshot = fixture();
        snapshot.rows[0].resolution = EvalMatrixResolution::Conflict;
        snapshot.rows[0].cells = vec![EvalMatrixCell::Pass, EvalMatrixCell::Pass];
        let matrix = captured(&snapshot).unwrap();
        assert_eq!(matrix.rows[0].common_summary.passed, 2);
        assert_eq!(matrix.rows[0].health(), RowHealth::Unavailable);
    }

    #[test]
    fn explicit_nonapplicability_is_excluded_but_bound_missing_is_unknown() {
        let mut snapshot = fixture();
        snapshot.rows[0].policies.remove(0);
        snapshot.rows[0].cells = vec![EvalMatrixCell::NotAssigned, EvalMatrixCell::NotAssigned];
        let matrix = captured(&snapshot).unwrap();
        let summary = matrix.rows[0].common_summary;
        assert_eq!(summary.applicable, 1);
        assert_eq!(summary.unknown, 1);
        assert_eq!(summary.passed, 0);
        assert_eq!(matrix.rows[0].health(), RowHealth::Unavailable);
    }

    #[test]
    fn complete_contexts_distinguish_empty_assignment_from_nonassignment() {
        let mut snapshot = fixture();
        snapshot.columns.clear();
        snapshot.rows[0].policies.clear();
        snapshot.rows[0].cells.clear();
        let mut unassigned = snapshot.rows[0].clone();
        unassigned.assigned_bundles.clear();
        snapshot.rows.push(unassigned);
        let matrix = captured(&snapshot).unwrap();
        assert_eq!(matrix.bundles.len(), 2);
        for bundle in &matrix.bundles {
            assert!(!bundle.synthetic);
            assert!(bundle.control_ids.is_empty());
            assert_eq!(
                matrix.rows[0].bundle_applicability(bundle),
                BundleApplicability::Assigned
            );
            assert_eq!(
                matrix.rows[1].bundle_applicability(bundle),
                BundleApplicability::NotAssigned
            );
            assert_eq!(matrix.rows[0].bundle_summary(bundle), Summary::default());
        }
        assert_eq!(matrix.rows[0].health(), RowHealth::Unavailable);
    }

    #[test]
    fn older_captured_json_without_contexts_preserves_unknown_metadata() {
        let mut json = serde_json::to_value(fixture()).unwrap();
        json["empty_bundle_metadata_complete"] = json!(false);
        json["rows"][0]
            .as_object_mut()
            .unwrap()
            .remove("assigned_bundles");
        let mut old: EvalPolicyMatrixSnapshot = serde_json::from_value(json).unwrap();
        assert!(old.rows[0].assigned_bundles.is_empty());
        assert_eq!(
            old.rows[0].policies[0].memberships[0].source,
            EvalBundleSource::Assignment
        );
        let matrix = captured(&old).unwrap();
        let bundle = &matrix.bundles[0];
        assert_eq!(
            matrix.rows[0].bundle_applicability(bundle),
            BundleApplicability::Unknown
        );
        assert_eq!(matrix.rows[0].bundle_summary(bundle).passed, 1);
        assert_eq!(model::shared_control_count(&matrix.bundles), 1);
        old.columns.clear();
        old.rows[0].policies.clear();
        let empty = captured(&old).unwrap();
        assert_eq!(
            empty.rows[0].bundle_applicability(&empty.bundles[0]),
            BundleApplicability::Unknown
        );
        assert_eq!(empty.rows[0].health(), RowHealth::Unavailable);
    }

    #[test]
    fn virtual_baseline_without_assignment_ids_remains_a_real_exact_bundle() {
        let baseline: EvalBundleMembership = serde_json::from_value(json!({
            "bundle_version_id": "00000000-0000-0000-0000-000000000021",
            "assignment_id": null, "assignment_version_id": null,
            "source": "virtual_baseline"
        }))
        .unwrap();
        assert_eq!(baseline.source, EvalBundleSource::VirtualBaseline);
        assert!(baseline.assignment_id.is_none());
        assert!(baseline.assignment_version_id.is_none());
        let mut snapshot = fixture();
        snapshot.rows[0].assigned_bundles[0] = baseline.clone();
        snapshot.rows[0].policies[0].memberships[0] = baseline;
        let matrix = captured(&snapshot).unwrap();
        let bundle = &matrix.bundles[0];
        assert!(!bundle.synthetic);
        assert_eq!(
            matrix.rows[0].bundle_applicability(bundle),
            BundleApplicability::Assigned
        );
        assert_eq!(matrix.rows[0].bundle_summary(bundle).passed, 1);
        assert_eq!(model::shared_control_count(&matrix.bundles), 1);
    }

    #[test]
    fn complete_contexts_do_not_infer_assignment_from_policy_bindings() {
        let mut snapshot = fixture();
        snapshot.rows[0].assigned_bundles.remove(0);
        let matrix = captured(&snapshot).unwrap();
        let bundle = &matrix.bundles[0];
        assert_eq!(
            matrix.rows[0].bundle_applicability(bundle),
            BundleApplicability::NotAssigned
        );
        assert_eq!(matrix.rows[0].bundle_summary(bundle), Summary::default());
    }

    #[test]
    fn legacy_and_older_wire_never_gain_inferred_bundles() {
        let old: EvalPolicyMatrixResponse = serde_json::from_value(
            json!({"commit_id": 1, "policies": ["stig.ssh"], "systems": []}),
        )
        .unwrap();
        assert!(old.matrix.is_none());
        let mut snapshot = fixture();
        snapshot.provenance = EvalMatrixProvenance::LegacyUnavailable;
        assert!(captured(&snapshot).is_none());
    }
}
