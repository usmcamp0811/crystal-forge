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
use crate::components::notifications::Toast;
use crate::routes::Route;
use crate::theme;

fn query_param(name: &str) -> Option<String> {
    let window = web_sys::window()?;
    let search = window.location().search().ok()?;
    let query = search.trim_start_matches('?');
    if query.is_empty() {
        return None;
    }

    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or_default();
        let value = parts.next().unwrap_or_default();
        if key == name {
            return js_sys::decode_uri_component(value)
                .ok()
                .map(|v| v.as_string().unwrap_or_default());
        }
    }

    None
}

fn deployment_policy_status_color(policy: &str) -> &'static str {
    match policy.to_lowercase().as_str() {
        "automatic" => "#34d399",
        "scheduled" => "#60a5fa",
        "manual" => "#fbbf24",
        _ => "#9ca3af",
    }
}

fn sync_cve_url_query(
    severity: Option<&str>,
    fix_status: Option<&str>,
    triage_status: Option<&str>,
    package: Option<&str>,
    search: Option<&str>,
    sort: &str,
    view: &str,
    cve: Option<&str>,
) {
    let Some(window) = web_sys::window() else {
        return;
    };

    let mut parts: Vec<String> = Vec::new();
    let push = |parts: &mut Vec<String>, key: &str, value: &str| {
        if !value.trim().is_empty() {
            let encoded: String = js_sys::encode_uri_component(value).into();
            parts.push(format!("{key}={encoded}"));
        }
    };

    if let Some(v) = severity {
        push(&mut parts, "severity", v);
    }
    if let Some(v) = fix_status {
        push(&mut parts, "fix_status", v);
    }
    if let Some(v) = triage_status {
        push(&mut parts, "triage_status", v);
    }
    if let Some(v) = package {
        push(&mut parts, "package", v);
    }
    if let Some(v) = search {
        push(&mut parts, "search", v);
    }
    if sort != "severity" {
        push(&mut parts, "sort", sort);
    }
    if view != "grouped" {
        push(&mut parts, "view", view);
    }
    if let Some(v) = cve {
        push(&mut parts, "cve", v);
    }

    let query = if parts.is_empty() {
        String::new()
    } else {
        format!("?{}", parts.join("&"))
    };

    let pathname = window
        .location()
        .pathname()
        .ok()
        .unwrap_or_else(|| "/cves".to_string());
    if let Ok(history) = window.history() {
        let _ = history.replace_state_with_url(
            &wasm_bindgen::JsValue::NULL,
            "",
            Some(&format!("{pathname}{query}")),
        );
    }
}

#[component]
pub fn CvesView() -> Element {
    let initial_severity = query_param("severity");
    let initial_fix = query_param("fix_status").or_else(|| query_param("fix"));
    let initial_triage = query_param("triage_status").or_else(|| query_param("triage"));
    let initial_package = query_param("package");
    let initial_search = query_param("search").unwrap_or_default();
    let initial_sort = query_param("sort").unwrap_or_else(|| "severity".to_string());
    let initial_view = query_param("view").unwrap_or_else(|| "grouped".to_string());
    let initial_cve = query_param("cve");

    // Filter state
    let mut severity_filter = use_signal(move || initial_severity.clone());
    let mut fix_status_filter = use_signal(move || initial_fix.clone());
    let mut triage_status_filter = use_signal(move || initial_triage.clone());
    let mut package_filter = use_signal(move || initial_package.clone());
    let mut search_query = use_signal(move || initial_search.clone());
    let mut sort_by = use_signal(move || initial_sort.clone());
    let mut view_mode = use_signal(move || initial_view.clone()); // "flat" or "grouped"
    let mut selected_cve_id = use_signal(move || initial_cve.clone());
    let mut toast_message: Signal<Option<(String, bool)>> = use_signal(|| None);

    use_effect(move || {
        let severity = severity_filter();
        let fix_status = fix_status_filter();
        let triage_status = triage_status_filter();
        let package = package_filter();
        let search = search_query();
        let sort = sort_by();
        let view = view_mode();
        let cve = selected_cve_id();

        sync_cve_url_query(
            severity.as_deref(),
            fix_status.as_deref(),
            triage_status.as_deref(),
            package.as_deref(),
            if search.trim().is_empty() {
                None
            } else {
                Some(search.as_str())
            },
            &sort,
            &view,
            cve.as_deref(),
        );
    });

    // Data resources
    let stats = use_resource(move || async move { client::fetch_cve_fleet_stats().await });
    let package_names =
        use_resource(move || async move { client::fetch_cve_package_names().await });

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
            style: "display: flex; flex-direction: column; gap: 16px;",

            // Page Header
            div {
                class: "page-head",
                div {
                    h1 { class: "page-title", "CVEs" }
                    if let Some(Ok(s)) = stats.read().as_ref() {
                        p {
                            class: "page-subtitle",
                            "{s.total_cves} vulnerabilities · {s.systems_affected} systems affected · {s.fixable} have patches"
                        }
                    }
                }
                div {
                    style: "display: flex; gap: 8px;",
                    button {
                        class: "btn btn-ghost focus-ring",
                        onclick: move |_| {
                            let mut toast_message = toast_message;
                            spawn(async move {
                                match client::trigger_cve_fleet_rescan().await {
                                    Ok(_) => {
                                        toast_message.set(Some(("Fleet rescan initiated".to_string(), true)));
                                        gloo_timers::future::TimeoutFuture::new(4000).await;
                                        toast_message.set(None);
                                    }
                                    Err(e) => {
                                        toast_message.set(Some((format!("Rescan failed: {e}"), false)));
                                        gloo_timers::future::TimeoutFuture::new(5000).await;
                                        toast_message.set(None);
                                    }
                                }
                            });
                        },
                        // Sync icon
                        svg {
                            width: "14",
                            height: "14",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3M22 12.5a10 10 0 0 1-18.8 4.2" }
                        }
                        " Rescan fleet"
                    }
                    button {
                        class: "btn btn-ghost focus-ring",
                        onclick: move |_| {
                            let mut toast_message = toast_message;
                            spawn(async move {
                                match client::export_cves_csv(&CveFilters {
                                    severity: severity_filter(),
                                    fix_status: fix_status_filter(),
                                    triage_status: triage_status_filter(),
                                    package: package_filter(),
                                    search: if search_query().is_empty() { None } else { Some(search_query()) },
                                    sort: Some(sort_by()),
                                    limit: None,
                                }).await {
                                    Ok(_) => {
                                        toast_message.set(Some(("Export report started".to_string(), true)));
                                        gloo_timers::future::TimeoutFuture::new(3000).await;
                                        toast_message.set(None);
                                    }
                                    Err(e) => {
                                        toast_message.set(Some((format!("Export failed: {e}"), false)));
                                        gloo_timers::future::TimeoutFuture::new(5000).await;
                                        toast_message.set(None);
                                    }
                                }
                            });
                        },
                        svg {
                            width: "14",
                            height: "14",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
                            polyline { points: "7 10 12 15 17 10" }
                            line { x1: "12", y1: "15", x2: "12", y2: "3" }
                        }
                        " Export report"
                    }
                }
            }

            // Statistics Strip
            if let Some(Ok(fleet_stats)) = stats.read().as_ref() {
                div {
                    class: "stat-strip",

                    // Critical
                    div {
                        class: "stat",
                        span { class: "stat-accent", style: "--stat-color: #f87171;" }
                        div { class: "stat-label", "Critical" }
                        div { class: "stat-value", style: "color: #f87171;", "{fleet_stats.critical}" }
                        div { class: "stat-meta", "{fleet_stats.exploited} actively exploited" }
                    }

                    // High
                    div {
                        class: "stat",
                        span { class: "stat-accent", style: "--stat-color: #fbbf24;" }
                        div { class: "stat-label", "High" }
                        div { class: "stat-value", style: "color: #fbbf24;", "{fleet_stats.high}" }
                    }

                    // Patchable
                    div {
                        class: "stat",
                        span { class: "stat-accent", style: "--stat-color: #60a5fa;" }
                        div { class: "stat-label", "Patchable now" }
                        div { class: "stat-value", style: "color: #60a5fa;", "{fleet_stats.fixable}" }
                        div { class: "stat-meta", "Just deploy newer flake" }
                    }

                    // Accepted Risk
                    div {
                        class: "stat",
                        span { class: "stat-accent", style: "--stat-color: #a78bfa;" }
                        div { class: "stat-label", "Accepted risk" }
                        div { class: "stat-value", style: "color: #a78bfa;", "{fleet_stats.accepted + fleet_stats.scheduled}" }
                        div { class: "stat-meta", "{fleet_stats.accepted} accepted · {fleet_stats.scheduled} scheduled" }
                    }

                    // Outstanding
                    div {
                        class: "stat",
                        span { class: "stat-accent", style: "--stat-color: #34d399;" }
                        div { class: "stat-label", "Outstanding" }
                        div {
                            class: "stat-value",
                            style: if fleet_stats.outstanding > 20 { "color: #f87171;" } else { "color: #34d399;" },
                            "{fleet_stats.outstanding}"
                        }
                        div { class: "stat-meta", "need triage" }
                    }
                }
            }

            // Filter Bar
            div {
                class: "filterbar",

                // Search input
                div {
                    class: "filter-search",
                    style: "max-width: 300px;",
                    svg {
                        width: "14",
                        height: "14",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        circle { cx: "11", cy: "11", r: "8" }
                        path { d: "m21 21-4.3-4.3" }
                    }
                    input {
                        class: "input focus-ring",
                        r#type: "text",
                        placeholder: "Search CVE / package / title…",
                        value: "{search_query}",
                        oninput: move |evt| search_query.set(evt.value()),
                    }
                }

                // Severity Filter
                div {
                    class: "seg",
                    for (sev, label) in [("all", "All"), ("critical", "Critical"), ("high", "High"), ("medium", "Medium"), ("low", "Low")] {
                        button {
                            class: if severity_filter().as_deref() == if sev == "all" { None } else { Some(sev) } { "active" } else { "" },
                            onclick: move |_| {
                                if sev == "all" {
                                    severity_filter.set(None);
                                } else {
                                    severity_filter.set(Some(sev.to_string()));
                                }
                            },
                            "{label}"
                        }
                    }
                }

                // Fix Status Filter
                div {
                    class: "seg",
                    for status in [("all", "Any status"), ("available", "Has patch"), ("pending", "No patch"), ("exploited", "Exploited")] {
                        button {
                            class: if fix_status_filter().as_deref() == if status.0 == "all" { None } else { Some(status.0) } { "active" } else { "" },
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
                    class: "seg",
                    for status in [("all", "Any triage"), ("outstanding", "Outstanding"), ("scheduled", "Scheduled"), ("accepted", "Accepted")] {
                        button {
                            class: if triage_status_filter().as_deref() == if status.0 == "all" { None } else { Some(status.0) } { "active" } else { "" },
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

                // Package filter
                div {
                    style: "position: relative; max-width: 200px;",
                    input {
                        class: "input focus-ring mono",
                        style: if package_filter().is_some() { "font-size: 12px; padding-right: 28px;" } else { "font-size: 12px; padding-right: 12px;" },
                        r#type: "text",
                        list: "cve-pkg-list",
                        placeholder: "All packages…",
                        value: "{package_filter().unwrap_or_default()}",
                        oninput: move |evt| {
                            let value = evt.value();
                            if value.trim().is_empty() {
                                package_filter.set(None);
                            } else {
                                package_filter.set(Some(value));
                            }
                        },
                    }
                    datalist {
                        id: "cve-pkg-list",
                        if let Some(Ok(packages)) = package_names.read().as_ref() {
                            for package in packages {
                                option { value: "{package}" }
                            }
                        }
                    }
                    if package_filter().is_some() {
                        button {
                            class: "btn-icon focus-ring",
                            style: "position: absolute; right: 4px; top: 50%; transform: translateY(-50%); padding: 4px;",
                            title: "Clear",
                            onclick: move |_| package_filter.set(None),
                            svg {
                                width: "11",
                                height: "11",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M18 6 6 18" }
                                path { d: "M6 6l12 12" }
                            }
                        }
                    }
                }

                // Group label + toggle
                span {
                    class: "filter-count",
                    style: "margin-left: auto; margin-right: 0;",
                    "Group"
                }
                div {
                    class: "seg",
                    button {
                        class: if view_mode() == "grouped" { "active" } else { "" },
                        onclick: move |_| view_mode.set("grouped".to_string()),
                        "By package"
                    }
                    button {
                        class: if view_mode() == "flat" { "active" } else { "" },
                        onclick: move |_| view_mode.set("flat".to_string()),
                        "Flat list"
                    }
                }

                // Sort label + toggle
                span {
                    class: "filter-count",
                    style: "margin-left: 0; margin-right: 0;",
                    "Sort"
                }
                div {
                    class: "seg",
                    for sort in [("severity", "Severity"), ("cvss", "CVSS"), ("age", "Newest"), ("affected", "Most affected")] {
                        button {
                            class: if sort_by() == sort.0 { "active" } else { "" },
                            onclick: move |_| sort_by.set(sort.0.to_string()),
                            "{sort.1}"
                        }
                    }
                }
            }

            // CVE List
            if view_mode() == "grouped" {
                CvePackageGroupsView {
                    on_open_cve: move |cve_id: String| {
                        selected_cve_id.set(Some(cve_id));
                    },
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
                div {
                    class: "card",
                    style: "overflow: hidden;",
                    match &*cve_list.read_unchecked() {
                        Some(Ok(cves)) => rsx! {
                            table {
                                class: "sys-table",
                                thead {
                                    tr {
                                        th { "CVE" }
                                        th { "Severity" }
                                        th { "CVSS" }
                                        th { "Package" }
                                        th { "Title" }
                                        th { "Affected" }
                                        th { "Fix" }
                                        th { "Triage" }
                                        th { "Age" }
                                        th { style: "text-align: right;", " " }
                                    }
                                }
                                tbody {
                                    if cves.is_empty() {
                                        tr {
                                            td {
                                                colspan: "10",
                                                style: "padding: 24px; text-align: center; color: var(--cf-text-muted); font-size: 13px;",
                                                "No CVEs match the current filters."
                                            }
                                        }
                                    } else {
                                        for cve in cves {
                                            CveRow {
                                                cve: cve.clone(),
                                                total_systems: stats.read().as_ref().and_then(|r| r.as_ref().ok()).map(|s| s.systems_affected).unwrap_or(0),
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
                                style: "padding: 24px; text-align: center; color: var(--cf-text-muted); font-size: 13px;",
                                "Error loading CVEs: {err}"
                            }
                        },
                        None => rsx! {
                            div {
                                style: "padding: 24px; text-align: center; color: var(--cf-text-muted); font-size: 13px;",
                                "Loading CVEs..."
                            }
                        },
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

            if let Some((ref message, is_success)) = *toast_message.read() {
                Toast {
                    message: message.clone(),
                    is_success,
                    on_dismiss: move |_| toast_message.set(None)
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Flat List Components
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn CveRow(cve: CveListItem, total_systems: i64, on_open: EventHandler<String>) -> Element {
    let sev_cls = match cve.severity.to_uppercase().as_str() {
        "CRITICAL" => "chip-critical",
        "HIGH" => "chip-warning",
        "MEDIUM" => "chip-info",
        _ => "chip-unknown",
    };
    let sev_color = match cve.severity.to_uppercase().as_str() {
        "CRITICAL" => "#f87171",
        "HIGH" => "#fbbf24",
        "MEDIUM" => "#60a5fa",
        _ => "#9ca3af",
    };

    let cve_id_for_onclick = cve.cve_id.clone();
    let cve_id_for_row = cve_id_for_onclick.clone();
    let cve_id_for_link = cve_id_for_onclick.clone();
    let cve_id_for_open = cve_id_for_onclick.clone();

    rsx! {
        tr {
            style: "cursor: pointer;",
            onclick: move |_| on_open.call(cve_id_for_row.clone()),

            // CVE ID
            td {
                div {
                    class: "mono",
                    style: "font-weight: 600; font-size: 13px; display: flex; align-items: center; gap: 8px;",
                    "{cve.cve_id}"
                    if cve.exploited {
                        span {
                            class: "chip chip-critical",
                            style: "font-size: 10px;",
                            title: "Actively exploited in the wild",
                            "exploited"
                        }
                    }
                }
            }

            // Severity
            td {
                span {
                    class: "chip {sev_cls}",
                    span {
                        class: "chip-dot",
                        style: "background: {sev_color};",
                    }
                    "{cve.severity}"
                }
            }

            // CVSS
            td {
                if let Some(cvss) = cve.cvss_v3_score {
                    div {
                        style: "display: flex; align-items: center; gap: 6px;",
                        div {
                            style: "width: 40px; height: 5px; background: var(--cf-subtle-bg); border-radius: 99px; overflow: hidden;",
                            div {
                                style: "width: {cvss * 10.0}%; height: 100%; background: {sev_color};",
                            }
                        }
                        span {
                            class: "mono",
                            style: "font-size: 12px; color: var(--cf-text-primary); font-weight: 600;",
                            "{cvss:.1}"
                        }
                    }
                }
            }

            // Package
            td {
                class: "mono",
                style: "font-size: 12px;",
                "{cve.package_name.as_deref().unwrap_or(\"\")}"
            }

            // Title
            td {
                style: "font-size: 13px; max-width: 340px;",
                div {
                    class: "truncate",
                    title: "{cve.title}",
                    "{cve.title}"
                }
            }

            // Affected
            td {
                div {
                    style: "display: flex; align-items: center; gap: 6px;",
                    // Server icon
                    svg {
                        width: "11",
                        height: "11",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        style: "color: var(--cf-text-muted);",
                        rect { x: "2", y: "2", width: "20", height: "8", rx: "2", ry: "2" }
                        rect { x: "2", y: "14", width: "20", height: "8", rx: "2", ry: "2" }
                        line { x1: "6", y1: "6", x2: "6.01", y2: "6" }
                        line { x1: "6", y1: "18", x2: "6.01", y2: "18" }
                    }
                    span {
                        class: "mono",
                        style: if cve.affected_count > 0 { "font-size: 12px; font-weight: 600; color: var(--cf-text-primary);" } else { "font-size: 12px; font-weight: 600; color: var(--cf-text-muted);" },
                        "{cve.affected_count}"
                    }
                    span {
                        style: "font-size: 11px; color: var(--cf-text-muted);",
                        "/ {total_systems}"
                    }
                }
            }

            // Fix Status
            td {
                if cve.fix_status == "fix_available" {
                    span {
                        class: "chip chip-healthy",
                        title: "{cve.fixed_version.as_deref().unwrap_or(\"\")}",
                        // Check icon
                        svg {
                            width: "10",
                            height: "10",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "3",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            style: "display: inline; vertical-align: middle;",
                            polyline { points: "20 6 9 17 4 12" }
                        }
                        " "
                        if let Some(ver) = &cve.fixed_version {
                            "{ver}"
                        }
                    }
                } else {
                    span {
                        class: "chip chip-warning",
                        "no patch yet"
                    }
                }
            }

            // Triage Status
            td {
                match cve.triage_status.as_str() {
                    "accepted" => rsx! {
                        span {
                            class: "chip chip-info",
                            "accepted"
                        }
                    },
                    "scheduled" => rsx! {
                        span {
                            class: "chip chip-info",
                            "scheduled"
                        }
                    },
                    _ => rsx! {
                        span {
                            class: "chip chip-critical",
                            "outstanding"
                        }
                    },
                }
            }

            // Age
            td {
                style: "font-size: 12px; color: var(--cf-text-muted);",
                "{cve.age_days}d"
            }

            // Actions
            td {
                div {
                    class: "row-actions",
                    button {
                        class: "btn-icon focus-ring",
                        title: "Open advisory",
                        onclick: move |evt| {
                            evt.stop_propagation();
                            let _ = web_sys::window().and_then(|w| {
                                w.open_with_url_and_target(
                                    &format!("https://nvd.nist.gov/vuln/detail/{}", cve_id_for_link),
                                    "_blank"
                                ).ok()
                            });
                        },
                        // Link icon
                        svg {
                            width: "14",
                            height: "14",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" }
                            path { d: "M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" }
                        }
                    }
                    button {
                        class: "btn-icon focus-ring",
                        title: "Details",
                        onclick: move |evt| {
                            evt.stop_propagation();
                            on_open.call(cve_id_for_open.clone());
                        },
                        // Arrow-right icon
                        svg {
                            width: "14",
                            height: "14",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            line { x1: "5", y1: "12", x2: "19", y2: "12" }
                            polyline { points: "12 5 19 12 12 19" }
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Grouped View Components
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn CvePackageGroupsView(filters: CveFilters, on_open_cve: EventHandler<String>) -> Element {
    let grouped_cves = use_resource(move || {
        let f = filters.clone();
        async move { client::fetch_cves_grouped(&f).await }
    });

    rsx! {
        match &*grouped_cves.read_unchecked() {
            Some(Ok(groups)) => rsx! {
                if groups.is_empty() {
                    div {
                        class: "empty",
                        style: "margin: 0;",
                        h3 { "No CVEs match" }
                        div { "Try clearing a filter." }
                    }
                } else {
                    div {
                        style: "display: flex; flex-direction: column; gap: 10px;",
                        for group in groups {
                            CvePackageGroupCard {
                                group: group.clone(),
                                on_open_cve: on_open_cve
                            }
                        }
                    }
                }
            },
            Some(Err(err)) => rsx! {
                div {
                    class: "empty",
                    style: "margin: 0;",
                    h3 { "Error loading CVEs" }
                    div { "{err}" }
                }
            },
            None => rsx! {
                div {
                    class: "empty",
                    style: "margin: 0;",
                    h3 { "Loading CVEs..." }
                }
            },
        }
    }
}

#[component]
fn CvePackageGroupCard(group: CvePackageGroup, on_open_cve: EventHandler<String>) -> Element {
    let mut is_expanded = use_signal(|| false);

    let sev_color = if group.critical_count > 0 {
        "#f87171"
    } else if group.high_count > 0 {
        "#fbbf24"
    } else if group.medium_count > 0 {
        "#60a5fa"
    } else {
        "#9ca3af"
    };

    rsx! {
        div {
            class: "card",
            style: "overflow: hidden;",

            // Header button
            button {
                class: "focus-ring",
                style: format!(
                    "all: unset; display: grid; grid-template-columns: 24px 1fr auto auto; align-items: center; gap: 14px; padding: 14px 18px; cursor: pointer; width: 100%; background: {}; border-left: 3px solid {}; box-sizing: border-box;",
                    if is_expanded() { "color-mix(in oklab, var(--cf-brand-purple) 6%, var(--cf-card-bg))" } else { "transparent" },
                    sev_color
                ),
                onclick: move |_| is_expanded.set(!is_expanded()),

                // Chevron icon
                svg {
                    width: "14",
                    height: "14",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    style: "color: var(--cf-text-muted);",
                    if is_expanded() {
                        polyline { points: "6 9 12 15 18 9" }
                    } else {
                        polyline { points: "9 18 15 12 9 6" }
                    }
                }

                // Package info
                div {
                    style: "display: flex; flex-direction: column; gap: 2px; min-width: 0;",

                    // First row: package name + CVE count + exploited chip
                    div {
                        style: "display: flex; align-items: center; gap: 10px; flex-wrap: wrap;",
                        span {
                            class: "mono",
                            style: "font-size: 14px; font-weight: 700;",
                            "{group.package_name}"
                        }
                        span {
                            style: "font-size: 12px; color: var(--cf-text-muted);",
                            {
                                let cve_plural = if group.cve_count == 1 { "" } else { "s" };
                                format!("{} CVE{}", group.cve_count, cve_plural)
                            }
                        }
                        if group.exploited_count > 0 {
                            span {
                                class: "chip chip-critical",
                                style: "font-size: 10px;",
                                "{group.exploited_count} exploited"
                            }
                        }
                    }

                    // Second row: systems/patchable/outstanding
                    div {
                        style: "font-size: 11px; color: var(--cf-text-secondary);",
                        {
                            let sys_plural = if group.total_affected_systems == 1 { "" } else { "s" };
                            format!("{} system{} affected · {} patchable · {} outstanding",
                                group.total_affected_systems, sys_plural,
                                group.fixable_count, group.outstanding_count)
                        }
                    }
                }

                // Severity chips
                div {
                    style: "display: flex; gap: 5px; flex-wrap: wrap; justify-content: flex-end;",
                    if group.critical_count > 0 {
                        span {
                            class: "chip chip-critical",
                            style: "font-size: 10px;",
                            "{group.critical_count} crit"
                        }
                    }
                    if group.high_count > 0 {
                        span {
                            class: "chip chip-warning",
                            style: "font-size: 10px;",
                            "{group.high_count} high"
                        }
                    }
                    if group.medium_count > 0 {
                        span {
                            class: "chip chip-info",
                            style: "font-size: 10px;",
                            "{group.medium_count} med"
                        }
                    }
                    if group.low_count > 0 {
                        span {
                            class: "chip chip-unknown",
                            style: "font-size: 10px;",
                            "{group.low_count} low"
                        }
                    }
                }

                // Max CVSS
                div {
                    style: "display: flex; flex-direction: column; align-items: flex-end; gap: 2px; min-width: 96px;",
                    div {
                        style: "font-size: 10px; color: var(--cf-text-muted); text-transform: uppercase; letter-spacing: 0.06em;",
                        "Worst CVSS"
                    }
                    if let Some(cvss) = group.max_cvss {
                        div {
                            style: "display: flex; align-items: center; gap: 6px;",
                            div {
                                style: "width: 50px; height: 5px; background: var(--cf-subtle-bg); border-radius: 99px; overflow: hidden;",
                                div {
                                    style: "width: {cvss * 10.0}%; height: 100%; background: {sev_color};",
                                }
                            }
                            span {
                                class: "mono",
                                style: "font-size: 12px; color: var(--cf-text-primary); font-weight: 600;",
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
                        style: "border-top: 1px solid var(--cf-divider);",
                        table {
                            class: "sys-table",
                            style: "font-size: 12px;",
                            thead {
                                tr {
                                    th { "CVE" }
                                    th { "Severity" }
                                    th { "CVSS" }
                                    th { "Title" }
                                    th { "Affected" }
                                    th { "Fix" }
                                    th { "Triage" }
                                    th { "Age" }
                                }
                            }
                            tbody {
                                for cve in cves {
                                    CveRowInGroup {
                                        cve: cve.clone(),
                                        total_systems: group.total_affected_systems,
                                        on_open: move |cve_id: String| {
                                            on_open_cve.call(cve_id);
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

/// CVE row inside a grouped package card (no actions column, matching JSX reference)
#[component]
fn CveRowInGroup(cve: CveListItem, total_systems: i64, on_open: EventHandler<String>) -> Element {
    let sev_cls = match cve.severity.to_uppercase().as_str() {
        "CRITICAL" => "chip-critical",
        "HIGH" => "chip-warning",
        "MEDIUM" => "chip-info",
        _ => "chip-unknown",
    };
    let sev_color = match cve.severity.to_uppercase().as_str() {
        "CRITICAL" => "#f87171",
        "HIGH" => "#fbbf24",
        "MEDIUM" => "#60a5fa",
        _ => "#9ca3af",
    };

    let cve_id_for_row = cve.cve_id.clone();

    rsx! {
        tr {
            style: "cursor: pointer;",
            onclick: move |_| on_open.call(cve_id_for_row.clone()),

            // CVE ID
            td {
                div {
                    class: "mono",
                    style: "font-weight: 600; font-size: 13px; display: flex; align-items: center; gap: 8px;",
                    "{cve.cve_id}"
                    if cve.exploited {
                        span {
                            class: "chip chip-critical",
                            style: "font-size: 10px;",
                            title: "Actively exploited in the wild",
                            "exploited"
                        }
                    }
                }
            }

            // Severity
            td {
                span {
                    class: "chip {sev_cls}",
                    span {
                        class: "chip-dot",
                        style: "background: {sev_color};",
                    }
                    "{cve.severity}"
                }
            }

            // CVSS
            td {
                if let Some(cvss) = cve.cvss_v3_score {
                    div {
                        style: "display: flex; align-items: center; gap: 6px;",
                        div {
                            style: "width: 40px; height: 5px; background: var(--cf-subtle-bg); border-radius: 99px; overflow: hidden;",
                            div {
                                style: "width: {cvss * 10.0}%; height: 100%; background: {sev_color};",
                            }
                        }
                        span {
                            class: "mono",
                            style: "font-size: 12px; color: var(--cf-text-primary); font-weight: 600;",
                            "{cvss:.1}"
                        }
                    }
                }
            }

            // Title
            td {
                style: "font-size: 13px; max-width: 340px;",
                div {
                    class: "truncate",
                    title: "{cve.title}",
                    "{cve.title}"
                }
            }

            // Affected
            td {
                div {
                    style: "display: flex; align-items: center; gap: 6px;",
                    // Server icon
                    svg {
                        width: "11",
                        height: "11",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        style: "color: var(--cf-text-muted);",
                        rect { x: "2", y: "2", width: "20", height: "8", rx: "2", ry: "2" }
                        rect { x: "2", y: "14", width: "20", height: "8", rx: "2", ry: "2" }
                        line { x1: "6", y1: "6", x2: "6.01", y2: "6" }
                        line { x1: "6", y1: "18", x2: "6.01", y2: "18" }
                    }
                    span {
                        class: "mono",
                        style: if cve.affected_count > 0 { "font-size: 12px; font-weight: 600; color: var(--cf-text-primary);" } else { "font-size: 12px; font-weight: 600; color: var(--cf-text-muted);" },
                        "{cve.affected_count}"
                    }
                    span {
                        style: "font-size: 11px; color: var(--cf-text-muted);",
                        "/ {total_systems}"
                    }
                }
            }

            // Fix Status
            td {
                if cve.fix_status == "fix_available" {
                    span {
                        class: "chip chip-healthy",
                        title: "{cve.fixed_version.as_deref().unwrap_or(\"\")}",
                        // Check icon
                        svg {
                            width: "10",
                            height: "10",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "3",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            style: "display: inline; vertical-align: middle;",
                            polyline { points: "20 6 9 17 4 12" }
                        }
                        " "
                        if let Some(ver) = &cve.fixed_version {
                            "{ver}"
                        }
                    }
                } else {
                    span {
                        class: "chip chip-warning",
                        "no patch yet"
                    }
                }
            }

            // Triage Status
            td {
                match cve.triage_status.as_str() {
                    "accepted" => rsx! {
                        span {
                            class: "chip chip-info",
                            "accepted"
                        }
                    },
                    "scheduled" => rsx! {
                        span {
                            class: "chip chip-info",
                            "scheduled"
                        }
                    },
                    _ => rsx! {
                        span {
                            class: "chip chip-critical",
                            "outstanding"
                        }
                    },
                }
            }

            // Age
            td {
                style: "font-size: 12px; color: var(--cf-text-muted);",
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
    let cve_id_label = cve_id.clone();
    let cve_id_for_save_seed = cve_id.clone();
    let mut justification_category = use_signal(|| "accepted_risk".to_string());
    let mut justification_reason = use_signal(String::new);
    let mut save_status = use_signal(|| Option::<String>::None);
    let mut justifications_refresh = use_signal(|| 0_u64);
    let mut esc_listener_attached = use_signal(|| false);
    let advisory_cve_id = cve_id.clone();

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
        let _tick = justifications_refresh();
        let id = cve_id_justs.clone();
        async move { client::fetch_cve_justifications(&id).await }
    });

    use_effect(move || {
        use wasm_bindgen::JsCast;
        use wasm_bindgen::closure::Closure;

        let Some(window) = web_sys::window() else {
            return;
        };

        if esc_listener_attached() {
            return;
        }
        esc_listener_attached.set(true);

        let on_close_for_esc = on_close.clone();
        let handler = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
            if event.key() == "Escape" {
                on_close_for_esc.call(());
            }
        }) as Box<dyn FnMut(_)>);

        let _ =
            window.add_event_listener_with_callback("keydown", handler.as_ref().unchecked_ref());
        handler.forget();
    });

    rsx! {
        // Backdrop
        div {
            class: "fl-tray-backdrop fixed inset-0 bg-black/50 z-40",
            onclick: move |_| on_close.call(()),
        }

        // Drawer panel
        aside {
            class: "fl-tray fixed top-0 right-0 h-full w-full max-w-2xl bg-gray-900 border-l border-white/10 z-50 flex flex-col shadow-xl",
            role: "dialog",
            aria_label: "{cve_id_label}",

            // Header
            header {
                class: "fl-tray-head",
                match &*cve_detail.read_unchecked() {
                    Some(Ok(detail)) => {
                        let sev_color = match detail.severity.to_uppercase().as_str() {
                            "CRITICAL" => "#f87171",
                            "HIGH" => "#fbbf24",
                            "MEDIUM" => "#60a5fa",
                            _ => "#9ca3af",
                        };
                        let sev_cls = match detail.severity.to_uppercase().as_str() {
                            "CRITICAL" => "chip-critical",
                            "HIGH" => "chip-warning",
                            "MEDIUM" => "chip-info",
                            _ => "chip-unknown",
                        };
                        rsx! {
                            div {
                                style: "display: flex; align-items: center; gap: 12px; min-width: 0; flex: 1;",
                                // Shield icon colored by severity
                                svg {
                                    width: "18",
                                    height: "18",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    style: "color: {sev_color}; flex-shrink: 0;",
                                    path { d: "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" }
                                }
                                div {
                                    style: "min-width: 0;",
                                    div {
                                        style: "display: flex; align-items: center; gap: 8px; flex-wrap: wrap;",
                                        span {
                                            class: "mono",
                                            style: "font-weight: 700; font-size: 15px;",
                                            "{detail.cve_id}"
                                        }
                                        span {
                                            class: "chip {sev_cls}",
                                            span {
                                                class: "chip-dot",
                                                style: "background: {sev_color};",
                                            }
                                            "{detail.severity}"
                                        }
                                        if detail.exploited {
                                            span {
                                                class: "chip chip-critical",
                                                "exploited in the wild"
                                            }
                                        }
                                    }
                                    div {
                                        style: "font-size: 12px; color: var(--cf-text-secondary); margin-top: 3px;",
                                        "{detail.title}"
                                    }
                                }
                            }
                        }
                    },
                    _ => rsx! {
                        span { class: "mono", style: "font-weight: 700;", "{cve_id_label}" }
                    }
                }
                div {
                    style: "display: flex; gap: 6px;",
                    button {
                        class: "btn btn-ghost focus-ring xs",
                        title: "https://nvd.nist.gov/vuln/detail/{advisory_cve_id}",
                        onclick: move |_| {
                            let _ = web_sys::window().and_then(|w| {
                                w.open_with_url_and_target(
                                    &format!("https://nvd.nist.gov/vuln/detail/{}", advisory_cve_id),
                                    "_blank"
                                ).ok()
                            });
                        },
                        // Link icon
                        svg {
                            width: "11",
                            height: "11",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" }
                            path { d: "M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" }
                        }
                        " Advisory"
                    }
                    button {
                        class: "btn-icon focus-ring",
                        onclick: move |_| on_close.call(()),
                        // X icon
                        svg {
                            width: "16",
                            height: "16",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M18 6 6 18" }
                            path { d: "M6 6l12 12" }
                        }
                    }
                }
            }

            // Stats band
            match &*cve_detail.read_unchecked() {
                Some(Ok(detail)) => {
                    let sev_color = match detail.severity.to_uppercase().as_str() {
                        "CRITICAL" => "#f87171",
                        "HIGH" => "#fbbf24",
                        "MEDIUM" => "#60a5fa",
                        _ => "#9ca3af",
                    };
                    rsx! {
                        div {
                            class: "ed-stats",
                            div {
                                class: "ed-stat",
                                div { class: "ed-stat-label", "CVSS" }
                                div {
                                    class: "ed-stat-val",
                                    style: "color: {sev_color};",
                                    if let Some(cvss) = detail.cvss_v3_score {
                                        "{cvss:.1}"
                                    } else {
                                        "N/A"
                                    }
                                }
                            }
                            div {
                                class: "ed-stat",
                                div { class: "ed-stat-label", "Package" }
                                div {
                                    class: "ed-stat-val mono",
                                    style: "font-size: 14px;",
                                    "{detail.package_name.as_deref().unwrap_or(\"N/A\")}"
                                }
                            }
                            div {
                                class: "ed-stat",
                                div { class: "ed-stat-label", "Affected" }
                                div {
                                    class: "ed-stat-val",
                                    "{affected_systems.read().as_ref().and_then(|r| r.as_ref().ok()).map(|s| s.len()).unwrap_or(0)}"
                                }
                            }
                            div {
                                class: "ed-stat",
                                div { class: "ed-stat-label", "Fix" }
                                div {
                                    class: "ed-stat-val",
                                    style: "font-size: 14px;",
                                    if detail.fix_status == "fix_available" {
                                        span {
                                            class: "mono",
                                            style: "color: #34d399;",
                                            "{detail.fixed_version.as_deref().unwrap_or(\"available\")}"
                                        }
                                    } else {
                                        span {
                                            style: "color: #fbbf24;",
                                            "pending"
                                        }
                                    }
                                }
                            }
                            div {
                                class: "ed-stat",
                                div { class: "ed-stat-label", "Discovered" }
                                div {
                                    class: "ed-stat-val",
                                    style: "font-size: 14px;",
                                    if let Some(date) = detail.published_date {
                                        "{date}"
                                    } else {
                                        "N/A"
                                    }
                                }
                            }
                        }
                    }
                },
                _ => rsx! { }
            }

            // Body (scrollable)
            div {
                class: "ed-body",
                style: "padding: 18px 22px; display: flex; flex-direction: column; gap: 18px; overflow: auto;",

                match &*cve_detail.read_unchecked() {
                    Some(Ok(detail)) => rsx! {
                        // CVSS Vector
                        section {
                            h3 {
                                style: "font-size: 11px; text-transform: uppercase; letter-spacing: 0.08em; color: var(--cf-text-muted); margin: 0 0 8px; font-weight: 600;",
                                "CVSS vector"
                            }
                            if let Some(vector) = &detail.cvss_vector {
                                code {
                                    class: "mono",
                                    style: "font-size: 12px; color: var(--cf-text-primary); background: var(--cf-subtle-bg); padding: 6px 10px; border-radius: 6px; display: inline-block;",
                                    "{vector}"
                                }
                            } else {
                                div { style: "font-size: 12px; color: var(--cf-text-muted);", "No CVSS vector available." }
                            }
                        }

                        // Remediation
                        section {
                            h3 {
                                style: "font-size: 11px; text-transform: uppercase; letter-spacing: 0.08em; color: var(--cf-text-muted); margin: 0 0 10px; font-weight: 600;",
                                "Remediation"
                            }
                            if detail.fix_status == "fix_available" {
                                div {
                                    class: "sd-callout sd-callout-info",
                                    // Check icon
                                    svg {
                                        width: "13",
                                        height: "13",
                                        view_box: "0 0 24 24",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "3",
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        polyline { points: "20 6 9 17 4 12" }
                                    }
                                    div {
                                        style: "font-size: 12px;",
                                        "Fixed in "
                                        span {
                                            class: "mono",
                                            style: "font-weight: 600; color: #34d399;",
                                            "{detail.package_name.as_deref().unwrap_or(\"package\")}-{detail.fixed_version.as_deref().unwrap_or(\"version\")}"
                                        }
                                        ". Affected systems will pick up the fix automatically once the upstream flake bumps the package and an eval passes."
                                    }
                                }
                            } else {
                                div {
                                    class: "sd-callout sd-callout-danger",
                                    // Warning icon
                                    svg {
                                        width: "13",
                                        height: "13",
                                        view_box: "0 0 24 24",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        path { d: "M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" }
                                        line { x1: "12", y1: "9", x2: "12", y2: "13" }
                                        line { x1: "12", y1: "17", x2: "12.01", y2: "17" }
                                    }
                                    div {
                                        style: "font-size: 12px;",
                                        strong { "No upstream patch yet." }
                                        " Watch the advisory for updates. Consider applying compensating controls (network isolation, WAF rule) on affected hosts."
                                    }
                                }
                            }
                            dl {
                                class: "kv-grid",
                                style: "margin-top: 10px;",
                                dt { "Introduced in" }
                                dd { class: "mono", "{detail.package_name.as_deref().unwrap_or(\"\")}-{detail.installed_version.as_deref().unwrap_or(\"N/A\")}" }
                                dt { "Fixed in" }
                                dd { class: "mono", if detail.fix_status == "fix_available" { "{detail.package_name.as_deref().unwrap_or(\"\")}-{detail.fixed_version.as_deref().unwrap_or(\"\")}" } else { "—" } }
                                dt { "Advisory" }
                                dd {
                                    class: "mono",
                                    a {
                                        href: "https://nvd.nist.gov/vuln/detail/{cve_id_label}",
                                        target: "_blank",
                                        style: "color: var(--cf-brand-purple);",
                                        "nvd.nist.gov"
                                    }
                                }
                            }
                        }

                        // Affected Systems
                        section {
                            h3 {
                                style: "font-size: 11px; text-transform: uppercase; letter-spacing: 0.08em; color: var(--cf-text-muted); margin: 0 0 10px; font-weight: 600;",
                                "Affected systems · {affected_systems.read().as_ref().and_then(|r| r.as_ref().ok()).map(|s| s.len()).unwrap_or(0)}"
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
                                class: "text-xs {theme::text::SECONDARY} uppercase tracking-[0.08em] mb-2 font-semibold",
                                "Triage Justifications"
                            }

                            div {
                                class: "mb-3 space-y-2 p-3 rounded border border-white/10 bg-white/5",
                                div {
                                    class: "text-xs {theme::text::SECONDARY}",
                                    "Add justification (admin required)"
                                }
                                select {
                                    class: "w-full px-3 py-2 rounded-md border border-white/15 bg-black/20 text-sm",
                                    value: "{justification_category}",
                                    onchange: move |evt| justification_category.set(evt.value()),
                                    option { value: "accepted_risk", "Accepted risk" }
                                    option { value: "patch_scheduled", "Patch scheduled" }
                                    option { value: "mitigated", "Mitigated" }
                                    option { value: "false_positive", "False positive" }
                                }
                                textarea {
                                    class: "w-full px-3 py-2 rounded-md border border-white/15 bg-black/20 text-sm min-h-24",
                                    placeholder: "Reason (10-2000 chars)",
                                    value: "{justification_reason}",
                                    oninput: move |evt| justification_reason.set(evt.value()),
                                }
                                div {
                                    class: "text-[11px] {theme::text::SECONDARY}",
                                    "Reason should explain risk acceptance or mitigation details for audit traceability."
                                }
                                div {
                                    class: "flex items-center gap-2",
                                    button {
                                        class: "px-3 py-2 text-sm rounded-md border border-white/15 hover:bg-white/5",
                                        onclick: move |_| {
                                            let cve_id = cve_id_for_save_seed.clone();
                                            let category = justification_category();
                                            let reason = justification_reason();

                                            if reason.trim().len() < 10 || reason.trim().len() > 2000 {
                                                save_status.set(Some("Reason must be 10-2000 characters".to_string()));
                                                return;
                                            }

                                            spawn(async move {
                                                let payload = CveJustificationInput {
                                                    system_id: None,
                                                    category,
                                                    reason,
                                                };

                                                match client::save_cve_justification(&cve_id, &payload).await {
                                                    Ok(_) => {
                                                        save_status.set(Some("Justification saved".to_string()));
                                                        justification_reason.set(String::new());
                                                        justifications_refresh.set(justifications_refresh() + 1);
                                                    }
                                                    Err(err) => {
                                                        save_status.set(Some(format!("Save failed: {}", err)));
                                                    }
                                                }
                                            });
                                        },
                                        "Save justification"
                                    }
                                    if let Some(msg) = save_status() {
                                        span {
                                            class: "text-xs {theme::text::SECONDARY}",
                                            "{msg}"
                                        }
                                    }
                                }
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
    // Group by environment while preserving encounter order for stable rendering.
    let mut by_env: Vec<(String, Vec<CveAffectedSystemDetail>)> = Vec::new();
    for sys in systems {
        let env = sys
            .environment
            .clone()
            .unwrap_or_else(|| "unknown".to_string());

        if let Some((_, entries)) = by_env.iter_mut().find(|(k, _)| k == &env) {
            entries.push(sys);
        } else {
            by_env.push((env, vec![sys]));
        }
    }

    // If empty, show message
    if by_env.is_empty() {
        return rsx! {
            div {
                style: "font-size: 12px; color: var(--cf-text-muted); padding: 12px 0;",
                "No active systems affected. This CVE may apply to systems no longer in the registry."
            }
        };
    }

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 14px;",
            for (env, sys_list) in by_env.iter() {
                div {
                    // Environment header with badge and host count
                    div {
                        style: "display: flex; align-items: center; gap: 8px; margin-bottom: 6px;",
                        EnvBadge { env: env.clone() }
                        span {
                            style: "font-size: 11px; color: var(--cf-text-muted);",
                            {
                                let host_plural = if sys_list.len() == 1 { "" } else { "s" };
                                format!("{} host{}", sys_list.len(), host_plural)
                            }
                        }
                    }
                    // Card with table
                    div {
                        class: "card",
                        style: "overflow: hidden; border: 1px solid var(--cf-divider);",
                        table {
                            class: "sys-table",
                            style: "font-size: 12px;",
                            tbody {
                                for sys in sys_list {
                                    tr {
                                        // Hostname with status dot (40% width)
                                        td {
                                            style: "width: 40%;",
                                            div {
                                                style: "display: flex; align-items: center; gap: 8px;",
                                                // Status dot - green for healthy
                                                span {
                                                    class: "status-dot",
                                                    style: "--status-color: {deployment_policy_status_color(&sys.deployment_policy)};",
                                                }
                                                span {
                                                    class: "mono",
                                                    style: "font-weight: 600;",
                                                    "{sys.hostname}"
                                                }
                                            }
                                        }
                                        // Flake name
                                        td {
                                            class: "mono",
                                            style: "font-size: 11px; color: var(--cf-text-muted);",
                                            if let Some(flake) = &sys.flake_name {
                                                "{flake}"
                                            }
                                        }
                                        // Commit hash (first 7 chars)
                                        td {
                                            class: "mono",
                                            style: "font-size: 11px;",
                                            if let Some(commit) = &sys.commit_hash {
                                                "{commit}"
                                            }
                                        }
                                        // Deployment chip (based on deployment_policy)
                                        td {
                                            DeploymentChip { state: sys.deployment_policy.clone() }
                                        }
                                        // Arrow button to open system
                                        td {
                                            style: "text-align: right;",
                                            Link {
                                                to: Route::SystemDetailView { id: sys.system_id.to_string() },
                                                class: "btn-icon focus-ring",
                                                title: "Open {sys.hostname}",
                                                // Arrow-right icon
                                                svg {
                                                    width: "13",
                                                    height: "13",
                                                    view_box: "0 0 24 24",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    line { x1: "5", y1: "12", x2: "19", y2: "12" }
                                                    polyline { points: "12 5 19 12 12 19" }
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

/// Environment badge with colored dot and text.
/// Matches JSX EnvBadge component.
#[component]
fn EnvBadge(env: String) -> Element {
    // Environment style mapping matching JSX ENV_STYLE
    let (bg, fg, border) = match env.to_lowercase().as_str() {
        "production" => ("rgba(220,38,38,0.10)", "#f87171", "rgba(248,113,113,0.25)"),
        "staging" => ("rgba(217,119,6,0.10)", "#fbbf24", "rgba(251,191,36,0.25)"),
        "dev" => ("rgba(37,99,235,0.10)", "#60a5fa", "rgba(96,165,250,0.25)"),
        "edge" => ("rgba(15,118,110,0.12)", "#2dd4bf", "rgba(45,212,191,0.25)"),
        "lab" => ("rgba(124,58,237,0.10)", "#a78bfa", "rgba(167,139,250,0.25)"),
        _ => ("rgba(37,99,235,0.10)", "#60a5fa", "rgba(96,165,250,0.25)"), // default to dev
    };

    rsx! {
        span {
            class: "env-badge",
            style: "--env-bg: {bg}; --env-fg: {fg}; --env-border: {border};",
            span { class: "chip-dot" }
            "{env}"
        }
    }
}

/// Deployment state chip.
/// Matches JSX DeploymentChip component.
#[component]
fn DeploymentChip(state: String) -> Element {
    let (chip_class, label) = match state.to_lowercase().as_str() {
        "up-to-date" => ("chip-healthy", "up to date"),
        "behind" => ("chip-warning", "behind"),
        "failed" => ("chip-critical", "deploy failed"),
        "drift" => ("chip-warning", "drift"),
        "deploying" => ("chip-info", "deploying"),
        "automatic" => ("chip-healthy", "automatic"),
        "manual" => ("chip-info", "manual"),
        "scheduled" => ("chip-info", "scheduled"),
        _ => ("chip-unknown", "unknown"),
    };

    rsx! {
        span {
            class: "chip {chip_class}",
            "{label}"
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
