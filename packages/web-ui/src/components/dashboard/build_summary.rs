//! Build summary panel — design-reference parity (TASK-342.2).
//!
//! Layout: big building-count hero in #60a5fa + "building" label, then a
//! 2-col `dash-w-mini` grid for Queued and total active counts.

use dioxus::prelude::*;

use crate::api::models::BuildQueueSummary;

/// Build summary panel using the canonical dash-w-body / dash-w-mini layout.
#[component]
pub fn BuildSummaryPanel(
    queue: BuildQueueSummary,
    #[props(default)] flake_filter: Option<String>,
) -> Element {
    let building = queue.building_count;
    let queued = queue.queued_count;
    let _ = flake_filter;

    rsx! {
        div {
            class: "dash-w-body",
            "data-testid": "build-summary-panel",

            // Hero: building count
            div {
                style: "display:flex; align-items:baseline; gap:10px;",
                span {
                    style: "font-size:32px; font-weight:700; color:#60a5fa; line-height:1; font-variant-numeric:tabular-nums;",
                    "{building}"
                }
                span {
                    style: "font-size:12px; color:var(--cf-text-muted);",
                    "building"
                }
            }

            // 2-col mini grid: Queued + Total
            div {
                style: "display:grid; grid-template-columns:1fr 1fr; gap:6px; font-size:11px;",
                div {
                    class: "dash-w-mini",
                    span { "Queued" }
                    strong { "{queued}" }
                }
                div {
                    class: "dash-w-mini",
                    span { "Active" }
                    strong { "{building + queued}" }
                }
            }
        }
    }
}
