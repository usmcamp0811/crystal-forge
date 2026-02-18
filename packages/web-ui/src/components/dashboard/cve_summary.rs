//! CVE summary panel components.

use dioxus::prelude::*;

use crate::api::models::CveSummary;
use crate::theme;

/// CVE summary panel with severity badges.
#[component]
pub fn CveSummaryPanel(
    cves: CveSummary,
    #[props(default)] flake_filter: Option<String>,
) -> Element {
    // Apply filter - in real app, this would come from API
    let display_cves = if let Some(ref _flake_name) = flake_filter {
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
            class: "flex flex-col h-full",
            "data-testid": "cve-summary",

            // Show filter indicator if filtered
            if let Some(ref flake_name) = flake_filter {
                div {
                    class: "text-xs text-blue-400 mb-2 flex items-center gap-1 shrink-0",
                    svg {
                        class: "w-3 h-3",
                        fill: "none",
                        stroke: "currentColor",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            stroke_width: "2",
                            d: "M3 4a1 1 0 011-1h16a1 1 0 011 1v2.586a1 1 0 01-.293.707l-6.414 6.414a1 1 0 00-.293.707V17l-4 4v-6.586a1 1 0 00-.293-.707L3.293 7.293A1 1 0 013 6.586V4z"
                        }
                    }
                    span { "{flake_name}" }
                }
            }

            // Total count header
            div {
                class: "flex items-baseline gap-2 mb-3 shrink-0",
                span { class: "text-2xl font-bold text-white", "{total}" }
                span { class: "{theme::text::SECONDARY} text-sm", "vulnerabilities" }
            }

            // Severity breakdown - fills remaining space
            div {
                class: "grid grid-cols-2 gap-2 flex-1 min-h-0",
                CveSeverityBadge { label: "Critical", count: display_cves.critical, text_class: theme::cve::CRITICAL_TEXT, bg_class: theme::cve::CRITICAL_BG }
                CveSeverityBadge { label: "High", count: display_cves.high, text_class: theme::cve::HIGH_TEXT, bg_class: theme::cve::HIGH_BG }
                CveSeverityBadge { label: "Medium", count: display_cves.medium, text_class: theme::cve::MEDIUM_TEXT, bg_class: theme::cve::MEDIUM_BG }
                CveSeverityBadge { label: "Low", count: display_cves.low, text_class: theme::cve::LOW_TEXT, bg_class: theme::cve::LOW_BG }
            }
        }
    }
}

/// A single CVE severity badge with count.
#[component]
pub fn CveSeverityBadge(
    label: &'static str,
    count: i64,
    text_class: &'static str,
    bg_class: &'static str,
) -> Element {
    rsx! {
        div {
            class: "flex items-center justify-between px-3 py-2 rounded-lg {bg_class}",
            span { class: "{text_class} font-medium text-sm", "{label}" }
            span { class: "{text_class} text-lg font-bold", "{count}" }
        }
    }
}
