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
    /// Retains a configuration gate failure separately from control counts.
    pub(super) configuration_failed: bool,
    /// True when multiple host contexts refer to one evaluator configuration.
    /// Such contexts retain per-host assignments but are not separate executions.
    pub(super) shared_configuration: bool,
    /// Explains discovery exclusion without reusing old captured outcomes.
    availability: Option<&'static str>,
    /// Precomputed per-bundle counts; rendering does not rescan policy bindings.
    bundle_summaries: HashMap<String, Summary>,
}

impl MatrixRow {
    /// Classifies health without certifying unresolved applicability as clean.
    pub(super) fn health(&self) -> RowHealth {
        if self.configuration_failed {
            return RowHealth::Failing;
        }
        let health = model::row_health(&self.common_summary);
        if health == RowHealth::Clean && !self.resolved() {
            RowHealth::Unavailable
        } else {
            health
        }
    }

    /// Returns whether an empty applicability set is authoritative.
    pub(super) fn resolved(&self) -> bool {
        self.availability.is_none() && self.source.resolution == EvalMatrixResolution::Resolved
    }

    /// Returns the compact provenance or discovery state for this row.
    /// A missing captured system UUID does not establish unmanaged status.
    pub(super) fn status_label(&self) -> Option<&'static str> {
        if self.source.system_id.is_none() && self.availability == Some("Not selected") {
            return Some("No captured system · Not selected");
        }
        self.availability.or_else(|| {
            if self.source.system_id.is_none() {
                Some("No captured system")
            } else if !self.resolved() {
                Some(match self.source.resolution {
                    EvalMatrixResolution::Conflict => "Assignment conflict",
                    EvalMatrixResolution::Skipped => "Policy execution skipped",
                    _ => "Provenance unavailable",
                })
            } else if self.shared_configuration {
                Some("Shared configuration")
            } else {
                None
            }
        })
    }

    /// Describes the row's real configuration gate separately from control health.
    pub(super) fn status_description(&self) -> Option<String> {
        if self.configuration_failed {
            let context = self
                .status_label()
                .map(|label| format!("; {label}"))
                .unwrap_or_default();
            Some(format!("Configuration requirement failed{context}"))
        } else {
            self.status_label().map(str::to_owned)
        }
    }

    /// Returns context presence without inferring real assignments from policies.
    ///
    /// Synthetic unbundled groups have no assignment identity. Their presence
    /// follows known unbundled controls and complete policy resolution instead.
    pub(super) fn bundle_applicability(&self, bundle: &Bundle) -> BundleApplicability {
        if self.availability.is_some() {
            return BundleApplicability::Unknown;
        }
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
        self.bundle_summaries
            .get(&bundle.id)
            .copied()
            .unwrap_or_default()
    }

    /// Computes a bundle once using an indexed exact-version binding lookup.
    fn calculate_bundle_summary(
        &self,
        bundle: &Bundle,
        bindings: &HashMap<String, &EvalPolicyBinding>,
    ) -> Summary {
        if self.bundle_applicability(bundle) == BundleApplicability::NotAssigned {
            return Summary::default();
        }
        let outcomes = bundle
            .control_ids
            .iter()
            .map(|id| {
                let binding = bindings.get(id);
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

/// Adapts captured tracked-system rows without inventing identities or memberships.
/// Inventory alone cannot supply historical system mappings. Legacy snapshots
/// without capture or inventory remain unavailable.
pub(super) fn captured(snapshot: &EvalPolicyMatrixSnapshot) -> Option<Matrix> {
    if snapshot.provenance != EvalMatrixProvenance::Captured && snapshot.inventory.is_none() {
        return None;
    }
    // INVARIANT: Only captured UUID-bearing host contexts contribute rows,
    // shared-configuration counts, or memberships. Full inventory remains intact
    // for header coverage; current registry data cannot enrich historical rows.
    let sources: Vec<_> = snapshot
        .rows
        .iter()
        .filter(|row| row.system_id.is_some())
        .cloned()
        .collect();
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
    let mut positions = HashMap::new();
    for (position, control) in snapshot.columns.iter().enumerate() {
        positions
            .entry(control.policy_version_id)
            .or_insert(position);
    }
    let mut contributions: HashMap<uuid::Uuid, HashSet<String>> = HashMap::new();
    let mut unbundled_ids = HashSet::new();
    for policy in sources.iter().flat_map(|row| &row.policies) {
        let id = policy.policy_version_id.to_string();
        if policy.memberships.is_empty() {
            unbundled_ids.insert(id.clone());
        }
        for membership in &policy.memberships {
            contributions
                .entry(membership.bundle_version_id)
                .or_default()
                .insert(id.clone());
        }
    }
    let mut bundle_ids = HashSet::new();
    let mut bundles: Vec<_> = snapshot
        .bundles
        .iter()
        .filter(|b| bundle_ids.insert(b.bundle_version_id))
        .map(|b| {
            let members = contributions.get(&b.bundle_version_id);
            Bundle {
                id: b.bundle_version_id.to_string(),
                synthetic: false,
                control_ids: ids
                    .iter()
                    .filter(|id| members.is_some_and(|members| members.contains(*id)))
                    .cloned()
                    .collect(),
            }
        })
        .collect();
    let unbundled: Vec<_> = ids
        .iter()
        .filter(|id| {
            unbundled_ids.contains(*id) || !bundles.iter().any(|b| b.control_ids.contains(id))
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
    let discovered: Option<HashSet<_>> = snapshot.inventory.as_ref().map(|inventory| {
        inventory
            .configurations
            .iter()
            .map(String::as_str)
            .collect()
    });
    let selected: Option<HashSet<_>> = snapshot.inventory.as_ref().map(|inventory| {
        inventory
            .selected_configurations
            .iter()
            .map(String::as_str)
            .collect()
    });
    let mut contexts = HashMap::<&str, usize>::new();
    for source in &sources {
        *contexts
            .entry(source.configuration_name.as_str())
            .or_default() += 1;
    }
    // PERFORMANCE: A fleet-wide agent failure must not scan every diagnostic
    // for every host context. Gate identity is the exact configuration key.
    let failed_configurations: HashSet<_> = snapshot
        .configuration_failures
        .iter()
        .map(|failure| failure.configuration_name.as_str())
        .collect();
    let rows = sources
        .iter()
        .map(|source| {
            let availability = if discovered
                .as_ref()
                .is_some_and(|names| !names.contains(source.configuration_name.as_str()))
            {
                Some("Not discovered")
            } else if selected
                .as_ref()
                .is_some_and(|names| !names.contains(source.configuration_name.as_str()))
            {
                Some("Not selected")
            } else {
                None
            };
            let mut source = source.clone();
            // Preserve immutable assignment metadata. Discovery exclusions mask
            // execution evidence, not the fact that these assignments were planned.
            if availability.is_some() {
                source.cells.clear();
                source.details.clear();
            }
            let mut details = HashMap::new();
            let bindings: HashMap<_, _> = source
                .policies
                .iter()
                .map(|binding| (binding.policy_version_id, binding))
                .collect();
            let outcomes: HashMap<_, _> = controls
                .iter()
                .map(|p| {
                    let bound = bindings.get(&p.policy_version_id).copied();
                    let position = positions.get(&p.policy_version_id).copied();
                    let cell = position
                        .and_then(|i| source.cells.get(i))
                        .copied()
                        .unwrap_or(EvalMatrixCell::Unknown);
                    let outcome = if availability.is_some() {
                        Outcome::Unknown
                    } else if source.resolution == EvalMatrixResolution::Resolved && bound.is_none()
                    {
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
            let mut row = MatrixRow {
                // INVARIANT: Visibility changes cannot retarget an expanded
                // check by shifting row indices. Captured UUIDs identify host
                // contexts independently of shared configuration names.
                key: match source.system_id {
                    Some(id) => format!("system:{id}:{}", source.configuration_name),
                    None => format!("configuration:{}", source.configuration_name),
                },
                source: source.clone(),
                common_summary: model::aggregate(&ids, &outcomes),
                // A snapshot-wide completeness flag cannot establish absence
                // for unresolved applicability.
                bundle_metadata_complete: snapshot.empty_bundle_metadata_complete
                    && source.resolution == EvalMatrixResolution::Resolved,
                configuration_failed: availability.is_none()
                    && failed_configurations.contains(source.configuration_name.as_str()),
                shared_configuration: contexts
                    .get(source.configuration_name.as_str())
                    .copied()
                    .unwrap_or(0)
                    > 1,
                availability,
                outcomes,
                details,
                bundle_summaries: HashMap::new(),
            };
            let bindings = source
                .policies
                .iter()
                .map(|binding| (binding.policy_version_id.to_string(), binding))
                .collect();
            row.bundle_summaries = bundles
                .iter()
                .map(|bundle| {
                    (
                        bundle.id.clone(),
                        row.calculate_bundle_summary(bundle, &bindings),
                    )
                })
                .collect();
            row
        })
        .collect();
    Some(Matrix {
        controls,
        bundles,
        rows,
    })
}

/// Classifies each configuration once using its worst visible host context.
/// Row filters count mapped configurations, while the table retains each host's
/// exact captured assignments. Header inventory can include additional names.
pub(super) fn configuration_health(rows: &[MatrixRow]) -> HashMap<&str, RowHealth> {
    let rank = |health| match health {
        RowHealth::Clean => 0,
        RowHealth::Unavailable => 1,
        RowHealth::Warning => 2,
        RowHealth::Failing => 3,
    };
    let mut groups = HashMap::new();
    for row in rows {
        let health = row.health();
        groups
            .entry(row.source.configuration_name.as_str())
            .and_modify(|current| {
                if rank(health) > rank(*current) {
                    *current = health;
                }
            })
            .or_insert(health);
    }
    groups
}

/// Returns whether an outcome needs attention, including unavailable evidence.
pub(super) fn nonpass(outcome: Outcome) -> bool {
    matches!(outcome, Outcome::Fail | Outcome::Warning | Outcome::Unknown)
}

/// Selects trim evidence without excluding unknown or all-greenfield rows.
/// Health and bundle summaries must continue to use the original row set.
pub(super) fn trim_baseline(rows: Vec<&MatrixRow>) -> Vec<&MatrixRow> {
    let established: Vec<_> = rows
        .iter()
        .copied()
        .filter(|row| row.source.greenfield != Some(true))
        .collect();
    if established.is_empty() {
        rows
    } else {
        established
    }
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
            "rows": [{"system_id": "00000000-0000-0000-0000-000000000061", "system_name": "real-host", "configuration_name": "config", "resolution": "resolved", "evaluation_digest": null,
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

    fn with_inventory(configurations: &[&str], selected: &[&str]) -> EvalPolicyMatrixSnapshot {
        let mut value = serde_json::to_value(fixture()).unwrap();
        value["inventory"] = json!({
            "execution_mode": "native",
            "configurations": configurations,
            "selected_configurations": selected
        });
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn shared_contexts_count_one_configuration_without_losing_host_assignments() {
        let mut snapshot = with_inventory(&["config"], &["config"]);
        snapshot.rows[0].system_id = Some(uuid::Uuid::new_v4());
        let mut second = snapshot.rows[0].clone();
        second.system_id = Some(uuid::Uuid::new_v4());
        second.system_name = "second-context".into();
        second.cells[0] = EvalMatrixCell::Warn;
        snapshot.rows.push(second);
        let matrix = captured(&snapshot).unwrap();
        assert_eq!(matrix.rows.len(), 2);
        assert!(matrix.rows.iter().all(|row| row.shared_configuration));
        assert!(
            matrix
                .rows
                .iter()
                .all(|row| !row.source.policies.is_empty())
        );
        let groups = configuration_health(&matrix.rows);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups.get("config"), Some(&RowHealth::Warning));
    }

    #[test]
    fn discovery_exclusion_masks_results_but_preserves_planned_assignments() {
        for (discovered, selected, label) in [
            (vec![], vec![], "Not discovered"),
            (vec!["config"], vec![], "Not selected"),
        ] {
            let mut snapshot = with_inventory(&discovered, &selected);
            snapshot.rows[0].system_id = Some(uuid::Uuid::new_v4());
            let matrix = captured(&snapshot).unwrap();
            let row = &matrix.rows[0];
            assert_eq!(row.status_label(), Some(label));
            assert!(!row.resolved());
            assert_eq!(row.health(), RowHealth::Unavailable);
            assert_eq!(row.common_summary.passed, 0);
            assert_eq!(row.common_summary.unknown, 2);
            assert!(row.details.is_empty());
            assert_eq!(row.source.policies.len(), snapshot.rows[0].policies.len());
            assert_eq!(
                row.source.assigned_bundles.len(),
                snapshot.rows[0].assigned_bundles.len()
            );
            assert_eq!(
                row.bundle_applicability(&matrix.bundles[0]),
                BundleApplicability::Unknown
            );
        }
    }

    #[test]
    fn inventory_only_and_backend_uuidless_rows_are_excluded() {
        let mut snapshot = with_inventory(
            &["config", "new-config", "new-config", "inventory-only"],
            &["config", "new-config"],
        );
        let inventory = snapshot.inventory.clone();
        let mut unmapped = snapshot.rows[0].clone();
        unmapped.system_id = None;
        unmapped.cells = vec![EvalMatrixCell::Fail; 2];
        // An unmapped context must not mark the mapped configuration shared or
        // add an unbundled contribution for an otherwise bundled control.
        unmapped.policies[0].memberships.clear();
        snapshot.rows.push(unmapped.clone());
        unmapped.configuration_name = "new-config".into();
        snapshot.rows.push(unmapped);
        let matrix = captured(&snapshot).unwrap();
        assert_eq!(matrix.rows.len(), 1);
        assert!(!matrix.rows[0].shared_configuration);
        assert_eq!(matrix.rows[0].common_summary.failed, 0);
        assert_eq!(configuration_health(&matrix.rows).len(), 1);
        let bundled_id = snapshot.columns[0].policy_version_id.to_string();
        assert!(
            matrix
                .bundles
                .iter()
                .filter(|b| b.synthetic)
                .all(|b| !b.control_ids.contains(&bundled_id))
        );
        assert_eq!(snapshot.inventory, inventory);
        snapshot.rows[0].system_id = None;
        assert!(captured(&snapshot).unwrap().rows.is_empty());
    }

    #[test]
    fn mapped_missing_results_remain_visible_with_unknown_health() {
        let mut snapshot = with_inventory(&["config"], &["config"]);
        snapshot.rows[0].cells.clear();
        snapshot.rows[0].details.clear();
        let matrix = captured(&snapshot).unwrap();
        assert_eq!(matrix.rows.len(), 1);
        assert_eq!(matrix.rows[0].source.system_id, snapshot.rows[0].system_id);
        assert_eq!(matrix.rows[0].common_summary.unknown, 2);
        assert_eq!(matrix.rows[0].health(), RowHealth::Unavailable);
    }

    #[test]
    fn unmapped_bundle_memberships_do_not_contribute_to_overlap() {
        let mut snapshot = fixture();
        let mut unmapped = snapshot.rows[0].clone();
        unmapped.system_id = None;
        snapshot.rows[0].policies[0].memberships.pop();
        snapshot.rows.push(unmapped);
        let matrix = captured(&snapshot).unwrap();
        assert_eq!(matrix.rows.len(), 1);
        assert_eq!(model::shared_control_count(&matrix.bundles), 0);
        assert!(matrix.bundles[1].control_ids.is_empty());
    }

    #[test]
    fn configuration_failure_changes_row_filter_without_becoming_a_control() {
        let mut value = serde_json::to_value(with_inventory(&["config"], &["config"])).unwrap();
        value["configuration_failures"] = json!([
            {"configuration_name": "config", "detail": "Saved agent requirement failure"},
            {"configuration_name": "other-config", "detail": "Different configuration"}
        ]);
        let snapshot: EvalPolicyMatrixSnapshot = serde_json::from_value(value).unwrap();
        let matrix = captured(&snapshot).unwrap();
        let row = &matrix.rows[0];
        assert!(row.configuration_failed);
        assert_eq!(row.health(), RowHealth::Failing);
        assert_eq!(row.common_summary.failed, 0);
        assert_eq!(row.common_summary.passed, 1);
        assert_eq!(matrix.controls.len(), 2);
        let mut excluded = snapshot.clone();
        excluded.inventory.as_mut().unwrap().configurations.clear();
        let matrix = captured(&excluded).unwrap();
        assert!(!matrix.rows[0].configuration_failed);
        assert_eq!(matrix.rows[0].health(), RowHealth::Unavailable);
    }

    #[test]
    fn trim_requires_explicit_greenfield_and_retains_all_greenfield_fallback() {
        let mut snapshot = fixture();
        snapshot.rows.push(snapshot.rows[0].clone());
        snapshot.rows[0].greenfield = Some(true);
        snapshot.rows[1].greenfield = None;
        let matrix = captured(&snapshot).unwrap();
        let baseline = trim_baseline(matrix.rows.iter().collect());
        assert_eq!(baseline.len(), 1);
        assert_eq!(baseline[0].source.greenfield, None);
        snapshot.rows[1].greenfield = Some(true);
        let all = captured(&snapshot).unwrap();
        assert_eq!(trim_baseline(all.rows.iter().collect()).len(), 2);
        assert_eq!(all.rows[0].common_summary.applicable, 2);
    }

    #[test]
    fn indexed_bundle_aggregation_preserves_overlapping_exact_contributions() {
        let matrix = captured(&fixture()).unwrap();
        let row = &matrix.rows[0];
        assert_eq!(row.bundle_summaries.len(), matrix.bundles.len());
        assert_eq!(row.bundle_summary(&matrix.bundles[0]).passed, 1);
        assert_eq!(row.bundle_summary(&matrix.bundles[1]).passed, 1);
        assert_eq!(row.common_summary.passed, 1);
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
