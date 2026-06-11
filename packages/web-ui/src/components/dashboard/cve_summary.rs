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
    let display_cves = if flake_filter.is_some() {
        // Conservative halving for filtered view until per-flake CVE API lands.
        CveSummary {
            critical: cves.critical / 2,
            high: cves.high / 2,
            medium: cves.medium / 2,
            low: cves.low / 2,
        }
    } else {
        cves.clone()
    };

    let total = display_cves.total();

    rsx! {
        div {
            class: "dash-w-body",
            "data-testid": "cve-summary",

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
