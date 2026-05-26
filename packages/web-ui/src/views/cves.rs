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
use crate::api::models::{
    CveAffectedSystemDetail, CveDetail, CveFilters, CveFleetStats, CveJustification,
    CveJustificationInput, CveListItem, CvePackageGroup,
};
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
    let mut selected_cve_id = use_signal(|| Option::<String>::None);

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

            // Action Buttons
            div {
                class: "flex gap-2",
                button {
                    class: "px-4 py-2 text-sm rounded-md border border-white/15 hover:bg-white/5 flex items-center gap-2",
                    onclick: move |_| {
                        spawn(async move {
                            match client::trigger_cve_fleet_rescan().await {
                                Ok(_) => {
                                    // TODO: Toast notification "Fleet rescan initiated"
                                }
                                Err(e) => {
                                    // TODO: Toast notification "Rescan failed: {e}"
                                }
                            }
                        });
                    },
                    "🔄 Rescan fleet"
                }
                button {
                    class: "px-4 py-2 text-sm rounded-md border border-white/15 hover:bg-white/5 flex items-center gap-2",
                    onclick: move |_| {
                        spawn(async move {
                            match client::export_cves_csv(&CveFilters {
                                severity: severity_filter(),
                                fix_status: fix_status_filter(),
                                triage_status: triage_status_filter(),
                                package: package_filter(),
                                search: if search_query().is_empty() { None } else { Some(search_query()) },
                                sort: Some(sort_by()),
                                limit: None, // Export all
                            }).await {
                                Ok(_) => {
                                    // TODO: Toast notification "CSV export started"
                                }
                                Err(_e) => {
                                    // TODO: Toast notification "Export failed: {e}"
                                }
                            }
                        });
                    },
                    "📥 Export CSV"
                }
            }

            // View Mode Toggle
            div {
                class: "flex gap-2",
                span {
                    class: "text-xs {theme::text::SECONDARY} self-center mr-2",
                    "View:"
                }
                button {
                    class: if view_mode() == "flat" {
                        "px-3 py-1.5 rounded-md text-xs font-medium border bg-violet-600/20 border-violet-400/50 text-violet-100"
                    } else {
                        "px-3 py-1.5 rounded-md text-xs font-medium border border-white/15 text-gray-300 hover:bg-white/5"
                    },
                    onclick: move |_| view_mode.set("flat".to_string()),
                    "Flat list"
                }
                button {
                    class: if view_mode() == "grouped" {
                        "px-3 py-1.5 rounded-md text-xs font-medium border bg-violet-600/20 border-violet-400/50 text-violet-100"
                    } else {
                        "px-3 py-1.5 rounded-md text-xs font-medium border border-white/15 text-gray-300 hover:bg-white/5"
                    },
                    onclick: move |_| view_mode.set("grouped".to_string()),
                    "By package"
                }
            }

            // CVE List
            if view_mode() == "grouped" {
                CvePackageGroupsView {
                    filters: CveFilters {
                        severity: severity_filter(),
                        fix_status: fix_status_filter(),
                        triage_status: triage_status_filter(),
                        package: package_filter(),
                        search: if search_query().is_empty() { None } else { Some(search_query()) },
                        sort: Some(sort_by()),
                        limit: Some(100),
                    }
                }
            } else {
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
                                                    th { class: "py-2 pr-3 text-right", " " }
                                                }
                                            }
                                            tbody {
                                                for cve in cves {
                                                    CveRow {
                                                        cve: cve.clone(),
                                                        on_open: move |cve_id: String| {
                                                            selected_cve_id.set(Some(cve_id));
                                                        }
                                                    }
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

            // CVE Detail Drawer
            if let Some(cve_id) = selected_cve_id() {
                CveDrawer {
                    cve_id: cve_id.clone(),
                    on_close: move |_| selected_cve_id.set(None)
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Flat List Components
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn CveRow(cve: CveListItem, on_open: EventHandler<String>) -> Element {
    let severity_color = match cve.severity.to_uppercase().as_str() {
        "CRITICAL" => theme::cve::CRITICAL_TEXT,
        "HIGH" => theme::cve::HIGH_TEXT,
        "MEDIUM" => theme::cve::MEDIUM_TEXT,
        _ => theme::cve::LOW_TEXT,
    };
    
    let cve_id_for_onclick = cve.cve_id.clone();

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
                "{cve.package_name.as_deref().unwrap_or(\"\")}"
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

            // Actions
            td {
                class: "py-2 pr-3 text-right",
                button {
                    class: "px-2 py-1 text-xs rounded hover:bg-white/10",
                    onclick: move |_| on_open.call(cve_id_for_onclick.clone()),
                    "→"
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Grouped View Components
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn CvePackageGroupsView(filters: CveFilters) -> Element {
    let grouped_cves = use_resource(move || {
        let f = filters.clone();
        async move { client::fetch_cves_grouped(&f).await }
    });

    rsx! {
        div {
            class: "space-y-4",
            match &*grouped_cves.read_unchecked() {
                Some(Ok(groups)) => rsx! {
                    if groups.is_empty() {
                        Card {
                            children: rsx! {
                                div {
                                    class: "p-8 text-center {theme::text::SECONDARY}",
                                    "No CVE packages match the current filters."
                                }
                            }
                        }
                    } else {
                        for group in groups {
                            CvePackageGroupCard { group: group.clone() }
                        }
                    }
                },
                Some(Err(err)) => rsx! {
                    Card {
                        children: rsx! {
                            div {
                                class: "p-8 text-center {theme::text::SECONDARY}",
                                "Error loading CVE groups: {err}"
                            }
                        }
                    }
                },
                None => rsx! {
                    Card {
                        children: rsx! {
                            div {
                                class: "p-8 text-center {theme::text::SECONDARY}",
                                "Loading CVE groups..."
                            }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn CvePackageGroupCard(group: CvePackageGroup) -> Element {
    let mut is_expanded = use_signal(|| false);

    let severity_color = if group.critical_count > 0 {
        theme::cve::CRITICAL_TEXT
    } else if group.high_count > 0 {
        theme::cve::HIGH_TEXT
    } else if group.medium_count > 0 {
        theme::cve::MEDIUM_TEXT
    } else {
        theme::cve::LOW_TEXT
    };

    rsx! {
        Card {
            children: rsx! {
                div {
                    class: "overflow-hidden",

                    // Header button
                    button {
                        class: "w-full p-4 flex items-center gap-4 hover:bg-white/5 text-left",
                        style: if is_expanded() { "background: rgba(124, 58, 237, 0.1);" } else { "" },
                        onclick: move |_| is_expanded.set(!is_expanded()),

                        // Chevron
                        span {
                            class: "text-xs {theme::text::SECONDARY}",
                            if is_expanded() { "▼" } else { "▶" }
                        }

                        // Package name
                        div {
                            class: "flex-1 min-w-0",
                            div {
                                class: "font-mono font-bold text-sm",
                                "{group.package_name}"
                            }
                            div {
                                class: "text-xs {theme::text::SECONDARY} mt-1",
                                {
                                    let cve_plural = if group.cve_count == 1 { "" } else { "s" };
                                    format!("{} CVE{} · {} systems affected · {} patchable · {} outstanding",
                                        group.cve_count, cve_plural, group.total_affected_systems,
                                        group.fixable_count, group.outstanding_count)
                                }
                            }
                        }

                        // Severity chips
                        div {
                            class: "flex gap-2 flex-wrap",
                            if group.critical_count > 0 {
                                span {
                                    class: "px-2 py-1 text-xs rounded-md {theme::cve::CRITICAL_TEXT} bg-red-500/20",
                                    "{group.critical_count} crit"
                                }
                            }
                            if group.high_count > 0 {
                                span {
                                    class: "px-2 py-1 text-xs rounded-md {theme::cve::HIGH_TEXT} bg-orange-500/20",
                                    "{group.high_count} high"
                                }
                            }
                            if group.medium_count > 0 {
                                span {
                                    class: "px-2 py-1 text-xs rounded-md {theme::cve::MEDIUM_TEXT} bg-blue-500/20",
                                    "{group.medium_count} med"
                                }
                            }
                            if group.low_count > 0 {
                                span {
                                    class: "px-2 py-1 text-xs rounded-md {theme::cve::LOW_TEXT} bg-gray-500/20",
                                    "{group.low_count} low"
                                }
                            }
                            if group.exploited_count > 0 {
                                span {
                                    class: "px-2 py-1 text-xs rounded-md bg-red-500/30 text-red-200 border border-red-500/50",
                                    "{group.exploited_count} exploited"
                                }
                            }
                        }

                        // Max CVSS
                        div {
                            class: "flex flex-col items-end gap-1 min-w-24",
                            div {
                                class: "text-xs {theme::text::SECONDARY} uppercase tracking-wide",
                                "Worst CVSS"
                            }
                            if let Some(cvss) = group.max_cvss {
                                div {
                                    class: "flex items-center gap-2",
                                    div {
                                        class: "w-12 h-1 rounded-full bg-white/10 overflow-hidden",
                                        div {
                                            class: "h-full {severity_color}",
                                            style: "width: {cvss * 10.0}%",
                                        }
                                    }
                                    span {
                                        class: "font-mono text-sm font-semibold",
                                        "{cvss:.1}"
                                    }
                                }
                            }
                        }
                    }

                    // Expanded CVE list
                    if is_expanded() {
                        if let Some(cves) = &group.cves {
                            div {
                                class: "border-t border-white/10",
                                table {
                                    class: "min-w-full text-sm",
                                    thead {
                                        tr {
                                            class: "border-b border-white/5 text-left {theme::text::SECONDARY}",
                                            th { class: "py-2 px-3", "CVE" }
                                            th { class: "py-2 px-3", "Severity" }
                                            th { class: "py-2 px-3", "CVSS" }
                                            th { class: "py-2 px-3", "Title" }
                                            th { class: "py-2 px-3", "Affected" }
                                            th { class: "py-2 px-3", "Fix" }
                                            th { class: "py-2 px-3", "Triage" }
                                            th { class: "py-2 px-3", "Age" }
                                        }
                                    }
                                    tbody {
                                        for cve in cves {
                                            CveRowInGroup { cve: cve.clone() }
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

#[component]
fn CveRowInGroup(cve: CveListItem) -> Element {
    let severity_color = match cve.severity.to_uppercase().as_str() {
        "CRITICAL" => theme::cve::CRITICAL_TEXT,
        "HIGH" => theme::cve::HIGH_TEXT,
        "MEDIUM" => theme::cve::MEDIUM_TEXT,
        _ => theme::cve::LOW_TEXT,
    };

    rsx! {
        tr {
            class: "border-b border-white/5 hover:bg-white/5",

            td {
                class: "py-2 px-3 font-mono text-sm font-medium",
                "{cve.cve_id}"
                if cve.exploited {
                    span {
                        class: "ml-2 px-2 py-0.5 text-xs rounded bg-red-500/20 text-red-300 border border-red-500/30",
                        "exploited"
                    }
                }
            }

            td {
                class: "py-2 px-3",
                span {
                    class: "px-2 py-1 text-xs rounded-md {severity_color}",
                    "{cve.severity}"
                }
            }

            td {
                class: "py-2 px-3",
                if let Some(cvss) = cve.cvss_v3_score {
                    span {
                        class: "font-mono text-sm",
                        "{cvss:.1}"
                    }
                }
            }

            td {
                class: "py-2 px-3 max-w-xs truncate",
                title: "{cve.title}",
                "{cve.title}"
            }

            td {
                class: "py-2 px-3 font-mono text-xs",
                "{cve.affected_count}"
            }

            td {
                class: "py-2 px-3",
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

            td {
                class: "py-2 px-3",
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

            td {
                class: "py-2 px-3 text-xs {theme::text::SECONDARY}",
                "{cve.age_days}d"
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CVE Detail Drawer
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn CveDrawer(cve_id: String, on_close: EventHandler<()>) -> Element {
    let cve_id_detail = cve_id.clone();
    let cve_detail = use_resource(move || {
        let id = cve_id_detail.clone();
        async move { client::fetch_cve_detail(&id).await }
    });

    let cve_id_systems = cve_id.clone();
    let affected_systems = use_resource(move || {
        let id = cve_id_systems.clone();
        async move { client::fetch_cve_systems(&id).await }
    });

    let cve_id_justs = cve_id.clone();
    let justifications = use_resource(move || {
        let id = cve_id_justs.clone();
        async move { client::fetch_cve_justifications(&id).await }
    });

    rsx! {
        // Backdrop
        div {
            class: "fixed inset-0 bg-black/50 z-40",
            onclick: move |_| on_close.call(()),
        }

        // Drawer panel
        aside {
            class: "fixed top-0 right-0 h-full w-full max-w-2xl bg-gray-900 border-l border-white/10 z-50 flex flex-col shadow-xl",

            // Header
            header {
                class: "flex items-center justify-between p-4 border-b border-white/10",
                match &*cve_detail.read_unchecked() {
                    Some(Ok(detail)) => rsx! {
                        div {
                            class: "flex items-center gap-3 flex-1 min-w-0",
                            span { class: "text-xl", "🛡️" }
                            div {
                                class: "min-w-0",
                                div {
                                    class: "flex items-center gap-2 flex-wrap",
                                    span {
                                        class: "font-mono font-bold text-base",
                                        "{detail.cve_id}"
                                    }
                                    {
                                        let sev_class = match detail.severity.to_uppercase().as_str() {
                                            "CRITICAL" => format!("px-2 py-1 text-xs rounded-md {} bg-red-500/20", theme::cve::CRITICAL_TEXT),
                                            "HIGH" => format!("px-2 py-1 text-xs rounded-md {} bg-orange-500/20", theme::cve::HIGH_TEXT),
                                            "MEDIUM" => format!("px-2 py-1 text-xs rounded-md {} bg-blue-500/20", theme::cve::MEDIUM_TEXT),
                                            _ => format!("px-2 py-1 text-xs rounded-md {} bg-gray-500/20", theme::cve::LOW_TEXT),
                                        };
                                        rsx! {
                                            span {
                                                class: "{sev_class}",
                                                "{detail.severity}"
                                            }
                                        }
                                    }
                                    if detail.exploited {
                                        span {
                                            class: "px-2 py-1 text-xs rounded bg-red-500/30 text-red-200 border border-red-500/50",
                                            "exploited in the wild"
                                        }
                                    }
                                }
                                div {
                                    class: "text-xs {theme::text::SECONDARY} mt-1",
                                    "{detail.title}"
                                }
                            }
                        }
                    },
                    _ => rsx! {
                        span { class: "font-mono font-bold", "{cve_id}" }
                    }
                }
                button {
                    class: "px-2 py-1 text-sm rounded hover:bg-white/10",
                    onclick: move |_| on_close.call(()),
                    "✕"
                }
            }

            // Stats band
            match &*cve_detail.read_unchecked() {
                Some(Ok(detail)) => rsx! {
                    div {
                        class: "grid grid-cols-5 gap-4 p-4 border-b border-white/10 bg-white/5",
                        div {
                            class: "text-center",
                            div { class: "text-xs {theme::text::SECONDARY} uppercase tracking-wide", "CVSS" }
                            {
                                let cvss_class = match detail.severity.to_uppercase().as_str() {
                                    "CRITICAL" => format!("text-lg font-bold mt-1 {}", theme::cve::CRITICAL_TEXT),
                                    "HIGH" => format!("text-lg font-bold mt-1 {}", theme::cve::HIGH_TEXT),
                                    "MEDIUM" => format!("text-lg font-bold mt-1 {}", theme::cve::MEDIUM_TEXT),
                                    _ => format!("text-lg font-bold mt-1 {}", theme::cve::LOW_TEXT),
                                };
                                let cvss_text = if let Some(cvss) = detail.cvss_v3_score {
                                    format!("{:.1}", cvss)
                                } else {
                                    "N/A".to_string()
                                };
                                rsx! {
                                    div {
                                        class: "{cvss_class}",
                                        "{cvss_text}"
                                    }
                                }
                            }
                        }
                        div {
                            class: "text-center",
                            div { class: "text-xs {theme::text::SECONDARY} uppercase tracking-wide", "Package" }
                            div { class: "text-sm font-mono font-semibold mt-1", "{detail.package_name.as_deref().unwrap_or(\"N/A\")}" }
                        }
                        div {
                            class: "text-center",
                            div { class: "text-xs {theme::text::SECONDARY} uppercase tracking-wide", "Affected" }
                            div { class: "text-lg font-bold mt-1", "{affected_systems.read().as_ref().and_then(|r| r.as_ref().ok()).map(|s| s.len()).unwrap_or(0)}" }
                        }
                        div {
                            class: "text-center",
                            div { class: "text-xs {theme::text::SECONDARY} uppercase tracking-wide", "Fix" }
                            div {
                                class: "text-sm font-mono font-semibold mt-1",
                                if detail.fix_status == "fix_available" {
                                    span { class: "text-green-400", "{detail.fixed_version.as_deref().unwrap_or(\"available\")}" }
                                } else {
                                    span { class: "text-yellow-400", "pending" }
                                }
                            }
                        }
                        div {
                            class: "text-center",
                            div { class: "text-xs {theme::text::SECONDARY} uppercase tracking-wide", "Discovered" }
                            div {
                                class: "text-sm font-semibold mt-1",
                                if let Some(date) = detail.published_date {
                                    "{date}"
                                } else {
                                    "N/A"
                                }
                            }
                        }
                    }
                },
                _ => rsx! { }
            }

            // Body (scrollable)
            div {
                class: "flex-1 overflow-y-auto p-4 space-y-6",

                match &*cve_detail.read_unchecked() {
                    Some(Ok(detail)) => rsx! {
                        // CVSS Vector
                        section {
                            h3 {
                                class: "text-xs {theme::text::SECONDARY} uppercase tracking-wide mb-2 font-semibold",
                                "CVSS Vector"
                            }
                            if let Some(vector) = &detail.cvss_vector {
                                code {
                                    class: "block font-mono text-xs bg-black/30 p-3 rounded border border-white/10",
                                    "{vector}"
                                }
                            } else {
                                p { class: "text-xs {theme::text::SECONDARY}", "No CVSS vector available." }
                            }
                        }

                        // Remediation
                        section {
                            h3 {
                                class: "text-xs {theme::text::SECONDARY} uppercase tracking-wide mb-2 font-semibold",
                                "Remediation"
                            }
                            if detail.fix_status == "fix_available" {
                                div {
                                    class: "p-3 rounded border border-green-500/30 bg-green-500/10 text-sm",
                                    p {
                                        "✅ Fixed in "
                                        span { class: "font-mono font-semibold text-green-400", "{detail.package_name.as_deref().unwrap_or(\"package\")}-{detail.fixed_version.as_deref().unwrap_or(\"version\")}" }
                                        ". Affected systems will pick up the fix automatically once the upstream flake bumps the package and an eval passes."
                                    }
                                }
                            } else {
                                div {
                                    class: "p-3 rounded border border-yellow-500/30 bg-yellow-500/10 text-sm",
                                    p {
                                        class: "font-semibold",
                                        "⚠️ No upstream patch yet."
                                    }
                                    p {
                                        class: "mt-1 text-xs",
                                        "Watch the advisory for updates. Consider applying compensating controls (network isolation, WAF rule) on affected hosts."
                                    }
                                }
                            }
                            div {
                                class: "mt-3 grid grid-cols-2 gap-2 text-xs",
                                div { class: "{theme::text::SECONDARY}", "Introduced in:" }
                                div { class: "font-mono", "{detail.installed_version.as_deref().unwrap_or(\"N/A\")}" }
                                div { class: "{theme::text::SECONDARY}", "Fixed in:" }
                                div { class: "font-mono", "{detail.fixed_version.as_deref().unwrap_or(\"—\")}" }
                            }
                        }

                        // Affected Systems
                        section {
                            h3 {
                                class: "text-xs {theme::text::SECONDARY} uppercase tracking-wide mb-2 font-semibold",
                                "Affected Systems · {affected_systems.read().as_ref().and_then(|r| r.as_ref().ok()).map(|s| s.len()).unwrap_or(0)}"
                            }
                            match &*affected_systems.read_unchecked() {
                                Some(Ok(systems)) => rsx! {
                                    if systems.is_empty() {
                                        p { class: "text-xs {theme::text::SECONDARY}", "No active systems affected." }
                                    } else {
                                        AffectedSystemsList { systems: systems.clone() }
                                    }
                                },
                                Some(Err(err)) => rsx! {
                                    p { class: "text-xs text-red-400", "Error loading systems: {err}" }
                                },
                                None => rsx! {
                                    p { class: "text-xs {theme::text::SECONDARY}", "Loading systems..." }
                                }
                            }
                        }

                        // Justifications
                        section {
                            h3 {
                                class: "text-xs {theme::text::SECONDARY} uppercase tracking-wide mb-2 font-semibold",
                                "Triage Justifications"
                            }
                            match &*justifications.read_unchecked() {
                                Some(Ok(justs)) => rsx! {
                                    if justs.is_empty() {
                                        p { class: "text-xs {theme::text::SECONDARY}", "No justifications recorded." }
                                    } else {
                                        div {
                                            class: "space-y-2",
                                            for just in justs {
                                                JustificationCard { justification: just.clone() }
                                            }
                                        }
                                    }
                                },
                                Some(Err(err)) => rsx! {
                                    p { class: "text-xs text-red-400", "Error loading justifications: {err}" }
                                },
                                None => rsx! {
                                    p { class: "text-xs {theme::text::SECONDARY}", "Loading justifications..." }
                                }
                            }
                        }
                    },
                    Some(Err(err)) => rsx! {
                        p { class: "text-sm text-red-400", "Error loading CVE details: {err}" }
                    },
                    None => rsx! {
                        p { class: "text-sm {theme::text::SECONDARY}", "Loading CVE details..." }
                    }
                }
            }
        }
    }
}

#[component]
fn AffectedSystemsList(systems: Vec<CveAffectedSystemDetail>) -> Element {
    // Group by environment
    let mut by_env: std::collections::HashMap<String, Vec<CveAffectedSystemDetail>> =
        std::collections::HashMap::new();
    for sys in systems {
        let env = sys.environment.clone().unwrap_or_else(|| "unknown".to_string());
        by_env.entry(env).or_default().push(sys);
    }

    rsx! {
        div {
            class: "space-y-3",
            for (env, sys_list) in by_env.iter() {
                div {
                    div {
                        class: "text-xs {theme::text::SECONDARY} mb-1",
                        {
                            let host_plural = if sys_list.len() == 1 { "" } else { "s" };
                            format!("Environment: {} ({} host{})", env, sys_list.len(), host_plural)
                        }
                    }
                    div {
                        class: "border border-white/10 rounded overflow-hidden",
                        table {
                            class: "min-w-full text-xs",
                            tbody {
                                for sys in sys_list {
                                    tr {
                                        class: "border-b border-white/5 hover:bg-white/5",
                                        td {
                                            class: "py-2 px-3 font-mono font-semibold",
                                            "{sys.hostname}"
                                        }
                                        td {
                                            class: "py-2 px-3 font-mono {theme::text::SECONDARY}",
                                            if let Some(flake) = &sys.flake_name {
                                                "{flake}"
                                            }
                                        }
                                        td {
                                            class: "py-2 px-3 font-mono text-xs {theme::text::SECONDARY}",
                                            if let Some(commit) = &sys.commit_hash {
                                                "{&commit[..7]}"
                                            }
                                        }
                                        td {
                                            class: "py-2 px-3 font-mono text-xs",
                                            if let Some(ver) = &sys.current_package_version {
                                                "{ver}"
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

#[component]
fn JustificationCard(justification: CveJustification) -> Element {
    let category_label = match justification.category.as_str() {
        "mitigated" => "Mitigated",
        "false_positive" => "False Positive",
        "accepted_risk" => "Accepted Risk",
        "patch_scheduled" => "Patch Scheduled",
        _ => "Other",
    };

    rsx! {
        div {
            class: "p-3 rounded border border-white/10 bg-white/5 text-xs space-y-1",
            div {
                class: "flex items-center justify-between",
                span {
                    class: "px-2 py-1 rounded bg-purple-500/20 text-purple-300 text-xs font-semibold",
                    "{category_label}"
                }
                span {
                    class: "{theme::text::SECONDARY}",
                    { format!("Updated {}", justification.updated_at.format("%Y-%m-%d %H:%M UTC")) }
                }
            }
            p {
                class: "text-sm",
                "{justification.reason}"
            }
            if let Some(username) = &justification.updated_by_username {
                p {
                    class: "{theme::text::SECONDARY}",
                    "by {username}"
                }
            }
        }
    }
}
