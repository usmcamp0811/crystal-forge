//! CVE dashboard view.

use dioxus::prelude::*;

use crate::api::client::{
    ApiClientError, fetch_cve_dashboard_summary, fetch_cve_dashboard_vulnerabilities,
    fetch_cve_scan_freshness, fetch_cve_top_systems,
};
use crate::api::models::{
    CveDashboardTopSystem, CveDashboardVulnerability, CveDashboardVulnerabilityParams,
    CveScanFreshnessRow,
};
use crate::components::layout::Card;
use crate::components::stat_card::StatCard;
use crate::routes::Route;
use crate::theme;

/// The CVE dashboard page.
#[component]
pub fn CvesView() -> Element {
    let mut severity_filter = use_signal(|| None::<String>);
    let mut status_filter = use_signal(|| None::<String>);
    let mut system_filter = use_signal(String::new);
    let mut environment_filter = use_signal(String::new);
    let mut package_filter = use_signal(String::new);
    let mut date_from_filter = use_signal(String::new);
    let mut date_to_filter = use_signal(String::new);

    let summary = use_resource(move || async move { fetch_cve_dashboard_summary().await });
    let top_systems = use_resource(move || async move { fetch_cve_top_systems().await });
    let scan_freshness = use_resource(move || async move { fetch_cve_scan_freshness().await });
    let vulnerabilities = use_resource(move || {
        let severity = severity_filter();
        let status = status_filter();
        let system = system_filter();
        let environment = environment_filter();
        let package = package_filter();
        let date_from = date_from_filter();
        let date_to = date_to_filter();

        async move {
            let params = CveDashboardVulnerabilityParams {
                severity,
                status,
                system: non_empty(system),
                environment: non_empty(environment),
                package: non_empty(package),
                date_from: non_empty(date_from),
                date_to: non_empty(date_to),
                limit: Some(200),
            };
            fetch_cve_dashboard_vulnerabilities(&params).await
        }
    });

    let content = match &*summary.read_unchecked() {
        Some(Ok(metrics)) => {
            let oldest = metrics
                .oldest_cve_age_days
                .map(|d| format!("{d} days"))
                .unwrap_or_else(|| "n/a".to_string());

            let active_severity = severity_filter();
            let active_status = status_filter();

            let drilldown = match &*vulnerabilities.read_unchecked() {
                Some(Ok(items)) => render_vulnerability_table(items),
                Some(Err(error)) => rsx! {
                    p { class: "{theme::text::SECONDARY}", "Failed to load vulnerabilities: {error}" }
                },
                None => rsx! {
                    p { class: "{theme::text::SECONDARY}", "Loading vulnerabilities..." }
                },
            };

            let top_systems_content = match &*top_systems.read_unchecked() {
                Some(Ok(systems)) => render_top_systems_table(systems),
                Some(Err(_)) => {
                    rsx! { p { class: "{theme::text::SECONDARY}", "Failed to load top affected systems." } }
                }
                None => rsx! { p { class: "{theme::text::SECONDARY}", "Loading..." } },
            };

            let freshness_content = match &*scan_freshness.read_unchecked() {
                Some(Ok(rows)) => render_scan_freshness_table(rows),
                Some(Err(_)) => {
                    rsx! { p { class: "{theme::text::SECONDARY}", "Failed to load scan freshness data." } }
                }
                None => rsx! { p { class: "{theme::text::SECONDARY}", "Loading..." } },
            };

            rsx! {
                div {
                    class: "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4",
                    StatCard {
                        label: "Total Open CVEs".to_string(),
                        value: metrics.total_open.to_string(),
                        color_class: theme::cve::CRITICAL_TEXT.to_string(),
                    }
                    StatCard {
                        label: "Affected Systems".to_string(),
                        value: metrics.affected_systems.to_string(),
                    }
                    StatCard {
                        label: "New CVEs (7 days)".to_string(),
                        value: metrics.new_cves_last_7_days.to_string(),
                    }
                    StatCard {
                        label: "Oldest Open CVE".to_string(),
                        value: oldest,
                    }
                }

                Card {
                    title: Some("Severity Breakdown".to_string()),
                    children: rsx! {
                        div { class: "grid grid-cols-2 md:grid-cols-4 gap-3",
                            StatCard {
                                label: "Critical".to_string(),
                                value: metrics.severity.critical.to_string(),
                                color_class: theme::cve::CRITICAL_TEXT.to_string(),
                            }
                            StatCard {
                                label: "High".to_string(),
                                value: metrics.severity.high.to_string(),
                                color_class: theme::cve::HIGH_TEXT.to_string(),
                            }
                            StatCard {
                                label: "Medium".to_string(),
                                value: metrics.severity.medium.to_string(),
                                color_class: theme::cve::MEDIUM_TEXT.to_string(),
                            }
                            StatCard {
                                label: "Low".to_string(),
                                value: metrics.severity.low.to_string(),
                                color_class: theme::cve::LOW_TEXT.to_string(),
                            }
                        }

                        div { class: "mt-4 flex flex-wrap gap-2",
                            FilterButton {
                                label: "All severities",
                                active: active_severity.is_none(),
                                onclick: move |_| severity_filter.set(None),
                            }
                            FilterButton {
                                label: "Critical",
                                active: active_severity.as_deref() == Some("critical"),
                                onclick: move |_| severity_filter.set(Some("critical".to_string())),
                            }
                            FilterButton {
                                label: "High",
                                active: active_severity.as_deref() == Some("high"),
                                onclick: move |_| severity_filter.set(Some("high".to_string())),
                            }
                            FilterButton {
                                label: "Medium",
                                active: active_severity.as_deref() == Some("medium"),
                                onclick: move |_| severity_filter.set(Some("medium".to_string())),
                            }
                            FilterButton {
                                label: "Low",
                                active: active_severity.as_deref() == Some("low"),
                                onclick: move |_| severity_filter.set(Some("low".to_string())),
                            }
                        }
                    }
                }

                div { "data-testid": "cve-top-systems",
                    Card {
                        title: Some("Top Affected Systems".to_string()),
                        children: rsx! { {top_systems_content} }
                    }
                }

                div { "data-testid": "cve-scan-freshness",
                    Card {
                        title: Some("Scan Freshness / Coverage".to_string()),
                        children: rsx! { {freshness_content} }
                    }
                }

                div { "data-testid": "cve-drill-down",
                Card {
                    title: Some("CVE Drill-down".to_string()),
                    children: rsx! {
                        div { class: "mb-4 grid grid-cols-1 md:grid-cols-2 xl:grid-cols-5 gap-2",
                            input {
                                class: "px-3 py-2 rounded-md border border-white/15 bg-black/20 text-sm",
                                r#type: "text",
                                placeholder: "System",
                                value: "{system_filter}",
                                oninput: move |evt| system_filter.set(evt.value()),
                            }
                            input {
                                class: "px-3 py-2 rounded-md border border-white/15 bg-black/20 text-sm",
                                r#type: "text",
                                placeholder: "Environment",
                                value: "{environment_filter}",
                                oninput: move |evt| environment_filter.set(evt.value()),
                            }
                            input {
                                class: "px-3 py-2 rounded-md border border-white/15 bg-black/20 text-sm",
                                r#type: "text",
                                placeholder: "Package",
                                value: "{package_filter}",
                                oninput: move |evt| package_filter.set(evt.value()),
                            }
                            input {
                                class: "px-3 py-2 rounded-md border border-white/15 bg-black/20 text-sm",
                                r#type: "date",
                                value: "{date_from_filter}",
                                oninput: move |evt| date_from_filter.set(evt.value()),
                            }
                            input {
                                class: "px-3 py-2 rounded-md border border-white/15 bg-black/20 text-sm",
                                r#type: "date",
                                value: "{date_to_filter}",
                                oninput: move |evt| date_to_filter.set(evt.value()),
                            }
                        }

                        div { class: "mb-4 flex flex-wrap gap-2",
                            FilterButton {
                                label: "All statuses",
                                active: active_status.is_none(),
                                onclick: move |_| status_filter.set(None),
                            }
                            FilterButton {
                                label: "Open (no fix)",
                                active: active_status.as_deref() == Some("open"),
                                onclick: move |_| status_filter.set(Some("open".to_string())),
                            }
                            FilterButton {
                                label: "Fix available",
                                active: active_status.as_deref() == Some("fix_available"),
                                onclick: move |_| status_filter.set(Some("fix_available".to_string())),
                            }
                        }
                        {drilldown}
                    }
                }
                }
            }
        }
        Some(Err(error)) => {
            let message = if is_forbidden_error(error) {
                "Admin privileges are required to view CVE dashboard data.".to_string()
            } else {
                format!("Failed to load CVE dashboard data: {error}")
            };

            rsx! {
                Card {
                    title: Some("Security Status".to_string()),
                    children: rsx! {
                        p { class: "{theme::text::SECONDARY}", "{message}" }
                    }
                }
            }
        }
        None => rsx! {
            Card {
                title: Some("Security Status".to_string()),
                children: rsx! {
                    p { class: "{theme::text::SECONDARY}", "Loading CVE dashboard..." }
                }
            }
        },
    };

    rsx! {
        div {
            class: "space-y-6",
            h1 {
                class: "{theme::typography::PAGE_TITLE}",
                "CVE Dashboard"
            }
            {content}
        }
    }
}

fn is_forbidden_error(error: &ApiClientError) -> bool {
    matches!(error, ApiClientError::Status { code: 403, .. })
}

#[component]
fn FilterButton(label: &'static str, active: bool, onclick: EventHandler<MouseEvent>) -> Element {
    let class = if active {
        "px-3 py-1.5 rounded-md text-xs font-medium border bg-violet-600/20 border-violet-400/50 text-violet-100"
    } else {
        "px-3 py-1.5 rounded-md text-xs font-medium border border-white/15 text-gray-300 hover:bg-white/5"
    };

    rsx! {
        button {
            class: "{class}",
            onclick: move |evt| onclick.call(evt),
            "{label}"
        }
    }
}

fn render_vulnerability_table(items: &[CveDashboardVulnerability]) -> Element {
    if items.is_empty() {
        return rsx! {
            p { class: "{theme::text::SECONDARY}", "No vulnerabilities match the current filters." }
        };
    }

    rsx! {
        div { class: "overflow-x-auto",
            table { class: "min-w-full text-sm",
                thead {
                    tr { class: "border-b border-white/10 text-left {theme::text::SECONDARY}",
                        th { class: "py-2 pr-3", "CVE" }
                        th { class: "py-2 pr-3", "Severity" }
                        th { class: "py-2 pr-3", "Package" }
                        th { class: "py-2 pr-3", "System" }
                        th { class: "py-2 pr-3", "Fix" }
                        th { class: "py-2 pr-3", "First Seen" }
                    }
                }
                tbody {
                    for item in items.iter() {
                        tr { class: "border-b border-white/5",
                            td { class: "py-2 pr-3 font-medium {theme::text::PRIMARY}", "{item.cve_id}" }
                            td { class: "py-2 pr-3", "{item.severity.label()}" }
                            td { class: "py-2 pr-3",
                                span { title: "installed: {item.installed_version}",
                                    "{item.package_name} {item.installed_version}"
                                }
                            }
                            td { class: "py-2 pr-3",
                                Link {
                                    to: Route::SystemDetailView { id: item.system_id.to_string() },
                                    class: "text-violet-300 hover:text-violet-200 underline",
                                    "{item.hostname}"
                                }
                            }
                            td { class: "py-2 pr-3", "{format_fix_status(&item.status, item.fixed_version.as_deref())}" }
                            td { class: "py-2 pr-3", "{format_first_seen(item)}" }
                        }
                    }
                }
            }
        }
    }
}

fn format_first_seen(item: &CveDashboardVulnerability) -> String {
    item.first_seen
        .map(|value| value.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "n/a".to_string())
}

/// Renders a human-readable fix status label.
///
/// `status` is one of `'open'` or `'fix_available'` as returned by the API.
/// A non-null `fixed_version` means an upstream patched version exists; it does
/// NOT imply the affected system has been updated.
fn format_fix_status(status: &str, fixed_version: Option<&str>) -> String {
    match status {
        "fix_available" => {
            if let Some(ver) = fixed_version {
                format!("Fix in {ver}")
            } else {
                "Fix available".to_string()
            }
        }
        _ => "No fix yet".to_string(),
    }
}

fn render_top_systems_table(items: &[CveDashboardTopSystem]) -> Element {
    if items.is_empty() {
        return rsx! {
            p { class: "{theme::text::SECONDARY}", "No systems with CVE data found." }
        };
    }

    rsx! {
        div { class: "overflow-x-auto",
            table { class: "min-w-full text-sm",
                thead {
                    tr { class: "border-b border-white/10 text-left {theme::text::SECONDARY}",
                        th { class: "py-2 pr-3", "System" }
                        th { class: "py-2 pr-3", "Critical" }
                        th { class: "py-2 pr-3", "High" }
                        th { class: "py-2 pr-3", "Medium" }
                        th { class: "py-2 pr-3", "Low" }
                        th { class: "py-2 pr-3", "Total" }
                        th { class: "py-2 pr-3", "Last Scan" }
                    }
                }
                tbody {
                    for item in items.iter() {
                        tr { class: "border-b border-white/5",
                            td { class: "py-2 pr-3",
                                Link {
                                    to: Route::SystemDetailView { id: item.system_id.to_string() },
                                    class: "text-violet-300 hover:text-violet-200 underline",
                                    "{item.hostname}"
                                }
                            }
                            td { class: "py-2 pr-3 text-red-400 font-medium", "{item.critical_cves}" }
                            td { class: "py-2 pr-3 text-orange-400", "{item.high_cves}" }
                            td { class: "py-2 pr-3 text-yellow-400", "{item.medium_cves}" }
                            td { class: "py-2 pr-3 text-gray-400", "{item.low_cves}" }
                            td { class: "py-2 pr-3 font-medium", "{item.total_cves}" }
                            td { class: "py-2 pr-3", "{format_days_ago(item.days_since_scan)}" }
                        }
                    }
                }
            }
        }
    }
}

fn render_scan_freshness_table(items: &[CveScanFreshnessRow]) -> Element {
    if items.is_empty() {
        return rsx! {
            p { class: "{theme::text::SECONDARY}", "No scan data found." }
        };
    }

    // Pre-compute per-row derived values outside RSX to avoid let-in-rsx restrictions.
    let rows: Vec<(String, String, &'static str, &'static str, String)> = items
        .iter()
        .map(|item| {
            let stale = item.days_since_scan.map(|d| d > 30).unwrap_or(true);
            let age_class = if stale {
                "text-red-400"
            } else {
                "text-green-400"
            };
            let status = if stale { "Stale" } else { "Fresh" };
            let scan_date = item
                .last_cve_scan
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "never".to_string());
            let age = format_days_ago(item.days_since_scan);
            (scan_date, age, age_class, status, item.hostname.clone())
        })
        .collect();

    rsx! {
        div { class: "overflow-x-auto",
            table { class: "min-w-full text-sm",
                thead {
                    tr { class: "border-b border-white/10 text-left {theme::text::SECONDARY}",
                        th { class: "py-2 pr-3", "System" }
                        th { class: "py-2 pr-3", "Last Scan" }
                        th { class: "py-2 pr-3", "Age" }
                        th { class: "py-2 pr-3", "CVEs" }
                        th { class: "py-2 pr-3", "Status" }
                    }
                }
                tbody {
                    for (item, (scan_date, age, age_class, status, _hostname)) in
                        items.iter().zip(rows.iter())
                    {
                        tr { class: "border-b border-white/5",
                            td { class: "py-2 pr-3",
                                Link {
                                    to: Route::SystemDetailView { id: item.system_id.to_string() },
                                    class: "text-violet-300 hover:text-violet-200 underline",
                                    "{item.hostname}"
                                }
                            }
                            td { class: "py-2 pr-3 {theme::text::SECONDARY}", "{scan_date}" }
                            td { class: "py-2 pr-3 {age_class}", "{age}" }
                            td { class: "py-2 pr-3", "{item.total_cves}" }
                            td { class: "py-2 pr-3 {age_class}", "{status}" }
                        }
                    }
                }
            }
        }
    }
}

fn format_days_ago(days: Option<i64>) -> String {
    match days {
        None => "never".to_string(),
        Some(0) => "today".to_string(),
        Some(1) => "1 day ago".to_string(),
        Some(d) => format!("{d} days ago"),
    }
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
