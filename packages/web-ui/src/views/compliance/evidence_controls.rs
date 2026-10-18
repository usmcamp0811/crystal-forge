//! Selects the controls an evaluation-evidence receiver can step through.
//!
//! Every function reads only the captured attempt response. None consults the
//! current bundle catalog, so a later catalog edit cannot add or remove a
//! control from a historical check.

use std::collections::HashMap;

use uuid::Uuid;

use crate::api::models::eval_policy_matrix::{
    EvalMatrixCell, EvalMatrixRow, EvalPolicyMatrixSnapshot,
};

/// One control the captured system received through the captured bundle version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NavControl {
    /// Position of the control in the snapshot's column and cell order.
    pub position: usize,
    /// Exact policy version. The evidence route addresses a control by this ID.
    pub policy_version_id: Uuid,
    /// Captured version-owned control name.
    pub name: String,
    /// Captured version label.
    pub version: String,
    /// Captured outcome for this system. Never inferred from the catalog.
    pub outcome: EvalMatrixCell,
}

/// Returns the controls that `row` received through `bundle_version_id`.
///
/// A control is included only when all of the following hold:
///
/// - exactly one snapshot column carries its policy version;
/// - exactly one binding on `row` names that policy version;
/// - that binding records a membership of `bundle_version_id`.
///
/// These are the same conditions the receiver applies to the selected check.
/// A control that fails any condition is left out because the evidence route
/// cannot resolve it unambiguously. Snapshot column order is preserved.
pub(super) fn contributed_controls(
    snapshot: &EvalPolicyMatrixSnapshot,
    row: &EvalMatrixRow,
    bundle_version_id: Uuid,
) -> Vec<NavControl> {
    // PERFORMANCE: Count versions once. Checking each column against every
    // other column would make a receiver render quadratic in bundle size.
    let mut version_counts = HashMap::new();
    for column in &snapshot.columns {
        *version_counts
            .entry(column.policy_version_id)
            .or_insert(0_usize) += 1;
    }
    snapshot
        .columns
        .iter()
        .enumerate()
        .filter_map(|(position, column)| {
            if version_counts.get(&column.policy_version_id) != Some(&1) {
                return None;
            }
            let mut bindings = row
                .policies
                .iter()
                .filter(|binding| binding.policy_version_id == column.policy_version_id);
            let binding = bindings.next()?;
            if bindings.next().is_some()
                || !binding
                    .memberships
                    .iter()
                    .any(|membership| membership.bundle_version_id == bundle_version_id)
            {
                return None;
            }
            Some(NavControl {
                position,
                policy_version_id: column.policy_version_id,
                name: column.name.clone(),
                version: column.version.clone(),
                outcome: row
                    .cells
                    .get(position)
                    .copied()
                    .unwrap_or(EvalMatrixCell::Unknown),
            })
        })
        .collect()
}

/// Names a captured outcome without implying more than the evaluation proved.
pub(super) fn outcome_label(outcome: EvalMatrixCell) -> &'static str {
    match outcome {
        EvalMatrixCell::Pass => "Pass",
        EvalMatrixCell::Warn => "Warn",
        EvalMatrixCell::Fail => "Fail",
        EvalMatrixCell::Unknown => "Unknown",
        EvalMatrixCell::NotAssigned => "Not assigned",
    }
}

/// Keeps the controls whose name, version, or outcome label contains `query`.
///
/// Matching ignores case and surrounding whitespace. An empty query keeps
/// every control. Order is preserved.
pub(super) fn filter_controls<'a>(controls: &'a [NavControl], query: &str) -> Vec<&'a NavControl> {
    let query = query.trim().to_lowercase();
    controls
        .iter()
        .filter(|control| {
            query.is_empty()
                || control.name.to_lowercase().contains(&query)
                || control.version.to_lowercase().contains(&query)
                || outcome_label(control.outcome)
                    .to_lowercase()
                    .contains(&query)
        })
        .collect()
}

/// Picks the control `step` places from `current` in the visible list.
///
/// A positive `step` moves down and a negative `step` moves up. The result
/// stops at the first or last visible control and never wraps. When `current`
/// is hidden by the filter, a positive step selects the first visible control
/// and a negative step selects the last. Returns `None` for an empty list or a
/// zero `step`.
pub(super) fn adjacent(visible: &[&NavControl], current: Uuid, step: isize) -> Option<Uuid> {
    if visible.is_empty() || step == 0 {
        return None;
    }
    let last = visible.len() - 1;
    let index = match visible
        .iter()
        .position(|control| control.policy_version_id == current)
    {
        Some(index) => index.saturating_add_signed(step).min(last),
        None if step > 0 => 0,
        None => last,
    };
    Some(visible[index].policy_version_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::eval_policy_matrix::{
        EvalBundleMembership, EvalBundleSource, EvalMatrixResolution, EvalPolicyBinding,
        EvalPolicyColumn,
    };
    use serde_json::json;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn control(n: u128, name: &str, outcome: EvalMatrixCell) -> NavControl {
        NavControl {
            position: n as usize,
            policy_version_id: id(n),
            name: name.to_string(),
            version: "1.0".to_string(),
            outcome,
        }
    }

    fn column(version: u128) -> EvalPolicyColumn {
        serde_json::from_value(json!({
            "policy_version_id": id(version),
            "policy_id": id(version + 1000),
            "name": format!("Control {version}"),
            "version": "1.0",
        }))
        .unwrap()
    }

    fn membership(bundle_version: u128) -> EvalBundleMembership {
        EvalBundleMembership {
            bundle_version_id: id(bundle_version),
            assignment_id: Some(id(bundle_version + 2000)),
            assignment_version_id: Some(id(bundle_version + 3000)),
            source: EvalBundleSource::Assignment,
        }
    }

    fn binding(version: u128, bundle_versions: &[u128]) -> EvalPolicyBinding {
        EvalPolicyBinding {
            policy_version_id: id(version),
            report_only: false,
            executable: true,
            memberships: bundle_versions.iter().map(|v| membership(*v)).collect(),
        }
    }

    fn snapshot(columns: Vec<EvalPolicyColumn>) -> EvalPolicyMatrixSnapshot {
        serde_json::from_value(json!({
            "evaluation_attempt_id": id(77),
            "attempt_number": 1,
            "provenance": "captured",
            "columns": columns,
            "bundles": [],
            "empty_bundle_metadata_complete": true,
            "rows": [],
        }))
        .unwrap_or_else(|error| panic!("snapshot fixture drifted: {error}"))
    }

    fn row(policies: Vec<EvalPolicyBinding>, cells: Vec<EvalMatrixCell>) -> EvalMatrixRow {
        EvalMatrixRow {
            system_id: Some(id(9)),
            greenfield: None,
            system_name: "host-a".into(),
            configuration_name: "host-a".into(),
            resolution: EvalMatrixResolution::Resolved,
            evaluation_digest: None,
            assigned_bundles: Vec::new(),
            policies,
            cells,
            details: Vec::new(),
        }
    }

    #[test]
    fn includes_only_controls_contributed_by_the_captured_bundle_version() {
        let snapshot = snapshot(vec![column(1), column(2), column(3)]);
        let row = row(
            vec![
                binding(1, &[500]),
                binding(2, &[501]),
                binding(3, &[500, 501]),
            ],
            vec![
                EvalMatrixCell::Pass,
                EvalMatrixCell::Fail,
                EvalMatrixCell::Warn,
            ],
        );
        let controls = contributed_controls(&snapshot, &row, id(500));
        assert_eq!(
            controls
                .iter()
                .map(|c| (c.position, c.outcome))
                .collect::<Vec<_>>(),
            vec![(0, EvalMatrixCell::Pass), (2, EvalMatrixCell::Warn)],
            "control 2 belongs only to another bundle version"
        );
    }

    #[test]
    fn omits_controls_the_route_cannot_resolve_unambiguously() {
        // Version 1 appears in two columns; version 2 has two bindings; version
        // 3 has no binding. None of them can be addressed by a policy version.
        let snapshot = snapshot(vec![column(1), column(1), column(2), column(3), column(4)]);
        let row = row(
            vec![
                binding(1, &[500]),
                binding(2, &[500]),
                binding(2, &[500]),
                binding(4, &[500]),
            ],
            vec![EvalMatrixCell::Pass; 5],
        );
        let controls = contributed_controls(&snapshot, &row, id(500));
        assert_eq!(
            controls
                .iter()
                .map(|c| c.policy_version_id)
                .collect::<Vec<_>>(),
            vec![id(4)]
        );
    }

    #[test]
    fn missing_cell_is_unknown_not_a_pass() {
        let snapshot = snapshot(vec![column(1)]);
        let row = row(vec![binding(1, &[500])], Vec::new());
        assert_eq!(
            contributed_controls(&snapshot, &row, id(500))[0].outcome,
            EvalMatrixCell::Unknown
        );
    }

    #[test]
    fn filter_matches_name_version_and_outcome_ignoring_case_and_padding() {
        let controls = vec![
            control(1, "Password Aging", EvalMatrixCell::Pass),
            control(2, "SSH Root Login", EvalMatrixCell::Fail),
        ];
        let names = |query: &str| {
            filter_controls(&controls, query)
                .iter()
                .map(|c| c.policy_version_id)
                .collect::<Vec<_>>()
        };
        assert_eq!(names("  "), vec![id(1), id(2)]);
        assert_eq!(names("ssh"), vec![id(2)]);
        assert_eq!(names("PASS"), vec![id(1)]);
        assert_eq!(names("1.0"), vec![id(1), id(2)]);
        assert!(names("absent").is_empty());
    }

    #[test]
    fn adjacent_stops_at_the_edges_and_never_wraps() {
        let controls = vec![
            control(1, "a", EvalMatrixCell::Pass),
            control(2, "b", EvalMatrixCell::Pass),
            control(3, "c", EvalMatrixCell::Pass),
        ];
        let visible = controls.iter().collect::<Vec<_>>();
        assert_eq!(adjacent(&visible, id(1), 1), Some(id(2)));
        assert_eq!(adjacent(&visible, id(3), 1), Some(id(3)));
        assert_eq!(adjacent(&visible, id(1), -1), Some(id(1)));
        assert_eq!(adjacent(&visible, id(3), -1), Some(id(2)));
        assert_eq!(adjacent(&visible, id(1), 0), None);
        assert_eq!(adjacent(&[], id(1), 1), None);
    }

    #[test]
    fn adjacent_from_a_filtered_out_control_enters_the_visible_list() {
        let controls = vec![
            control(2, "b", EvalMatrixCell::Pass),
            control(3, "c", EvalMatrixCell::Pass),
        ];
        let visible = controls.iter().collect::<Vec<_>>();
        assert_eq!(adjacent(&visible, id(1), 1), Some(id(2)));
        assert_eq!(adjacent(&visible, id(1), -1), Some(id(3)));
    }
}
