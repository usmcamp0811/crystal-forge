//! Renders captured evidence without inventing missing result details.

use super::policy_adapter::{BundleApplicability, MatrixRow};
use super::policy_matrix::{Outcome, Summary};
use super::{policy_cell_class_suffix, policy_cell_glyph};
use crate::api::models::EvalPolicyMatrixResponse;
use crate::api::models::eval_policy_matrix::{
    EvalBundleColumn, EvalBundleSource, EvalPolicyColumn,
};
use dioxus::prelude::*;

/// Returns a readable outcome label and its existing matrix style class.
pub(super) fn outcome_label(outcome: Outcome) -> (&'static str, &'static str) {
    match outcome {
        Outcome::Pass => ("Pass", "pm-pass"),
        Outcome::Warning => ("Warning", "pm-warn"),
        Outcome::Fail => ("Fail", "pm-fail"),
        Outcome::Unknown => ("Unknown", "pm-legacy-unknown"),
        Outcome::NotAssigned => ("Not assigned", "pm-not-assigned"),
    }
}

/// Totals per-system summaries for one header's exact control identity set.
///
/// Each input already deduplicates controls within its system. Across systems,
/// each applicable system/control pair contributes once, including unknowns.
pub(super) fn total_summary(summaries: impl IntoIterator<Item = Summary>) -> Summary {
    summaries
        .into_iter()
        .fold(Summary::default(), |mut total, row| {
            total.passed += row.passed;
            total.warnings += row.warnings;
            total.failed += row.failed;
            total.unknown += row.unknown;
            total.applicable += row.applicable;
            total
        })
}

/// Renders actual aggregate outcomes using the applicable-pair denominator.
///
/// Empty summaries leave a neutral bar. Unknowns use gray and remain in the
/// denominator; explicit nonassignment contributes no segment or denominator.
#[component]
pub(super) fn HeaderStatusBar(summary: Summary) -> Element {
    let denominator = summary.applicable.max(1) as f64;
    rsx! {
        span { class: "pm-th-policy-bar", role: "img",
            aria_label: "{summary.failed} fail, {summary.warnings} warning, {summary.passed} pass, {summary.unknown} unknown; {summary.applicable} applicable system/control pairs",
            for (count, color) in [(summary.failed, "#f87171"), (summary.warnings, "#f59e0b"), (summary.passed, "#34d399"), (summary.unknown, "var(--cf-text-muted)")] {
                if count > 0 { span { style: "width: {100.0 * count as f64 / denominator}%; background: {color};" } }
            }
        }
    }
}

/// Renders a ratio and bar from one common applicable-control summary.
///
/// Unresolved applicability shows known evidence without an authoritative ratio.
#[component]
pub(super) fn Health(summary: Summary, resolved: bool) -> Element {
    let denominator = summary.applicable.max(1) as f64;
    let color = if summary.failed > 0 {
        "#f87171"
    } else if summary.warnings > 0 {
        "#f59e0b"
    } else if !resolved || summary.unknown > 0 || summary.applicable == 0 {
        "var(--cf-text-muted)"
    } else {
        "#34d399"
    };
    let label = if !resolved {
        "Unknown".to_owned()
    } else if summary.applicable == 0 {
        "N/A".to_owned()
    } else {
        format!("{}/{}", summary.passed, summary.applicable)
    };
    rsx! {
        div { class: "pm-health", title: if resolved {
            format!("{} failed · {} warning · {} unknown", summary.failed, summary.warnings, summary.unknown)
        } else {
            "Applicable control set is unavailable; no authoritative health ratio can be shown.".to_owned()
        },
            div { class: "pm-health-bar",
                for (count, color) in [(summary.failed, "#f87171"), (summary.warnings, "#f59e0b"), (summary.passed, "#34d399"), (summary.unknown, "var(--cf-text-muted)")] {
                    if count > 0 { div { style: "width: {100.0 * count as f64 / denominator}%; background: {color};" } }
                }
            }
            span { class: "mono pm-health-num", style: "color: {color};", "{label}" }
        }
    }
}

/// Renders bundle context presence separately from eligible-control counts.
///
/// Empty assigned bundles are not passes. Older incomplete context metadata
/// retains known contribution counts without claiming assignment completeness.
#[component]
pub(super) fn BundleResult(
    summary: Summary,
    resolved: bool,
    applicability: BundleApplicability,
) -> Element {
    let class = if applicability == BundleApplicability::NotAssigned {
        "pm-bundle-cell pm-not-assigned"
    } else if summary.failed > 0 {
        "pm-bundle-cell pm-fail"
    } else if summary.warnings > 0 {
        "pm-bundle-cell pm-warn"
    } else if !resolved || summary.unknown > 0 || applicability == BundleApplicability::Unknown {
        "pm-bundle-cell pm-legacy-unknown"
    } else if summary.applicable == 0 {
        "pm-bundle-cell pm-not-assigned"
    } else {
        "pm-bundle-cell pm-pass"
    };
    rsx! {
        span { class,
            if applicability == BundleApplicability::NotAssigned { strong { "Not assigned" } }
            else if applicability == BundleApplicability::Unknown {
                strong { "Bundle metadata unknown" }
                if !resolved { small { "Applicability unresolved" } }
                else if summary.applicable > 0 { small { "{summary.passed}/{summary.applicable} known controls" } }
            }
            else if !resolved { strong { "Applicability unresolved" } }
            else if summary.applicable == 0 { strong { title: "Assigned bundle with no eligible controls", "N/A" } }
            else { strong { "{summary.passed}/{summary.applicable}" } }
            if summary.failed > 0 { small { class: "pm-bundle-fail", title: "{summary.failed} failed controls", "{summary.failed}" } }
            if summary.warnings > 0 { small { class: "pm-bundle-warn", title: "{summary.warnings} warning controls", "{summary.warnings}" } }
            if summary.unknown > 0 { small { class: "pm-bundle-unknown", title: "{summary.unknown} unavailable control results", "?{summary.unknown}" } }
        }
    }
}

/// Renders actual captured control outcomes for an expanded planned system.
#[component]
pub(super) fn RowDetails(
    row: MatrixRow,
    controls: Vec<EvalPolicyColumn>,
    bundles: Vec<EvalBundleColumn>,
    on_open_policy: EventHandler<String>,
) -> Element {
    let mut seen = std::collections::HashSet::new();
    let contexts: Vec<_> = row
        .source
        .assigned_bundles
        .iter()
        .filter(|context| seen.insert(context.bundle_version_id))
        .collect();
    rsx! {
        div { class: "pm-expand pm-captured-details",
            p { "Configuration: ", code { "{row.source.configuration_name}" } }
            if !row.resolved() {
                p { class: "pm-provenance-note", "Applicability is incomplete: {row.source.resolution:?}. Counts are known evidence, not a complete applicability claim." }
            } else if row.common_summary.applicable == 0 {
                p { "No applicable controls for this system." }
            }
            if !row.bundle_metadata_complete {
                p { class: "pm-provenance-note", "Bundle context metadata is incomplete. Missing contexts do not prove that a bundle was unassigned." }
            } else if contexts.is_empty() {
                p { "No assigned bundle contexts were captured for this system." }
            }
            for context in contexts {
                {
                    let metadata = bundles.iter().find(|b| b.bundle_version_id == context.bundle_version_id);
                    let source = match context.source {
                        EvalBundleSource::Assignment => "Persisted assignment",
                        EvalBundleSource::VirtualBaseline => "Virtual baseline",
                    };
                    rsx! {
                        div { class: "pm-captured-detail",
                            strong { "Assigned bundle: " }
                            if let Some(bundle) = metadata {
                                span { "{bundle.name} · {bundle.version} · {bundle.framework} {bundle.framework_version.as_deref().unwrap_or(\"\")}" }
                            } else { code { "{context.bundle_version_id} (display metadata unavailable)" } }
                            span { "{source}" }
                        }
                    }
                }
            }
            for control in controls {
                {
                    let id = control.policy_version_id.to_string();
                    let outcome = row.outcomes.get(&id).copied().unwrap_or(Outcome::Unknown);
                    let (label, class) = outcome_label(outcome);
                    let binding = row.source.policies.iter().find(|p| p.policy_version_id == control.policy_version_id);
                    let report_only = binding.is_some_and(|b| b.report_only);
                    let executable = binding.is_none_or(|b| b.executable);
                    // The existing drawer callback focuses the registry by name.
                    // Keep the captured version visible; navigation does not
                    // claim to open an immutable historical version.
                    let policy_name = control.name.clone();
                    rsx! {
                        div { key: "{id}", class: "pm-captured-detail",
                            span { class, "{label}" }
                            button { class: "btn btn-ghost xs focus-ring mono", title: "Find this policy name in the registry", onclick: move |_| on_open_policy.call(policy_name.clone()), "{control.name} · {control.version}" }
                            if report_only { span { "Report only" } }
                            if !executable { span { "Implementation skipped — evidence unavailable" } }
                            if outcome == Outcome::Unknown { span { "No trustworthy matching-attempt outcome is available." } }
                            if let Some(detail) = row.details.get(&id) {
                                p { class: "mono", style: "flex-basis: 100%; white-space: pre-wrap; overflow-wrap: anywhere; margin: 0;", "{detail}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Preserves the global CF-agent compatibility check outside bundle counts.
///
/// The flat API keys this evidence by evaluator configuration. It does not yet
/// provide structured platform-check identity or captured bundle membership.
#[component]
pub(super) fn PlatformEvidence(data: EvalPolicyMatrixResponse) -> Element {
    let column = data.policies.iter().position(|name| name == "CF agent");
    rsx! {
        details { class: "pm-platform-evidence",
            summary { "Platform evidence · global CF agent" }
            p { "Per-configuration compatibility evidence. This global check is separate from captured bundle/control totals." }
            if let Some(index) = column {
                if data.systems.is_empty() { p { "No platform evidence is available yet." } }
                for row in &data.systems {
                    {
                        let result = row.results.get(index).map(String::as_str).unwrap_or("legacy_unknown");
                        let class = format!("pm-{}", policy_cell_class_suffix(result));
                        let detail = row.details.get(index).and_then(|d| d.as_deref());
                        rsx! {
                            div { class: "pm-platform-result",
                                code { "{row.system_name}" }
                                span { class, "{policy_cell_glyph(result)} {result}" }
                                if let Some(detail) = detail { span { "{detail}" } }
                            }
                        }
                    }
                }
            } else { p { "Global CF-agent evidence is unavailable in this response." } }
        }
    }
}
