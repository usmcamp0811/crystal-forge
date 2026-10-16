//! Composes the captured bundle/control matrix and local presentation choices.
//!
//! Transport, loading, cancellation, and retries remain in the drawer owner.
//! All interactions here change local presentation only.

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

/// Renders captured exact-version memberships and preserves choices across modes.
#[component]
pub(super) fn CapturedMatrix(
    snapshot: EvalPolicyMatrixSnapshot,
    compatibility: EvalPolicyMatrixResponse,
    on_open_policy: EventHandler<String>,
) -> Element {
    let Some(matrix) = captured(&snapshot) else {
        return rsx! {};
    };
    let count = matrix.controls.len();
    let mut state = use_signal(move || ColumnState::new(count));
    let mut expanded = use_signal(|| None::<String>);
    let mut issue = use_signal(|| None::<String>);
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
        rows.clone()
    };
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
    let real_bundles = matrix.bundles.iter().filter(|b| !b.synthetic).count();
    let shared = model::shared_control_count(&matrix.bundles);
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
                                    onclick: move |_| { issue.set(if active { None } else { Some(id.clone()) }); },
                                    span { class: "pm-issue-dot" }
                                    span { class: "mono", "{control.name} · {control.version}" }
                                    " {failed}/{applicable} hosts fail"
                                }
                            }
                        }
                    }
                    if issue.read().is_some() { button { class: "btn btn-ghost xs focus-ring", onclick: move |_| issue.set(None), "Clear issue filter" } }
                }
            }
            div { class: "pm-controls pm-controls-wrap",
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
                span { class: "pm-col-note", "Columns" }
                div { class: "seg",
                    button { class: if bundle_mode { "active" } else { "" }, onclick: move |_| state.write().set_mode(ColumnMode::Bundles), "Bundles ", span { class: "pm-count", "{real_bundles}" } }
                    button { class: if !bundle_mode { "active" } else { "" }, onclick: move |_| state.write().set_mode(ColumnMode::Controls), "Controls ", span { class: "pm-count", "{count}" } }
                }
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
                span { class: "pm-col-note", "Sort" }
                div { class: "seg",
                    button { class: if choices.sort == RowSort::WorstFirst { "active" } else { "" }, onclick: move |_| state.write().sort = RowSort::WorstFirst, "Worst first" }
                    button { class: if choices.sort == RowSort::Name { "active" } else { "" }, onclick: move |_| state.write().sort = RowSort::Name, "Name" }
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
                                    rsx! { th { key: "{id}", class: "pm-th-bundle",
                                        button { class: "pm-bundle-heading focus-ring", title: "{bundle_label(&bundle.id)} · {framework} — {bundle.control_ids.len()} controls; {summary.failed} fail / {summary.warnings} warning / {summary.passed} pass / {summary.unknown} unknown. View constituent controls.", onclick: move |_| state.write().focus_bundle(&id),
                                            span { class: "pm-th-bundle-inner",
                                                span { class: "pm-th-bundle-name", "{bundle_label(&bundle.id)}" }
                                                span { class: "pm-th-bundle-sub",
                                                    if !framework.trim().is_empty() { "{framework.trim()} · " }
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
                    } }
                    tbody {
                        for row in &rows {
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
                                    }
                                    if is_expanded {
                                        tr { class: "pm-expand-row", td { colspan: columns + 2,
                                            RowDetails { row: (**row).clone(), controls: matrix.controls.clone(), bundles: snapshot.bundles.clone(), on_open_policy }
                                        } }
                                    }
                                }
                            }
                        }
                        if rows.is_empty() { tr { td { colspan: columns + 2, class: "pm-empty", "No planned systems match this filter." } } }
                    }
                }
            }
            div { class: "pm-legend",
                span { span { class: "pm-legend-sw pm-pass", "✓" } " Pass" }
                span { span { class: "pm-legend-sw pm-warn", "!" } " Warning" }
                span { span { class: "pm-legend-sw pm-fail", "✗" } " Fail" }
                span { title: "Unknown applicable outcomes remain in the health denominator.", span { class: "pm-legend-sw pm-legacy-unknown", "?" } " Unknown" }
                span { title: "Not assigned is excluded from the health denominator; empty assigned bundles are not passes.", span { class: "pm-legend-sw pm-not-assigned", "–" } " N/A" }
                span { class: "pm-legend-guidance",
                    if bundle_mode { "Click a bundle cell to expand its controls · Click a row for detail" }
                    else { "Click any control header or cell to filter · Click a row to expand" }
                }
            }
            PlatformEvidence { data: compatibility }
        }
    }
}
