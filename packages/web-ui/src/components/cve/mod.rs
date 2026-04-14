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
    status: Option<String>,
    package_instances: Vec<SystemVulnerability>,
    justification_category: Option<String>,
    justification_reason: Option<String>,
    justification_updated_at: Option<chrono::DateTime<chrono::Utc>>,
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
) -> Element {
    let mut severity_filter = use_signal(|| "all".to_string());
    let mut cve_search = use_signal(String::new);
    let mut package_search = use_signal(String::new);
    let mut description_search = use_signal(String::new);
    let mut expanded_cve: Signal<Option<String>> = use_signal(|| None);

    let mut editing_cve: Signal<Option<String>> = use_signal(|| None);
    let mut draft_category: Signal<Option<String>> = use_signal(|| None);
    let mut draft_reason: Signal<String> = use_signal(String::new);
    let mut save_status: Signal<Option<String>> = use_signal(|| None);
    let mut save_in_progress = use_signal(|| false);

    let grouped = group_vulnerabilities_by_cve(&vulnerabilities);

    let filtered = grouped
        .into_iter()
        .filter(|group| {
            let severity_ok = match severity_filter.read().as_str() {
                "critical" => group.severity == CveSeverity::Critical,
                "high" => group.severity == CveSeverity::High,
                "medium" => group.severity == CveSeverity::Medium,
                "low" => group.severity == CveSeverity::Low,
                _ => true,
            };

            if !severity_ok {
                return false;
            }

            let cve_query = cve_search.read().trim().to_lowercase();
            if !cve_query.is_empty() && !group.cve_id.to_lowercase().contains(&cve_query) {
                return false;
            }

            let package_query = package_search.read().trim().to_lowercase();
            if !package_query.is_empty()
                && !group.package_instances.iter().any(|item| {
                    item.package_name.to_lowercase().contains(&package_query)
                        || item
                            .installed_version
                            .to_lowercase()
                            .contains(&package_query)
                })
            {
                return false;
            }

            let desc_query = description_search.read().trim().to_lowercase();
            if !desc_query.is_empty() && !group.description.to_lowercase().contains(&desc_query) {
                return false;
            }

            true
        })
        .collect::<Vec<_>>();

    let has_active_filters = !cve_search.read().trim().is_empty()
        || !package_search.read().trim().is_empty()
        || !description_search.read().trim().is_empty()
        || severity_filter.read().as_str() != "all";

    let status_is_error = save_status
        .read()
        .as_ref()
        .map(|message| message.starts_with("Failed") || message.contains("required"))
        .unwrap_or(false);

    rsx! {
        div {
            class: "pt-6 space-y-5",

            div {
                class: "rounded-xl border border-slate-800/90 bg-gradient-to-br from-slate-950 to-slate-900 p-4 md:p-5",

                div { class: "flex flex-wrap items-end justify-between gap-4",
                    div {
                        class: "space-y-1",
                        div { class: "text-xs uppercase tracking-[0.14em] text-slate-500", "System CVE review" }
                        div { class: "flex items-baseline gap-3",
                            span {
                                class: "text-3xl font-bold text-white",
                                "{cve_counts.total()}"
                            }
                            span {
                                class: "{theme::text::SECONDARY}",
                                "known vulnerabilities"
                            }
                        }
                    }

                    div { class: "flex items-center gap-2 flex-wrap",
                        span {
                            class: "text-xs px-2.5 py-1 rounded-md border border-slate-700 bg-slate-900 text-slate-300",
                            "{filtered.len()} grouped CVEs"
                        }
                        span { class: "text-xs px-2.5 py-1 rounded-md border border-red-800/60 bg-red-950/40 text-red-300", "Critical {cve_counts.critical}" }
                        span { class: "text-xs px-2.5 py-1 rounded-md border border-orange-800/60 bg-orange-950/40 text-orange-300", "High {cve_counts.high}" }
                        span { class: "text-xs px-2.5 py-1 rounded-md border border-yellow-800/60 bg-yellow-950/40 text-yellow-300", "Medium {cve_counts.medium}" }
                        span { class: "text-xs px-2.5 py-1 rounded-md border border-blue-800/60 bg-blue-950/40 text-blue-300", "Low {cve_counts.low}" }
                    }
                }
            }

            // Filters
            div {
                class: "rounded-xl border border-slate-800/90 bg-slate-950/50 p-4 space-y-3",

                div { class: "flex items-center justify-between gap-3 flex-wrap",
                    div { class: "text-xs uppercase tracking-[0.12em] text-slate-500", "Filters" }
                    button {
                        class: "px-3 py-1.5 rounded-md border border-slate-700 text-xs font-medium text-slate-300 hover:bg-slate-800/80 transition-colors disabled:opacity-40",
                        disabled: !has_active_filters,
                        onclick: move |_| {
                            cve_search.set(String::new());
                            package_search.set(String::new());
                            description_search.set(String::new());
                            severity_filter.set("all".to_string());
                        },
                        "Reset filters"
                    }
                }

                div {
                    class: "grid grid-cols-1 md:grid-cols-4 gap-3",
                    input {
                        class: "h-10 px-3 rounded-lg border border-slate-700 bg-slate-900/90 text-sm text-white placeholder:text-slate-500 focus:outline-none focus:ring-2 focus:ring-blue-600/60",
                        placeholder: "Search CVE ID (e.g. CVE-2025)",
                        value: cve_search.read().clone(),
                        oninput: move |evt| cve_search.set(evt.value()),
                    }
                    input {
                        class: "h-10 px-3 rounded-lg border border-slate-700 bg-slate-900/90 text-sm text-white placeholder:text-slate-500 focus:outline-none focus:ring-2 focus:ring-blue-600/60",
                        placeholder: "Filter package/version",
                        value: package_search.read().clone(),
                        oninput: move |evt| package_search.set(evt.value()),
                    }
                    input {
                        class: "h-10 px-3 rounded-lg border border-slate-700 bg-slate-900/90 text-sm text-white placeholder:text-slate-500 focus:outline-none focus:ring-2 focus:ring-blue-600/60",
                        placeholder: "Search description",
                        value: description_search.read().clone(),
                        oninput: move |evt| description_search.set(evt.value()),
                    }
                    select {
                        class: "h-10 px-3 rounded-lg border border-slate-700 bg-slate-900/90 text-sm text-white focus:outline-none focus:ring-2 focus:ring-blue-600/60",
                        value: severity_filter.read().clone(),
                        onchange: move |evt| severity_filter.set(evt.value()),
                        option { value: "all", "All severities" }
                        option { value: "critical", "Critical" }
                        option { value: "high", "High" }
                        option { value: "medium", "Medium" }
                        option { value: "low", "Low" }
                    }
                }
            }

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

            if filtered.is_empty() {
                div {
                    class: "rounded-xl border border-slate-800 bg-slate-950/40 p-8 text-sm text-slate-400",
                    "No CVEs match current filters."
                }
            } else {
                div {
                    class: "space-y-3",
                    for group in filtered {
                        {
                            let is_expanded = *expanded_cve.read() == Some(group.cve_id.clone());
                            let has_justification = group
                                .justification_reason
                                .as_ref()
                                .map(|value| !value.trim().is_empty())
                                .unwrap_or(false);
                            let is_editing = *editing_cve.read() == Some(group.cve_id.clone());
                            let cve_id = group.cve_id.clone();
                            let package_label = format!(
                                "{} package{}",
                                group.package_instances.len(),
                                if group.package_instances.len() == 1 {
                                    ""
                                } else {
                                    "s"
                                }
                            );
                            let published_label = group
                                .published_at
                                .map(|published_at| published_at.format("%Y-%m-%d").to_string());
                            let justification_updated_label = group
                                .justification_updated_at
                                .map(|updated_at| updated_at.format("%Y-%m-%d %H:%M").to_string());
                            let chevron_class = if is_expanded { "rotate-180" } else { "" };

                            rsx! {
                                div {
                                    key: "{group.cve_id}",
                                    class: "rounded-xl border border-slate-800/90 bg-slate-950/40 overflow-hidden shadow-[0_6px_24px_rgba(0,0,0,0.25)]",

                                    button {
                                        class: "w-full p-4 text-left hover:bg-slate-900/60 transition-colors",
                                        onclick: move |_| {
                                            let current = expanded_cve.read().clone();
                                            if current == Some(cve_id.clone()) {
                                                expanded_cve.set(None);
                                            } else {
                                                expanded_cve.set(Some(cve_id.clone()));
                                            }
                                        },

                                        div { class: "flex items-start justify-between gap-4",
                                            div { class: "min-w-0",
                                                div { class: "flex items-center gap-2 flex-wrap",
                                                    span { class: "font-mono text-sm font-semibold text-white tracking-wide", "{group.cve_id}" }
                                                    span {
                                                        class: "text-xs px-2 py-0.5 rounded-md border border-slate-700 bg-slate-900 text-slate-300",
                                                        "{package_label}"
                                                    }
                                                    span {
                                                        class: "text-xs px-2 py-0.5 rounded-md {group.severity.bg_class()} text-white",
                                                        "{group.severity.label()}"
                                                    }
                                                    if has_justification {
                                                        span {
                                                            class: "text-xs px-2 py-0.5 rounded-md bg-emerald-700/30 text-emerald-300 border border-emerald-600/40",
                                                            "Justified"
                                                        }
                                                    }
                                                }

                                                p { class: "mt-2 text-sm text-slate-300 line-clamp-2", "{group.description}" }
                                            }

                                            div { class: "text-right shrink-0 space-y-1",
                                                if let Some(score) = group.cvss_score {
                                                    div { class: "text-lg font-bold {group.severity.color_class()}", "{score:.1}" }
                                                    div { class: "text-xs text-slate-500", "CVSS" }
                                                }
                                                div {
                                                    class: "text-slate-500",
                                                    svg {
                                                        class: "w-4 h-4 inline-block transition-transform {chevron_class}",
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
                                    }

                                    if is_expanded {
                                        div {
                                            class: "border-t border-slate-800 p-4 space-y-4 bg-slate-950/40",

                                            div {
                                                class: "flex items-center gap-3 flex-wrap text-sm",
                                                a {
                                                    class: "text-blue-400 hover:text-blue-300 underline underline-offset-2",
                                                    href: "https://nvd.nist.gov/vuln/detail/{group.cve_id}",
                                                    target: "_blank",
                                                    rel: "noopener noreferrer",
                                                    "View on NVD"
                                                }
                                                if let Some(ref published_label) = published_label {
                                                    span { class: "text-slate-400", "Published: {published_label}" }
                                                }
                                                if let Some(ref status) = group.status {
                                                    span { class: "text-slate-400", "Status: {status}" }
                                                }
                                            }

                                            div {
                                                class: "space-y-2",
                                                h4 { class: "text-xs uppercase tracking-[0.12em] text-slate-500", "Affected packages" }
                                                for item in group.package_instances.iter() {
                                                    div {
                                                        key: "{group.cve_id}-{item.package_name}-{item.installed_version}",
                                                        class: "text-sm text-slate-300 flex items-center gap-3 flex-wrap rounded-lg border border-slate-800 bg-slate-900/35 px-3 py-2",
                                                        span { class: "font-medium text-white", "{item.package_name}" }
                                                        span { class: "text-slate-500", "Installed: {item.installed_version}" }
                                                        if let Some(ref fixed) = item.fixed_version {
                                                            span { class: "text-emerald-400", "Fix: {fixed}" }
                                                        } else {
                                                            span { class: "text-slate-500", "No fix yet" }
                                                        }
                                                    }
                                                }
                                            }

                                            div {
                                                class: "rounded-xl border border-slate-800 bg-slate-900/35 p-4 space-y-3",
                                                div { class: "flex items-center justify-between gap-3",
                                                    h4 { class: "text-sm font-semibold text-white", "Justification" }
                                                    if let Some(ref justification_updated_label) = justification_updated_label {
                                                        span { class: "text-xs text-slate-500", "Updated {justification_updated_label}" }
                                                    }
                                                }

                                                if !is_editing {
                                                    if let Some(ref reason) = group.justification_reason {
                                                        div { class: "text-sm text-slate-200 leading-relaxed", "{reason}" }
                                                        if let Some(ref category) = group.justification_category {
                                                            div {
                                                                class: "inline-flex text-xs rounded-md border border-emerald-700/40 bg-emerald-950/30 px-2 py-1 text-emerald-300",
                                                                "Category: {category}"
                                                            }
                                                        }
                                                    } else {
                                                        div { class: "text-sm text-slate-500", "No justification saved yet." }
                                                    }

                                                    button {
                                                        class: "px-3 py-2 rounded-md border border-slate-700 bg-slate-800 hover:bg-slate-700 text-sm font-medium text-white transition-colors disabled:opacity-50",
                                                        disabled: !allow_mutations,
                                                        onclick: {
                                                            let cve_id = group.cve_id.clone();
                                                            let existing_category = group.justification_category.clone();
                                                            let existing_reason = group.justification_reason.clone().unwrap_or_default();
                                                            move |_| {
                                                                editing_cve.set(Some(cve_id.clone()));
                                                                draft_category.set(existing_category.clone());
                                                                draft_reason.set(existing_reason.clone());
                                                                save_status.set(None);
                                                            }
                                                        },
                                                        if allow_mutations { "Edit justification" } else { "Operator/Admin required" }
                                                    }
                                                } else {
                                                    div { class: "space-y-2",
                                                        select {
                                                            class: "w-full px-3 py-2 rounded-lg border border-slate-700 bg-slate-900 text-sm text-white focus:outline-none focus:ring-2 focus:ring-blue-600/60",
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
                                                            class: "w-full px-3 py-2 rounded-lg border border-slate-700 bg-slate-900 text-sm text-white min-h-[100px] focus:outline-none focus:ring-2 focus:ring-blue-600/60",
                                                            placeholder: "Document risk acceptance / mitigation rationale",
                                                            value: draft_reason.read().clone(),
                                                            oninput: move |evt| draft_reason.set(evt.value()),
                                                        }

                                                        div { class: "text-xs text-slate-500", "This note is persisted per system + CVE for audit review." }

                                                        div { class: "flex items-center gap-2",
                                                            button {
                                                                class: "px-3 py-2 rounded-md bg-blue-600 hover:bg-blue-500 text-sm font-semibold text-white transition-colors disabled:opacity-50",
                                                                disabled: *save_in_progress.read(),
                                                                onclick: {
                                                                    let cve_id = group.cve_id.clone();
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
                                                                if *save_in_progress.read() { "Saving..." } else { "Save" }
                                                            }
                                                            button {
                                                                class: "px-3 py-2 rounded-md border border-slate-700 bg-slate-800 hover:bg-slate-700 text-sm text-white transition-colors",
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
                status: vuln.status.clone(),
                package_instances: Vec::new(),
                justification_category: vuln.justification_category.clone(),
                justification_reason: vuln.justification_reason.clone(),
                justification_updated_at: vuln.justification_updated_at,
            });

        entry.package_instances.push(vuln.clone());

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
    }

    let mut groups = grouped.into_values().collect::<Vec<_>>();
    groups.sort_by(|a, b| {
        b.cvss_score
            .partial_cmp(&a.cvss_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.cve_id.cmp(&b.cve_id))
    });
    groups
}

fn severity_rank(severity: &CveSeverity) -> i32 {
    match severity {
        CveSeverity::Critical => 4,
        CveSeverity::High => 3,
        CveSeverity::Medium => 2,
        CveSeverity::Low => 1,
    }
}
