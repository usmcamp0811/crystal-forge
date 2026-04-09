//! CVE (Common Vulnerabilities and Exposures) display components.
//!
//! Components for displaying vulnerability information with severity
//! classification and expandable details.

use dioxus::prelude::*;

use crate::api::models::{CveSeverity, CveSummary, SystemVulnerability};
use crate::theme;

/// CVE tab showing vulnerability summary and severity breakdown.
#[component]
pub fn CvesTab(cve_counts: CveSummary, vulnerabilities: Vec<SystemVulnerability>) -> Element {
    let mut expanded_severity: Signal<Option<CveSeverity>> = use_signal(|| None);

    let total = cve_counts.total();

    rsx! {
        div {
            class: "pt-6 space-y-6",

            // Summary header
            div {
                class: "flex items-baseline gap-3",
                span {
                    class: "text-3xl font-bold text-white",
                    "{total}"
                }
                span {
                    class: "{theme::text::SECONDARY}",
                    "known vulnerabilities"
                }
            }

            // Severity breakdown - clickable to expand
            div {
                class: "space-y-3",
                CveSeverityRow {
                    severity: CveSeverity::Critical,
                    count: cve_counts.critical,
                    vulnerabilities: vulnerabilities.clone(),
                    expanded: *expanded_severity.read() == Some(CveSeverity::Critical),
                    on_toggle: move |_| {
                        let current = *expanded_severity.read();
                        if current == Some(CveSeverity::Critical) {
                            expanded_severity.set(None);
                        } else {
                            expanded_severity.set(Some(CveSeverity::Critical));
                        }
                    }
                }
                CveSeverityRow {
                    severity: CveSeverity::High,
                    count: cve_counts.high,
                    vulnerabilities: vulnerabilities.clone(),
                    expanded: *expanded_severity.read() == Some(CveSeverity::High),
                    on_toggle: move |_| {
                        let current = *expanded_severity.read();
                        if current == Some(CveSeverity::High) {
                            expanded_severity.set(None);
                        } else {
                            expanded_severity.set(Some(CveSeverity::High));
                        }
                    }
                }
                CveSeverityRow {
                    severity: CveSeverity::Medium,
                    count: cve_counts.medium,
                    vulnerabilities: vulnerabilities.clone(),
                    expanded: *expanded_severity.read() == Some(CveSeverity::Medium),
                    on_toggle: move |_| {
                        let current = *expanded_severity.read();
                        if current == Some(CveSeverity::Medium) {
                            expanded_severity.set(None);
                        } else {
                            expanded_severity.set(Some(CveSeverity::Medium));
                        }
                    }
                }
                CveSeverityRow {
                    severity: CveSeverity::Low,
                    count: cve_counts.low,
                    vulnerabilities: vulnerabilities.clone(),
                    expanded: *expanded_severity.read() == Some(CveSeverity::Low),
                    on_toggle: move |_| {
                        let current = *expanded_severity.read();
                        if current == Some(CveSeverity::Low) {
                            expanded_severity.set(None);
                        } else {
                            expanded_severity.set(Some(CveSeverity::Low));
                        }
                    }
                }
            }
        }
    }
}

/// Expandable row for a severity level showing count and vulnerability list.
#[component]
pub fn CveSeverityRow(
    severity: CveSeverity,
    count: i64,
    vulnerabilities: Vec<SystemVulnerability>,
    expanded: bool,
    on_toggle: EventHandler<()>,
) -> Element {
    let filtered_vulns: Vec<_> = vulnerabilities
        .iter()
        .filter(|v| v.severity == severity)
        .collect();

    let has_vulns = !filtered_vulns.is_empty();

    let severity_dot_color = match severity {
        CveSeverity::Critical => "bg-red-500",
        CveSeverity::High => "bg-orange-500",
        CveSeverity::Medium => "bg-yellow-500",
        CveSeverity::Low => "bg-blue-500",
    };
    let severity_text_color = severity.color_class();
    let chevron_class = if expanded { "rotate-180" } else { "" };
    let button_bg = if expanded { "bg-gray-800/30" } else { "" };

    rsx! {
        div {
            class: "rounded-lg border {theme::surface::CARD_BORDER} overflow-hidden",

            // Header row (clickable)
            button {
                class: "w-full flex items-center justify-between p-4 text-left transition-colors hover:bg-gray-800/50 {button_bg}",
                disabled: !has_vulns,
                onclick: move |_| on_toggle.call(()),

                div {
                    class: "flex items-center gap-3",
                    // Severity indicator
                    span {
                        class: "w-3 h-3 rounded-full {severity_dot_color}",
                    }
                    span {
                        class: "font-medium {severity_text_color}",
                        "{severity.label()}"
                    }
                }

                div {
                    class: "flex items-center gap-3",
                    span {
                        class: "text-xl font-bold {severity_text_color}",
                        "{count}"
                    }
                    if has_vulns {
                        svg {
                            class: "w-4 h-4 text-gray-500 transition-transform {chevron_class}",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M19 9l-7 7-7-7"
                            }
                        }
                    }
                }
            }

            // Expanded content
            if expanded && has_vulns {
                div {
                    class: "border-t {theme::surface::CARD_BORDER} divide-y divide-gray-800",
                    for vuln in filtered_vulns.iter() {
                        VulnerabilityRow { vuln: (*vuln).clone() }
                    }
                }
            }
        }
    }
}

/// Individual vulnerability row showing CVE details.
#[component]
pub fn VulnerabilityRow(vuln: SystemVulnerability) -> Element {
    let severity_color = vuln.severity.color_class();

    rsx! {
        div {
            class: "p-4 hover:bg-gray-800/30 transition-colors",

            div {
                class: "flex items-start justify-between gap-4",
                div {
                    class: "flex-1 min-w-0",

                    // CVE ID and package
                    div {
                        class: "flex items-center gap-2 flex-wrap",
                        span {
                            class: "font-mono text-sm font-medium text-white",
                            "{vuln.cve_id}"
                        }
                        span {
                            class: "text-xs px-2 py-0.5 rounded bg-gray-700 text-gray-300",
                            "{vuln.package_name}"
                        }
                    }

                    // Description
                    p {
                        class: "text-sm {theme::text::SECONDARY} mt-1 line-clamp-2",
                        "{vuln.description}"
                    }

                    // Version info
                    div {
                        class: "flex items-center gap-4 mt-2 text-xs {theme::text::MUTED}",
                        span { "Installed: {vuln.installed_version}" }
                        if let Some(ref fixed) = vuln.fixed_version {
                            span {
                                class: "text-emerald-400",
                                "Fixed in: {fixed}"
                            }
                        }
                        if let Some(ref status) = vuln.status {
                            span { "Status: {status}" }
                        }
                        if let Some(first_seen) = vuln.first_seen {
                            span { "First seen: {first_seen.format(\"%Y-%m-%d\")}" }
                        }
                    }
                }

                // CVSS score
                if let Some(score) = vuln.cvss_score {
                    div {
                        class: "shrink-0 text-right",
                        div {
                            class: "text-lg font-bold {severity_color}",
                            "{score:.1}"
                        }
                        div {
                            class: "text-xs {theme::text::MUTED}",
                            "CVSS"
                        }
                    }
                }
            }
        }
    }
}
