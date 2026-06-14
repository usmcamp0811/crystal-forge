//! CVE (Common Vulnerabilities and Exposures) display components.

use std::collections::BTreeMap;

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::client::save_system_cve_justification;
use crate::api::models::{
    CveSeverity, CveSummary, SaveSystemCveJustificationRequest, SystemVulnerability,
};
use crate::theme;

#[derive(Clone)]
struct GroupedCve {
    cve_id: String,
    severity: CveSeverity,
    cvss_score: Option<f32>,
    description: String,
    published_at: Option<chrono::DateTime<chrono::Utc>>,
    status: String,
    package_instances: Vec<SystemVulnerability>,
    justification_category: Option<String>,
    justification_reason: Option<String>,
    justification_updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// A single CVE entry within a package group (design "package-first" view).
#[derive(Clone)]
struct PackageCve {
    cve_id: String,
    severity: CveSeverity,
    cvss_score: Option<f32>,
    description: String,
    published_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Whether a fix is available for this package instance.
    has_fix: bool,
    justification_category: Option<String>,
    justification_reason: Option<String>,
    justification_updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// A package and the (deduplicated) CVEs affecting it, mirroring the design's
/// package-first grouping.
#[derive(Clone)]
struct PackageGroup {
    package_name: String,
    version: String,
    cves: Vec<PackageCve>,
    critical: usize,
    high: usize,
    medium: usize,
    low: usize,
    fixable: usize,
    max_cvss: f32,
    /// Severity-weighted sort score (higher = more severe).
    sort_weight: i64,
}

const JUSTIFICATION_PRESETS: [(&str, &str); 5] = [
    ("false_positive", "False positive"),
    ("accepted_risk", "Accepted risk"),
    (
        "compensating_control",
        "Compensating controls are in place and documented",
    ),
    (
        "planned_remediation",
        "Planned remediation approved; waiting for patch window",
    ),
    (
        "vendor_pending_fix",
        "Vendor fix not yet available; temporary risk acceptance",
    ),
];

/// CVE tab showing grouped vulnerabilities with search/filter and justification workflow.
#[component]
pub fn CvesTab(
    system_id: Uuid,
    cve_counts: CveSummary,
    vulnerabilities: Vec<SystemVulnerability>,
    allow_mutations: bool,
    on_saved: EventHandler<()>,
    /// True while the vulnerabilities resource is still loading.
    #[props(default = false)]
    loading: bool,
    /// Error message when the vulnerabilities load failed. When set, the tab
    /// renders an error state instead of (mock) data — security data must never
    /// silently fall back to fake CVEs in production paths.
    #[props(default = None)]
    error: Option<String>,
) -> Element {
    let mut expanded_cve: Signal<Option<String>> = use_signal(|| None);

    let mut editing_cve: Signal<Option<String>> = use_signal(|| None);
    let mut draft_category: Signal<Option<String>> = use_signal(|| None);
    let mut draft_reason: Signal<String> = use_signal(String::new);
    let mut save_status: Signal<Option<String>> = use_signal(|| None);
    let mut save_in_progress = use_signal(|| false);

    // Package-first grouping matching the design reference. The System Detail CVE
    // example does not include a filter/search bar; filtering remains available on the
    // dedicated CVE surface, while this tab focuses on the per-system package rollup.
    let filtered_groups = group_vulnerabilities_by_package(&vulnerabilities);

    let shown_cve_count: usize = filtered_groups.iter().map(|group| group.cves.len()).sum();
    let shown_package_count = filtered_groups.len();
    let shown_package_suffix = if shown_package_count == 1 { "" } else { "s" };
    let total_cves = cve_counts.total();

    let status_is_error = save_status
        .read()
        .as_ref()
        .map(|message| message.starts_with("Failed") || message.contains("required"))
        .unwrap_or(false);

    // Loading state — show a spinner instead of an empty/fake list while the
    // vulnerabilities resource is still in flight.
    if loading {
        return rsx! {
            div {
                class: "empty",
                "data-testid": "system-cves-loading",
                crate::components::loading::DashboardLoadingSpinner {
                    label: "Loading vulnerabilities".to_string(),
                    size: 36,
                }
                div { "Fetching the latest CVE scan results." }
            }
        };
    }

    // Error state — never render mock CVEs on API failure (security data).
    if let Some(message) = error {
        return rsx! {
            div {
                class: "empty",
                "data-testid": "system-cves-error",
                h3 { "Unable to load vulnerabilities" }
                div { "{message}" }
            }
        };
    }

    rsx! {
            div {
                style: "display:flex;flex-direction:column;gap:14px;",

            if let Some(message) = save_status() {
                div {
                    class: if status_is_error {
                        "px-4 py-2.5 rounded-lg border border-red-800/80 bg-red-950/50 text-sm text-red-200"
                    } else {
                        "px-4 py-2.5 rounded-lg border border-emerald-700/70 bg-emerald-950/40 text-sm text-emerald-200"
                    },
                    "{message}"
                }
            }

            // Results: package-first grouping matching the design reference.
            section {
                class: "card",
                style: "overflow: hidden;",

                div {
                    class: "sd-card-head",
                    style: "padding: 14px 18px;",
                    h2 { "Vulnerabilities" }
                    span {
                        class: "sd-card-meta",
                        "{format_count(shown_cve_count as i64)} of {format_count(total_cves)} shown · {format_count(shown_package_count as i64)} package{shown_package_suffix}"
                    }
                }

                if filtered_groups.is_empty() {
                    div {
                        class: "empty",
                        h3 { "No vulnerabilities detected" }
                        div { "Last scan data did not report any package-level CVEs for this host." }
                    }
                } else {
                    div {
                        style: "display: flex; flex-direction: column; gap: 10px; padding: 14px;",
                        for group in filtered_groups {
                            {
                                let is_open = *expanded_cve.read() == Some(group.package_name.clone());
                                let sev_color = package_group_color(&group);
                                let package_name = group.package_name.clone();
                                let pending = group.cves.len().saturating_sub(group.fixable);
                                let cve_suffix = if group.cves.len() == 1 { "" } else { "s" };
                                let head_bg = if is_open {
                                    "color-mix(in oklab, var(--cf-brand-purple) 6%, var(--cf-card-bg))"
                                } else {
                                    "transparent"
                                };
                                let chevron_d = if is_open { "M19 9l-7 7-7-7" } else { "M9 5l7 7-7 7" };

                                rsx! {
                                    div {
                                        key: "{group.package_name}",
                                        class: "card",
                                        style: "overflow: hidden;",

                                        button {
                                            class: "focus-ring",
                                            style: "all: unset; display: grid; grid-template-columns: 24px 1fr auto; align-items: center; gap: 14px; padding: 12px 16px; cursor: pointer; width: 100%; box-sizing: border-box; border-left: 3px solid {sev_color}; background: {head_bg};",
                                            onclick: move |_| {
                                                let current = expanded_cve.read().clone();
                                                if current == Some(package_name.clone()) {
                                                    expanded_cve.set(None);
                                                } else {
                                                    expanded_cve.set(Some(package_name.clone()));
                                                }
                                            },

                                            svg {
                                                width: "14",
                                                height: "14",
                                                fill: "none",
                                                stroke: "currentColor",
                                                stroke_width: "2",
                                                view_box: "0 0 24 24",
                                                style: "color: var(--cf-text-muted);",
                                                path { stroke_linecap: "round", stroke_linejoin: "round", d: "{chevron_d}" }
                                            }

                                            div { style: "min-width: 0;",
                                                div { style: "display: flex; align-items: center; gap: 10px; flex-wrap: wrap;",
                                                    span { class: "mono", style: "font-size: 14px; font-weight: 700;", "{group.package_name}" }
                                                    span { class: "mono", style: "font-size: 11px; color: var(--cf-text-muted);", "{group.version}" }
                                                    span { style: "font-size: 12px; color: var(--cf-text-muted);",
                                                        "{group.cves.len()} CVE{cve_suffix}"
                                                    }
                                                }
                                                div { style: "font-size: 11px; color: var(--cf-text-secondary); margin-top: 2px;",
                                                    "max CVSS {group.max_cvss:.1} · {group.fixable} patchable · {pending} pending"
                                                }
                                            }

                                            div { style: "display: flex; gap: 5px; flex-wrap: wrap; justify-content: flex-end;",
                                                if group.critical > 0 {
                                                    span { class: "chip chip-critical", style: "font-size: 10px;", "{group.critical} crit" }
                                                }
                                                if group.high > 0 {
                                                    span { class: "chip chip-warning", style: "font-size: 10px;", "{group.high} high" }
                                                }
                                                if group.medium > 0 {
                                                    span { class: "chip chip-unknown", style: "font-size: 10px;", "{group.medium} med" }
                                                }
                                            }
                                        }

                                        if is_open {
                                            table { class: "sys-table",
                                                thead {
                                                    tr {
                                                        th { "CVE" }
                                                        th { "Severity" }
                                                        th { "CVSS" }
                                                        th { "Fix" }
                                                        th { style: "text-align: right;", " " }
                                                    }
                                                }
                                                tbody {
                                                    for cve in group.cves.iter() {
                                                        {
                                                            let cve_id = cve.cve_id.clone();
                                                            let is_editing = *editing_cve.read() == Some(cve.cve_id.clone());
                                                            let has_justification = cve
                                                                .justification_reason
                                                                .as_ref()
                                                                .map(|value| !value.trim().is_empty())
                                                                .unwrap_or(false);
                                                            let cvss_label = cve
                                                                .cvss_score
                                                                .map(|score| format!("{score:.1}"))
                                                                .unwrap_or_else(|| "—".to_string());
                                                            let justification_updated_label = cve
                                                                .justification_updated_at
                                                                .map(|updated_at| updated_at.format("%Y-%m-%d %H:%M").to_string());

                                                            rsx! {
                                                                tr {
                                                                    key: "{group.package_name}-{cve.cve_id}",
                                                                    td { class: "mono", style: "color: var(--cf-text-primary);", "{cve.cve_id}" }
                                                                    td {
                                                                        span { class: "{severity_chip_class(&cve.severity)}", "{cve.severity.label()}" }
                                                                    }
                                                                    td { class: "mono", "{cvss_label}" }
                                                                    td {
                                                                        if cve.has_fix {
                                                                            span { class: "chip chip-healthy", "available" }
                                                                        } else {
                                                                            span { class: "chip chip-unknown", "pending" }
                                                                        }
                                                                    }
                                                                    td {
                                                                        div { class: "row-actions",
                                                                            button {
                                                                                class: "btn-icon focus-ring",
                                                                                title: if has_justification { "Edit justification" } else { "Justify" },
                                                                                onclick: {
                                                                                    let cve_id = cve.cve_id.clone();
                                                                                    let existing_category = cve.justification_category.clone();
                                                                                    let existing_reason = cve.justification_reason.clone().unwrap_or_default();
                                                                                    move |_| {
                                                                                        if *editing_cve.read() == Some(cve_id.clone()) {
                                                                                            editing_cve.set(None);
                                                                                        } else {
                                                                                            editing_cve.set(Some(cve_id.clone()));
                                                                                            draft_category.set(existing_category.clone());
                                                                                            draft_reason.set(existing_reason.clone());
                                                                                            save_status.set(None);
                                                                                        }
                                                                                    }
                                                                                },
                                                                                if has_justification {
                                                                                    span { class: "chip chip-healthy", style: "font-size: 10px;", "justified" }
                                                                                } else {
                                                                                    svg {
                                                                                        class: "w-3.5 h-3.5",
                                                                                        fill: "none",
                                                                                        stroke: "currentColor",
                                                                                        stroke_width: "2",
                                                                                        view_box: "0 0 24 24",
                                                                                        path { stroke_linecap: "round", stroke_linejoin: "round", d: "M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" }
                                                                                    }
                                                                                }
                                                                            }
                                                                            a {
                                                                                class: "btn-icon focus-ring",
                                                                                title: "Open advisory",
                                                                                href: "https://nvd.nist.gov/vuln/detail/{cve.cve_id}",
                                                                                target: "_blank",
                                                                                rel: "noopener noreferrer",
                                                                                svg {
                                                                                    class: "w-3.5 h-3.5",
                                                                                    fill: "none",
                                                                                    stroke: "currentColor",
                                                                                    stroke_width: "2",
                                                                                    view_box: "0 0 24 24",
                                                                                    path { stroke_linecap: "round", stroke_linejoin: "round", d: "M13.828 10.172a4 4 0 010 5.656l-3 3a4 4 0 01-5.656-5.656l1.5-1.5m9.656-1.328l1.5-1.5a4 4 0 00-5.656-5.656l-3 3a4 4 0 000 5.656" }
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }

                                                                if is_editing {
                                                                    tr {
                                                                        key: "{group.package_name}-{cve.cve_id}-justify",
                                                                        td {
                                                                            colspan: "5",
                                                                            style: "background: var(--cf-subtle-bg);",
                                                                            div {
                                                                                class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} p-3 space-y-2",
                                                                                div { class: "flex items-center justify-between gap-3",
                                                                                    h4 { class: "text-sm font-semibold {theme::text::PRIMARY}", "Justification — {cve.cve_id}" }
                                                                                    if let Some(ref justification_updated_label) = justification_updated_label {
                                                                                        span { class: "text-xs {theme::text::MUTED}", "Updated {justification_updated_label}" }
                                                                                    }
                                                                                }

                                                                                select {
                                                                                    class: "{theme::interactive::INPUT} w-full",
                                                                                    value: draft_category.read().clone().unwrap_or_else(|| "".to_string()),
                                                                                    onchange: move |evt| {
                                                                                        let value = evt.value();
                                                                                        if value.trim().is_empty() {
                                                                                            draft_category.set(None);
                                                                                            return;
                                                                                        }
                                                                                        draft_category.set(Some(value.clone()));
                                                                                        if let Some((_, default_reason)) = JUSTIFICATION_PRESETS
                                                                                            .iter()
                                                                                            .find(|(key, _)| *key == value)
                                                                                        {
                                                                                            if draft_reason.read().trim().is_empty() {
                                                                                                draft_reason.set((*default_reason).to_string());
                                                                                            }
                                                                                        }
                                                                                    },
                                                                                    option { value: "", "Select category (optional)" }
                                                                                    for (value, label) in JUSTIFICATION_PRESETS {
                                                                                        option { value: "{value}", "{label}" }
                                                                                    }
                                                                                }

                                                                                textarea {
                                                                                    class: "{theme::interactive::INPUT} w-full min-h-[100px]",
                                                                                    placeholder: "Document risk acceptance / mitigation rationale",
                                                                                    value: draft_reason.read().clone(),
                                                                                    oninput: move |evt| draft_reason.set(evt.value()),
                                                                                }

                                                                                div { class: "text-xs {theme::text::MUTED}", "This note is persisted per system + CVE for audit review." }

                                                                                div { class: "flex items-center gap-2",
                                                                                    button {
                                                                                        class: "px-3 py-2 rounded-md {theme::interactive::PRIMARY_BTN} text-sm font-semibold text-white transition-colors disabled:opacity-50 {theme::interactive::FOCUS_RING}",
                                                                                        disabled: *save_in_progress.read() || !allow_mutations,
                                                                                        onclick: {
                                                                                            let cve_id = cve_id.clone();
                                                                                            move |_| {
                                                                                                let reason = draft_reason.read().trim().to_string();
                                                                                                if reason.is_empty() {
                                                                                                    save_status.set(Some("Justification reason is required".to_string()));
                                                                                                    return;
                                                                                                }

                                                                                                let category = draft_category.read().clone();
                                                                                                let cve_id_for_request = cve_id.clone();
                                                                                                save_in_progress.set(true);
                                                                                                save_status.set(None);

                                                                                                spawn(async move {
                                                                                                    let result = save_system_cve_justification(
                                                                                                        &system_id,
                                                                                                        &cve_id_for_request,
                                                                                                        &SaveSystemCveJustificationRequest { category, reason },
                                                                                                    ).await;

                                                                                                    save_in_progress.set(false);
                                                                                                    match result {
                                                                                                        Ok(_) => {
                                                                                                            save_status.set(Some("Justification saved".to_string()));
                                                                                                            editing_cve.set(None);
                                                                                                            on_saved.call(());
                                                                                                        }
                                                                                                        Err(err) => {
                                                                                                            save_status.set(Some(format!("Failed to save justification: {err}")));
                                                                                                        }
                                                                                                    }
                                                                                                });
                                                                                            }
                                                                                        },
                                                                                        if !allow_mutations {
                                                                                            "Operator/Admin required"
                                                                                        } else if *save_in_progress.read() {
                                                                                            "Saving..."
                                                                                        } else {
                                                                                            "Save"
                                                                                        }
                                                                                    }
                                                                                    button {
                                                                                        class: "px-3 py-2 rounded-md border {theme::surface::CARD_BORDER} {theme::surface::SUBTLE_BG} {theme::interactive::HOVER_BG} text-sm {theme::text::PRIMARY} transition-colors {theme::interactive::FOCUS_RING}",
                                                                                        onclick: move |_| editing_cve.set(None),
                                                                                        "Cancel"
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn group_vulnerabilities_by_cve(vulnerabilities: &[SystemVulnerability]) -> Vec<GroupedCve> {
    let mut grouped: BTreeMap<String, GroupedCve> = BTreeMap::new();

    for vuln in vulnerabilities {
        let entry = grouped
            .entry(vuln.cve_id.clone())
            .or_insert_with(|| GroupedCve {
                cve_id: vuln.cve_id.clone(),
                severity: vuln.severity.clone(),
                cvss_score: vuln.cvss_score,
                description: vuln.description.clone(),
                published_at: vuln.published_at,
                status: normalize_status(vuln.status.as_deref()),
                package_instances: Vec::new(),
                justification_category: vuln.justification_category.clone(),
                justification_reason: vuln.justification_reason.clone(),
                justification_updated_at: vuln.justification_updated_at,
            });

        // Deduplicate by package_name + installed_version to avoid showing
        // go-1.24.4 repeated 50+ times from different store paths/derivations
        let already_has_package = entry.package_instances.iter().any(|item| {
            item.package_name == vuln.package_name
                && item.installed_version == vuln.installed_version
        });

        if !already_has_package {
            entry.package_instances.push(vuln.clone());
        }

        if severity_rank(&vuln.severity) > severity_rank(&entry.severity) {
            entry.severity = vuln.severity.clone();
        }

        if entry.cvss_score.unwrap_or_default() < vuln.cvss_score.unwrap_or_default() {
            entry.cvss_score = vuln.cvss_score;
        }

        if entry.justification_reason.is_none() && vuln.justification_reason.is_some() {
            entry.justification_reason = vuln.justification_reason.clone();
            entry.justification_category = vuln.justification_category.clone();
            entry.justification_updated_at = vuln.justification_updated_at;
        }

        entry.status = reconcile_group_status(&entry.status, vuln.status.as_deref());
    }

    let mut groups = grouped.into_values().collect::<Vec<_>>();
    for group in &mut groups {
        group.package_instances.sort_by(|a, b| {
            a.package_name
                .cmp(&b.package_name)
                .then_with(|| a.installed_version.cmp(&b.installed_version))
        });
    }
    groups.sort_by(|a, b| {
        b.cvss_score
            .partial_cmp(&a.cvss_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.cve_id.cmp(&b.cve_id))
    });
    groups
}

/// Group vulnerabilities by package name (inverse of `group_vulnerabilities_by_cve`).
/// Within each package, CVEs are deduplicated by CVE id and the worst severity /
/// highest CVSS / available-fix flag is retained.
fn group_vulnerabilities_by_package(vulnerabilities: &[SystemVulnerability]) -> Vec<PackageGroup> {
    let mut packages: BTreeMap<String, PackageGroup> = BTreeMap::new();

    for vuln in vulnerabilities {
        let group = packages
            .entry(vuln.package_name.clone())
            .or_insert_with(|| PackageGroup {
                package_name: vuln.package_name.clone(),
                version: vuln.installed_version.clone(),
                cves: Vec::new(),
                critical: 0,
                high: 0,
                medium: 0,
                low: 0,
                fixable: 0,
                max_cvss: 0.0,
                sort_weight: 0,
            });

        let has_fix = vuln.fixed_version.is_some();

        if let Some(existing) = group.cves.iter_mut().find(|c| c.cve_id == vuln.cve_id) {
            // Merge into the existing CVE entry, keeping the worst observed values.
            if severity_rank(&vuln.severity) > severity_rank(&existing.severity) {
                existing.severity = vuln.severity.clone();
            }
            if existing.cvss_score.unwrap_or_default() < vuln.cvss_score.unwrap_or_default() {
                existing.cvss_score = vuln.cvss_score;
            }
            existing.has_fix = existing.has_fix || has_fix;
            if existing.justification_reason.is_none() && vuln.justification_reason.is_some() {
                existing.justification_reason = vuln.justification_reason.clone();
                existing.justification_category = vuln.justification_category.clone();
                existing.justification_updated_at = vuln.justification_updated_at;
            }
        } else {
            group.cves.push(PackageCve {
                cve_id: vuln.cve_id.clone(),
                severity: vuln.severity.clone(),
                cvss_score: vuln.cvss_score,
                description: vuln.description.clone(),
                published_at: vuln.published_at,
                has_fix,
                justification_category: vuln.justification_category.clone(),
                justification_reason: vuln.justification_reason.clone(),
                justification_updated_at: vuln.justification_updated_at,
            });
        }
    }

    let sev_weight = |severity: &CveSeverity| -> i64 {
        match severity {
            CveSeverity::Critical => 1000,
            CveSeverity::High => 100,
            CveSeverity::Medium => 10,
            CveSeverity::Low => 1,
        }
    };

    let mut groups = packages.into_values().collect::<Vec<_>>();
    for group in &mut groups {
        group.cves.sort_by(|a, b| {
            severity_rank(&b.severity)
                .cmp(&severity_rank(&a.severity))
                .then_with(|| {
                    b.cvss_score
                        .partial_cmp(&a.cvss_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.cve_id.cmp(&b.cve_id))
        });

        for cve in &group.cves {
            match cve.severity {
                CveSeverity::Critical => group.critical += 1,
                CveSeverity::High => group.high += 1,
                CveSeverity::Medium => group.medium += 1,
                CveSeverity::Low => group.low += 1,
            }
            if cve.has_fix {
                group.fixable += 1;
            }
            let score = cve.cvss_score.unwrap_or_default();
            if score > group.max_cvss {
                group.max_cvss = score;
            }
            group.sort_weight += sev_weight(&cve.severity);
        }
    }

    groups.sort_by(|a, b| {
        b.sort_weight
            .cmp(&a.sort_weight)
            .then_with(|| a.package_name.cmp(&b.package_name))
    });
    groups
}

/// Left-border / accent color for a package group based on its worst severity,
/// matching the design reference palette.
fn package_group_color(group: &PackageGroup) -> &'static str {
    if group.critical > 0 {
        "#f87171"
    } else if group.high > 0 {
        "#fbbf24"
    } else if group.medium > 0 {
        "#60a5fa"
    } else {
        "#9ca3af"
    }
}

fn severity_chip_class(severity: &CveSeverity) -> &'static str {
    match severity {
        CveSeverity::Critical => "chip chip-critical",
        CveSeverity::High => "chip chip-warning",
        CveSeverity::Medium => "chip chip-unknown",
        CveSeverity::Low => "chip chip-unknown",
    }
}

fn severity_rank(severity: &CveSeverity) -> i32 {
    match severity {
        CveSeverity::Critical => 4,
        CveSeverity::High => 3,
        CveSeverity::Medium => 2,
        CveSeverity::Low => 1,
    }
}

fn normalize_status(value: Option<&str>) -> String {
    match value.unwrap_or("open") {
        "open" => "open".to_string(),
        "fix_available" => "fix_available".to_string(),
        "mixed" => "mixed".to_string(),
        other => other.trim().to_lowercase(),
    }
}

fn reconcile_group_status(current: &str, next: Option<&str>) -> String {
    let next_status = normalize_status(next);
    if current == "mixed" || next_status == "mixed" {
        return "mixed".to_string();
    }
    if current == next_status {
        return current.to_string();
    }
    "mixed".to_string()
}

fn status_label(status: &str) -> &'static str {
    match status {
        "fix_available" => "Fix available",
        "open" => "No known fix",
        "mixed" => "Mixed package status",
        _ => "Status unknown",
    }
}

fn justification_category_class(category: &str) -> &'static str {
    match category {
        "false_positive" => "text-violet-300 bg-violet-500/15 border border-violet-500/35",
        "accepted_risk" => "text-amber-300 bg-amber-500/15 border border-amber-500/35",
        "compensating_control" => "text-blue-300 bg-blue-500/15 border border-blue-500/35",
        "planned_remediation" => "text-emerald-300 bg-emerald-500/15 border border-emerald-500/35",
        "vendor_pending_fix" => "text-orange-300 bg-orange-500/15 border border-orange-500/35",
        _ => "text-slate-300 bg-slate-500/15 border border-slate-500/35",
    }
}

fn humanize_category(category: &str) -> String {
    category
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_count(value: i64) -> String {
    let negative = value < 0;
    let reversed: Vec<char> = value.abs().to_string().chars().rev().collect();
    let mut grouped = String::new();

    for (idx, ch) in reversed.iter().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(*ch);
    }

    let mut formatted: String = grouped.chars().rev().collect();
    if negative {
        formatted.insert(0, '-');
    }
    formatted
}
