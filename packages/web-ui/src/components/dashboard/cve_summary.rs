//! CVE summary panel — design-reference parity (TASK-342.2).
//!
//! Layout: big critical count hero + "critical CVEs" label, then a 2-col
//! `dash-w-mini` grid showing High and total CVE count.

use dioxus::prelude::*;

use crate::api::models::CveSummary;

/// CVE summary panel using the canonical dash-w-body / dash-w-mini layout.
#[component]
pub fn CveSummaryPanel(
    cves: CveSummary,
    #[props(default)] flake_filter: Option<String>,
) -> Element {
    let display_cves = cves.clone();
    let filter_note = flake_filter
        .as_ref()
        .map(|flake| format!("{flake} filter active · CVE summary remains fleet-wide"));

    let total = display_cves.total();

    rsx! {
        div {
            class: "dash-w-body",
            "data-testid": "cve-summary",

            if let Some(note) = filter_note {
                div {
                    style: "font-size:11px; color:var(--cf-text-muted);",
                    "{note}"
                }
            }

            // Hero: critical count
            div {
                style: "display:flex; align-items:baseline; gap:10px;",
                span {
                    style: "font-size:32px; font-weight:700; color:#f87171; line-height:1; font-variant-numeric:tabular-nums;",
                    "{display_cves.critical}"
                }
                span {
                    style: "font-size:12px; color:var(--cf-text-muted);",
                    "critical CVEs"
                }
            }

            // 2-col mini grid: High + Total
            div {
                style: "display:grid; grid-template-columns:1fr 1fr; gap:6px; font-size:11px;",
                div {
                    class: "dash-w-mini",
                    span { "High" }
                    strong { style: "color:#fbbf24;", "{display_cves.high}" }
                }
                div {
                    class: "dash-w-mini",
                    span { "Total" }
                    strong { "{total}" }
                }
            }
        }
    }
}
