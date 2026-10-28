//! Compact figures drawn inside a walkthrough card.
//!
//! Content is plain data so tests can check the teaching points. Copy follows
//! `CoachFigure` in `docs/design/CrystalForge/components/CoachTours.jsx`.

use dioxus::prelude::*;

use super::tours::Figure;

/// One run of text in a list item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Span {
    /// Ordinary text.
    Text(&'static str),
    /// Emphasized text.
    Bold(&'static str),
}

/// Marker color of a list item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Neutral marker.
    Plain,
    /// Red marker for a result that is not a pass.
    Bad,
    /// Green marker for the only passing path.
    Good,
}

/// One labelled row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    /// Marker color as a CSS color, when the row has one.
    pub color: Option<&'static str>,
    /// Row label.
    pub label: &'static str,
    /// Row description.
    pub detail: Option<&'static str>,
    /// Small trailing tag.
    pub tag: Option<&'static str>,
}

const fn row(label: &'static str, detail: &'static str) -> Row {
    Row {
        color: None,
        label,
        detail: Some(detail),
        tag: None,
    }
}

const fn dot(
    color: &'static str,
    label: &'static str,
    detail: &'static str,
    tag: Option<&'static str>,
) -> Row {
    Row {
        color: Some(color),
        label,
        detail: Some(detail),
        tag,
    }
}

/// Figure content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FigureData {
    /// Stacked labelled rows.
    Rows(&'static [Row]),
    /// Bulleted list with optional emphasis and tone.
    List(&'static [(Tone, &'static [Span])]),
    /// Side-by-side cards of a heading and a description.
    Two(&'static [(&'static str, &'static str)]),
    /// A three-column table with a note.
    Table {
        /// Column headings.
        head: [&'static str; 3],
        /// Table rows.
        rows: &'static [[&'static str; 3]],
        /// Note below the table.
        note: &'static str,
    },
    /// A lifecycle flow.
    Flow {
        /// Ordinary lifecycle states.
        steps: &'static [&'static str],
        /// The verification gate.
        gate: &'static str,
        /// The closing state.
        done: &'static str,
    },
}

use Span::{Bold, Text};

const SCHEDULE: &[Row] = &[
    row(
        "Scan on build",
        "Scan a freshly built exact configuration before deployment.",
    ),
    row(
        "Deployed configs",
        "Rescan currently running configurations.",
    ),
    row(
        "Recent configs",
        "Rescan recent configurations that are not deployed.",
    ),
    row(
        "Superseded configs",
        "Reduce work for superseded configurations.",
    ),
    row(
        "Rebuild to scan old configs",
        "Permit policy-driven rebuilds when an archived closure is unavailable.",
    ),
];

const RELATIONS: &[Row] = &[
    dot(
        "#34d399",
        "Current",
        "Exact evidence for the running configuration.",
        Some("triage"),
    ),
    dot(
        "#60a5fa",
        "Scheduled deployment target",
        "Exact evidence for a scheduled target.",
        Some("read-only"),
    ),
    dot(
        "#9ca3af",
        "Historical",
        "Retained, no current or scheduled authority.",
        Some("read-only"),
    ),
];

const TABS: &[Row] = &[
    row(
        "CVEs",
        "Package vulnerabilities and source-authoritative CVE triage.",
    ),
    row(
        "Hardening",
        "Hardening and posture evidence for the selected system state.",
    ),
    row(
        "Compliance",
        "Policy and bundle assessment evidence, remediation findings.",
    ),
];

const DISPOSITIONS: &[Row] = &[
    dot(
        "#f87171",
        "Leave open",
        "No active accepted or scheduled decision for the environment.",
        None,
    ),
    dot(
        "#a78bfa",
        "Accept risk",
        "Needs a justification; review date optional. Tolerates the finding. Does not hide it, pass it, or claim remediation.",
        None,
    ),
    dot(
        "#60a5fa",
        "Schedule patch",
        "Creates or uses a POA&M. Needs Current exact evidence. Does not mean fixed or verified.",
        None,
    ),
];

const BATCH: &[(Tone, &[Span])] = &[
    (
        Tone::Plain,
        &[Text("Up to "), Bold("100 exact pairs"), Text(" per apply.")],
    ),
    (
        Tone::Plain,
        &[
            Text("Unavailable or inventory-only pairs stay listed and "),
            Bold("block Apply"),
            Text("."),
        ],
    ),
    (
        Tone::Plain,
        &[Text(
            "Accept risk: one rationale, individually auditable decisions.",
        )],
    ),
    (
        Tone::Plain,
        &[
            Text("Schedule patch: "),
            Bold("One POA&M"),
            Text(", "),
            Bold("One per package"),
            Text(" or "),
            Bold("One per environment"),
            Text(". A shared POA&M keeps each system/CVE/package finding separate."),
        ],
    ),
];

const ASSIGNMENT_ROWS: &[[&str; 3]] = &[
    ["production", "nixos-stig · v1r1", "Enforce"],
    ["staging", "nixos-stig · v1r2", "Enforce"],
    ["dev", "nixos-stig · v1r2", "Report only"],
];

const ENFORCE: &[(&str, &str)] = &[
    (
        "Enforce",
        "A failing applicable policy can participate in deployment blocking under the enforcement rules.",
    ),
    (
        "Report only",
        "The failure is still FAIL. It produces a finding, supports a waiver or POA&M, retains evidence. It does not block deployment.",
    ),
];

const QUEUES: &[(&str, &str)] = &[
    (
        "Plans",
        "Overdue · Due in 14 days · Awaiting verification · Blocked · No activity · Unassigned",
    ),
    (
        "Acceptances",
        "Expired · Review soon · No review · Accepted",
    ),
];

const VERIFY: &[(Tone, &[Span])] = &[
    (
        Tone::Plain,
        &[
            Text("Policy findings: the "),
            Bold("current assessment"),
            Text(" decides."),
        ],
    ),
    (
        Tone::Plain,
        &[
            Text("CVE findings: needs exact scan evidence "),
            Bold("newer than the finding's baseline"),
            Text("."),
        ],
    ),
    (Tone::Bad, &[Text("A current occurrence is FAIL.")]),
    (
        Tone::Bad,
        &[Text("Missing or inconsistent evidence is not PASS.")],
    ),
    (
        Tone::Bad,
        &[Text(
            "Operator justification or a scanner whitelist is not PASS.",
        )],
    ),
    (
        Tone::Good,
        &[Text(
            "Only exact absence in sufficiently new authoritative evidence closes a CVE finding.",
        )],
    ),
];

const EXPORTS: &[(&str, &str)] = &[
    ("Bundle XCCDF", "Baseline definition: rules and checks."),
    (
        "Evidence package",
        "Assessed results per host, with POA&Ms and acceptances.",
    ),
];

const IDENTITIES: &[Row] = &[
    row(
        "RA-0042",
        "Human, operator-facing acceptance lifecycle. Crystal Forge property in OSCAL.",
    ),
    row(
        "cve_risk_acceptance · 7f3c…e21a",
        "Typed source + UUID. Exact immutable decision. OSCAL source-id.",
    ),
];

/// Returns the content of `figure`.
pub const fn data(figure: Figure) -> FigureData {
    match figure {
        Figure::Schedule => FigureData::Rows(SCHEDULE),
        Figure::Relations => FigureData::Rows(RELATIONS),
        Figure::Tabs => FigureData::Rows(TABS),
        Figure::Dispositions => FigureData::Rows(DISPOSITIONS),
        Figure::Batch => FigureData::List(BATCH),
        Figure::Assignments => FigureData::Table {
            head: ["Scope", "Bundle version", "Mode"],
            rows: ASSIGNMENT_ROWS,
            note: "Illustrative. Read the real assignment on the environment or system.",
        },
        Figure::Enforce => FigureData::Two(ENFORCE),
        Figure::Queues => FigureData::Two(QUEUES),
        Figure::Lifecycle => FigureData::Flow {
            steps: &["Open", "In progress", "Blocked", "Awaiting verification"],
            gate: "Verify now",
            done: "Authoritative close",
        },
        Figure::Verify => FigureData::List(VERIFY),
        Figure::Exports => FigureData::Two(EXPORTS),
        Figure::Identities => FigureData::Rows(IDENTITIES),
    }
}

/// Returns every visible string of `figure`, joined by newlines.
#[cfg(test)]
pub fn plain_text(figure: Figure) -> String {
    let mut out: Vec<String> = Vec::new();
    match data(figure) {
        FigureData::Rows(rows) => {
            for row in rows {
                out.push(row.label.into());
                out.extend(row.detail.map(String::from));
                out.extend(row.tag.map(String::from));
            }
        }
        FigureData::List(items) => {
            for (_, spans) in items {
                out.push(
                    spans
                        .iter()
                        .map(|span| match span {
                            Span::Text(text) | Span::Bold(text) => *text,
                        })
                        .collect(),
                );
            }
        }
        FigureData::Two(cards) => {
            for (head, text) in cards {
                out.push((*head).into());
                out.push((*text).into());
            }
        }
        FigureData::Table { head, rows, note } => {
            out.extend(head.iter().map(|text| text.to_string()));
            out.extend(rows.iter().flatten().map(|text| text.to_string()));
            out.push(note.into());
        }
        FigureData::Flow { steps, gate, done } => {
            out.extend(steps.iter().map(|text| text.to_string()));
            out.push(gate.into());
            out.push(done.into());
        }
    }
    out.join("\n")
}

/// Renders a figure.
#[component]
pub fn CoachFigure(figure: Figure) -> Element {
    match data(figure) {
        FigureData::Rows(rows) => rsx! {
            div { class: "cf-fig", "data-testid": "coach-figure",
                for row in rows.iter() {
                    div { class: "cf-fig-row",
                        if let Some(color) = row.color {
                            span { class: "cf-fig-dot", style: "background:{color}" }
                        }
                        div { style: "min-width:0;flex:1",
                            b { "{row.label}" }
                            if let Some(detail) = row.detail { span { "{detail}" } }
                        }
                        if let Some(tag) = row.tag { em { "{tag}" } }
                    }
                }
            }
        },
        FigureData::List(items) => rsx! {
            ul { class: "cf-fig cf-fig-list", "data-testid": "coach-figure",
                for (tone, spans) in items.iter() {
                    li { class: match tone { Tone::Plain => "", Tone::Bad => "bad", Tone::Good => "good" },
                        for span in spans.iter() {
                            match span {
                                Span::Text(text) => rsx! { "{text}" },
                                Span::Bold(text) => rsx! { b { "{text}" } },
                            }
                        }
                    }
                }
            }
        },
        FigureData::Two(cards) => rsx! {
            div { class: "cf-fig cf-fig-two", "data-testid": "coach-figure",
                for (head, text) in cards.iter() {
                    div { b { "{head}" } span { "{text}" } }
                }
            }
        },
        FigureData::Table { head, rows, note } => rsx! {
            div { class: "cf-fig", "data-testid": "coach-figure",
                div { class: "cf-fig-table",
                    for text in head.iter() { span { "{text}" } }
                    for row in rows.iter() {
                        b { "{row[0]}" }
                        b { class: "mono", "{row[1]}" }
                        b { "{row[2]}" }
                    }
                }
                div { class: "cf-fig-note", "{note}" }
            }
        },
        FigureData::Flow { steps, gate, done } => rsx! {
            div { class: "cf-fig", "data-testid": "coach-figure",
                div { class: "cf-fig-flow",
                    for step in steps.iter() { span { "{step}" } }
                    span { class: "gate", "{gate}" }
                    span { class: "done", "{done}" }
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::onboarding::tours::SECURITY_MODULES;

    #[test]
    fn every_figure_a_stop_names_has_content() {
        for module in &SECURITY_MODULES {
            for stop in module.stops {
                if let Some(figure) = stop.figure {
                    assert!(
                        !plain_text(figure).is_empty(),
                        "{} figure is empty",
                        stop.id
                    );
                }
            }
        }
    }

    #[test]
    fn disposition_figure_keeps_acceptance_and_scheduling_honest() {
        let text = plain_text(Figure::Dispositions);
        assert!(text.contains("Does not hide it, pass it, or claim remediation"));
        assert!(text.contains("Does not mean fixed or verified"));
        assert!(text.contains("Needs Current exact evidence"));
    }

    #[test]
    fn enforce_figure_keeps_report_only_fail_and_non_blocking() {
        let text = plain_text(Figure::Enforce);
        assert!(text.contains("The failure is still FAIL"));
        assert!(text.contains("It does not block deployment"));
    }

    #[test]
    fn verify_figure_keeps_the_only_closing_path() {
        let text = plain_text(Figure::Verify);
        assert!(text.contains("Missing or inconsistent evidence is not PASS"));
        assert!(text.contains("Operator justification or a scanner whitelist is not PASS"));
        assert!(text.contains(
            "Only exact absence in sufficiently new authoritative evidence closes a CVE finding"
        ));
    }

    #[test]
    fn relations_figure_labels_only_current_evidence_as_triageable() {
        let FigureData::Rows(rows) = data(Figure::Relations) else {
            panic!("relations is a row figure");
        };
        assert_eq!(rows[0].tag, Some("triage"));
        assert!(rows[1..].iter().all(|row| row.tag == Some("read-only")));
    }

    #[test]
    fn identity_figure_separates_the_human_and_source_identities() {
        let text = plain_text(Figure::Identities);
        assert!(text.contains("RA-0042"));
        assert!(text.contains("Typed source + UUID"));
        assert!(text.contains("OSCAL source-id"));
    }

    #[test]
    fn assignment_figure_is_marked_illustrative() {
        assert!(plain_text(Figure::Assignments).contains("Illustrative"));
    }
}
