//! Renders captured evidence without inventing missing result details.

use super::policy_adapter::{BundleApplicability, MatrixRow};
use super::policy_matrix::{Outcome, Summary};
use crate::api::models::eval_policy_matrix::{
    EvalBundleColumn, EvalBundleSource, EvalConfigurationFailureKind, EvalPolicyColumn,
    EvalPolicyMatrixSnapshot,
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
            aria_label: "{summary.failed} fail, {summary.warnings} warning, {summary.passed} pass, {summary.unknown} unknown; {summary.applicable} known or unresolved system/control slots",
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
    // Cell text stays compact for matrix density; the title carries the full
    // state so unknown applicability is never shown as a clean or empty cell.
    let title = match (applicability, resolved) {
        (BundleApplicability::NotAssigned, _) => "Not assigned to this system".to_owned(),
        (BundleApplicability::Unknown, false) => {
            "Bundle metadata unknown; applicability unresolved".to_owned()
        }
        (BundleApplicability::Unknown, true) => format!(
            "Bundle metadata unknown; {}/{} known controls pass",
            summary.passed, summary.applicable
        ),
        (_, false) => "Applicability unresolved".to_owned(),
        _ if summary.applicable == 0 => "Assigned bundle with no eligible controls".to_owned(),
        _ => format!(
            "{}/{} applicable controls pass",
            summary.passed, summary.applicable
        ),
    };
    rsx! {
        span { class, title: "{title}",
            if applicability == BundleApplicability::NotAssigned { strong { "N/A" } }
            else if applicability == BundleApplicability::Unknown {
                if resolved && summary.applicable > 0 { strong { "{summary.passed}/{summary.applicable}" } small { class: "pm-bundle-unknown", "?" } }
                else { strong { "Unknown" } }
            }
            else if !resolved { strong { "Unknown" } }
            else if summary.applicable == 0 { strong { "N/A" } }
            else { strong { "{summary.passed}/{summary.applicable}" } }
            if summary.failed > 0 { small { class: "pm-bundle-fail", title: "{summary.failed} failed controls", "{summary.failed}" } }
            if summary.warnings > 0 { small { class: "pm-bundle-warn", title: "{summary.warnings} warning controls", "{summary.warnings}" } }
            if summary.unknown > 0 { small { class: "pm-bundle-unknown", title: "{summary.unknown} unavailable control results", "?{summary.unknown}" } }
        }
    }
}

/// Orders expanded-row cards: failures, warnings, unknowns, passes, then N/A.
pub(super) fn card_rank(outcome: Outcome) -> u8 {
    match outcome {
        Outcome::Fail => 0,
        Outcome::Warning => 1,
        Outcome::Unknown => 2,
        Outcome::Pass => 3,
        Outcome::NotAssigned => 4,
    }
}

/// Returns true for outcomes shown before the optional passed/N/A section.
pub(super) fn needs_attention(outcome: Outcome) -> bool {
    matches!(outcome, Outcome::Fail | Outcome::Warning | Outcome::Unknown)
}

/// Returns the one-line card summary from captured data and outcome semantics.
///
/// A captured description takes precedence. Without one, the summary states
/// only the projected outcome meaning; it never invents assertion text.
pub(super) fn card_summary(
    outcome: Outcome,
    report_only: bool,
    executable: bool,
    description: Option<&str>,
) -> String {
    if !executable {
        return "Implementation skipped".into();
    }
    if let Some(description) = description.filter(|d| !d.trim().is_empty()) {
        return description.to_owned();
    }
    match outcome {
        Outcome::Fail => "Blocking failure (enforce mode)".into(),
        Outcome::Warning if report_only => "Report-only failure — not blocking".into(),
        Outcome::Warning => "Warning — not blocking".into(),
        Outcome::Unknown => "Unknown".into(),
        Outcome::Pass => "Passed".into(),
        Outcome::NotAssigned => "Not assigned to this system".into(),
    }
}

#[cfg(test)]
mod card_tests {
    use super::*;

    #[test]
    fn cards_rank_failures_first_and_keep_passes_secondary() {
        let mut outcomes = [
            Outcome::Pass,
            Outcome::NotAssigned,
            Outcome::Unknown,
            Outcome::Warning,
            Outcome::Fail,
        ];
        outcomes.sort_by_key(|o| card_rank(*o));
        assert_eq!(
            outcomes,
            [
                Outcome::Fail,
                Outcome::Warning,
                Outcome::Unknown,
                Outcome::Pass,
                Outcome::NotAssigned
            ]
        );
        assert!(needs_attention(Outcome::Unknown));
        assert!(!needs_attention(Outcome::Pass));
        assert!(!needs_attention(Outcome::NotAssigned));
    }

    #[test]
    fn card_summary_uses_captured_text_and_never_invents_assertions() {
        assert_eq!(
            card_summary(Outcome::Fail, false, true, Some("Root login disabled")),
            "Root login disabled"
        );
        assert_eq!(
            card_summary(Outcome::Fail, false, true, None),
            "Blocking failure (enforce mode)"
        );
        assert_eq!(
            card_summary(Outcome::Warning, true, true, Some("  ")),
            "Report-only failure — not blocking"
        );
        assert_eq!(
            card_summary(Outcome::Fail, false, false, Some("ignored")),
            "Implementation skipped"
        );
    }

    #[test]
    fn only_unknown_cards_without_saved_evidence_need_an_unavailable_state() {
        for outcome in [
            Outcome::Pass,
            Outcome::Warning,
            Outcome::Fail,
            Outcome::NotAssigned,
        ] {
            assert!(!evidence_unavailable(outcome, None, None, None));
        }
        assert!(evidence_unavailable(Outcome::Unknown, None, None, None));
        assert!(evidence_unavailable(
            Outcome::Unknown,
            Some(" "),
            None,
            None
        ));
        for evidence in [
            (Some("saved assertion"), None, None),
            (None, Some("config.security.audit"), None),
            (None, None, Some("saved cause")),
        ] {
            assert!(!evidence_unavailable(
                Outcome::Unknown,
                evidence.0,
                evidence.1,
                evidence.2
            ));
        }
    }
}

/// Renders actual captured control outcomes for an expanded planned system.
///
/// Failures, warnings and unknown outcomes appear first as collapsible cards.
/// Passed and not-assigned controls stay behind an explicit toggle. Assertion,
/// path, and detail text is displayed only when saved. Unknown cards
/// without any saved evidence show one compact unavailable state.
#[component]
pub(super) fn RowDetails(
    row: MatrixRow,
    controls: Vec<EvalPolicyColumn>,
    bundles: Vec<EvalBundleColumn>,
    on_open_policy: EventHandler<String>,
    snapshot: EvalPolicyMatrixSnapshot,
    mut finding: Signal<Option<String>>,
) -> Element {
    let outcome_of = |control: &EvalPolicyColumn| {
        row.outcomes
            .get(&control.policy_version_id.to_string())
            .copied()
            .unwrap_or(Outcome::Unknown)
    };
    // Restoring an expanded passed card must reveal the secondary section.
    let restoring_passed = controls.iter().any(|control| {
        finding.peek().as_ref() == Some(&format!("{}:{}", row.key, control.policy_version_id))
            && !needs_attention(outcome_of(control))
    });
    let mut show_passed = use_signal(|| restoring_passed);
    let mut controls = controls;
    controls.sort_by_key(|control| card_rank(outcome_of(control)));
    let attention = controls
        .iter()
        .filter(|control| needs_attention(outcome_of(control)))
        .count();
    let secondary = controls.len() - attention;
    let bindings: std::collections::HashMap<_, _> = row
        .source
        .policies
        .iter()
        .map(|binding| (binding.policy_version_id, binding))
        .collect();
    let mut seen = std::collections::HashSet::new();
    let contexts: Vec<_> = row
        .source
        .assigned_bundles
        .iter()
        .filter(|context| seen.insert(context.bundle_version_id))
        .map(|context| {
            let source = match context.source {
                EvalBundleSource::Assignment => "assignment",
                EvalBundleSource::VirtualBaseline => "virtual baseline",
            };
            match bundles
                .iter()
                .find(|b| b.bundle_version_id == context.bundle_version_id)
            {
                Some(bundle) => format!("{} {} ({source})", bundle.name, bundle.version),
                None => format!(
                    "{} (display metadata unavailable, {source})",
                    context.bundle_version_id
                ),
            }
        })
        .collect();
    let config = row.source.configuration_name.clone();
    rsx! {
        div { class: "pm-expand",
            if let Some(status) = row.status_label() {
                p { class: "pm-failcard-unavailable", "{status}" }
            }
            for failure in snapshot.configuration_failures.iter().filter(|failure| row.configuration_failed && failure.configuration_name == config) {
                div { class: "pm-failcard-wrap",
                    div { class: "pm-failcard pm-failcard-fail",
                        span { class: "pm-failcard-glyph pm-fail", "✗" }
                        div { class: "pm-failcard-body",
                            div { class: "pm-failcard-title", match failure.kind {
                                EvalConfigurationFailureKind::CfAgent => "Configuration requirement · Crystal Forge agent",
                                EvalConfigurationFailureKind::Requirements => "Configuration policy requirements",
                            } }
                            if !failure.detail.is_empty() { p { class: "mono pm-failcard-evidence", "{failure.detail}" } }
                        }
                    }
                }
            }
            div { class: "pm-failcards",
                for control in controls.iter().filter(|control| show_passed() || needs_attention(outcome_of(control))) {
                    {
                        let id = control.policy_version_id.to_string();
                        let outcome = outcome_of(control);
                        let (label, class) = outcome_label(outcome);
                        let card_class = format!("pm-failcard pm-failcard-{}", class.trim_start_matches("pm-"));
                        let binding = bindings.get(&control.policy_version_id).copied();
                        let report_only = binding.is_some_and(|b| b.report_only);
                        let executable = binding.is_none_or(|b| b.executable);
                        let summary = card_summary(outcome, report_only, executable, control.description.as_deref());
                        // The existing drawer callback focuses the registry by name.
                        // It does not claim to open the immutable captured version.
                        let policy_name = control.name.clone();
                        let card_key = format!("{}:{id}", row.key);
                        let open = finding.read().as_ref() == Some(&card_key);
                        let detail = row.details.get(&id).filter(|text| !text.trim().is_empty()).cloned();
                        let mut seen_memberships = std::collections::HashSet::new();
                        let contributions: Vec<_> = binding.into_iter().flat_map(|b| &b.memberships).filter(|m| seen_memberships.insert(m.bundle_version_id)).filter_map(|m| bundles.iter().find(|b| b.bundle_version_id == m.bundle_version_id)).cloned().collect();
                        rsx! {
                            div { key: "{id}", class: "pm-failcard-wrap",
                                button { class: card_class, aria_expanded: open,
                                    aria_label: "{control.name} · {control.version}",
                                    title: "{label}: show captured evidence for this check",
                                    onclick: move |event: MouseEvent| { event.stop_propagation(); finding.set(if open { None } else { Some(card_key.clone()) }); },
                                    span { class: "pm-failcard-glyph {class}",
                                        match outcome { Outcome::Pass => "✓", Outcome::Warning => "!", Outcome::Fail => "✗", Outcome::Unknown => "?", Outcome::NotAssigned => "–" }
                                    }
                                    span { class: "pm-failcard-body",
                                        span { class: "mono pm-failcard-title", style: "display:block", "{control.name} · {control.version}" }
                                        span { class: "pm-failcard-sub", style: "display:block", "{summary}" }
                                    }
                                    span { class: "pm-failcard-chevron", if open { "⌄" } else { "›" } }
                                }
                                if open {
                                    div { class: "pm-failcard-detail",
                                        if let Some(path) = control.attribute_path.as_deref().filter(|text| !text.trim().is_empty()) {
                                            p { class: "mono pm-failcard-path", "nixosConfigurations.{config}.{path}" }
                                        }
                                        if let Some(assertion) = control.assertion.as_deref().filter(|text| !text.trim().is_empty()) {
                                            p { class: "pm-failcard-assertion", span { class: "mono", style: "font-weight:600", "assertion: " } "{assertion}" }
                                        }
                                        if let Some(detail) = &detail {
                                            p { class: "mono pm-failcard-evidence", "{detail}" }
                                        }
                                        if evidence_unavailable(outcome, control.assertion.as_deref(), control.attribute_path.as_deref(), detail.as_deref()) {
                                            p { class: "pm-failcard-unavailable", "Evidence unavailable" }
                                        }
                                        p { class: "pm-failcard-meta",
                                            {format!("{label} · {} · policy version {} · lineage {}", if report_only { "report-only" } else { "enforce" }, control.policy_version_id, control.policy_id)}
                                        }
                                        if !control.identifiers.is_empty() {
                                            p { class: "pm-failcard-meta", {format!("Captured identifiers: {}", control.identifiers.join(", "))} }
                                        }
                                        div { class: "pm-failcard-actions",
                                            button { class: "btn btn-ghost xs focus-ring", title: "Find this policy name in the current registry; the captured version above is authoritative for this evaluation.", onclick: move |_| on_open_policy.call(policy_name.clone()), "View policy definition" }
                                            for bundle in contributions {
                                                super::evidence_navigation::OpenEvidence { snapshot: snapshot.clone(), system_id: row.source.system_id, policy: control.clone(), bundle }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if attention == 0 && row.resolved() && row.common_summary.applicable > 0 {
                    div { class: "pm-all-pass", "✓ All applicable controls pass for this system." }
                } else if row.resolved() && row.common_summary.applicable == 0 {
                    p { class: "pm-failcard-unavailable", "No applicable controls for this system." }
                }
            }
            if secondary > 0 {
                div { class: "pm-expand-actions",
                    button { class: "btn btn-ghost xs focus-ring", aria_expanded: show_passed(), onclick: move |_| show_passed.toggle(),
                        if show_passed() { "Hide passed and N/A controls" } else { "Show {secondary} passed and N/A controls" }
                    }
                }
            }
            div { class: "pm-expand-meta",
                span { "Configuration: ", code { "{row.source.configuration_name}" } }
                span { "Operator classification: ", match row.source.greenfield { Some(true) => "Greenfield", Some(false) => "Established", None => "Unknown" } }
                if !row.bundle_metadata_complete {
                    span { "Bundle context metadata is incomplete; missing contexts do not prove nonassignment." }
                } else if contexts.is_empty() {
                    span { "Assigned bundles: none captured" }
                } else {
                    span { {format!("Assigned bundles: {}", contexts.join(" · "))} }
                }
            }
            div { class: "pm-expand-actions",
                super::evidence_navigation::OpenSystem { snapshot: snapshot.clone(), system_id: row.source.system_id }
            }
        }
    }
}

/// Returns whether an unknown outcome has no saved diagnostic evidence.
fn evidence_unavailable(
    outcome: Outcome,
    assertion: Option<&str>,
    path: Option<&str>,
    detail: Option<&str>,
) -> bool {
    outcome == Outcome::Unknown
        && [assertion, path, detail]
            .into_iter()
            .all(|text| text.is_none_or(|text| text.trim().is_empty()))
}
