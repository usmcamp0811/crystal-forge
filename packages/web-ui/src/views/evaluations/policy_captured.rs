//! Composes the captured matrix from owner-held presentation choices.
//!
//! Transport, loading, cancellation, and retries remain in the drawer owner.
//! Exact-context links use typed receiver handoffs without mutating evidence.

use super::policy_adapter::{captured, nonpass};
use super::policy_cells::{
    BundleResult, HeaderStatusBar, Health, PlatformEvidence, RowDetails, outcome_label,
    total_summary,
};
use super::policy_matrix::{
    self as model, ColumnMode, ColumnState, Outcome, RowFilter, RowHealth, RowSort,
};
use crate::api::models::{EvalPolicyMatrixResponse, eval_policy_matrix::*};
use dioxus::prelude::*;

/// Describes the drawer header populations that the matrix must reconcile.
///
/// The header counts evaluated flake configurations. Its policy counters
/// classify configurations by the CF-agent and assigned-requirement gate. The
/// matrix counts registered systems with a captured plan and their controls.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HeaderScope {
    /// Evaluated NixOS configurations reported by the queue/history row.
    pub configurations: i64,
    /// Configurations that passed the configuration-level policy gate.
    pub passed: i64,
    /// Configurations that failed the configuration-level policy gate.
    pub failed: i64,
    /// False when the attempt-scoped counters are not attributable.
    pub available: bool,
}

/// Returns explicit population notes when header and matrix scopes differ.
///
/// The result never adds matrix rows or outcomes. `captured_configurations`
/// is the distinct captured configuration count, `failing_rows` counts matrix
/// rows with a failed control, and `agent_failures` counts compatibility rows
/// whose global CF-agent result failed.
pub(super) fn population_notes(
    header: &HeaderScope,
    planned_systems: usize,
    captured_configurations: usize,
    failing_rows: usize,
    agent_failures: usize,
) -> Vec<String> {
    let mut notes = Vec::new();
    let configurations = usize::try_from(header.configurations).unwrap_or(0);
    if configurations != captured_configurations {
        let mut note = format!(
            "Matrix: {planned_systems} planned system(s) from {captured_configurations} captured configuration(s). Header Systems: {configurations} evaluated flake configuration(s)"
        );
        if configurations > captured_configurations {
            note.push_str(&format!(
                "; {} configuration(s) have no registered system or captured plan visible here.",
                configurations - captured_configurations
            ));
        } else {
            note.push('.');
        }
        notes.push(note);
    }
    // Explain the policy counter only when it disagrees with the matrix, or
    // when a global platform failure has no bundle-control representation.
    if header.available && ((header.failed > 0 && failing_rows == 0) || agent_failures > 0) {
        let mut note = format!(
            "Header Policy counts configurations ({} passed / {} failed), not controls; {failing_rows} matrix system(s) have a failed control.",
            header.passed, header.failed
        );
        if agent_failures > 0 {
            note.push_str(&format!(
                " {agent_failures} configuration(s) fail the global CF-agent check (Platform evidence); it is not a bundle control."
            ));
        }
        notes.push(note);
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(configurations: i64, failed: i64) -> HeaderScope {
        HeaderScope {
            configurations,
            passed: configurations - failed,
            failed,
            available: true,
        }
    }

    #[test]
    fn population_notes_explain_unplanned_configurations_and_platform_failures() {
        let notes = population_notes(&header(41, 1), 12, 12, 0, 1);
        assert_eq!(notes.len(), 2);
        assert!(notes[0].contains("12 planned system(s)"));
        assert!(notes[0].contains("41 evaluated flake configuration(s)"));
        assert!(notes[0].contains("29 configuration(s) have no registered system"));
        assert!(notes[1].contains("40 passed / 1 failed"));
        assert!(notes[1].contains("0 matrix system(s) have a failed control"));
        assert!(notes[1].contains("1 configuration(s) fail the global CF-agent check"));
    }

    #[test]
    fn population_notes_stay_silent_when_scopes_agree_or_counts_are_unavailable() {
        assert!(population_notes(&header(9, 0), 9, 9, 0, 0).is_empty());
        // Header and matrix both show failures; no discrepancy to explain.
        assert!(population_notes(&header(9, 2), 9, 9, 1, 0).is_empty());
        let unavailable = HeaderScope {
            available: false,
            ..header(9, 3)
        };
        assert!(population_notes(&unavailable, 9, 9, 0, 0).is_empty());
        // Shared configurations: more systems than configurations is consistent.
        assert!(population_notes(&header(2, 0), 5, 2, 0, 0).is_empty());
    }
}

/// Counts compatibility rows whose global CF-agent result failed.
fn agent_failures(data: &EvalPolicyMatrixResponse) -> usize {
    let Some(index) = data.policies.iter().position(|name| name == "CF agent") else {
        return 0;
    };
    data.systems
        .iter()
        .filter(|row| {
            row.results
                .get(index)
                .is_some_and(|result| super::policy_cell_class_suffix(result) == "fail")
        })
        .count()
}

/// Renders captured exact-version memberships and preserves choices across modes.
#[component]
pub(super) fn CapturedMatrix(
    snapshot: EvalPolicyMatrixSnapshot,
    compatibility: EvalPolicyMatrixResponse,
    #[props(default)] header: Option<HeaderScope>,
    on_open_policy: EventHandler<String>,
    mut state: Signal<ColumnState>,
    mut expanded: Signal<Option<String>>,
    mut issue: Signal<Option<String>>,
    finding: Signal<Option<String>>,
    mut page: Signal<usize>,
) -> Element {
    let snapshot_for_memo = snapshot.clone();
    let adapted = use_memo(use_reactive(&snapshot_for_memo, move |snapshot| {
        captured(&snapshot)
    }));
    let adapted = adapted.read();
    let Some(matrix) = adapted.as_ref() else {
        return rsx! {};
    };
    let count = matrix.controls.len();
    let choices = state.read().clone();
    let matches_filter = |health| match choices.filter {
        RowFilter::All => true,
        RowFilter::Failing => health == RowHealth::Failing,
        RowFilter::Warning => health == RowHealth::Warning,
        RowFilter::Clean => health == RowHealth::Clean,
        RowFilter::Unavailable => health == RowHealth::Unavailable,
    };
    let mut rows: Vec<_> = matrix
        .rows
        .iter()
        .filter(|r| matches_filter(r.health()))
        .filter(|r| {
            issue
                .read()
                .as_ref()
                .is_none_or(|id| nonpass(r.outcomes.get(id).copied().unwrap_or(Outcome::Unknown)))
        })
        .collect();
    rows.sort_by(|a, b| {
        let weight = |r: &super::policy_adapter::MatrixRow| {
            model::worst_first_key(&r.common_summary, r.resolved())
        };
        if choices.sort == RowSort::WorstFirst {
            weight(b)
                .cmp(&weight(a))
                .then_with(|| a.source.system_name.cmp(&b.source.system_name))
        } else {
            a.source.system_name.cmp(&b.source.system_name)
        }
    });
    let focus = choices
        .focused_bundle
        .as_ref()
        .and_then(|id| matrix.bundles.iter().find(|b| &b.id == id));
    let scoped: Vec<_> = matrix
        .controls
        .iter()
        .filter(|c| focus.is_none_or(|b| b.control_ids.contains(&c.policy_version_id.to_string())))
        .collect();
    // Trim uses visible rows. An empty row filter falls back to all planned rows
    // so changing a row filter cannot erase every column without explanation.
    let baseline = if rows.is_empty() {
        matrix.rows.iter().collect::<Vec<_>>()
    } else {
        super::policy_adapter::trim_baseline(rows.clone())
    };
    // Only an explicit operator classification can remove rows from trimming.
    // Unknown classifications remain. All-greenfield selections retain all rows.
    let mut controls: Vec<_> = scoped
        .iter()
        .copied()
        .filter(|c| {
            !choices.trim
                || baseline.iter().any(|r| {
                    nonpass(
                        r.outcomes
                            .get(&c.policy_version_id.to_string())
                            .copied()
                            .unwrap_or(Outcome::Unknown),
                    )
                })
        })
        .collect();
    if controls.is_empty() {
        controls = scoped.clone();
    }
    let hidden = scoped.len() - controls.len();
    let mut issues: Vec<_> = matrix
        .controls
        .iter()
        .filter_map(|c| {
            let id = c.policy_version_id.to_string();
            let failed = matrix
                .rows
                .iter()
                .filter(|r| r.outcomes.get(&id) == Some(&Outcome::Fail))
                .count();
            let applicable = matrix
                .rows
                .iter()
                .filter(|r| r.outcomes.get(&id) != Some(&Outcome::NotAssigned))
                .count();
            (failed > 0).then_some((c, failed, applicable))
        })
        .collect();
    issues.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.name.cmp(&b.0.name)));
    let bundle_mode = choices.mode == ColumnMode::Bundles;
    let columns = if bundle_mode {
        matrix.bundles.len()
    } else {
        controls.len()
    };
    let page_size = super::policy_matrix::rows_per_page(columns);
    let pages = rows.len().div_ceil(page_size).max(1);
    let current_page = page().min(pages - 1);
    let visible_rows = rows.iter().skip(current_page * page_size).take(page_size);
    let real_bundles = matrix.bundles.iter().filter(|b| !b.synthetic).count();
    let shared = model::shared_control_count(&matrix.bundles);
    let notes = header
        .as_ref()
        .map(|header| {
            let configurations = snapshot
                .rows
                .iter()
                .map(|row| row.configuration_name.as_str())
                .collect::<std::collections::HashSet<_>>()
                .len();
            let failing_rows = matrix
                .rows
                .iter()
                .filter(|row| row.health() == RowHealth::Failing)
                .count();
            population_notes(
                header,
                matrix.rows.len(),
                configurations,
                failing_rows,
                agent_failures(&compatibility),
            )
        })
        .unwrap_or_default();
    let bundle_label = |id: &str| {
        snapshot
            .bundles
            .iter()
            .find(|b| b.bundle_version_id.to_string() == id)
            .map(|b| format!("{} · {}", b.name, b.version))
            .unwrap_or_else(|| "Controls without bundle membership".into())
    };

    rsx! {
        div { class: "pm-captured",
            if !snapshot.empty_bundle_metadata_complete {
                div { class: "pm-provenance-note", role: "note",
                    "Bundle context metadata is incomplete. Assigned bundles with no eligible controls may be absent; missing contexts do not prove nonassignment. This is not a complete bundle list."
                }
            }
            if !issues.is_empty() {
                div { class: "pm-issues",
                    div { class: "pm-issues-label", "Top issues" }
                    for (control, failed, applicable) in issues.into_iter().take(3) {
                        {
                            let id = control.policy_version_id.to_string();
                            let active = issue.read().as_ref() == Some(&id);
                            rsx! {
                                button { key: "{id}", class: if active { "pm-issue-chip active" } else { "pm-issue-chip" },
                                    aria_pressed: active,
                                    title: "{control.name} · {control.version}: {failed} of {applicable} applicable systems fail. Filter non-pass systems.",
                                    onclick: move |_| { issue.set(if active { None } else { Some(id.clone()) }); },
                                    span { class: "pm-issue-dot" }
                                    span { class: "mono", "{control.name} · {control.version}" }
                                    span { class: "pm-issue-fail", "{failed}" }
                                    span { class: "pm-issue-total", "/{applicable} fail" }
                                }
                            }
                        }
                    }
                    if issue.read().is_some() {
                        button { class: "btn-icon focus-ring pm-issues-clear", title: "Clear issue filter", aria_label: "Clear issue filter", onclick: move |_| issue.set(None), "×" }
                    }
                }
            }
            div { class: "pm-controls",
              div { class: "pm-controls-line",
                div { class: "seg",
                    for (filter, label, health) in [
                        (RowFilter::All, "All", None),
                        (RowFilter::Failing, "Failing", Some(RowHealth::Failing)),
                        (RowFilter::Warning, "Warning", Some(RowHealth::Warning)),
                        (RowFilter::Clean, "Clean", Some(RowHealth::Clean)),
                        (RowFilter::Unavailable, "Unavailable / N/A", Some(RowHealth::Unavailable)),
                    ] {
                        {
                            let count = matrix.rows.iter().filter(|r| health.is_none_or(|h| r.health() == h)).count();
                            rsx! {
                                if filter != RowFilter::Unavailable || count > 0 || choices.filter == filter {
                                    button { class: if choices.filter == filter { "active" } else { "" }, onclick: move |_| state.write().filter = filter,
                                        "{label} ", span {
                                            class: match filter {
                                                RowFilter::Failing => "pm-count pm-count-fail",
                                                RowFilter::Warning => "pm-count pm-count-warn",
                                                RowFilter::Clean => "pm-count pm-count-pass",
                                                _ => "pm-count",
                                            },
                                            "{count}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div { style: "flex: 1;" }
                span { class: "pm-controls-label", "Columns" }
                div { class: "seg",
                    button { class: if bundle_mode { "active" } else { "" }, title: "One roll-up column per captured bundle version", onclick: move |_| state.write().set_mode(ColumnMode::Bundles), "Bundles ", span { class: "pm-count", "{real_bundles}" } }
                    button { class: if !bundle_mode { "active" } else { "" }, title: "One column per exact policy version", onclick: move |_| state.write().set_mode(ColumnMode::Controls), "Controls ", span { class: "pm-count", "{count}" } }
                }
              }
              div { class: "pm-controls-line",
                if bundle_mode {
                    span { class: "pm-col-note",
                        title: "Controls are unique exact policy-version IDs. Shared controls belong to at least two distinct real bundle-version IDs. Synthetic groups do not count as bundles. Counts do not assert framework requirement compliance.",
                        "{count} controls · {shared} shared across bundles"
                    }
                }
                if !bundle_mode {
                    if let Some(bundle) = focus {
                        button { class: "btn btn-ghost xs focus-ring active-filter", onclick: move |_| state.write().focused_bundle = None, "{bundle_label(&bundle.id)} ×" }
                    }
                    button { class: if choices.trim { "btn btn-ghost xs focus-ring active-filter" } else { "btn btn-ghost xs focus-ring" },
                        title: "Keep failures, warnings, and unknown outcomes", onclick: move |_| { let trim = state.read().trim; state.write().trim = !trim; }, "Failing only"
                    }
                    if hidden > 0 {
                        button { class: "pm-col-note focus-ring", onclick: move |_| state.write().trim = false,
                            "{controls.len()} of {scoped.len()} columns · {hidden} without non-pass evidence hidden · Show all"
                        }
                    }
                }
                span { class: "pm-controls-label", "Sort" }
                div { class: "seg",
                    button { class: if choices.sort == RowSort::WorstFirst { "active" } else { "" }, onclick: move |_| state.write().sort = RowSort::WorstFirst, "Worst first" }
                    button { class: if choices.sort == RowSort::Name { "active" } else { "" }, onclick: move |_| state.write().sort = RowSort::Name, "Name" }
                }
              }
            }
            if !notes.is_empty() {
                div { class: "pm-scope-note", role: "note", aria_label: "Matrix scope",
                    for note in notes { p { "{note}" } }
                }
            }
            if matrix.controls.is_empty() {
                div { class: "pm-provenance-note", "No eligible control columns were captured. See each planned system's resolution; an unavailable plan is not an authoritative empty assignment." }
            }
            if bundle_mode && real_bundles == 0 {
                div { class: "pm-provenance-note", "No bundle contexts are represented. Controls without bundle membership remain separate." }
            }
            if focus.is_some_and(|bundle| bundle.control_ids.is_empty()) {
                div { class: "pm-provenance-note", "This captured bundle has no eligible control contributions. Assignment presence does not imply that any policy passed." }
            }
            div { class: "pm-scroll",
                table { class: "pm-table pm-captured-table",
                    thead { tr {
                        th { class: "pm-th-host", "System" }
                        th { class: "pm-th-health", "Health" }
                        if bundle_mode {
                            for bundle in &matrix.bundles {
                                {
                                    let id = bundle.id.clone();
                                    let metadata = snapshot.bundles.iter().find(|b| b.bundle_version_id.to_string() == id);
                                    let summary = total_summary(matrix.rows.iter().map(|row| row.bundle_summary(bundle)));
                                    let framework = metadata.map(|meta| format!("{} {}", meta.framework, meta.framework_version.as_deref().unwrap_or(""))).unwrap_or_default();
                                    let framework = framework.trim().to_owned();
                                    // Version stays visible beside the name; it is
                                    // part of the captured bundle identity.
                                    let (name, version) = metadata.map(|meta| (meta.name.clone(), Some(meta.version.clone()))).unwrap_or_else(|| (bundle_label(&bundle.id), None));
                                    rsx! { th { key: "{id}", class: "pm-th-bundle",
                                        button { class: "pm-bundle-heading focus-ring", title: "{bundle_label(&bundle.id)} · {framework} — {bundle.control_ids.len()} controls; {summary.failed} fail / {summary.warnings} warning / {summary.passed} pass / {summary.unknown} unknown. View constituent controls.", onclick: move |_| state.write().focus_bundle(&id),
                                            span { class: "pm-th-bundle-inner",
                                                span { class: "pm-th-bundle-name",
                                                    "{name}"
                                                    if let Some(version) = version { span { class: "pm-th-bundle-version", "{version}" } }
                                                }
                                                span { class: "pm-th-bundle-sub",
                                                    if !framework.is_empty() { "{framework} · " }
                                                    "{bundle.control_ids.len()} controls"
                                                }
                                            }
                                            HeaderStatusBar { summary }
                                        }
                                    } }
                                }
                            }
                        } else {
                            for control in &controls {
                                {
                                    let id = control.policy_version_id.to_string();
                                    let active = issue.read().as_ref() == Some(&id);
                                    let memberships = matrix.bundles.iter().filter(|b| !b.synthetic && b.control_ids.contains(&id)).map(|b| bundle_label(&b.id)).collect::<Vec<_>>().join("; ");
                                    let summary = total_summary(matrix.rows.iter().map(|row| model::aggregate(std::slice::from_ref(&id), &row.outcomes)));
                                    rsx! { th { key: "{id}", class: if active { "pm-th-policy filtered" } else { "pm-th-policy" }, title: "{control.name} · {control.version} — {memberships}; {summary.failed} fail / {summary.warnings} warning / {summary.passed} pass / {summary.unknown} unknown",
                                        button { class: "pm-control-heading focus-ring", aria_pressed: active, onclick: move |_| issue.set(if active { None } else { Some(id.clone()) }),
                                            span { class: "pm-th-policy-inner",
                                                span { class: "pm-th-policy-label", "{control.name} · {control.version}" }
                                            }
                                            HeaderStatusBar { summary }
                                        }
                                    } }
                                }
                            }
                        }
                        // Layout only: absorbs spare width. It is hidden from assistive
                        // technology and carries no evidence.
                        th { class: "pm-th-fill", aria_hidden: "true" }
                    } }
                    tbody {
                        for row in visible_rows {
                            {
                                let key = row.key.clone();
                                let is_expanded = expanded.read().as_ref() == Some(&key);
                                let class = if is_expanded { "pm-row expanded" } else { "pm-row" };
                                rsx! {
                                    tr { key: "{key}", class,
                                        onclick: move |_| expanded.set(if is_expanded { None } else { Some(key.clone()) }),
                                        td { class: "pm-td-host", button { class: "pm-row-toggle mono focus-ring", aria_label: "{row.source.system_name}", aria_expanded: is_expanded, "{row.source.system_name}" } }
                                        td { class: "pm-td-health", Health { summary: row.common_summary, resolved: row.resolved() } }
                                        if bundle_mode {
                                            for bundle in &matrix.bundles {
                                                {
                                                    let id = bundle.id.clone();
                                                    let summary = row.bundle_summary(bundle);
                                                    let label = if row.resolved() && summary.applicable > 0 {
                                                        Some(format!("{}: {}/{} pass · {} failed · {} warning · {} unknown — View constituent controls",
                                                            bundle_label(&id), summary.passed, summary.applicable,
                                                            summary.failed, summary.warnings, summary.unknown))
                                                    } else {
                                                        None
                                                    };
                                                    rsx! { td { key: "{id}", class: "pm-td-bundle",
                                                        button { class: "pm-bundle-button focus-ring", aria_label: label, title: "View constituent controls", onclick: move |event: MouseEvent| { event.stop_propagation(); state.write().focus_bundle(&id); },
                                                            BundleResult { summary, resolved: row.resolved(), applicability: row.bundle_applicability(bundle) }
                                                        }
                                                    } }
                                                }
                                            }
                                        } else {
                                            for control in &controls {
                                                {
                                                    let id = control.policy_version_id.to_string();
                                                    let outcome = row.outcomes.get(&id).copied().unwrap_or(Outcome::Unknown);
                                                    let (label, class) = outcome_label(outcome);
                                                    let active = issue.read().as_ref() == Some(&id);
                                                    let cell_class = if active { format!("pm-td-cell {class} col-filtered") } else { format!("pm-td-cell {class}") };
                                                    rsx! { td { key: "{id}", class: cell_class, title: "{control.name} · {control.version}: {label}",
                                                        button { class: "pm-control-cell-button pm-glyph focus-ring", aria_label: "{row.source.system_name}: {control.name} · {control.version}: {label}; filter non-pass systems", aria_pressed: active,
                                                            onclick: move |event: MouseEvent| { event.stop_propagation(); issue.set(if active { None } else { Some(id.clone()) }); },
                                                            match outcome {
                                                                Outcome::Pass => "✓", Outcome::Warning => "!", Outcome::Fail => "✗", Outcome::Unknown => "?", Outcome::NotAssigned => "–",
                                                            }
                                                        }
                                                    } }
                                                }
                                            }
                                        }
                                        td { class: "pm-td-fill", aria_hidden: "true" }
                                    }
                                    if is_expanded {
                                        tr { class: "pm-expand-row", td { colspan: columns + 3,
                                             RowDetails { row: (**row).clone(), controls: matrix.controls.clone(), bundles: snapshot.bundles.clone(), on_open_policy, snapshot: snapshot.clone(), finding }
                                        } }
                                    }
                                }
                            }
                        }
                        if rows.is_empty() { tr { td { colspan: columns + 3, class: "pm-empty", "No planned systems match this filter." } } }
                    }
                }
            }
            if pages > 1 {
                nav { aria_label: "Matrix system pages",
                    button { class: "btn btn-ghost xs focus-ring", disabled: current_page == 0, onclick: move |_| page.set(current_page.saturating_sub(1)), "Previous systems" }
                    span { role: "status", "Page {current_page + 1} of {pages} · {rows.len()} matching systems" }
                    button { class: "btn btn-ghost xs focus-ring", disabled: current_page + 1 >= pages, onclick: move |_| page.set(current_page + 1), "Next systems" }
                }
            }
            PlatformEvidence { data: compatibility.clone() }
            div { class: "pm-legend",
                span { span { class: "pm-legend-sw pm-pass", "✓" } " Pass" }
                span { span { class: "pm-legend-sw pm-warn", "!" } " Warning" }
                span { title: "Enforce-mode failure. Report-only failures are warnings.", span { class: "pm-legend-sw pm-fail", "✗" } " Fail — blocking" }
                span { title: "Unknown applicable outcomes remain in the health denominator.", span { class: "pm-legend-sw pm-legacy-unknown", "?" } " Unknown" }
                span { title: "Not assigned is excluded from the health denominator; empty assigned bundles are not passes.", span { class: "pm-legend-sw pm-not-assigned", "–" } " N/A" }
                span { class: "pm-legend-guidance",
                    if bundle_mode { "Click a bundle cell to expand its controls · Click a row for detail" }
                    else { "Click any control header or cell to filter · Click a row to expand" }
                }
            }
        }
    }
}
