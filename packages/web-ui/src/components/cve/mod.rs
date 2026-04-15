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
                class: "{theme::presets::CARD}",

                div { class: "flex flex-wrap items-end justify-between gap-4",
                    div {
                        class: "space-y-1",
                        div { class: "{theme::typography::TABLE_HEADER}", "System CVE review" }
                        div { class: "flex items-baseline gap-3",
                            span {
                                class: "{theme::typography::STAT_VALUE} {theme::text::PRIMARY}",
                                "{cve_counts.total()}"
                            }
                            span {
                                class: "{theme::text::SECONDARY}",
                                "known vulnerabilities"
                            }
                        }
                    }

                    div { class: "flex items-center gap-1.5 flex-wrap",
                        span {
                            class: "text-[11px] px-2 py-0.5 rounded-md border {theme::surface::CARD_BORDER} {theme::surface::SUBTLE_BG} {theme::text::SECONDARY}",
                            "{format_count(filtered.len() as i64)} grouped CVEs"
                        }
                        span { class: "inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium {CveSeverity::Critical.color_class()} {CveSeverity::Critical.bg_class()}", "Critical {format_count(cve_counts.critical)}" }
                        span { class: "inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium {CveSeverity::High.color_class()} {CveSeverity::High.bg_class()}", "High {format_count(cve_counts.high)}" }
                        span { class: "inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium {CveSeverity::Medium.color_class()} {CveSeverity::Medium.bg_class()}", "Medium {format_count(cve_counts.medium)}" }
                        span { class: "inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium {CveSeverity::Low.color_class()} {CveSeverity::Low.bg_class()}", "Low {format_count(cve_counts.low)}" }
                    }
                }
            }

            // Filters
            div {
                class: "{theme::presets::CARD} space-y-3",

                div { class: "flex items-center justify-between gap-3 flex-wrap",
                    div { class: "{theme::typography::TABLE_HEADER}", "Filters" }
                    button {
                        class: "px-3 py-1.5 rounded-md border {theme::surface::CARD_BORDER} text-xs font-medium {theme::text::SECONDARY} {theme::interactive::HOVER_BG} transition-colors disabled:opacity-40 {theme::interactive::FOCUS_RING}",
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
                        class: "{theme::interactive::INPUT} h-10",
                        placeholder: "Search CVE ID (e.g. CVE-2025)",
                        value: cve_search.read().clone(),
                        oninput: move |evt| cve_search.set(evt.value()),
                    }
                    input {
                        class: "{theme::interactive::INPUT} h-10",
                        placeholder: "Filter package/version",
                        value: package_search.read().clone(),
                        oninput: move |evt| package_search.set(evt.value()),
                    }
                    input {
                        class: "{theme::interactive::INPUT} h-10",
                        placeholder: "Search description",
                        value: description_search.read().clone(),
                        oninput: move |evt| description_search.set(evt.value()),
                    }
                    select {
                        class: "{theme::interactive::INPUT} h-10",
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
                    class: "{theme::presets::CARD} text-sm {theme::text::MUTED}",
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
                                    class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} overflow-hidden",

                                    button {
                                        class: "w-full p-3 text-left {theme::interactive::HOVER_BG} transition-colors {theme::interactive::FOCUS_RING}",
                                        onclick: move |_| {
                                            let current = expanded_cve.read().clone();
                                            if current == Some(cve_id.clone()) {
                                                expanded_cve.set(None);
                                            } else {
                                                expanded_cve.set(Some(cve_id.clone()));
                                            }
                                        },

                                        div { class: "flex items-start justify-between gap-3",
                                            div { class: "min-w-0",
                                                div { class: "flex items-center gap-2 flex-wrap",
                                                    span { class: "{theme::typography::MONO} font-semibold {theme::text::PRIMARY} tracking-wide", "{group.cve_id}" }
                                                    span {
                                                        class: "text-xs px-2 py-0.5 rounded-md border {theme::surface::CARD_BORDER} {theme::surface::SUBTLE_BG} {theme::text::SECONDARY}",
                                                        "{package_label}"
                                                    }
                                                    span {
                                                        class: "inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium {group.severity.color_class()} {group.severity.bg_class()}",
                                                        "{group.severity.label()}"
                                                    }
                                                    if has_justification {
                                                        span {
                                                            class: "inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-semibold text-emerald-100 bg-emerald-600/80",
                                                            "Justified"
                                                        }
                                                    }
                                                }

                                                p { class: "mt-1 text-sm {theme::text::SECONDARY} line-clamp-1", "{group.description}" }

                                                if let Some(ref reason) = group.justification_reason {
                                                    p {
                                                        class: "mt-1 text-xs text-emerald-300/90 line-clamp-1",
                                                        "Justification: {reason}"
                                                    }
                                                }
                                            }

                                            div { class: "text-right shrink-0 space-y-1",
                                                if let Some(score) = group.cvss_score {
                                                    div { class: "text-base font-bold {group.severity.color_class()}", "{score:.1}" }
                                                    div { class: "text-xs {theme::text::MUTED}", "CVSS" }
                                                }
                                                div {
                                                    class: "{theme::text::MUTED}",
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

                                    div { class: "px-3 pb-2 text-xs {theme::text::MUTED}", "{status_label(&group.status)}" }

                                    if is_expanded {
                                        div {
                                            class: "border-t {theme::surface::DIVIDER} p-3 space-y-3 {theme::surface::SUBTLE_BG}",

                                            div {
                                                class: "flex items-center gap-3 flex-wrap text-sm",
                                                a {
                                                    class: "text-blue-400 hover:text-blue-300 underline underline-offset-2 {theme::interactive::FOCUS_RING}",
                                                    href: "https://nvd.nist.gov/vuln/detail/{group.cve_id}",
                                                    target: "_blank",
                                                    rel: "noopener noreferrer",
                                                    "View on NVD"
                                                }
                                                if let Some(ref published_label) = published_label {
                                                    span { class: "{theme::text::MUTED}", "Published: {published_label}" }
                                                }
                                                span { class: "{theme::text::MUTED}", "Status: {status_label(&group.status)}" }
                                            }

                                            div {
                                                class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} p-3 space-y-2",
                                                div { class: "flex items-center justify-between gap-3",
                                                    h4 { class: "text-sm font-semibold {theme::text::PRIMARY}", "Justification" }
                                                    if let Some(ref justification_updated_label) = justification_updated_label {
                                                        span { class: "text-xs {theme::text::MUTED}", "Updated {justification_updated_label}" }
                                                    }
                                                }

                                                if !is_editing {
                                                    if let Some(ref reason) = group.justification_reason {
                                                        div { class: "text-sm {theme::text::SECONDARY} leading-relaxed", "{reason}" }
                                                        if let Some(ref category) = group.justification_category {
                                                            span {
                                                                class: "inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium {justification_category_class(category)}",
                                                                "Category: {humanize_category(category)}"
                                                            }
                                                        }
                                                    } else {
                                                        div { class: "text-sm {theme::text::MUTED}", "No justification saved yet." }
                                                    }

                                                    button {
                                                        class: "px-3 py-2 rounded-md border {theme::surface::CARD_BORDER} {theme::surface::SUBTLE_BG} {theme::interactive::HOVER_BG} text-sm font-medium {theme::text::PRIMARY} transition-colors disabled:opacity-50 {theme::interactive::FOCUS_RING}",
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
                                                                class: "px-3 py-2 rounded-md border {theme::surface::CARD_BORDER} {theme::surface::SUBTLE_BG} {theme::interactive::HOVER_BG} text-sm {theme::text::PRIMARY} transition-colors {theme::interactive::FOCUS_RING}",
                                                                onclick: move |_| editing_cve.set(None),
                                                                "Cancel"
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            div {
                                                class: "space-y-2",
                                                h4 { class: "{theme::typography::TABLE_HEADER}", "Affected packages" }
                                                for item in group.package_instances.iter() {
                                                    div {
                                                        key: "{group.cve_id}-{item.package_name}-{item.installed_version}",
                                                        class: "text-sm {theme::text::SECONDARY} flex items-center gap-2.5 flex-wrap rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} px-2.5 py-1.5",
                                                        span { class: "font-medium {theme::text::PRIMARY}", "{item.package_name}" }
                                                        span { class: "{theme::text::MUTED}", "Installed: {item.installed_version}" }
                                                        if let Some(ref fixed) = item.fixed_version {
                                                            span { class: "text-emerald-400", "Fix: {fixed}" }
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
