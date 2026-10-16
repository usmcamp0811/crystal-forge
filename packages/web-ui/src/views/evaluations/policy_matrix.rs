//! Provides pure aggregation and presentation state for the policy matrix.
//!
//! A control is an exact policy-version identity, not a framework requirement.
//! A policy can map to several requirements, and a requirement can need several
//! policies. These counts therefore do not establish requirement compliance.
//!
//! Callers supply captured memberships and a row's outcomes keyed by stable
//! identity. This module does not resolve catalog state or infer applicability
//! from outcome availability. Missing requested outcomes remain unknown. The
//! caller handles transport, resolution, stale-attempt, and row errors upstream;
//! unavailable applicability must not be presented as a successful empty plan.

use std::collections::{HashMap, HashSet};

/// Identifies an exact policy version independently of its display label.
pub(super) type ControlId = String;

/// Describes one control's outcome for one system and evaluation attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    /// The applicable control completed successfully.
    Pass,
    /// The applicable control completed with a warning, not a pass.
    Warning,
    /// The applicable control failed.
    Fail,
    /// The control applies, but a trustworthy outcome is unavailable.
    Unknown,
    /// The control is explicitly known not to apply to this system.
    NotAssigned,
}

/// Contains captured bundle membership, with duplicate controls permitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Bundle {
    /// Stable exact bundle-version identity; duplicate IDs denote one bundle.
    pub(super) id: String,
    /// Whether this is a presentation group rather than a real bundle.
    pub(super) synthetic: bool,
    /// Exact policy-version identities represented by this bundle.
    pub(super) control_ids: Vec<ControlId>,
}

/// Counts distinct applicable controls within a requested identity set.
///
/// Aggregation guarantees `applicable == passed + warnings + failed + unknown`.
/// Unknown outcomes remain in the denominator; unassigned controls do not.
/// Callers should obtain summaries through [`aggregate`] or [`bundle_summary`]
/// so that this invariant holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Summary {
    /// Applicable controls with a passing outcome.
    pub(super) passed: usize,
    /// Applicable controls with a warning outcome.
    pub(super) warnings: usize,
    /// Applicable controls with a failing outcome.
    pub(super) failed: usize,
    /// Applicable controls without a trustworthy outcome.
    pub(super) unknown: usize,
    /// Denominator for both the passing ratio and the health bar.
    pub(super) applicable: usize,
}

/// Aggregates requested controls once each, using exact identities for lookup.
///
/// An absent map entry means [`Outcome::Unknown`], never
/// [`Outcome::NotAssigned`]. Only explicit `NotAssigned` entries are excluded
/// from the denominator. Map entries outside `control_ids` are ignored.
/// For a global row summary, pass the union of all represented memberships;
/// duplicates in that union are safe and do not inflate health counts.
pub(super) fn aggregate(
    control_ids: &[ControlId],
    outcomes: &HashMap<ControlId, Outcome>,
) -> Summary {
    let mut summary = Summary::default();
    let mut seen = HashSet::new();
    for id in control_ids {
        if !seen.insert(id.as_str()) {
            continue;
        }
        match outcomes.get(id).copied().unwrap_or(Outcome::Unknown) {
            Outcome::Pass => summary.passed += 1,
            Outcome::Warning => summary.warnings += 1,
            Outcome::Fail => summary.failed += 1,
            Outcome::Unknown => summary.unknown += 1,
            Outcome::NotAssigned => continue,
        }
        summary.applicable += 1;
    }
    summary
}

/// Aggregates one bundle, counting each of its controls at most once.
///
/// A shared control contributes once to each containing bundle, but contributes
/// only once to a global [`aggregate`] over the union of memberships.
pub(super) fn bundle_summary(bundle: &Bundle, outcomes: &HashMap<ControlId, Outcome>) -> Summary {
    aggregate(&bundle.control_ids, outcomes)
}

/// Counts controls present in at least two distinct real bundle identities.
///
/// Synthetic groups contribute no memberships. Repeated memberships and
/// repeated records for the same bundle ID cannot create false overlap.
/// If records repeat a real bundle ID, their memberships form that bundle's
/// union. This is membership metadata, independent of any system's outcomes.
pub(super) fn shared_control_count(bundles: &[Bundle]) -> usize {
    let mut memberships: HashMap<&str, HashSet<&str>> = HashMap::new();
    for bundle in bundles.iter().filter(|bundle| !bundle.synthetic) {
        for control in &bundle.control_ids {
            memberships
                .entry(control.as_str())
                .or_default()
                .insert(bundle.id.as_str());
        }
    }
    memberships.values().filter(|ids| ids.len() > 1).count()
}

/// Classifies row health without treating empty or partial coverage as clean.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowHealth {
    /// At least one applicable control failed, regardless of missing results.
    Failing,
    /// No control failed, but at least one produced a warning.
    Warning,
    /// At least one control applies, and every applicable control passed.
    Clean,
    /// No controls apply, or results are incomplete without a known issue.
    Unavailable,
}

/// Returns failure, warning, unavailable, or clean health in that precedence.
///
/// Empty summaries are N/A. Partial summaries retain known failures or warnings
/// while their `unknown` count still exposes incomplete coverage. Errors and
/// unresolved applicability must be handled by the caller before classification.
pub(super) fn row_health(summary: &Summary) -> RowHealth {
    if summary.failed > 0 {
        RowHealth::Failing
    } else if summary.warnings > 0 {
        RowHealth::Warning
    } else if summary.applicable == 0 || summary.unknown > 0 {
        RowHealth::Unavailable
    } else {
        RowHealth::Clean
    }
}

/// Tests legacy flat results for a nonempty, fully passing evaluated subset.
///
/// This adapter controls filtering only. It does not establish bundle
/// membership or claim complete applicability for a legacy evaluation.
pub(super) fn legacy_row_is_clean(
    passed: usize,
    failed: usize,
    warnings: usize,
    unknown: usize,
) -> bool {
    row_health(&Summary {
        passed,
        failed,
        warnings,
        unknown,
        applicable: passed + failed + warnings + unknown,
    }) == RowHealth::Clean
}

/// Returns descending Worst-first priority without treating unknown as passing.
///
/// The design weights a known failure by ten and a warning by one. Preserve
/// that ordering for known results; missing evidence breaks ties before clean
/// rows rather than allowing a large unknown set to outrank a known failure.
pub(super) fn worst_first_key(summary: &Summary, resolved: bool) -> (usize, usize) {
    (
        summary.failed * 10 + summary.warnings,
        summary.unknown + usize::from(!resolved),
    )
}

/// Selects rows by their mutually exclusive [`RowHealth`] classification.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum RowFilter {
    /// Includes every row whose upstream data is ready for classification.
    #[default]
    All,
    /// Includes rows with known failing controls.
    Failing,
    /// Includes warning rows without known failures.
    Warning,
    /// Includes only fully passing, nonempty rows.
    Clean,
    /// Includes empty rows and incomplete rows without known issues.
    Unavailable,
}

impl RowFilter {
    /// Returns whether a summary belongs to this filter; `All` always matches.
    pub(super) fn matches(self, summary: &Summary) -> bool {
        matches!(
            (self, row_health(summary)),
            (Self::All, _)
                | (Self::Failing, RowHealth::Failing)
                | (Self::Warning, RowHealth::Warning)
                | (Self::Clean, RowHealth::Clean)
                | (Self::Unavailable, RowHealth::Unavailable)
        )
    }
}

/// Selects the unit represented by matrix columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ColumnMode {
    /// Shows one rollup column per captured bundle or synthetic group.
    Bundles,
    /// Shows exact policy-version controls, optionally within a focused bundle.
    Controls,
}

/// Records the row ordering choice independently of column presentation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum RowSort {
    /// Requests issue-first ordering from the consuming view.
    #[default]
    WorstFirst,
    /// Requests system-name ordering from the consuming view.
    Name,
}

/// Stores matrix choices across mode changes and bundle drilldown.
///
/// Initialize once per matrix, not on each render or filter update. A focus is
/// a bundle identity, not an index or label. The caller validates that identity
/// against the displayed capture before selecting focused controls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ColumnState {
    /// Current rollup or individual-control presentation.
    pub(super) mode: ColumnMode,
    /// Selected row filter, preserved by column transitions.
    pub(super) filter: RowFilter,
    /// Selected row ordering, preserved by column transitions.
    pub(super) sort: RowSort,
    /// Exact bundle identity selected for control drilldown, if any.
    pub(super) focused_bundle: Option<String>,
    /// Whether the view should trim its initial column presentation.
    pub(super) trim: bool,
}

impl ColumnState {
    /// Creates readable defaults from the number of distinct represented controls.
    ///
    /// The design's dense threshold is strictly greater than 14 controls.
    /// Dense matrices start in trimmed bundle mode; smaller matrices start in
    /// untrimmed control mode. This count must be deduplicated by the caller.
    pub(super) fn new(unique_control_count: usize) -> Self {
        let dense = unique_control_count > 14;
        Self {
            mode: if dense {
                ColumnMode::Bundles
            } else {
                ColumnMode::Controls
            },
            filter: RowFilter::All,
            sort: RowSort::WorstFirst,
            focused_bundle: None,
            trim: dense,
        }
    }

    /// Changes column mode while preserving filter, sort, and trim choices.
    ///
    /// Selecting either mode explicitly leaves drilldown and shows that mode's
    /// full identity set. Use [`Self::focus_bundle`] for scoped control columns.
    pub(super) fn set_mode(&mut self, mode: ColumnMode) {
        self.mode = mode;
        self.focused_bundle = None;
    }

    /// Opens a bundle's controls while preserving filter, sort, and trim choices.
    ///
    /// The caller supplies an exact captured bundle identity. This transition
    /// does not expand a system row or mutate any evaluation outcome.
    pub(super) fn focus_bundle(&mut self, bundle_id: &str) {
        self.mode = ColumnMode::Controls;
        self.focused_bundle = Some(bundle_id.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(values: &[&str]) -> Vec<ControlId> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn outcomes(values: &[(&str, Outcome)]) -> HashMap<ControlId, Outcome> {
        values
            .iter()
            .map(|(id, outcome)| ((*id).to_owned(), *outcome))
            .collect()
    }

    fn bundle(id: &str, synthetic: bool, controls: &[&str]) -> Bundle {
        Bundle {
            id: id.to_owned(),
            synthetic,
            control_ids: ids(controls),
        }
    }

    #[test]
    fn overlap_counts_once_globally_and_once_in_each_bundle() {
        let bundles = [
            bundle("a@1", false, &["shared@1", "only-a@1"]),
            bundle("b@1", false, &["shared@1", "only-b@1"]),
        ];
        let outcomes = outcomes(&[
            ("shared@1", Outcome::Pass),
            ("only-a@1", Outcome::Warning),
            ("only-b@1", Outcome::Fail),
        ]);
        let union: Vec<_> = bundles
            .iter()
            .flat_map(|bundle| bundle.control_ids.iter().cloned())
            .collect();
        assert_eq!(
            aggregate(&union, &outcomes),
            Summary {
                passed: 1,
                warnings: 1,
                failed: 1,
                unknown: 0,
                applicable: 3,
            }
        );
        let a = bundle_summary(&bundles[0], &outcomes);
        let b = bundle_summary(&bundles[1], &outcomes);
        assert_eq!((a.passed, a.warnings, a.applicable), (1, 1, 2));
        assert_eq!((b.passed, b.failed, b.applicable), (1, 1, 2));
        assert_eq!(shared_control_count(&bundles), 1);
    }

    #[test]
    fn duplicate_memberships_and_unrequested_outcomes_do_not_inflate_counts() {
        let bundle = bundle("a@1", false, &["p@1", "p@1", "p@2", "p@2"]);
        let outcomes = outcomes(&[("p@1", Outcome::Pass), ("outside", Outcome::Fail)]);
        let summary = bundle_summary(&bundle, &outcomes);
        assert_eq!(summary.passed, 1);
        assert_eq!(summary.unknown, 1);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.applicable, 2);
        assert_eq!(summary, aggregate(&bundle.control_ids, &outcomes));
    }

    #[test]
    fn shared_count_uses_distinct_real_bundle_ids() {
        let bundles = [
            bundle("a@1", false, &["shared", "shared", "only-a"]),
            bundle("a@1", false, &["shared", "only-a", "split-record"]),
            bundle("a@1", false, &["split-record"]),
            bundle("b@1", false, &["shared", "only-b"]),
            bundle("synthetic", true, &["only-a", "only-b", "synthetic-only"]),
            bundle("synthetic-2", true, &["synthetic-only"]),
        ];
        assert_eq!(shared_control_count(&bundles), 1);
        assert_eq!(shared_control_count(&bundles[..3]), 0);
        assert_eq!(shared_control_count(&[]), 0);
    }

    #[test]
    fn unknown_is_applicable_but_explicit_not_assigned_is_na() {
        let outcomes = outcomes(&[
            ("pass", Outcome::Pass),
            ("unknown", Outcome::Unknown),
            ("na", Outcome::NotAssigned),
        ]);
        let summary = aggregate(&ids(&["pass", "unknown", "missing", "na"]), &outcomes);
        assert_eq!(summary.passed, 1);
        assert_eq!(summary.unknown, 2);
        assert_eq!(summary.applicable, 3);
        assert_eq!(row_health(&summary), RowHealth::Unavailable);
        let na = aggregate(&ids(&["na"]), &outcomes);
        assert_eq!(na, Summary::default());
        assert_eq!(row_health(&na), RowHealth::Unavailable);
        assert!(!RowFilter::Clean.matches(&na));
        assert!(RowFilter::Unavailable.matches(&na));
        assert_eq!(row_health(&Summary::default()), RowHealth::Unavailable);
    }

    #[test]
    fn warnings_are_not_passes_and_known_issues_take_precedence() {
        let outcomes = outcomes(&[
            ("pass", Outcome::Pass),
            ("warn", Outcome::Warning),
            ("fail", Outcome::Fail),
        ]);
        let warning = aggregate(&ids(&["pass", "warn", "missing"]), &outcomes);
        assert_eq!(warning.passed, 1);
        assert_eq!(warning.warnings, 1);
        assert_eq!(warning.applicable, 3);
        assert_eq!(row_health(&warning), RowHealth::Warning);
        assert!(RowFilter::Warning.matches(&warning));
        assert!(!RowFilter::Clean.matches(&warning));
        let failing = aggregate(&ids(&["warn", "fail", "missing"]), &outcomes);
        assert_eq!(row_health(&failing), RowHealth::Failing);
        assert!(RowFilter::Failing.matches(&failing));
        assert!(!RowFilter::Warning.matches(&failing));
    }

    #[test]
    fn filters_partition_ready_rows_and_all_includes_each_health() {
        let outcomes = outcomes(&[
            ("pass", Outcome::Pass),
            ("warn", Outcome::Warning),
            ("fail", Outcome::Fail),
        ]);
        let filters = [
            RowFilter::Clean,
            RowFilter::Warning,
            RowFilter::Failing,
            RowFilter::Unavailable,
        ];
        for (id, expected) in [
            ("pass", RowHealth::Clean),
            ("warn", RowHealth::Warning),
            ("fail", RowHealth::Failing),
            ("missing", RowHealth::Unavailable),
        ] {
            let summary = aggregate(&ids(&[id]), &outcomes);
            assert_eq!(row_health(&summary), expected);
            assert!(RowFilter::All.matches(&summary));
            assert_eq!(filters.iter().filter(|f| f.matches(&summary)).count(), 1);
        }
    }

    #[test]
    fn dense_and_small_defaults_respect_strict_threshold() {
        for count in [0, 1, 14] {
            let state = ColumnState::new(count);
            assert_eq!(state.mode, ColumnMode::Controls);
            assert!(!state.trim);
            assert_eq!(state.filter, RowFilter::All);
            assert_eq!(state.sort, RowSort::WorstFirst);
            assert_eq!(state.focused_bundle, None);
        }
        for count in [15, 100] {
            let state = ColumnState::new(count);
            assert_eq!(state.mode, ColumnMode::Bundles);
            assert!(state.trim);
        }
    }

    #[test]
    fn focus_and_mode_changes_preserve_filter_sort_and_trim() {
        for filter in [
            RowFilter::All,
            RowFilter::Failing,
            RowFilter::Warning,
            RowFilter::Clean,
            RowFilter::Unavailable,
        ] {
            for sort in [RowSort::WorstFirst, RowSort::Name] {
                for count in [4, 20] {
                    let mut state = ColumnState::new(count);
                    state.filter = filter;
                    state.sort = sort;
                    let trim = state.trim;
                    state.focus_bundle("bundle@7");
                    assert_eq!(state.mode, ColumnMode::Controls);
                    assert_eq!(state.focused_bundle.as_deref(), Some("bundle@7"));
                    assert_eq!((state.filter, state.sort, state.trim), (filter, sort, trim));
                    state.focus_bundle("bundle@8");
                    assert_eq!(state.focused_bundle.as_deref(), Some("bundle@8"));
                    for mode in [ColumnMode::Bundles, ColumnMode::Controls] {
                        state.set_mode(mode);
                        assert_eq!(state.mode, mode);
                        assert_eq!(state.focused_bundle, None);
                        assert_eq!((state.filter, state.sort, state.trim), (filter, sort, trim));
                    }
                }
            }
        }
    }
}
