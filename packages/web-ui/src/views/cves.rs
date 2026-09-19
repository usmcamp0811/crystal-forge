//! Advanced CVE dashboard view (TASK-322).
//!
//! Complete refactor matching design reference with:
//! - Statistics strip
//! - Advanced filtering
//! - Dual view modes (flat/grouped)
//! - CVE detail drawer
//! - Triage workflow

use dioxus::prelude::*;
use std::{cell::Cell, rc::Rc};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, closure::Closure};

use crate::alerts::{NAV_BADGES, acknowledge_with_cursor_and_ids, should_flash};

use crate::api::client;
use crate::api::models::{CveFilters, CveFleetStats, CveListItem, CvePackageGroup};
use crate::components::dialog_focus::{
    DialogFocusBoundary, DialogFocusRestore, DialogFocusSentinel, DialogInitialFocus,
};
use crate::components::icon::{Icon, IconName};
use crate::components::layout::Card;
use crate::components::notifications::Toast;
use crate::routes::Route;
use crate::state::app_state::AppState;
use crate::state::auth;
use crate::views::poam_api::{self, PoamApiError};

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExactCveSelection {
    cve_id: String,
    package: String,
}

fn selection_from_query() -> Option<ExactCveSelection> {
    Some(ExactCveSelection {
        cve_id: query_param("cve")?,
        package: query_param("cve_package")?,
    })
}

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

const SUCCESS_TOAST_DURATION_MS: u32 = 3000;

#[derive(Clone, Copy)]
struct ToastPublication {
    generation: u64,
    auto_dismiss: bool,
}

#[derive(Default)]
struct ToastLifecycle {
    generation: u64,
}

impl ToastLifecycle {
    fn publish(&mut self, is_success: bool) -> ToastPublication {
        self.generation = self.generation.wrapping_add(1);
        ToastPublication {
            generation: self.generation,
            auto_dismiss: is_success,
        }
    }

    fn dismiss(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    fn expire(&mut self, publication: ToastPublication) -> bool {
        if publication.auto_dismiss && self.generation == publication.generation {
            self.dismiss();
            true
        } else {
            false
        }
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
    selection: Option<&ExactCveSelection>,
    push_history: bool,
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
    if let Some(selection) = selection {
        push(&mut parts, "cve", &selection.cve_id);
        push(&mut parts, "cve_package", &selection.package);
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
        let url = format!("{pathname}{query}");
        if push_history {
            let _ = history.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&url));
        } else {
            let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&url));
        }
    }
}

fn sync_cve_url_state(
    severity: Option<String>,
    fix_status: Option<String>,
    triage_status: Option<String>,
    package: Option<String>,
    search: String,
    sort: String,
    view: String,
    selection: Option<&ExactCveSelection>,
    push_history: bool,
) {
    sync_cve_url_query(
        severity.as_deref(),
        fix_status.as_deref(),
        triage_status.as_deref(),
        package.as_deref(),
        (!search.trim().is_empty()).then_some(search.as_str()),
        &sort,
        &view,
        selection,
        push_history,
    );
}

/// Renders the fleet CVE dashboard and its reload-safe exact-CVE selection.
///
/// `query` makes filter and drawer state part of the Dioxus route during a
/// direct page load. The view reads the browser URL so later selection changes
/// and `popstate` events remain authoritative.
#[component]
pub fn CvesView(query: String) -> Element {
    let _ = query;
    let app_state = use_context::<Signal<AppState>>();
    let is_admin_user = auth::is_admin(&app_state.read().auth);
    let initial_severity = query_param("severity");
    let initial_fix = query_param("fix_status").or_else(|| query_param("fix"));
    let initial_triage = query_param("triage_status").or_else(|| query_param("triage"));
    let initial_package = query_param("package");
    let initial_search = query_param("search").unwrap_or_default();
    let initial_sort = query_param("sort").unwrap_or_else(|| "severity".to_string());
    let initial_view = query_param("view").unwrap_or_else(|| "grouped".to_string());
    let initial_selection = selection_from_query();

    // Filter state
    let mut severity_filter = use_signal(move || initial_severity.clone());
    let mut fix_status_filter = use_signal(move || initial_fix.clone());
    let mut triage_status_filter = use_signal(move || initial_triage.clone());
    let mut package_filter = use_signal(move || initial_package.clone());
    let mut search_query = use_signal(move || initial_search.clone());
    let mut sort_by = use_signal(move || initial_sort.clone());
    let mut view_mode = use_signal(move || initial_view.clone()); // "flat" or "grouped"
    let mut selected_cve = use_signal(move || initial_selection.clone());
    let mut selection_hydrated = use_signal(|| false);
    let mut toast_message: Signal<Option<(String, bool)>> = use_signal(|| None);
    // CONCURRENCY: Publishing or dismissing feedback advances the lifecycle.
    // A success timer can clear only the publication that created the timer.
    let mut toast_lifecycle = use_signal(ToastLifecycle::default);
    let mut fleet_rescan_pending = use_signal(|| false);

    // Attention flash signal for the Critical stat card (set by the use_effect after stats resolves).
    let mut flash_crit_signal = use_signal(|| false);
    let flash_crit = flash_crit_signal();

    use_effect(move || {
        if !selection_hydrated() {
            // CONCURRENCY: The first effect can run before the mounted URL
            // selection reaches component state. Hydrate and yield so this
            // run cannot erase the deep link with `replaceState`.
            selected_cve.set(selection_from_query());
            selection_hydrated.set(true);
            return;
        }

        let severity = severity_filter();
        let fix_status = fix_status_filter();
        let triage_status = triage_status_filter();
        let package = package_filter();
        let search = search_query();
        let sort = sort_by();
        let view = view_mode();
        let selection = selected_cve();

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
            selection.as_ref(),
            false,
        );
    });

    #[cfg(target_arch = "wasm32")]
    {
        let popstate_listener = use_hook(|| {
            let callback = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                selected_cve.set(selection_from_query());
            });
            if let Some(window) = web_sys::window() {
                let _ = window.add_event_listener_with_callback(
                    "popstate",
                    callback.as_ref().unchecked_ref(),
                );
            }
            Rc::new(callback)
        });
        let listener_for_drop = popstate_listener.clone();
        use_drop(move || {
            if let Some(window) = web_sys::window() {
                let _ = window.remove_event_listener_with_callback(
                    "popstate",
                    listener_for_drop.as_ref().as_ref().unchecked_ref(),
                );
            }
        });
    }

    // Data resources
    let stats = use_resource(move || async move { client::fetch_cve_fleet_stats().await });

    // Attention flash for the Critical stat card when critical CVEs exist (TASK-385).
    // Must be placed after `stats` is declared so it can be captured in the closure.
    use_effect(move || {
        if let Some(Ok(s)) = stats.read().as_ref() {
            let crit_count = s.critical as i64;
            if should_flash("cves", crit_count > 0) {
                flash_crit_signal.set(true);
                spawn(async move {
                    gloo_timers::future::TimeoutFuture::new(3200).await;
                    flash_crit_signal.set(false);
                });
            }
        }
    });

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

    use_effect(move || {
        if let (Some(Ok(_s)), Some(Ok(_items))) = (stats.read().as_ref(), cve_list.read().as_ref())
        {
            let Some(cursor) = NAV_BADGES.read_unchecked().observed_at.clone() else {
                return;
            };
            let occurrence_ids = NAV_BADGES.read_unchecked().cves_occurrence_ids.clone();
            acknowledge_with_cursor_and_ids("cves", cursor, occurrence_ids);
        }
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
                    if is_admin_user {
                        button {
                            class: "btn btn-ghost focus-ring",
                            disabled: fleet_rescan_pending(),
                            onclick: move |_| {
                                if fleet_rescan_pending() {
                                    return;
                                }
                                fleet_rescan_pending.set(true);
                                spawn(async move {
                                    let result = client::trigger_cve_fleet_rescan().await;
                                    fleet_rescan_pending.set(false);
                                    match result {
                                        Ok(response) => {
                                            let publication = toast_lifecycle.write().publish(true);
                                            toast_message.set(Some((response.message, true)));
                                            gloo_timers::future::TimeoutFuture::new(SUCCESS_TOAST_DURATION_MS).await;
                                            if toast_lifecycle.write().expire(publication) {
                                                toast_message.set(None);
                                            }
                                        }
                                        Err(err) => {
                                            toast_lifecycle.write().publish(false);
                                            toast_message.set(Some((format!("Fleet rescan failed: {err}"), false)));
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
                            if fleet_rescan_pending() { " Rescanning…" } else { " Rescan fleet" }
                        }
                    }
                    button {
                        class: "btn btn-ghost focus-ring",
                        onclick: move |_| {
                            let mut toast_message = toast_message;
                            let mut toast_lifecycle = toast_lifecycle;
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
                                        let publication = toast_lifecycle.write().publish(true);
                                        toast_message.set(Some(("Export report started".to_string(), true)));
                                        gloo_timers::future::TimeoutFuture::new(SUCCESS_TOAST_DURATION_MS).await;
                                        if toast_lifecycle.write().expire(publication) {
                                            toast_message.set(None);
                                        }
                                    }
                                    Err(e) => {
                                        toast_lifecycle.write().publish(false);
                                        toast_message.set(Some((format!("Export failed: {e}"), false)));
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
                        class: if flash_crit { "stat attention-flash" } else { "stat" },
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
                    key: "{severity_filter().as_deref().unwrap_or(\"all\")}|{fix_status_filter().as_deref().unwrap_or(\"all\")}|{triage_status_filter().as_deref().unwrap_or(\"all\")}|{package_filter().as_deref().unwrap_or(\"all\")}|{search_query()}|{sort_by()}|{view_mode()}",
                    on_open_cve: move |selection: ExactCveSelection| {
                        sync_cve_url_state(
                            severity_filter(), fix_status_filter(), triage_status_filter(),
                            package_filter(), search_query(), sort_by(), view_mode(),
                            Some(&selection), true,
                        );
                        selected_cve.set(Some(selection));
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
                                                on_open: move |selection: ExactCveSelection| {
                                                    sync_cve_url_state(
                                                        severity_filter(), fix_status_filter(), triage_status_filter(),
                                                        package_filter(), search_query(), sort_by(), view_mode(),
                                                        Some(&selection), true,
                                                    );
                                                    selected_cve.set(Some(selection));
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
            if let Some(selection) = selected_cve() {
                ExactCveFleetDrawer {
                    key: "{selection.cve_id}|{selection.package}",
                    selection,
                    on_close: move |_| {
                        sync_cve_url_state(
                            severity_filter(), fix_status_filter(), triage_status_filter(),
                            package_filter(), search_query(), sort_by(), view_mode(), None, true,
                        );
                        selected_cve.set(None);
                    }
                }
            }

            if let Some((ref message, is_success)) = *toast_message.read() {
                Toast {
                    message: message.clone(),
                    is_success,
                    on_dismiss: move |_| {
                        toast_lifecycle.write().dismiss();
                        toast_message.set(None);
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Flat List Components
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn CveRow(
    cve: CveListItem,
    total_systems: i64,
    on_open: EventHandler<ExactCveSelection>,
) -> Element {
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
    let selection_for_row = cve.package_name.clone().map(|package| ExactCveSelection {
        cve_id: cve_id_for_onclick.clone(),
        package,
    });
    let selection_for_open = selection_for_row.clone();
    let cve_id_for_link = cve_id_for_onclick.clone();

    rsx! {
        tr {
            style: "cursor: pointer;",
            "data-testid": "cve-row",
            onclick: move |_| {
                if let Some(selection) = selection_for_row.clone() {
                    on_open.call(selection);
                }
            },

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
                        title: "{cve.exact_affected_count} exact · {cve.legacy_affected_count} legacy inventory",
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
                        aria_label: "Open fleet inventory for {cve.cve_id} {cve.package_name.as_deref().unwrap_or(\"unknown package\")}",
                        "data-testid": "cve-fleet-open",
                        onclick: move |evt| {
                            evt.stop_propagation();
                            if let Some(selection) = selection_for_open.clone() {
                                on_open.call(selection);
                            }
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
fn CvePackageGroupsView(
    filters: CveFilters,
    on_open_cve: EventHandler<ExactCveSelection>,
) -> Element {
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
fn CvePackageGroupCard(
    group: CvePackageGroup,
    on_open_cve: EventHandler<ExactCveSelection>,
) -> Element {
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
                                        on_open: move |selection: ExactCveSelection| {
                                            on_open_cve.call(selection);
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
fn CveRowInGroup(
    cve: CveListItem,
    total_systems: i64,
    on_open: EventHandler<ExactCveSelection>,
) -> Element {
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

    let selection_for_row = cve.package_name.clone().map(|package| ExactCveSelection {
        cve_id: cve.cve_id.clone(),
        package,
    });

    rsx! {
        tr {
            style: "cursor: pointer;",
            "data-testid": "cve-row",
            onclick: move |_| {
                if let Some(selection) = selection_for_row.clone() {
                    on_open.call(selection);
                }
            },

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
                        title: "{cve.exact_affected_count} exact · {cve.legacy_affected_count} legacy inventory",
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EnvironmentTriageChoice {
    Open,
    Accepted,
    Scheduled,
}

impl EnvironmentTriageChoice {
    fn value(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Accepted => "accepted",
            Self::Scheduled => "scheduled",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EnvironmentTriageDraft {
    environment_id: uuid::Uuid,
    environment_name: String,
    choice: EnvironmentTriageChoice,
    justification: String,
    review_date: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FleetTriageDraft {
    environments: Vec<EnvironmentTriageDraft>,
    title: String,
    target_date: String,
    plan: String,
    assignee: String,
    assignee_label: Option<String>,
    risk: poam_api::PoamRisk,
    preservation_error: Option<String>,
    default_milestones: bool,
}

impl FleetTriageDraft {
    fn from_detail(detail: &poam_api::FleetCveDetail) -> Self {
        let environments = detail
            .environments
            .iter()
            .filter(|environment| environment.exact_affected_system_count > 0)
            .map(|environment| {
                let (choice, justification, review_date) = match &environment.disposition {
                    Some(poam_api::CveEnvironmentDisposition::Accepted {
                        justification,
                        review_date,
                        ..
                    }) => (
                        EnvironmentTriageChoice::Accepted,
                        justification.clone(),
                        review_date.map(|date| date.to_string()).unwrap_or_default(),
                    ),
                    Some(poam_api::CveEnvironmentDisposition::Scheduled { .. }) => (
                        EnvironmentTriageChoice::Scheduled,
                        String::new(),
                        String::new(),
                    ),
                    None => (EnvironmentTriageChoice::Open, String::new(), String::new()),
                };
                EnvironmentTriageDraft {
                    environment_id: environment.environment_id,
                    environment_name: environment.environment_name.clone(),
                    choice,
                    justification,
                    review_date,
                }
            })
            .collect();
        let mut draft = Self {
            environments,
            title: format!(
                "{} - patch {}",
                detail.cve.cve_id, detail.canonical_package_name
            ),
            target_date: String::new(),
            plan: String::new(),
            assignee: String::new(),
            assignee_label: None,
            risk: fleet_risk(&detail.cve.severity),
            preservation_error: None,
            default_milestones: true,
        };
        let scheduled = detail
            .environments
            .iter()
            .filter_map(|environment| match &environment.disposition {
                Some(poam_api::CveEnvironmentDisposition::Scheduled { poam_id, poam, .. }) => {
                    Some((*poam_id, poam.as_ref()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if scheduled.is_empty() {
            return draft;
        }
        if scheduled.iter().any(|(_, poam)| poam.is_none()) {
            draft.preservation_error = Some(
                "Scheduled POA&M metadata is unavailable from this server version. Change all scheduled environments to OPEN or ACCEPTED, or retry after the server upgrade."
                    .to_string(),
            );
            return draft;
        }
        let first_id = scheduled[0].0;
        if scheduled.iter().any(|(poam_id, _)| *poam_id != first_id) {
            draft.preservation_error = Some(
                "Scheduled environments reference different POA&Ms. Change all scheduled environments to OPEN or ACCEPTED before submitting."
                    .to_string(),
            );
            return draft;
        }
        let metadata = scheduled[0].1.expect("checked scheduled metadata");
        if metadata.id != first_id
            || scheduled
                .iter()
                .any(|(_, candidate)| candidate.is_some_and(|candidate| candidate != metadata))
        {
            draft.preservation_error = Some(
                "Scheduled environments have conflicting POA&M metadata. Change all scheduled environments to OPEN or ACCEPTED before submitting."
                    .to_string(),
            );
            return draft;
        }
        if metadata.title.trim().is_empty() || metadata.plan.trim().is_empty() {
            draft.preservation_error = Some(
                "The scheduled POA&M metadata cannot satisfy compatible reuse. Change all scheduled environments to OPEN or ACCEPTED before submitting."
                    .to_string(),
            );
            return draft;
        }
        draft.title = metadata.title.clone();
        draft.plan = metadata.plan.clone();
        draft.target_date = metadata.target_date.to_string();
        draft.risk = metadata.risk;
        match scheduled_assignee_selection(&metadata.assignee) {
            Ok((value, label)) => {
                draft.assignee = value;
                draft.assignee_label = Some(label);
            }
            Err(message) => draft.preservation_error = Some(message),
        }
        draft
    }

    fn set_choice(&mut self, environment_id: uuid::Uuid, choice: EnvironmentTriageChoice) {
        if let Some(environment) = self
            .environments
            .iter_mut()
            .find(|environment| environment.environment_id == environment_id)
        {
            environment.choice = choice;
        }
    }

    fn request(&self, package: &str) -> Result<poam_api::FleetCveTriageRequest, String> {
        let mut actions = Vec::with_capacity(self.environments.len());
        let mut scheduled = false;
        for environment in &self.environments {
            let action = match environment.choice {
                EnvironmentTriageChoice::Open => poam_api::CveEnvironmentTriageAction::LeaveOpen {
                    environment_id: environment.environment_id,
                },
                EnvironmentTriageChoice::Accepted => {
                    let justification = environment.justification.trim();
                    if !(10..=2000).contains(&justification.len()) {
                        return Err(format!(
                            "Enter an acceptance justification of 10 to 2000 bytes for {}.",
                            environment.environment_name
                        ));
                    }
                    let review_date = if environment.review_date.trim().is_empty() {
                        None
                    } else {
                        Some(
                            chrono::NaiveDate::parse_from_str(
                                environment.review_date.trim(),
                                "%Y-%m-%d",
                            )
                            .map_err(|_| {
                                format!(
                                    "Enter a valid review date for {}.",
                                    environment.environment_name
                                )
                            })?,
                        )
                    };
                    poam_api::CveEnvironmentTriageAction::AcceptRisk {
                        environment_id: environment.environment_id,
                        justification: justification.to_string(),
                        review_date,
                    }
                }
                EnvironmentTriageChoice::Scheduled => {
                    scheduled = true;
                    poam_api::CveEnvironmentTriageAction::SchedulePatch {
                        environment_id: environment.environment_id,
                    }
                }
            };
            actions.push(action);
        }

        let poam = if scheduled {
            if let Some(message) = &self.preservation_error {
                return Err(message.clone());
            }
            if self.title.trim().is_empty() {
                return Err("Enter a POA&M title for scheduled patching.".to_string());
            }
            if self.plan.trim().is_empty() {
                return Err("Enter a remediation plan for scheduled patching.".to_string());
            }
            let target_date =
                chrono::NaiveDate::parse_from_str(self.target_date.trim(), "%Y-%m-%d")
                    .map_err(|_| "Enter a valid POA&M target date.".to_string())?;
            let assignee = parse_fleet_assignee(&self.assignee)?;
            Some(poam_api::FleetCvePoamRequest {
                title: self.title.trim().to_string(),
                plan: self.plan.trim().to_string(),
                assignee,
                target_date,
                risk: self.risk,
                default_milestones: self.default_milestones,
            })
        } else {
            None
        };

        Ok(poam_api::FleetCveTriageRequest {
            canonical_package_name: package.to_string(),
            actions,
            poam,
        })
    }
}

fn scheduled_assignee_selection(
    assignee: &poam_api::PoamAssigneeView,
) -> Result<(String, String), String> {
    match assignee {
        poam_api::PoamAssigneeView::User {
            user_id,
            display,
            available: true,
        } => Ok((format!("user:{user_id}"), display.clone())),
        poam_api::PoamAssigneeView::OidcGroup {
            group_name,
            display,
            available: true,
        } => Ok((format!("group:{group_name}"), display.clone())),
        poam_api::PoamAssigneeView::User { .. }
        | poam_api::PoamAssigneeView::OidcGroup { .. }
        | poam_api::PoamAssigneeView::Unassigned
        | poam_api::PoamAssigneeView::Legacy { .. } => Err(
            "The scheduled POA&M assignee is no longer available for compatible reuse. Change all scheduled environments to OPEN or ACCEPTED before submitting."
                .to_string(),
        ),
    }
}

fn parse_fleet_assignee(value: &str) -> Result<poam_api::PoamAssigneeRequest, String> {
    if let Some(user_id) = value.strip_prefix("user:") {
        return uuid::Uuid::parse_str(user_id)
            .map(|user_id| poam_api::PoamAssigneeRequest::User { user_id })
            .map_err(|_| "Select a valid POA&M assignee.".to_string());
    }
    if let Some(group_name) = value.strip_prefix("group:")
        && !group_name.trim().is_empty()
        && group_name == group_name.trim()
    {
        return Ok(poam_api::PoamAssigneeRequest::OidcGroup {
            group_name: group_name.to_string(),
        });
    }
    Err("Select a valid POA&M assignee.".to_string())
}

fn fleet_risk(severity: &str) -> poam_api::PoamRisk {
    match severity.to_ascii_lowercase().as_str() {
        "critical" | "high" => poam_api::PoamRisk::High,
        "medium" => poam_api::PoamRisk::Medium,
        _ => poam_api::PoamRisk::Low,
    }
}

#[derive(Clone, Debug, PartialEq)]
enum FleetDetailState {
    Loading,
    Loaded(poam_api::FleetCveDetail),
    Empty,
    Unauthorized,
    Error(String),
}

fn fleet_error_state(error: &PoamApiError) -> FleetDetailState {
    match error {
        PoamApiError::Server(server) if server.status == 401 || server.status == 403 => {
            FleetDetailState::Unauthorized
        }
        PoamApiError::Server(server) if server.status == 404 => FleetDetailState::Empty,
        _ => FleetDetailState::Error(error.to_string()),
    }
}

fn request_token_is_current(component_active: bool, requested: u64, current: u64) -> bool {
    component_active && requested == current
}

fn fleet_rollup_label(rollup: poam_api::FleetCveTriageRollup) -> &'static str {
    match rollup {
        poam_api::FleetCveTriageRollup::Outstanding => "OUTSTANDING",
        poam_api::FleetCveTriageRollup::Accepted => "ACCEPTED",
        poam_api::FleetCveTriageRollup::Scheduled => "SCHEDULED",
        poam_api::FleetCveTriageRollup::Partial => "MIXED",
    }
}

fn fleet_rollup_class(rollup: poam_api::FleetCveTriageRollup) -> &'static str {
    match rollup {
        poam_api::FleetCveTriageRollup::Outstanding => "chip-critical",
        poam_api::FleetCveTriageRollup::Accepted => "chip-info",
        poam_api::FleetCveTriageRollup::Scheduled => "chip-info",
        poam_api::FleetCveTriageRollup::Partial => "chip-warning",
    }
}

fn fleet_fix_label(detail: &poam_api::FleetCveDetail) -> String {
    detail
        .cve
        .fixed_version
        .clone()
        .filter(|version| !version.trim().is_empty())
        .unwrap_or_else(|| "Pending".to_string())
}

fn fleet_assignee_label(assignee: &poam_api::PoamAssigneeView) -> String {
    match assignee {
        poam_api::PoamAssigneeView::User { display, .. }
        | poam_api::PoamAssigneeView::OidcGroup { display, .. }
        | poam_api::PoamAssigneeView::Legacy { display } => display.clone(),
        poam_api::PoamAssigneeView::Unassigned => "Unassigned".to_string(),
    }
}

fn fleet_risk_label(risk: poam_api::PoamRisk) -> &'static str {
    match risk {
        poam_api::PoamRisk::High => "CAT I - High",
        poam_api::PoamRisk::Medium => "CAT II - Medium",
        poam_api::PoamRisk::Low => "CAT III - Low",
    }
}

#[component]
fn ExactCveFleetDrawer(selection: ExactCveSelection, on_close: EventHandler<()>) -> Element {
    let app_state = use_context::<Signal<AppState>>();
    let can_triage = auth::is_operator_or_above(&app_state.read().auth);
    let navigator = navigator();
    let mut state = use_signal(|| FleetDetailState::Loading);
    let mut load_generation = use_signal(|| 0_u64);
    let mut refresh_generation = use_signal(|| 0_u64);
    let mut refreshing = use_signal(|| false);
    let mut triage_open = use_signal(|| false);
    let mut mutation_error = use_signal(|| None::<String>);
    let mut result_poam = use_signal(|| None::<(uuid::Uuid, bool)>);
    let component_active = use_hook(|| Rc::new(Cell::new(true)));
    {
        let component_active = component_active.clone();
        use_drop(move || component_active.set(false));
    }

    use_effect({
        let selection = selection.clone();
        let component_active = component_active.clone();
        move || {
            let _refresh = refresh_generation();
            let generation = (*load_generation.peek()).wrapping_add(1);
            load_generation.set(generation);
            if !matches!(&*state.peek(), FleetDetailState::Loaded(_)) {
                state.set(FleetDetailState::Loading);
            }
            let cve_id = selection.cve_id.clone();
            let package = selection.package.clone();
            let component_active = component_active.clone();
            spawn(async move {
                let result = poam_api::fetch_fleet_cve_detail(&cve_id, &package).await;
                // CONCURRENCY: Only the newest request for this mounted exact
                // selection can replace authoritative drawer state.
                if !request_token_is_current(
                    component_active.get(),
                    generation,
                    *load_generation.peek(),
                ) {
                    return;
                }
                refreshing.set(false);
                match result {
                    Ok(detail) => state.set(FleetDetailState::Loaded(detail)),
                    Err(error) => state.set(fleet_error_state(&error)),
                }
            });
        }
    });

    let dialog_label = format!("{} {} fleet inventory", selection.cve_id, selection.package);
    rsx! {
        DialogFocusRestore {}
        button {
            class: "fl-tray-backdrop cve-fleet-backdrop",
            aria_label: "Close {dialog_label}",
            tabindex: "-1",
            onclick: move |_| on_close.call(()),
        }
        aside {
            id: "cve-fleet-drawer",
            class: "fl-tray cve-fleet-drawer",
            role: "dialog",
            aria_modal: "true",
            aria_label: "{dialog_label}",
            "data-testid": "cve-fleet-drawer",
            tabindex: "-1",
            onkeydown: move |event| if event.key() == Key::Escape && !triage_open() { on_close.call(()); },
            DialogFocusSentinel { dialog_id: "cve-fleet-drawer", boundary: DialogFocusBoundary::Last }
            header { class: "fl-tray-head cve-fleet-head",
                div { class: "cve-fleet-heading",
                    Icon { name: IconName::Shield, size: 18 }
                    div { class: "cve-fleet-heading-copy",
                        div { class: "cve-fleet-identity",
                            span { class: "mono", "{selection.cve_id}" }
                            if let FleetDetailState::Loaded(detail) = &*state.read() {
                                span { class: "chip chip-{detail.cve.severity.to_ascii_lowercase()}",
                                    span { class: "chip-dot" }
                                    "{detail.cve.severity}"
                                }
                                if detail.cve.exploited { span { class: "chip chip-critical", "exploited in the wild" } }
                            }
                        }
                        if let FleetDetailState::Loaded(detail) = &*state.read() {
                            div { class: "cve-fleet-title", title: "{detail.cve.title}", "{detail.cve.title}" }
                        } else {
                            div { class: "cve-fleet-title mono", "{selection.package}" }
                        }
                    }
                }
                div { class: "cve-fleet-actions",
                    if let FleetDetailState::Loaded(detail) = &*state.read() {
                        a { class: "btn btn-ghost xs focus-ring", href: "https://nvd.nist.gov/vuln/detail/{detail.cve.cve_id}", target: "_blank", rel: "noopener noreferrer", title: "Open NVD advisory", "data-testid": "cve-advisory-link", Icon { name: IconName::Link, size: 11 } " Advisory" }
                        if can_triage {
                            button { class: if detail.exact_mutation_target_count > 0 { "btn btn-primary xs focus-ring" } else { "btn btn-primary xs focus-ring cve-triage-disabled" }, "data-testid": "cve-triage-open", disabled: detail.exact_mutation_target_count == 0, title: if detail.exact_mutation_target_count == 0 { "Exact current scan evidence is required for fleet triage." } else { "Triage exact affected environments" }, onclick: move |_| { mutation_error.set(None); triage_open.set(true); }, Icon { name: IconName::Shield, size: 11 } if detail.exact_mutation_target_count > 0 { " Triage" } else { " Triage unavailable" } }
                        }
                    }
                    button { class: "btn-icon focus-ring", aria_label: "Close fleet inventory", autofocus: true, onclick: move |_| on_close.call(()), Icon { name: IconName::X, size: 16 } }
                }
            }
            div { class: "ed-body cve-fleet-body",
                if refreshing() {
                    div { class: "sd-callout sd-callout-warn", role: "status", "Fleet evidence changed. Refreshing authoritative detail..." }
                }
                if let Some(error) = mutation_error() {
                    div { class: "sd-callout sd-callout-danger", role: "alert", "{error}" }
                }
                if let Some((poam_id, reused)) = result_poam() {
                    div { class: "sd-callout sd-callout-info", role: "status",
                        if reused { "Scheduled environments use the existing POA&M. " } else { "Scheduled environments now use a new POA&M. " }
                        button { class: "btn btn-ghost xs focus-ring", "data-testid": "cve-triage-poam-link", onclick: move |_| { navigator.push(Route::ComplianceView { bundle: String::new(), version: String::new(), system: String::new(), policy: String::new(), poam: poam_id.to_string(), view: String::new() }); }, "Open POA&M" }
                    }
                }
                match &*state.read() {
                    FleetDetailState::Loading => rsx! { div { class: "empty", role: "status", "Loading fleet inventory..." } },
                    FleetDetailState::Empty => rsx! { div { class: "empty", h3 { "No current inventory findings" } p { "No visible active system currently reports this CVE and package." } } },
                    FleetDetailState::Unauthorized => rsx! { div { class: "empty", role: "alert", h3 { "Fleet detail unavailable" } p { "Your session cannot read this fleet inventory." } } },
                    FleetDetailState::Error(error) => rsx! { div { class: "empty", role: "alert", h3 { "Could not load fleet detail" } p { "{error}" } button { class: "btn btn-ghost focus-ring", onclick: move |_| { refreshing.set(true); let next = (*refresh_generation.peek()).wrapping_add(1); refresh_generation.set(next); }, "Retry" } } },
                    FleetDetailState::Loaded(detail) => rsx! { FleetCveDetailBody { detail: detail.clone() } },
                }
            }
            DialogFocusSentinel { dialog_id: "cve-fleet-drawer", boundary: DialogFocusBoundary::First }
        }
        if triage_open() {
            if let FleetDetailState::Loaded(detail) = &*state.read() {
                FleetCveTriageDialog {
                    key: "{detail.cve.cve_id}|{detail.canonical_package_name}",
                    detail: detail.clone(),
                    on_close: move |_| triage_open.set(false),
                    on_success: move |response: poam_api::FleetCveTriageResponse| {
                        result_poam.set(response.poam_id.map(|id| (id, response.poam_reused)));
                        mutation_error.set(None);
                        triage_open.set(false);
                        refreshing.set(true);
                        let next = (*refresh_generation.peek()).wrapping_add(1);
                        refresh_generation.set(next);
                    },
                    on_conflict: move |message: String| {
                        mutation_error.set(Some(message));
                        triage_open.set(false);
                        refreshing.set(true);
                        let next = (*refresh_generation.peek()).wrapping_add(1);
                        refresh_generation.set(next);
                    },
                }
            }
        }
    }
}

#[component]
fn FleetCveDetailBody(detail: poam_api::FleetCveDetail) -> Element {
    let total = detail.affected_system_count;
    let cvss = detail
        .cve
        .cvss_v3_score
        .map(|score| format!("{score:.1}"))
        .unwrap_or_else(|| "N/A".to_string());
    let fixed_version = fleet_fix_label(&detail);
    let installed_version = detail
        .cve
        .installed_version
        .clone()
        .unwrap_or_else(|| "Unknown".to_string());
    let published = detail
        .cve
        .published_date
        .map(|date| date.to_string())
        .unwrap_or_else(|| "Unknown".to_string());
    let cvss_vector = detail
        .cve
        .cvss_vector
        .clone()
        .unwrap_or_else(|| "Not published".to_string());
    let advisory_url = format!("https://nvd.nist.gov/vuln/detail/{}", detail.cve.cve_id);
    let dispositioned = detail
        .environments
        .iter()
        .filter(|environment| {
            environment.exact_affected_system_count > 0 && environment.disposition.is_some()
        })
        .map(|environment| environment.exact_affected_system_count)
        .sum::<i64>();
    rsx! {
        div { class: "ed-stats cve-fleet-stats",
            div { class: "ed-stat", div { class: "ed-stat-label", "CVSS" } div { class: "ed-stat-val", "{cvss}" } }
            div { class: "ed-stat", div { class: "ed-stat-label", "Package" } div { class: "ed-stat-val mono", "{detail.canonical_package_name}" } }
            div { class: "ed-stat", div { class: "ed-stat-label", "Affected" } div { class: "ed-stat-val", "{total}" } }
            div { class: "ed-stat", div { class: "ed-stat-label", "Fix" } div { class: "ed-stat-val mono cve-fix-value", "{fixed_version}" } }
            div { class: "ed-stat", div { class: "ed-stat-label", "Published" } div { class: "ed-stat-val cve-date-value", "{published}" } }
        }
        section { class: "cve-fleet-section cve-vector", "data-testid": "cve-cvss-vector",
            h3 { "CVSS vector" }
            code { class: "mono", "{cvss_vector}" }
        }
        div { class: "cve-authority-strip", "data-testid": "cve-authority-details",
            div { span { "Exact evidence" } strong { "{detail.exact_affected_system_count}" } }
            div { span { "Actionable" } strong { "{detail.exact_mutation_target_count}" } }
            div { span { "Legacy inventory" } strong { "{detail.legacy_affected_system_count}" } }
            div { span { "Exact rollup" } strong { class: "chip {fleet_rollup_class(detail.rollup)}", "{fleet_rollup_label(detail.rollup)}" } }
        }
        if detail.legacy_affected_system_count > 0 {
            div { class: "sd-callout sd-callout-warn", "data-testid": "cve-fleet-legacy",
                strong { "Legacy inventory cannot authorize fleet triage. " }
                "{detail.legacy_affected_system_count} affected host(s) do not have exact immutable evidence. Fleet accepted risk, scheduling, POA&M, verification, and closure remain unavailable for those hosts. Ordinary per-system justification remains separate."
            }
        }
        if detail.no_scan_system_count > 0 {
            div { class: "sd-callout sd-callout-warn", "data-testid": "cve-fleet-no-scan",
                "{detail.no_scan_system_count} visible active host(s) have no usable completed CVE scan and are not counted as affected."
            }
        }
        section { class: "cve-fleet-section",
            div { class: "cve-section-head",
                h3 { "Triage status" }
                span { "{dispositioned} of {detail.exact_mutation_target_count} actionable host(s) dispositioned" }
            }
            p { class: "cve-fleet-truth", "Accepted risk records rationale only. It does not verify or remediate the vulnerability. Scheduled patching remains separate from accepted risk." }
            if detail.environments.is_empty() {
                div { class: "empty", "No visible affected environments." }
            }
            for environment in detail.environments.iter().filter(|environment| environment.exact_affected_system_count > 0).cloned() {
                FleetEnvironmentCard { environment }
            }
            if detail.exact_mutation_target_count == 0 {
                div { class: "sd-callout sd-callout-warn", "No exact current scan subjects are available. Fleet triage remains read-only until an exact scan reports this CVE and package." }
            }
        }
        section { class: "cve-fleet-section cve-remediation", "data-testid": "cve-remediation",
            h3 { "Remediation" }
            if detail.cve.fixed_version.as_ref().is_some_and(|version| !version.trim().is_empty()) {
                div { class: "sd-callout sd-callout-info", Icon { name: IconName::Check, size: 13 } div { "Fixed in " strong { class: "mono", "{detail.canonical_package_name}-{fixed_version}" } ". Affected systems clear only after deployment and an exact follow-up scan verifies absence." } }
            } else {
                div { class: "sd-callout sd-callout-danger", Icon { name: IconName::Warn, size: 13 } div { strong { "No upstream patch is reported. " } "Watch the advisory and record compensating controls in accepted-risk rationale or the remediation plan." } }
            }
            dl { class: "kv-grid cve-remediation-meta",
                dt { "Observed version" } dd { class: "mono", "{installed_version}" }
                dt { "Fixed in" } dd { class: "mono", "{fixed_version}" }
                dt { "Advisory" } dd { a { href: "{advisory_url}", target: "_blank", rel: "noopener noreferrer", "nvd.nist.gov" } }
            }
        }
        section { class: "cve-fleet-section", "data-testid": "cve-affected-systems",
            h3 { "Affected systems · {detail.affected_system_count}" }
            for environment in detail.environments.clone() {
                FleetAffectedEnvironment { environment }
            }
            if detail.unassigned_affected_system_count > 0 {
                article { class: "cve-inventory-env", "data-testid": "cve-fleet-unassigned",
                    header { div { strong { "Unassigned" } span { "{detail.unassigned_affected_system_count} host(s)" } } span { class: "chip", "INVENTORY ONLY" } }
                    small { "These Admin-visible hosts have no environment. They are counted in inventory but cannot be fleet triage targets." }
                    FleetHostRows { systems: detail.unassigned_systems.clone() }
                }
            }
        }
    }
}

#[component]
fn FleetEnvironmentCard(environment: poam_api::CveAffectedEnvironment) -> Element {
    let status = if environment.exact_affected_system_count == 0 {
        "INVENTORY ONLY"
    } else {
        match &environment.disposition {
            Some(poam_api::CveEnvironmentDisposition::Accepted { .. }) => "EXACT ACCEPTED",
            Some(poam_api::CveEnvironmentDisposition::Scheduled { .. }) => "EXACT SCHEDULED",
            None => "EXACT OPEN",
        }
    };
    rsx! {
        article { class: "cve-fleet-env", "data-testid": "cve-fleet-environment", "data-state": "{status.to_ascii_lowercase()}",
            header { div { strong { "{environment.environment_name}" } span { class: "mono", " · {environment.affected_system_count} host(s)" } } span { class: "chip", "{status}" } }
            if environment.legacy_affected_system_count > 0 {
                small { "{environment.exact_affected_system_count} exact · {environment.legacy_affected_system_count} legacy inventory host(s). Legacy hosts cannot be triaged." }
            }
            match &environment.disposition {
                Some(poam_api::CveEnvironmentDisposition::Accepted { justification, review_date, actor, accepted_at }) => { let accepted_date = accepted_at.format("%Y-%m-%d").to_string(); rsx! {
                    p { "{justification}" }
                    small { "Accepted by {actor.display} on {accepted_date}" if let Some(review_date) = review_date { " · review {review_date}" } else { " · no review date" } }
                } },
                Some(poam_api::CveEnvironmentDisposition::Scheduled { poam_id, poam, actor, scheduled_at }) => { let scheduled_date = scheduled_at.format("%Y-%m-%d").to_string(); let label = poam.as_ref().map(|poam| format!("{}: {}", poam.human_id, poam.title)).unwrap_or_else(|| format!("POA&M {poam_id}")); rsx! {
                    if let Some(poam) = poam {
                        p { class: "cve-scheduled-plan", "{poam.plan}" }
                        div { class: "cve-scheduled-meta",
                            span { "Owner" strong { "{fleet_assignee_label(&poam.assignee)}" } }
                            span { "Target" strong { "{poam.target_date}" } }
                            span { "Risk" strong { "{fleet_risk_label(poam.risk)}" } }
                        }
                    }
                    div { class: "cve-fleet-scheduled", span { "Scheduled by {actor.display} on {scheduled_date}" } Link { to: Route::ComplianceView { bundle: String::new(), version: String::new(), system: String::new(), policy: String::new(), poam: poam_id.to_string(), view: String::new() }, class: "poam-ref focus-ring", Icon { name: IconName::File, size: 11 } " {label}" } }
                } },
                None => rsx! { small { "No active disposition covers the exact subjects. Exact subjects remain outstanding." } },
            }
        }
    }
}

#[component]
fn FleetAffectedEnvironment(environment: poam_api::CveAffectedEnvironment) -> Element {
    rsx! {
        article { class: "cve-inventory-env", "data-testid": "cve-affected-environment",
            header {
                div { strong { "{environment.environment_name}" } span { class: "mono", "{environment.affected_system_count} host(s)" } }
                span { class: "cve-inventory-authority", "{environment.exact_affected_system_count} exact · {environment.legacy_affected_system_count} legacy" }
            }
            FleetHostRows { systems: environment.systems.clone() }
            if environment.systems.len() < environment.affected_system_count.max(0) as usize {
                small { "Showing {environment.systems.len()} of {environment.affected_system_count} affected hosts." }
            }
        }
    }
}

#[component]
fn FleetHostRows(systems: Vec<crate::api::models::CveAffectedSystemDetail>) -> Element {
    rsx! {
        div { class: "cve-fleet-hosts",
            for system in systems {
                div { class: "cve-fleet-host", "data-testid": "cve-fleet-host",
                    Link { to: Route::SystemDetailView { id: system.system_id.to_string(), tab: "cves".to_string(), poam: String::new(), config_mode: String::new(), revision: String::new(), generation: String::new(), deploy_generation: String::new() }, class: "mono focus-ring cve-host-name", "{system.hostname}" }
                    span { class: "mono truncate", title: "{system.flake_name.as_deref().unwrap_or(\"Unknown flake\")}", "{system.flake_name.as_deref().unwrap_or(\"Unknown flake\")}" }
                    span { class: "mono truncate", title: "{system.commit_hash.as_deref().unwrap_or(\"Unknown revision\")}", "{system.commit_hash.as_deref().unwrap_or(\"Unknown revision\")}" }
                    span { class: "mono", "{system.current_package_version.as_deref().unwrap_or(\"Unknown version\")}" }
                    span { class: "chip", if system.inventory_authority == crate::api::models::SystemCveInventoryAuthority::Exact { "EXACT" } else { "LEGACY" } }
                    Link { to: Route::SystemDetailView { id: system.system_id.to_string(), tab: "cves".to_string(), poam: String::new(), config_mode: String::new(), revision: String::new(), generation: String::new(), deploy_generation: String::new() }, class: "btn-icon focus-ring", aria_label: "Open {system.hostname}", Icon { name: IconName::ArrowRight, size: 13 } }
                }
            }
        }
    }
}

#[component]
fn FleetCveTriageDialog(
    detail: poam_api::FleetCveDetail,
    on_close: EventHandler<()>,
    on_success: EventHandler<poam_api::FleetCveTriageResponse>,
    on_conflict: EventHandler<String>,
) -> Element {
    let mut draft = use_signal(|| FleetTriageDraft::from_detail(&detail));
    let mut catalog = use_signal(|| None::<Result<poam_api::PoamAssigneeCatalog, String>>);
    let mut error = use_signal(|| None::<String>);
    let mut pending = use_signal(|| false);
    use_effect(move || {
        spawn(async move {
            catalog.set(Some(
                poam_api::fetch_assignee_catalog()
                    .await
                    .map_err(|error| error.to_string()),
            ));
        });
    });
    let scheduled = draft
        .read()
        .environments
        .iter()
        .any(|environment| environment.choice == EnvironmentTriageChoice::Scheduled);
    let accepted_count = draft
        .read()
        .environments
        .iter()
        .filter(|environment| environment.choice == EnvironmentTriageChoice::Accepted)
        .map(|environment| {
            detail
                .environments
                .iter()
                .find(|candidate| candidate.environment_id == environment.environment_id)
                .map_or(0, |candidate| candidate.exact_affected_system_count)
        })
        .sum::<i64>();
    let scheduled_count = draft
        .read()
        .environments
        .iter()
        .filter(|environment| environment.choice == EnvironmentTriageChoice::Scheduled)
        .map(|environment| {
            detail
                .environments
                .iter()
                .find(|candidate| candidate.environment_id == environment.environment_id)
                .map_or(0, |candidate| candidate.exact_affected_system_count)
        })
        .sum::<i64>();
    let open_count = detail.exact_mutation_target_count - accepted_count - scheduled_count;
    let cvss = detail
        .cve
        .cvss_v3_score
        .map(|score| format!("{score:.1}"))
        .unwrap_or_else(|| "N/A".to_string());
    let fix = fleet_fix_label(&detail);
    let dialog_label = format!(
        "Triage {} {}",
        detail.cve.cve_id, detail.canonical_package_name
    );
    let submit_detail = detail.clone();
    let submit = move |_: MouseEvent| {
        let request = match draft.read().request(&submit_detail.canonical_package_name) {
            Ok(request) => request,
            Err(message) => {
                error.set(Some(message));
                return;
            }
        };
        pending.set(true);
        error.set(None);
        let cve_id = submit_detail.cve.cve_id.clone();
        spawn(async move {
            match poam_api::triage_fleet_cve(&cve_id, &request).await {
                Ok(response) => {
                    pending.set(false);
                    on_success.call(response);
                }
                Err(PoamApiError::Server(server))
                    if server.status == 409 || server.status == 412 =>
                {
                    pending.set(false);
                    on_conflict.call(format!(
                        "{} Refresh completed with the server's current exact fleet state.",
                        server.message
                    ));
                }
                Err(request_error) => {
                    pending.set(false);
                    error.set(Some(format!("Triage was not applied: {request_error}")));
                }
            }
        });
    };
    rsx! {
        DialogFocusRestore {}
        // The triage editor opens on top of the fleet drawer, so the browser
        // never applies its close button's `autofocus`. Move focus explicitly
        // to keep the nested modal keyboard-reachable and trapped.
        DialogInitialFocus { dialog_id: "cve-triage-dialog" }
        button { class: "modal-backdrop cve-triage-backdrop", aria_label: "Close {dialog_label}", tabindex: "-1", onclick: move |_| if !pending() { on_close.call(()) } }
        div { id: "cve-triage-dialog", class: "modal cve-triage-modal", role: "dialog", aria_modal: "true", aria_label: "{dialog_label}", "data-testid": "cve-triage-dialog", tabindex: "-1", onkeydown: move |event| if event.key() == Key::Escape && !pending() { event.stop_propagation(); on_close.call(()); },
            DialogFocusSentinel { dialog_id: "cve-triage-dialog", boundary: DialogFocusBoundary::Last }
            div { class: "modal-head", div { h2 { "Triage {detail.cve.cve_id}" } p { "Decide per environment. Legacy inventory remains read-only." } } button { class: "btn-icon focus-ring", aria_label: "Close triage editor", autofocus: true, disabled: pending(), onclick: move |_| on_close.call(()), Icon { name: IconName::X, size: 16 } } }
            div { class: "modal-body cve-triage-body",
                p { "Choose one intention for every affected environment. The server recomputes exact host scope when you submit." }
                div { class: "cve-triage-context", "data-testid": "cve-triage-context",
                    header { Icon { name: IconName::Shield, size: 12 } " Vulnerability" span { "Exact scope is carried over automatically" } }
                    div { class: "cve-triage-context-grid",
                        div { span { "CVE" } strong { class: "mono", "{detail.cve.cve_id}" } }
                        div { span { "Package" } strong { class: "mono", "{detail.canonical_package_name}" } }
                        div { span { "CVSS" } strong { "{cvss} · {detail.cve.severity}" } }
                        div { span { "Actionable hosts" } strong { "{detail.exact_mutation_target_count}" } }
                        div { span { "Fix" } strong { class: "mono", "{fix}" } }
                        div { span { "Exploited" } strong { if detail.cve.exploited { "yes — in the wild" } else { "not observed" } } }
                    }
                }
                if let Some(message) = error() { div { class: "sd-callout sd-callout-danger", role: "alert", "{message}" } }
                for environment in detail.environments.clone().into_iter().filter(|environment| environment.exact_affected_system_count > 0) {
                    { let environment_id = environment.environment_id; let current = draft.read().environments.iter().find(|item| item.environment_id == environment_id).cloned(); rsx! {
                        fieldset { class: "cve-triage-env", "data-testid": "cve-triage-environment",
                            legend { "{environment.environment_name} · {environment.exact_affected_system_count} exact host(s)" }
                            div { class: "seg", role: "group", aria_label: "Disposition for {environment.environment_name}",
                                for (choice, label) in [(EnvironmentTriageChoice::Open, "Leave open"), (EnvironmentTriageChoice::Accepted, "Accept risk"), (EnvironmentTriageChoice::Scheduled, "Schedule patch")] {
                                    button { r#type: "button", class: if current.as_ref().map(|item| item.choice) == Some(choice) { "active" } else { "" }, aria_pressed: if current.as_ref().map(|item| item.choice) == Some(choice) { "true" } else { "false" }, "data-action": "{choice.value()}", onclick: move |_| draft.write().set_choice(environment_id, choice), "{label}" }
                                }
                            }
                            if current.as_ref().map(|item| item.choice) == Some(EnvironmentTriageChoice::Accepted) {
                                label { class: "field", span { "Justification · required" } textarea { value: "{current.as_ref().map(|item| item.justification.as_str()).unwrap_or_default()}", "data-testid": "cve-accept-justification", oninput: move |event| if let Some(item) = draft.write().environments.iter_mut().find(|item| item.environment_id == environment_id) { item.justification = event.value(); } } }
                                label { class: "field", span { "Review date · optional" } input { r#type: "date", value: "{current.as_ref().map(|item| item.review_date.as_str()).unwrap_or_default()}", "data-testid": "cve-accept-review-date", oninput: move |event| if let Some(item) = draft.write().environments.iter_mut().find(|item| item.environment_id == environment_id) { item.review_date = event.value(); } } }
                            }
                        }
                    } }
                }
                if scheduled {
                    fieldset { class: "cve-triage-poam", legend { "Shared POA&M for scheduled environments" }
                        div { class: "sd-callout sd-callout-info", "The POA&M owns remediation for scheduled exact subjects. Verification and closure require a later exact scan that no longer reports this CVE and package." }
                        if let Some(message) = &draft.read().preservation_error { div { class: "sd-callout sd-callout-warn", role: "alert", "{message}" } }
                        label { class: "field", span { "Title" } input { value: "{draft.read().title}", "data-testid": "cve-poam-title", oninput: move |event| draft.write().title = event.value() } }
                        label { class: "field", span { "Target completion" } input { r#type: "date", value: "{draft.read().target_date}", "data-testid": "cve-poam-target", oninput: move |event| draft.write().target_date = event.value() } }
                        label { class: "field", span { "Remediation plan" } textarea { value: "{draft.read().plan}", "data-testid": "cve-poam-plan", oninput: move |event| draft.write().plan = event.value() } }
                        label { class: "field", span { "Assignee · required" }
                            select { value: "{draft.read().assignee}", "data-testid": "cve-poam-assignee", disabled: catalog.read().is_none(), onchange: move |event| draft.write().assignee = event.value(),
                                option { value: "", disabled: true, "Select a user or group" }
                                if let Some(label) = &draft.read().assignee_label { option { value: "{draft.read().assignee}", "{label} (current)" } }
                                if let Some(Ok(catalog)) = &*catalog.read() {
                                    optgroup { label: "People", for person in &catalog.people { option { value: "user:{person.user_id}", "{person.label}" } } }
                                    optgroup { label: "Groups", for group in &catalog.groups { option { value: "group:{group.group_name}", "{group.group_name}" } } }
                                }
                            }
                            if let Some(Err(message)) = &*catalog.read() { small { role: "alert", "Assignees unavailable: {message}" } }
                        }
                        label { class: "poam-check", input { r#type: "checkbox", checked: draft.read().default_milestones, onchange: move |event| draft.write().default_milestones = event.checked() } span { "Add the default vulnerability remediation milestones" } }
                    }
                }
            }
            div { class: "modal-foot cve-triage-foot",
                div { class: "cve-triage-outcome", "{accepted_count} accepted · {scheduled_count} scheduled · {open_count.max(0)} open" }
                button { class: "btn btn-ghost focus-ring", disabled: pending(), onclick: move |_| on_close.call(()), "Cancel" }
                button { class: "btn btn-primary focus-ring", "data-testid": "cve-triage-submit", disabled: pending() || (scheduled && draft.read().preservation_error.is_some()), onclick: submit, if pending() { "Applying..." } else { "Apply triage" } }
            }
            DialogFocusSentinel { dialog_id: "cve-triage-dialog", boundary: DialogFocusBoundary::First }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EnvironmentTriageChoice, EnvironmentTriageDraft, FleetDetailState, FleetTriageDraft,
        ToastLifecycle, fleet_error_state, request_token_is_current,
    };
    use crate::views::poam_api::{
        self, CveEnvironmentTriageAction, PoamApiError, PoamRisk, PoamServerError,
    };
    use uuid::Uuid;

    fn environment(
        id: &str,
        name: &str,
        choice: EnvironmentTriageChoice,
    ) -> EnvironmentTriageDraft {
        EnvironmentTriageDraft {
            environment_id: Uuid::parse_str(id).unwrap(),
            environment_name: name.to_string(),
            choice,
            justification: String::new(),
            review_date: String::new(),
        }
    }

    fn triage_draft() -> FleetTriageDraft {
        FleetTriageDraft {
            environments: vec![
                environment(
                    "00000000-0000-0000-0000-0000000000a1",
                    "Development",
                    EnvironmentTriageChoice::Accepted,
                ),
                environment(
                    "00000000-0000-0000-0000-0000000000b2",
                    "Production",
                    EnvironmentTriageChoice::Scheduled,
                ),
                environment(
                    "00000000-0000-0000-0000-0000000000c3",
                    "Lab",
                    EnvironmentTriageChoice::Open,
                ),
            ],
            title: "Patch openssl fleet".to_string(),
            target_date: "2026-10-15".to_string(),
            plan: "Promote the fixed package through environments.".to_string(),
            assignee: "group:platform-operators".to_string(),
            assignee_label: None,
            risk: PoamRisk::High,
            preservation_error: None,
            default_milestones: true,
        }
    }

    fn scheduled_detail(
        assignee: serde_json::Value,
        include_metadata: bool,
        second_poam_id: Option<&str>,
    ) -> poam_api::FleetCveDetail {
        let poam_id = "00000000-0000-0000-0000-0000000000d1";
        let metadata = serde_json::json!({
            "id": poam_id,
            "human_id": "POAM-0042",
            "title": "Existing fleet remediation",
            "plan": "Preserve the exact remediation plan.",
            "target_date": "2026-11-20",
            "risk": "medium",
            "assignee": assignee,
        });
        let disposition = |id: &str| {
            let mut value = serde_json::json!({
                "state": "scheduled",
                "poam_id": id,
                "actor": {
                    "user_id": "00000000-0000-0000-0000-0000000000f1",
                    "display": "Operator"
                },
                "scheduled_at": "2026-09-13T12:00:00Z"
            });
            if include_metadata {
                let mut row_metadata = metadata.clone();
                row_metadata["id"] = serde_json::Value::String(id.to_string());
                value["poam"] = row_metadata;
            }
            value
        };
        serde_json::from_value(serde_json::json!({
            "cve": {
                "cve_id": "CVE-2026-44010",
                "cvss_v3_score": 9.8,
                "severity": "critical",
                "title": "Test CVE",
                "cvss_vector": null,
                "cwe_id": null,
                "published_date": null,
                "modified_date": null,
                "exploited": false,
                "package_name": "openssl",
                "installed_version": "3.0.1",
                "fixed_version": "3.0.2",
                "detection_method": "test",
                "fix_status": "fix_available"
            },
            "canonical_package_name": "openssl",
            "rollup": "scheduled",
            "affected_system_count": 2,
            "exact_affected_system_count": 2,
            "exact_mutation_target_count": 2,
            "legacy_affected_system_count": 0,
            "no_scan_system_count": 0,
            "environments": [
                {
                    "environment_id": "00000000-0000-0000-0000-0000000000e1",
                    "environment_name": "Production",
                    "affected_system_count": 1,
                    "exact_affected_system_count": 1,
                    "legacy_affected_system_count": 0,
                    "systems": [],
                    "disposition": disposition(poam_id)
                },
                {
                    "environment_id": "00000000-0000-0000-0000-0000000000e2",
                    "environment_name": "Staging",
                    "affected_system_count": 1,
                    "exact_affected_system_count": 1,
                    "legacy_affected_system_count": 0,
                    "systems": [],
                    "disposition": disposition(second_poam_id.unwrap_or(poam_id))
                }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn exact_fleet_request_token_rejects_stale_and_unmounted_responses() {
        assert!(request_token_is_current(true, 2, 2));
        assert!(!request_token_is_current(true, 1, 2));
        assert!(!request_token_is_current(false, 2, 2));
    }

    #[test]
    fn toast_lifecycle_fences_timers_and_manual_dismissal() {
        let mut lifecycle = ToastLifecycle::default();

        let older = lifecycle.publish(true);
        let newer = lifecycle.publish(true);
        assert!(!lifecycle.expire(older));

        lifecycle.dismiss();
        assert!(!lifecycle.expire(newer));

        let persistent_error = lifecycle.publish(false);
        assert!(!lifecycle.expire(persistent_error));

        let current_success = lifecycle.publish(true);
        assert!(lifecycle.expire(current_success));
        assert!(!lifecycle.expire(current_success));
    }

    #[test]
    fn triage_validation_requires_environment_specific_acceptance_fields() {
        let mut draft = triage_draft();
        assert_eq!(
            draft.request("openssl").unwrap_err(),
            "Enter an acceptance justification of 10 to 2000 bytes for Development."
        );

        draft.environments[0].justification = "too short".to_string();
        assert_eq!(
            draft.request("openssl").unwrap_err(),
            "Enter an acceptance justification of 10 to 2000 bytes for Development."
        );

        draft.environments[0].justification = "Internal-only service.".to_string();
        draft.environments[0].review_date = "not-a-date".to_string();
        assert_eq!(
            draft.request("openssl").unwrap_err(),
            "Enter a valid review date for Development."
        );
    }

    #[test]
    fn triage_validation_rejects_incomplete_poam_and_invalid_assignee_shape() {
        let mut draft = triage_draft();
        draft.environments[0].justification = "Internal-only service.".to_string();
        draft.plan.clear();
        assert_eq!(
            draft.request("openssl").unwrap_err(),
            "Enter a remediation plan for scheduled patching."
        );

        draft.plan = "Promote and verify the fixed package.".to_string();
        draft.assignee = "platform-operators".to_string();
        assert_eq!(
            draft.request("openssl").unwrap_err(),
            "Select a valid POA&M assignee."
        );
    }

    #[test]
    fn mixed_triage_request_contains_only_environment_intentions_and_shared_poam() {
        let mut draft = triage_draft();
        draft.environments[0].justification = "Internal-only service.".to_string();
        draft.environments[0].review_date = "2026-10-01".to_string();
        let request = draft.request("openssl").unwrap();

        assert!(matches!(
            request.actions[0],
            CveEnvironmentTriageAction::AcceptRisk { .. }
        ));
        assert!(matches!(
            request.actions[1],
            CveEnvironmentTriageAction::SchedulePatch { .. }
        ));
        assert!(matches!(
            request.actions[2],
            CveEnvironmentTriageAction::LeaveOpen { .. }
        ));
        let json = serde_json::to_value(request).unwrap();
        assert_eq!(json["canonical_package_name"], "openssl");
        assert_eq!(json["poam"]["assignee"]["kind"], "oidc_group");
        let serialized = json.to_string();
        assert!(!serialized.contains("system_id"));
        assert!(!serialized.contains("hostname"));
        assert!(!serialized.contains("actor"));
        assert!(!serialized.contains("evidence"));
    }

    #[test]
    fn scheduled_draft_initializes_exact_user_and_group_metadata() {
        for (assignee, expected) in [
            (
                serde_json::json!({
                    "kind": "user",
                    "user_id": "00000000-0000-0000-0000-0000000000f2",
                    "display": "Fleet Owner",
                    "available": true
                }),
                "user:00000000-0000-0000-0000-0000000000f2",
            ),
            (
                serde_json::json!({
                    "kind": "oidc_group",
                    "group_name": "platform-operators",
                    "display": "platform-operators",
                    "available": true
                }),
                "group:platform-operators",
            ),
        ] {
            let draft = FleetTriageDraft::from_detail(&scheduled_detail(assignee, true, None));
            assert_eq!(draft.title, "Existing fleet remediation");
            assert_eq!(draft.plan, "Preserve the exact remediation plan.");
            assert_eq!(draft.target_date, "2026-11-20");
            assert_eq!(draft.risk, PoamRisk::Medium);
            assert_eq!(draft.assignee, expected);
            assert!(draft.preservation_error.is_none());
            let request = draft.request("openssl").unwrap();
            assert_eq!(request.poam.unwrap().risk, PoamRisk::Medium);
        }
    }

    #[test]
    fn scheduled_draft_blocks_old_or_conflicting_metadata_but_allows_removal() {
        let assignee = serde_json::json!({
            "kind": "oidc_group",
            "group_name": "platform-operators",
            "display": "platform-operators",
            "available": true
        });
        let mut old_server =
            FleetTriageDraft::from_detail(&scheduled_detail(assignee.clone(), false, None));
        assert!(
            old_server
                .request("openssl")
                .unwrap_err()
                .contains("server version")
        );
        for environment in &mut old_server.environments {
            environment.choice = EnvironmentTriageChoice::Open;
        }
        assert!(old_server.request("openssl").unwrap().poam.is_none());

        let conflicting = FleetTriageDraft::from_detail(&scheduled_detail(
            assignee,
            true,
            Some("00000000-0000-0000-0000-0000000000d2"),
        ));
        assert!(
            conflicting
                .request("openssl")
                .unwrap_err()
                .contains("different POA&Ms")
        );

        for unavailable in [
            serde_json::json!({
                "kind": "user",
                "user_id": "00000000-0000-0000-0000-0000000000f2",
                "display": "Former owner",
                "available": false
            }),
            serde_json::json!({"kind": "legacy", "display": "Historical owner"}),
        ] {
            let draft = FleetTriageDraft::from_detail(&scheduled_detail(unavailable, true, None));
            assert!(
                draft
                    .request("openssl")
                    .unwrap_err()
                    .contains("no longer available")
            );
        }
    }

    #[test]
    fn triage_draft_excludes_legacy_only_environments() {
        let mut detail = scheduled_detail(
            serde_json::json!({
                "kind": "oidc_group",
                "group_name": "platform-operators",
                "display": "platform-operators",
                "available": true
            }),
            true,
            None,
        );
        detail.environments[1].exact_affected_system_count = 0;
        detail.environments[1].legacy_affected_system_count = 1;
        detail.environments[1].disposition = None;

        let draft = FleetTriageDraft::from_detail(&detail);

        assert_eq!(draft.environments.len(), 1);
        assert_eq!(draft.environments[0].environment_name, "Production");
    }

    #[test]
    fn fleet_detail_errors_preserve_empty_unauthorized_and_retryable_states() {
        let error = |status| {
            PoamApiError::Server(PoamServerError {
                status,
                code: "test_error".to_string(),
                message: "test failure".to_string(),
                details: None,
            })
        };

        assert_eq!(fleet_error_state(&error(404)), FleetDetailState::Empty);
        assert_eq!(
            fleet_error_state(&error(403)),
            FleetDetailState::Unauthorized
        );
        assert_eq!(
            fleet_error_state(&error(401)),
            FleetDetailState::Unauthorized
        );
        assert!(matches!(
            fleet_error_state(&error(500)),
            FleetDetailState::Error(_)
        ));
    }
}
