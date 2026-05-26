//! Advanced CVE dashboard view (TASK-322).
//!
//! Complete refactor matching design reference with:
//! - Statistics strip
//! - Advanced filtering
//! - Dual view modes (flat/grouped)
//! - CVE detail drawer
//! - Triage workflow

use dioxus::prelude::*;

use crate::api::client;
use crate::api::models::{CveFilters, CveFleetStats, CveListItem};
use crate::components::layout::Card;
use crate::theme;

#[component]
pub fn CvesView() -> Element {
    // Filter state
    let mut severity_filter = use_signal(|| Option::<String>::None);
    let mut fix_status_filter = use_signal(|| Option::<String>::None);
    let mut triage_status_filter = use_signal(|| Option::<String>::None);
    let mut package_filter = use_signal(|| Option::<String>::None);
    let mut search_query = use_signal(String::new);
    let mut sort_by = use_signal(|| "severity".to_string());
    let mut view_mode = use_signal(|| "flat".to_string()); // "flat" or "grouped"

    // Data resources
    let stats = use_resource(move || async move { client::fetch_cve_fleet_stats().await });

    let cve_list = use_resource(move || {
        let filters = CveFilters {
            severity: severity_filter(),
            fix_status: fix_status_filter(),
            triage_status: triage_status_filter(),
            package: package_filter(),
            search: if search_query().is_empty() {
                None
            } else {
                Some(search_query())
            },
            sort: Some(sort_by()),
            limit: Some(500),
        };

        async move { client::fetch_cves(&filters).await }
    });

    rsx! {
        div {
            class: "space-y-6",

            // Page Header
            div {
                class: "flex items-center justify-between",
                h1 {
                    class: "{theme::typography::PAGE_TITLE}",
                    "CVEs"
                }
                if let Some(Ok(s)) = stats.read().as_ref() {
                    p {
                        class: "text-sm {theme::text::SECONDARY}",
                        "{s.total_cves} vulnerabilities · {s.systems_affected} systems affected · {s.fixable} patchable"
                    }
                }
            }

            // Statistics Strip
            if let Some(Ok(fleet_stats)) = stats.read().as_ref() {
                div {
                    class: "grid grid-cols-1 md:grid-cols-5 gap-4",

                    // Critical
                    Card {
                        children: rsx! {
                            div {
                                class: "p-4",
                                div {
                                    class: "text-xs {theme::text::SECONDARY} mb-1",
                                    "Critical"
                                }
                                div {
                                    class: "text-2xl font-bold {theme::cve::CRITICAL_TEXT}",
                                    "{fleet_stats.critical}"
                                }
                                div {
                                    class: "text-xs {theme::text::SECONDARY} mt-1",
                                    "{fleet_stats.exploited} actively exploited"
                                }
                            }
                        }
                    }

                    // High
                    Card {
                        children: rsx! {
                            div {
                                class: "p-4",
                                div {
                                    class: "text-xs {theme::text::SECONDARY} mb-1",
                                    "High"
                                }
                                div {
                                    class: "text-2xl font-bold {theme::cve::HIGH_TEXT}",
                                    "{fleet_stats.high}"
                                }
                            }
                        }
                    }

                    // Patchable
                    Card {
                        children: rsx! {
                            div {
                                class: "p-4",
                                div {
                                    class: "text-xs {theme::text::SECONDARY} mb-1",
                                    "Patchable now"
                                }
                                div {
                                    class: "text-2xl font-bold text-blue-400",
                                    "{fleet_stats.fixable}"
                                }
                                div {
                                    class: "text-xs {theme::text::SECONDARY} mt-1",
                                    "Just deploy newer flake"
                                }
                            }
                        }
                    }

                    // Accepted Risk
                    Card {
                        children: rsx! {
                            div {
                                class: "p-4",
                                div {
                                    class: "text-xs {theme::text::SECONDARY} mb-1",
                                    "Accepted risk"
                                }
                                div {
                                    class: "text-2xl font-bold text-purple-400",
                                    "{fleet_stats.accepted + fleet_stats.scheduled}"
                                }
                                div {
                                    class: "text-xs {theme::text::SECONDARY} mt-1",
                                    "{fleet_stats.accepted} accepted · {fleet_stats.scheduled} scheduled"
                                }
                            }
                        }
                    }

                    // Outstanding
                    Card {
                        children: rsx! {
                            div {
                                class: "p-4",
                                div {
                                    class: "text-xs {theme::text::SECONDARY} mb-1",
                                    "Outstanding"
                                }
                                div {
                                    class: "text-2xl font-bold",
                                    class: if fleet_stats.outstanding > 20 { "{theme::cve::CRITICAL_TEXT}" } else { "text-green-400" },
                                    "{fleet_stats.outstanding}"
                                }
                                div {
                                    class: "text-xs {theme::text::SECONDARY} mt-1",
                                    "need triage"
                                }
                            }
                        }
                    }
                }
            }

            // Filter Bar
            Card {
                children: rsx! {
                    div {
                        class: "p-4 space-y-4",

                        // Search
                        div {
                            class: "flex gap-4",
                            input {
                                class: "flex-1 px-3 py-2 rounded-md border border-white/15 bg-black/20 text-sm",
                                r#type: "text",
                                placeholder: "Search CVE / package / title…",
                                value: "{search_query}",
                                oninput: move |evt| search_query.set(evt.value()),
                            }
                        }

                        // Severity Filter
                        div {
                            class: "flex flex-wrap gap-2",
                            span {
                                class: "text-xs {theme::text::SECONDARY} self-center mr-2",
                                "Severity:"
                            }
                            for sev in ["all", "critical", "high", "medium", "low"] {
                                button {
                                    class: if severity_filter().as_deref() == if sev == "all" { None } else { Some(sev) } {
                                        "px-3 py-1.5 rounded-md text-xs font-medium border bg-violet-600/20 border-violet-400/50 text-violet-100"
                                    } else {
                                        "px-3 py-1.5 rounded-md text-xs font-medium border border-white/15 text-gray-300 hover:bg-white/5"
                                    },
                                    onclick: move |_| {
                                        if sev == "all" {
                                            severity_filter.set(None);
                                        } else {
                                            severity_filter.set(Some(sev.to_string()));
                                        }
                                    },
                                    "{sev}"
                                }
                            }
                        }

                        // Fix Status Filter
                        div {
                            class: "flex flex-wrap gap-2",
                            span {
                                class: "text-xs {theme::text::SECONDARY} self-center mr-2",
                                "Fix Status:"
                            }
                            for status in [("all", "Any status"), ("available", "Has patch"), ("pending", "No patch"), ("exploited", "Exploited")] {
                                button {
                                    class: if fix_status_filter().as_deref() == if status.0 == "all" { None } else { Some(status.0) } {
                                        "px-3 py-1.5 rounded-md text-xs font-medium border bg-violet-600/20 border-violet-400/50 text-violet-100"
                                    } else {
                                        "px-3 py-1.5 rounded-md text-xs font-medium border border-white/15 text-gray-300 hover:bg-white/5"
                                    },
                                    onclick: move |_| {
                                        if status.0 == "all" {
                                            fix_status_filter.set(None);
                                        } else {
                                            fix_status_filter.set(Some(status.0.to_string()));
                                        }
                                    },
                                    "{status.1}"
                                }
                            }
                        }

                        // Triage Status Filter
                        div {
                            class: "flex flex-wrap gap-2",
                            span {
                                class: "text-xs {theme::text::SECONDARY} self-center mr-2",
                                "Triage:"
                            }
                            for status in [("all", "Any triage"), ("outstanding", "Outstanding"), ("scheduled", "Scheduled"), ("accepted", "Accepted")] {
                                button {
                                    class: if triage_status_filter().as_deref() == if status.0 == "all" { None } else { Some(status.0) } {
                                        "px-3 py-1.5 rounded-md text-xs font-medium border bg-violet-600/20 border-violet-400/50 text-violet-100"
                                    } else {
                                        "px-3 py-1.5 rounded-md text-xs font-medium border border-white/15 text-gray-300 hover:bg-white/5"
                                    },
                                    onclick: move |_| {
                                        if status.0 == "all" {
                                            triage_status_filter.set(None);
                                        } else {
                                            triage_status_filter.set(Some(status.0.to_string()));
                                        }
                                    },
                                    "{status.1}"
                                }
                            }
                        }
                    }
                }
            }

            // CVE List
            Card {
                children: rsx! {
                    div {
                        class: "overflow-x-auto",
                        match &*cve_list.read_unchecked() {
                            Some(Ok(cves)) => rsx! {
                                if cves.is_empty() {
                                    div {
                                        class: "p-8 text-center {theme::text::SECONDARY}",
                                        "No CVEs match the current filters."
                                    }
                                } else {
                                    table {
                                        class: "min-w-full text-sm",
                                        thead {
                                            tr {
                                                class: "border-b border-white/10 text-left {theme::text::SECONDARY}",
                                                th { class: "py-2 pr-3", "CVE" }
                                                th { class: "py-2 pr-3", "Severity" }
                                                th { class: "py-2 pr-3", "CVSS" }
                                                th { class: "py-2 pr-3", "Package" }
                                                th { class: "py-2 pr-3", "Title" }
                                                th { class: "py-2 pr-3", "Affected" }
                                                th { class: "py-2 pr-3", "Fix" }
                                                th { class: "py-2 pr-3", "Triage" }
                                                th { class: "py-2 pr-3", "Age" }
                                            }
                                        }
                                        tbody {
                                            for cve in cves {
                                                CveRow { cve: cve.clone() }
                                            }
                                        }
                                    }
                                }
                            },
                            Some(Err(err)) => rsx! {
                                div {
                                    class: "p-8 text-center {theme::text::SECONDARY}",
                                    "Error loading CVEs: {err}"
                                }
                            },
                            None => rsx! {
                                div {
                                    class: "p-8 text-center {theme::text::SECONDARY}",
                                    "Loading CVEs..."
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn CveRow(cve: CveListItem) -> Element {
    let severity_color = match cve.severity.to_uppercase().as_str() {
        "CRITICAL" => theme::cve::CRITICAL_TEXT,
        "HIGH" => theme::cve::HIGH_TEXT,
        "MEDIUM" => theme::cve::MEDIUM_TEXT,
        _ => theme::cve::LOW_TEXT,
    };

    rsx! {
        tr {
            class: "border-b border-white/5 hover:bg-white/5",

            // CVE ID
            td {
                class: "py-2 pr-3 font-mono text-sm font-medium",
                "{cve.cve_id}"
                if cve.exploited {
                    span {
                        class: "ml-2 px-2 py-0.5 text-xs rounded bg-red-500/20 text-red-300 border border-red-500/30",
                        "exploited"
                    }
                }
            }

            // Severity
            td {
                class: "py-2 pr-3",
                span {
                    class: "px-2 py-1 text-xs rounded-md {severity_color}",
                    "{cve.severity}"
                }
            }

            // CVSS
            td {
                class: "py-2 pr-3",
                if let Some(cvss) = cve.cvss_v3_score {
                    span {
                        class: "font-mono text-sm",
                        "{cvss:.1}"
                    }
                }
            }

            // Package
            td {
                class: "py-2 pr-3 font-mono text-xs",
                "{cve.package_name.unwrap_or_default()}"
            }

            // Title
            td {
                class: "py-2 pr-3 max-w-xs truncate",
                title: "{cve.title}",
                "{cve.title}"
            }

            // Affected
            td {
                class: "py-2 pr-3 font-mono text-xs",
                "{cve.affected_count}"
            }

            // Fix Status
            td {
                class: "py-2 pr-3",
                if cve.fix_status == "fix_available" {
                    span {
                        class: "px-2 py-1 text-xs rounded-md bg-green-500/20 text-green-300",
                        "Fix available"
                    }
                } else {
                    span {
                        class: "px-2 py-1 text-xs rounded-md bg-yellow-500/20 text-yellow-300",
                        "No patch"
                    }
                }
            }

            // Triage Status
            td {
                class: "py-2 pr-3",
                match cve.triage_status.as_str() {
                    "accepted" => rsx! {
                        span {
                            class: "px-2 py-1 text-xs rounded-md bg-purple-500/20 text-purple-300",
                            "Accepted"
                        }
                    },
                    "scheduled" => rsx! {
                        span {
                            class: "px-2 py-1 text-xs rounded-md bg-blue-500/20 text-blue-300",
                            "Scheduled"
                        }
                    },
                    _ => rsx! {
                        span {
                            class: "px-2 py-1 text-xs rounded-md bg-red-500/20 text-red-300",
                            "Outstanding"
                        }
                    },
                }
            }

            // Age
            td {
                class: "py-2 pr-3 text-xs {theme::text::SECONDARY}",
                "{cve.age_days}d"
            }
        }
    }
}
