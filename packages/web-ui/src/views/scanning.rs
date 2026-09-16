use std::cmp::Ordering;
use std::collections::HashMap;

use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::client::{
    fetch_environments, fetch_scanning_deployed, fetch_scanning_queue, fetch_scanning_schedule,
    fetch_scanning_stats, fetch_scanning_system_scans, fetch_scanning_systems,
    update_scanning_schedule,
};
use crate::api::models::{
    ScanSchedulePolicyResponse, ScanningQueueItemResponse, UpdateScanSchedulePolicyRequest,
};
use crate::components::chips::EnvBadge;
use crate::components::icon::{Icon, IconName};
use crate::routes::Route;

const SCANNING_RESULT_LIMIT: usize = 500;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScanTab {
    Deployed,
    All,
    Systems,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScanSort {
    Name,
    Freshness,
    Status,
    Findings,
    LastScan,
}

#[derive(Clone, Copy)]
struct StatusMeta {
    key: &'static str,
    class: &'static str,
    color: &'static str,
    label: &'static str,
}

fn status_meta(status: &str) -> StatusMeta {
    match status {
        "in_progress" | "scanning" => StatusMeta {
            key: "scanning",
            class: "chip-info",
            color: "#60a5fa",
            label: "Scanning",
        },
        "pending" | "queued" => StatusMeta {
            key: "queued",
            class: "chip-info",
            color: "#a78bfa",
            label: "Queued",
        },
        "awaiting" => StatusMeta {
            key: "awaiting",
            class: "chip-unknown",
            color: "#94a3b8",
            label: "Awaiting closure",
        },
        "failed" => StatusMeta {
            key: "failed",
            class: "chip-critical",
            color: "#f87171",
            label: "Failed",
        },
        "stale" => StatusMeta {
            key: "stale",
            class: "chip-warning",
            color: "#fbbf24",
            label: "Stale",
        },
        "needs-build" | "needs_build" => StatusMeta {
            key: "needs-build",
            class: "chip-warning",
            color: "#f59e0b",
            label: "Needs build",
        },
        "never_scanned" | "unscanned" => StatusMeta {
            key: "unscanned",
            class: "chip-unknown",
            color: "#9ca3af",
            label: "Never scanned",
        },
        "completed" | "complete" => StatusMeta {
            key: "complete",
            class: "chip-healthy",
            color: "#34d399",
            label: "Complete",
        },
        _ => StatusMeta {
            key: "unknown",
            class: "chip-unknown",
            color: "#9ca3af",
            label: "Unknown",
        },
    }
}

fn normalize_freshness(freshness: &str) -> &'static str {
    match freshness {
        "deployed" => "deployed",
        "recent" => "recent",
        "archived" => "archived",
        _ => "unknown",
    }
}

fn status_rank(status: &str) -> u8 {
    match status_meta(status).key {
        "failed" => 0,
        "awaiting" => 1,
        "scanning" => 2,
        "queued" => 3,
        "stale" => 4,
        "complete" => 5,
        "needs-build" => 6,
        "unscanned" => 7,
        _ => 8,
    }
}

fn freshness_rank(freshness: &str) -> u8 {
    match normalize_freshness(freshness) {
        "deployed" => 0,
        "recent" => 1,
        "archived" => 2,
        _ => 3,
    }
}

fn finding_score(row: &ScanningQueueItemResponse) -> i64 {
    i64::from(row.critical_count) * 10_000
        + i64::from(row.high_count) * 100
        + i64::from(row.medium_count)
}

fn filter_and_sort_rows(
    rows: &[ScanningQueueItemResponse],
    query: &str,
    status: &str,
    freshness: &str,
    latest_only: bool,
    sort: ScanSort,
    descending: bool,
) -> Vec<ScanningQueueItemResponse> {
    let query = query.trim().to_ascii_lowercase();
    let mut filtered = rows
        .iter()
        .filter(|row| {
            let matches_query = query.is_empty()
                || row.hostname.to_ascii_lowercase().contains(&query)
                || row
                    .flake_name
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains(&query)
                || row
                    .commit_hash
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains(&query);
            let matches_status = status == "all" || status_meta(&row.status).key == status;
            let matches_freshness =
                freshness == "all" || normalize_freshness(&row.freshness) == freshness;
            matches_query
                && matches_status
                && matches_freshness
                && (!latest_only || row.is_latest_per_flake)
        })
        .cloned()
        .collect::<Vec<_>>();

    filtered.sort_by(|left, right| {
        let order = match sort {
            ScanSort::Name => left
                .hostname
                .to_ascii_lowercase()
                .cmp(&right.hostname.to_ascii_lowercase()),
            ScanSort::Freshness => {
                freshness_rank(&left.freshness).cmp(&freshness_rank(&right.freshness))
            }
            ScanSort::Status => status_rank(&left.status).cmp(&status_rank(&right.status)),
            ScanSort::Findings => finding_score(right).cmp(&finding_score(left)),
            ScanSort::LastScan => compare_scan_times(left, right),
        };
        let order = if descending { order.reverse() } else { order };
        order.then_with(|| left.hostname.cmp(&right.hostname))
    });
    filtered
}

fn bounded_count_label(count: usize) -> String {
    if count >= SCANNING_RESULT_LIMIT {
        format!("{count}+")
    } else {
        count.to_string()
    }
}

fn loaded_count_label(visible: usize, loaded: usize) -> String {
    if loaded >= SCANNING_RESULT_LIMIT {
        format!("{visible} of {} loaded", bounded_count_label(loaded))
    } else {
        format!("{visible} of {loaded}")
    }
}

fn compare_scan_times(
    left: &ScanningQueueItemResponse,
    right: &ScanningQueueItemResponse,
) -> Ordering {
    match (left.completed_at, right.completed_at) {
        (Some(left), Some(right)) => right.cmp(&left),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Renders the administrator view for CVE scan coverage and schedule policy.
///
/// Read-only scan data comes from the scanning APIs. Actions without a server
/// contract remain disabled so the view does not imply a mutation occurred.
#[component]
pub fn ScanningView() -> Element {
    let mut tab = use_signal(|| ScanTab::Deployed);
    let mut query = use_signal(String::new);
    let mut status_filter = use_signal(|| "all".to_string());
    let mut freshness_filter = use_signal(|| "all".to_string());
    let mut latest_only = use_signal(|| false);
    let mut sort = use_signal(|| ScanSort::Status);
    let mut sort_descending = use_signal(|| false);
    let mut system_query = use_signal(String::new);
    let mut system_environment = use_signal(|| "all".to_string());
    let mut expanded_system = use_signal(|| Option::<Uuid>::None);
    let mut system_scan_rows = use_signal(HashMap::<Uuid, Vec<ScanningQueueItemResponse>>::new);
    let mut system_scan_errors = use_signal(HashMap::<Uuid, String>::new);
    let mut loading_system = use_signal(|| Option::<Uuid>::None);
    let mut schedule_open = use_signal(|| false);

    let mut policy_on_build = use_signal(|| true);
    let mut policy_deployed_interval = use_signal(|| "24h".to_string());
    let mut policy_recent_interval = use_signal(|| "24h".to_string());
    let mut policy_archived_interval = use_signal(|| "168h".to_string());
    let mut policy_archived_enabled = use_signal(|| true);
    let mut policy_rebuild_to_scan = use_signal(|| false);
    let mut schedule_save_error = use_signal(|| Option::<String>::None);
    let mut schedule_saving = use_signal(|| false);

    let mut stats = use_resource(|| async { fetch_scanning_stats().await });
    let mut queue = use_resource(|| async { fetch_scanning_queue(Some(500)).await });
    let mut deployed = use_resource(|| async { fetch_scanning_deployed(Some(500), None).await });
    let mut systems = use_resource(|| async { fetch_scanning_systems(Some(500)).await });
    let environments = use_resource(|| async { fetch_environments().await });
    let mut schedule = use_resource(|| async { fetch_scanning_schedule().await });

    let mut deployed_rows = use_signal(Vec::<ScanningQueueItemResponse>::new);
    let mut deployed_cursor = use_signal(|| Option::<String>::None);
    let mut deployed_total = use_signal(|| 0_i64);
    let mut deployed_loading_more = use_signal(|| false);
    let mut deployed_load_more_error = use_signal(|| Option::<String>::None);

    use_effect(move || {
        if let Some(Ok(result)) = deployed.read().clone() {
            deployed_rows.set(result.items);
            deployed_cursor.set(result.next_cursor);
            deployed_total.set(result.total);
        }
    });

    use_effect(move || {
        let _ = tab();
        query.set(String::new());
        status_filter.set("all".to_string());
        freshness_filter.set("all".to_string());
        latest_only.set(false);
        sort.set(ScanSort::Status);
        sort_descending.set(false);
    });

    let queue_value = queue
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let systems_value = systems
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let schedule_value: Option<ScanSchedulePolicyResponse> = schedule
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned();
    let env_colors = environments
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .map(|items| {
            items
                .iter()
                .map(|environment| {
                    (
                        environment.name.to_ascii_lowercase(),
                        environment.color_hex.clone(),
                    )
                })
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();

    let deployed_count = if deployed_total() > 0 {
        deployed_total()
    } else {
        deployed_rows.read().len() as i64
    };

    rsx! {
        div { class: "scanning-view",
            div { class: "page-head scanning-head",
                div {
                    h1 { class: "page-title", "Scanning" }
                    p { class: "page-subtitle", "CVE scanning · vulnix · live data" }
                }
                div { class: "scanning-head-actions",
                    button {
                        class: "btn btn-ghost focus-ring",
                        onclick: move |_| {
                            if let Some(policy) = schedule_value.clone() {
                                policy_on_build.set(policy.on_build);
                                policy_deployed_interval.set(policy.deployed_interval);
                                policy_recent_interval.set(policy.recent_interval);
                                policy_archived_interval.set(policy.archived_interval);
                                policy_archived_enabled.set(policy.archived_enabled);
                                policy_rebuild_to_scan.set(policy.rebuild_to_scan);
                            }
                            schedule_save_error.set(None);
                            schedule_open.set(true);
                        },
                        Icon { name: IconName::Gear, size: 14 }
                        " Schedule"
                    }
                    button {
                        class: "btn btn-primary focus-ring",
                        disabled: true,
                        title: "Fleet rescan is unavailable because the server does not expose this action",
                        Icon { name: IconName::Sync, size: 14 }
                        " Rescan all"
                    }
                }
            }

            if let Some(Err(error)) = stats.read().as_ref() {
                div { role: "alert", class: "sd-callout sd-callout-danger scanning-alert",
                    div { "Scan summary could not be loaded: {error}" }
                    button { class: "btn btn-ghost xs focus-ring", onclick: move |_| stats.restart(), "Retry" }
                }
            }

            div { class: "stat-strip scanning-stats",
                if let Some(Ok(summary)) = stats.read().as_ref() {
                    { stat_card("Scanning now", &summary.scanning.to_string(), Some(&format!("{} queued", summary.queued)), "#60a5fa") }
                    { stat_card("Stale", &summary.stale.to_string(), Some("past rescan interval"), "#fbbf24") }
                    { stat_card("Never scanned", &summary.never_scanned.to_string(), None, "#9ca3af") }
                    { stat_card("Failed", &summary.failed.to_string(), None, if summary.failed > 0 { "#f87171" } else { "#34d399" }) }
                    { stat_card("Coverage", &format!("{}%", summary.coverage_percent), Some("configs with results"), "#34d399") }
                } else {
                    { stat_card("Scanning now", "—", None, "#60a5fa") }
                    { stat_card("Stale", "—", None, "#fbbf24") }
                    { stat_card("Never scanned", "—", None, "#9ca3af") }
                    { stat_card("Failed", "—", None, "#f87171") }
                    { stat_card("Coverage", "—", None, "#34d399") }
                }
            }

            section { class: "card scanning-card", aria_label: "CVE scans",
                div { class: "sd-tabs scanning-tabs", role: "tablist", aria_label: "Scan views",
                    { scan_tab_button(tab, ScanTab::Deployed, "Deployed", deployed_count.to_string()) }
                    { scan_tab_button(tab, ScanTab::All, "All scans", bounded_count_label(queue_value.len())) }
                    { scan_tab_button(tab, ScanTab::Systems, "By system", bounded_count_label(systems_value.len())) }
                }

                match tab() {
                    ScanTab::Deployed => {
                        let load_error = deployed
                            .read()
                            .as_ref()
                            .and_then(|result| result.as_ref().err())
                            .map(ToString::to_string);
                        rsx! {
                            { scan_queue_panel(
                                deployed_rows.read().clone(),
                                deployed.read().is_none(),
                                load_error,
                                false,
                                "Search deployed configs…",
                                query,
                                status_filter,
                                freshness_filter,
                                latest_only,
                                sort,
                                sort_descending,
                                move || deployed.restart(),
                            ) }
                            if let Some(error) = deployed_load_more_error() {
                                div { role: "alert", class: "sd-callout sd-callout-danger scanning-inline-alert", "{error}" }
                            }
                            if deployed_cursor.read().is_some() || deployed_loading_more() {
                                div { class: "scanning-pagination",
                                    span { "Showing {deployed_rows.read().len()} of {deployed_count} deployed configurations" }
                                    if deployed_cursor.read().is_some() {
                                        button {
                                            class: "btn btn-ghost xs focus-ring",
                                            disabled: deployed_loading_more(),
                                            onclick: move |_| {
                                                let Some(cursor) = deployed_cursor.read().clone() else { return };
                                                deployed_loading_more.set(true);
                                                deployed_load_more_error.set(None);
                                                spawn(async move {
                                                    match fetch_scanning_deployed(Some(500), Some(&cursor)).await {
                                                        Ok(result) => {
                                                            let mut rows = deployed_rows.read().clone();
                                                            rows.extend(result.items);
                                                            deployed_rows.set(rows);
                                                            deployed_cursor.set(result.next_cursor);
                                                            deployed_total.set(result.total);
                                                        }
                                                        Err(error) => deployed_load_more_error
                                                            .set(Some(format!("More deployed configurations could not be loaded: {error}"))),
                                                    }
                                                    deployed_loading_more.set(false);
                                                });
                                            },
                                            if deployed_loading_more() { "Loading…" } else { "Load more" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    ScanTab::All => {
                        let load_error = queue
                            .read()
                            .as_ref()
                            .and_then(|result| result.as_ref().err())
                            .map(ToString::to_string);
                        rsx! {
                            { scan_queue_panel(
                                queue_value.clone(),
                                queue.read().is_none(),
                                load_error,
                                true,
                                "Search all scans…",
                                query,
                                status_filter,
                                freshness_filter,
                                latest_only,
                                sort,
                                sort_descending,
                                move || queue.restart(),
                            ) }
                        }
                    }
                    ScanTab::Systems => {
                        let load_error = systems
                            .read()
                            .as_ref()
                            .and_then(|result| result.as_ref().err())
                            .map(ToString::to_string);
                        rsx! {
                            { systems_panel(
                                systems_value.clone(),
                                systems.read().is_none(),
                                load_error,
                                env_colors.clone(),
                                system_query,
                                system_environment,
                                expanded_system,
                                system_scan_rows,
                                system_scan_errors,
                                loading_system,
                                move || systems.restart(),
                            ) }
                        }
                    }
                }
            }

            if schedule_open() {
                div { class: "modal-backdrop", onclick: move |_| schedule_open.set(false),
                    div {
                        class: "modal scanning-schedule-modal",
                        role: "dialog",
                        aria_modal: "true",
                        aria_labelledby: "scan-schedule-title",
                        onclick: move |event| event.stop_propagation(),
                        div { class: "modal-head",
                            h2 { id: "scan-schedule-title", Icon { name: IconName::Gear, size: 14 } " Scan schedule" }
                            p { "Control how often vulnix rescans configurations. New and deployed configs scan most often; old configs scan least." }
                        }
                        div { class: "modal-body",
                            if let Some(Err(error)) = schedule.read().as_ref() {
                                div { role: "alert", class: "sd-callout sd-callout-danger",
                                    div { "The scan schedule could not be loaded: {error}" }
                                    button { class: "btn btn-ghost xs focus-ring", onclick: move |_| schedule.restart(), "Retry" }
                                }
                            } else if schedule_value.is_none() {
                                div { role: "status", class: "scanning-modal-state", "Loading schedule…" }
                            } else {
                                div { class: "scanning-schedule-rows",
                                    if let Some(error) = schedule_save_error() {
                                        div { role: "alert", class: "sd-callout sd-callout-danger", "{error}" }
                                    }
                                    { schedule_row("Scan on build", "Scan a freshly built config before deployment. The derivation is already in the store, so no extra build is needed.", rsx! {
                                        label { class: "scanning-toggle", input { r#type: "checkbox", checked: policy_on_build(), onchange: move |event| policy_on_build.set(event.checked()) } span { if policy_on_build() { "On" } else { "Off" } } }
                                    }) }
                                    { schedule_row("Deployed configs", "Currently running on at least one system. Rescan these configs to detect newly published advisories.", interval_select(policy_deployed_interval, false)) }
                                    { schedule_row("Recent configs", "Built in the last 30 days but not currently deployed.", interval_select(policy_recent_interval, false)) }
                                    { schedule_row("Archived configs", "Old or superseded configs. Scan these configs rarely or never to reduce builder load.", rsx! {
                                        div { class: "scanning-archive-control", input { aria_label: "Enable archived config scans", r#type: "checkbox", checked: policy_archived_enabled(), onchange: move |event| policy_archived_enabled.set(event.checked()) } { interval_select(policy_archived_interval, !policy_archived_enabled()) } }
                                    }) }
                                    { schedule_row("Rebuild to scan old configs", "Archived configs evicted from cache must be rebuilt before vulnix can scan them. When off, the scanner skips uncached configs.", rsx! {
                                        label { class: "scanning-toggle", input { r#type: "checkbox", checked: policy_rebuild_to_scan(), onchange: move |event| policy_rebuild_to_scan.set(event.checked()) } span { if policy_rebuild_to_scan() { "On" } else { "Off" } } }
                                    }) }
                                    div { class: "sd-callout sd-callout-info scanning-load-note",
                                        Icon { name: IconName::Shield, size: 12 }
                                        div { "Estimated load: " if policy_on_build() { "every build" } else { "no build" } " scans plus periodic rescans. Deployed configs at " strong { "{policy_deployed_interval()}" } " dominate builder cost." }
                                    }
                                }
                            }
                        }
                        div { class: "modal-foot",
                            button { class: "btn btn-ghost focus-ring", disabled: schedule_saving(), onclick: move |_| schedule_open.set(false), "Cancel" }
                            button {
                                class: "btn btn-primary focus-ring",
                                disabled: schedule_value.is_none() || schedule_saving(),
                                onclick: move |_| {
                                    let request = UpdateScanSchedulePolicyRequest {
                                        on_build: policy_on_build(),
                                        deployed_interval: policy_deployed_interval(),
                                        recent_interval: policy_recent_interval(),
                                        archived_interval: policy_archived_interval(),
                                        archived_enabled: policy_archived_enabled(),
                                        rebuild_to_scan: policy_rebuild_to_scan(),
                                    };
                                    schedule_save_error.set(None);
                                    schedule_saving.set(true);
                                    spawn(async move {
                                        match update_scanning_schedule(&request).await {
                                            Ok(_) => {
                                                schedule.restart();
                                                schedule_open.set(false);
                                            }
                                            Err(error) => schedule_save_error.set(Some(format!("The scan schedule could not be saved: {error}"))),
                                        }
                                        schedule_saving.set(false);
                                    });
                                },
                                Icon { name: IconName::Check, size: 13 }
                                if schedule_saving() { " Saving…" } else { " Save schedule" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn scan_tab_button(
    mut tab: Signal<ScanTab>,
    value: ScanTab,
    label: &'static str,
    count: String,
) -> Element {
    let selected = tab() == value;
    rsx! {
        button {
            class: if selected { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
            role: "tab",
            aria_selected: selected,
            onclick: move |_| tab.set(value),
            "{label}"
            span { class: "sd-tab-badge", "{count}" }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn scan_queue_panel(
    rows: Vec<ScanningQueueItemResponse>,
    loading: bool,
    error: Option<String>,
    show_freshness: bool,
    placeholder: &'static str,
    mut query: Signal<String>,
    mut status_filter: Signal<String>,
    mut freshness_filter: Signal<String>,
    mut latest_only: Signal<bool>,
    mut sort: Signal<ScanSort>,
    mut sort_descending: Signal<bool>,
    retry: impl FnMut() + 'static,
) -> Element {
    let mut retry = retry;
    let sorted = filter_and_sort_rows(
        &rows,
        &query(),
        &status_filter(),
        &freshness_filter(),
        latest_only(),
        sort(),
        sort_descending(),
    );
    let statuses = [
        "failed",
        "awaiting",
        "scanning",
        "queued",
        "stale",
        "complete",
        "needs-build",
        "unscanned",
        "unknown",
    ]
    .into_iter()
    .filter(|key| rows.iter().any(|row| status_meta(&row.status).key == *key))
    .collect::<Vec<_>>();
    rsx! {
        div { class: "scan-toolbar",
            div { class: "q-search scanning-search",
                Icon { name: IconName::Search, size: 13 }
                input {
                    class: "q-search-input",
                    aria_label: "Search scans",
                    placeholder,
                    value: query(),
                    oninput: move |event| query.set(event.value()),
                }
                if !query().is_empty() {
                    button { class: "btn-icon xs focus-ring", aria_label: "Clear search", onclick: move |_| query.set(String::new()), Icon { name: IconName::X, size: 13 } }
                }
            }
            select {
                class: "input filter-select focus-ring",
                aria_label: "Filter by scan status",
                value: status_filter(),
                oninput: move |event| status_filter.set(event.value()),
                option { value: "all", "All statuses" }
                for status in statuses {
                    option { value: "{status}", "{status_meta(status).label}" }
                }
            }
            if show_freshness {
                select {
                    class: "input filter-select focus-ring",
                    aria_label: "Filter by revision freshness",
                    value: freshness_filter(),
                    oninput: move |event| freshness_filter.set(event.value()),
                    option { value: "all", "All revisions" }
                    option { value: "deployed", "Deployed" }
                    option { value: "recent", "Recent" }
                    option { value: "archived", "Archived" }
                    option { value: "unknown", "Unknown" }
                }
            }
            button {
                class: if latest_only() { "btn btn-ghost xs focus-ring active-filter" } else { "btn btn-ghost xs focus-ring" },
                aria_pressed: latest_only(),
                title: "Show only the latest known commit per flake",
                onclick: move |_| latest_only.toggle(),
                Icon { name: IconName::Star, size: 12 }
                " Latest per flake"
            }
            span { class: "filter-count", "{loaded_count_label(sorted.len(), rows.len())}" }
        }

        if let Some(error) = error {
            div { class: "q-empty", role: "alert",
                Icon { name: IconName::Warn, size: 20 }
                h3 { "Scans could not be loaded" }
                div { "{error}" }
                button { class: "btn btn-ghost xs focus-ring", onclick: move |_| retry(), "Retry" }
            }
        } else if loading {
            div { class: "q-empty", role: "status", Icon { name: IconName::Sync, size: 20 } div { "Loading scans…" } }
        } else if sorted.is_empty() {
            if rows.is_empty() {
                div { class: "q-empty",
                    h3 { "No scans yet" }
                    div { "Scan results will appear here when the server records them." }
                }
            } else {
                div { class: "q-empty",
                    Icon { name: IconName::Search, size: 20 }
                    div { "No scans match these filters." }
                    button {
                        class: "btn btn-ghost xs focus-ring",
                        onclick: move |_| {
                            query.set(String::new());
                            status_filter.set("all".to_string());
                            freshness_filter.set("all".to_string());
                            latest_only.set(false);
                        },
                        "Reset filters"
                    }
                }
            }
        } else {
            div { class: "scanning-table-wrap",
                table { class: "sys-table scanning-table",
                    thead { tr {
                        { sortable_header("Config", ScanSort::Name, sort, sort_descending) }
                        if show_freshness { { sortable_header("Revision", ScanSort::Freshness, sort, sort_descending) } }
                        { sortable_header("Status", ScanSort::Status, sort, sort_descending) }
                        { sortable_header("Findings", ScanSort::Findings, sort, sort_descending) }
                        { sortable_header("Last scan", ScanSort::LastScan, sort, sort_descending) }
                        th { "Trigger" }
                        th { class: "scanning-actions-heading", span { class: "sr-only", "Actions" } }
                    } }
                    tbody {
                        for (index, row) in sorted.iter().enumerate() {
                            { scan_row(row, index, show_freshness) }
                        }
                    }
                }
            }
        }
    }
}

fn sortable_header(
    label: &'static str,
    key: ScanSort,
    mut sort: Signal<ScanSort>,
    mut descending: Signal<bool>,
) -> Element {
    let active = sort() == key;
    let aria_sort = if !active {
        "none"
    } else if matches!(key, ScanSort::Findings | ScanSort::LastScan) != descending() {
        "descending"
    } else {
        "ascending"
    };
    rsx! {
        th { aria_sort,
            button {
                class: if active { "th-sort focus-ring on" } else { "th-sort focus-ring" },
                onclick: move |_| {
                    if sort() == key {
                        descending.toggle();
                    } else {
                        sort.set(key);
                        descending.set(false);
                    }
                },
                "{label}"
                Icon { name: if active && descending() { IconName::ChevronDown } else { IconName::ChevronUp }, size: 10 }
            }
        }
    }
}

fn scan_row(row: &ScanningQueueItemResponse, index: usize, show_freshness: bool) -> Element {
    let nav = navigator();
    let meta = status_meta(&row.status);
    let has_important_findings = row.critical_count > 0 || row.high_count > 0;
    let row_key = row.scan_id.map(|id| id.to_string()).unwrap_or_else(|| {
        format!(
            "{}-{}-{index}",
            row.hostname,
            commit_label(&row.commit_hash)
        )
    });
    rsx! {
        tr { key: "{row_key}",
            td {
                div { class: "scanning-config-name", "{row.hostname}" }
                div { class: "mono scanning-config-revision",
                    if let Some(flake) = row.flake_name.as_deref() { "{flake} · " }
                    if row.is_latest_per_flake { span { class: "latest-star", title: "Latest known commit for this flake", Icon { name: IconName::Star, size: 9 } } }
                    span { title: row.commit_hash.as_deref().unwrap_or("Commit unavailable"), "{commit_label(&row.commit_hash)}" }
                }
            }
            if show_freshness { td { { freshness_chip(&row.freshness) } } }
            td {
                span { class: "chip {meta.class}", span { class: "chip-dot", style: "background:{meta.color};" } "{meta.label}" }
                if meta.key == "scanning" {
                    div { class: "scanning-running", span { class: "scan-pulse" } "running" }
                }
            }
            td { { findings_cell(row) } }
            td { class: "scanning-last-scan", "{last_scan(row)}" }
            td {
                if let Some(trigger) = row.trigger.as_deref().filter(|trigger| !trigger.is_empty()) {
                    span { class: "chip chip-unknown scanning-trigger", "{trigger}" }
                } else {
                    span { class: "scanning-unavailable", title: "Trigger provenance is not recorded by the server", "—" }
                }
            }
            td {
                div { class: "row-actions scanning-row-actions",
                    button { class: "btn-icon focus-ring", disabled: true, title: "Scan logs are not available from the server", aria_label: "Scan log unavailable", Icon { name: IconName::Terminal, size: 14 } }
                    button { class: "btn-icon focus-ring", disabled: true, title: "Rescan is not available from the server", aria_label: "Rescan unavailable", Icon { name: IconName::Sync, size: 14 } }
                    if has_important_findings {
                        button { class: "btn-icon focus-ring", title: "View CVEs", aria_label: "View CVEs", onclick: move |_| { let _ = nav.push(Route::CvesView { query: String::new() }); }, Icon { name: IconName::ArrowRight, size: 14 } }
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn systems_panel(
    rows: Vec<crate::api::models::ScanningSystemsItemResponse>,
    loading: bool,
    error: Option<String>,
    env_colors: HashMap<String, String>,
    mut query: Signal<String>,
    mut environment: Signal<String>,
    mut expanded: Signal<Option<Uuid>>,
    mut scans: Signal<HashMap<Uuid, Vec<ScanningQueueItemResponse>>>,
    mut scan_errors: Signal<HashMap<Uuid, String>>,
    mut loading_system: Signal<Option<Uuid>>,
    retry: impl FnMut() + 'static,
) -> Element {
    let mut retry = retry;
    let search = query().trim().to_ascii_lowercase();
    let selected_environment = environment();
    let mut environments = rows
        .iter()
        .filter_map(|row| row.environment.clone())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    environments.sort();
    environments.dedup();
    let mut visible = rows
        .iter()
        .filter(|row| {
            (search.is_empty() || row.hostname.to_ascii_lowercase().contains(&search))
                && (selected_environment == "all"
                    || row.environment.as_deref() == Some(selected_environment.as_str()))
        })
        .cloned()
        .collect::<Vec<_>>();
    visible.sort_by(|left, right| {
        right
            .total_configs
            .cmp(&left.total_configs)
            .then_with(|| left.hostname.cmp(&right.hostname))
    });
    let visible_configs = visible.iter().map(|row| row.total_configs).sum::<i64>();
    let visible_summary = if rows.len() >= SCANNING_RESULT_LIMIT {
        format!(
            "{} systems · {visible_configs} loaded configs",
            loaded_count_label(visible.len(), rows.len())
        )
    } else {
        format!("{} systems · {visible_configs} configs", visible.len())
    };

    rsx! {
        div { class: "scan-toolbar",
            div { class: "q-search scanning-search",
                Icon { name: IconName::Search, size: 13 }
                input { class: "q-search-input", aria_label: "Search systems", placeholder: "Search systems…", value: query(), oninput: move |event| query.set(event.value()) }
                if !query().is_empty() { button { class: "btn-icon xs focus-ring", aria_label: "Clear search", onclick: move |_| query.set(String::new()), Icon { name: IconName::X, size: 13 } } }
            }
            select { class: "input filter-select focus-ring", aria_label: "Filter systems by environment", value: environment(), oninput: move |event| environment.set(event.value()),
                option { value: "all", "All environments" }
                for value in environments { option { value: "{value}", "{value}" } }
            }
            span { class: "filter-count", "{visible_summary}" }
        }

        if let Some(error) = error {
            div { class: "q-empty", role: "alert", Icon { name: IconName::Warn, size: 20 } h3 { "Systems could not be loaded" } div { "{error}" } button { class: "btn btn-ghost xs focus-ring", onclick: move |_| retry(), "Retry" } }
        } else if loading {
            div { class: "q-empty", role: "status", Icon { name: IconName::Sync, size: 20 } div { "Loading systems…" } }
        } else if visible.is_empty() {
            if rows.is_empty() {
                div { class: "q-empty", h3 { "No system scan history yet" } div { "System scan history will appear here when the server records it." } }
            } else {
                div { class: "q-empty", Icon { name: IconName::Search, size: 20 } div { "No systems match these filters." }
                    button { class: "btn btn-ghost xs focus-ring", onclick: move |_| { query.set(String::new()); environment.set("all".to_string()); }, "Reset filters" }
                }
            }
        } else {
            div { class: "scanning-table-wrap",
                table { class: "sys-table scanning-table scanning-systems-table",
                    thead { tr { th { "System" } th { "Env" } th { "Configs" } th { "Scan freshness" } th { "Current findings" } th { class: "scanning-actions-heading", span { class: "sr-only", "Actions" } } } }
                    tbody {
                        for system in visible {
                            {
                                let system_id = system.system_id;
                                let is_open = expanded() == Some(system_id);
                                let system_rows = scans.read().get(&system_id).cloned().unwrap_or_default();
                                let system_error = scan_errors.read().get(&system_id).cloned();
                                let is_loading = loading_system() == Some(system_id);
                                let total = system.total_configs.max(1) as f64;
                                let scanned_width = system.scanned as f64 / total * 100.0;
                                let stale_width = system.stale as f64 / total * 100.0;
                                let needs_width = system.needs_build as f64 / total * 100.0;
                                let unscanned_width = system.unscanned as f64 / total * 100.0;
                                let history_count = if system_rows.len() >= SCANNING_RESULT_LIMIT {
                                    format!("{} configs loaded for this system", bounded_count_label(system_rows.len()))
                                } else {
                                    format!("{} configs for this system", system_rows.len())
                                };
                                rsx! {
                                    tr { key: "system-{system_id}", class: if is_open { "scanning-system-row expanded" } else { "scanning-system-row" },
                                        td {
                                            button {
                                                class: "scanning-system-toggle focus-ring",
                                                aria_expanded: is_open,
                                                onclick: move |_| toggle_system(system_id, expanded, scans, scan_errors, loading_system),
                                                Icon { name: if is_open { IconName::ChevronDown } else { IconName::ChevronRight }, size: 12 }
                                                div { span { class: "scanning-config-name", "{system.hostname}" } }
                                            }
                                        }
                                        td {
                                            if let Some(name) = system.environment.clone() {
                                                if let Some(color) = env_colors.get(&name.to_ascii_lowercase()) {
                                                    EnvBadge { name, fg: color.clone(), bg: format!("color-mix(in oklab, {color} 14%, var(--cf-card-bg))"), border: color.clone() }
                                                } else { EnvBadge { name } }
                                            } else { span { class: "scanning-unavailable", "—" } }
                                        }
                                        td { class: "mono", "{system.total_configs}" }
                                        td {
                                            div { class: "scanning-freshness", title: "{system.scanned} fresh · {system.stale} stale · {system.needs_build} need build · {system.unscanned} never scanned",
                                                div { class: "scanning-freshness-bar", div { style: "width:{scanned_width}%; background:#34d399;" } div { style: "width:{stale_width}%; background:#fbbf24;" } div { style: "width:{needs_width}%; background:#f59e0b;" } div { style: "width:{unscanned_width}%; background:#4b5563;" } }
                                                span { class: "mono", "{system.scanned}/{system.total_configs}" }
                                            }
                                            div { class: "scanning-freshness-legend", span { class: "fresh", "{system.scanned} fresh" } if system.stale > 0 { span { class: "stale", "{system.stale} stale" } } if system.needs_build > 0 { span { class: "needs", "{system.needs_build} need build" } } if system.unscanned > 0 { span { "{system.unscanned} never" } } }
                                        }
                                        td { { aggregate_findings(system.current_crit, system.current_high) } }
                                        td { div { class: "row-actions scanning-row-actions", button { class: "btn-icon focus-ring", disabled: true, title: "Rescan is not available from the server", aria_label: "Rescan current configuration unavailable", Icon { name: IconName::Sync, size: 14 } } } }
                                    }
                                    if is_open {
                                        tr { class: "scan-sys-expand-row", td { colspan: 6,
                                            div { class: "scan-sys-expand",
                                                div { class: "scan-sys-expand-head", span { "{history_count} · newest first" } button { class: "btn btn-ghost xs focus-ring", disabled: true, title: "Rescan is not available from the server", Icon { name: IconName::Sync, size: 10 } " Rescan all" } }
                                                if let Some(error) = system_error {
                                                    div { class: "q-empty scanning-system-state", role: "alert", div { "Scan history could not be loaded: {error}" } button { class: "btn btn-ghost xs focus-ring", onclick: move |_| toggle_system_reload(system_id, scans, scan_errors, loading_system), "Retry" } }
                                                } else if is_loading {
                                                    div { class: "q-empty scanning-system-state", role: "status", "Loading scan history…" }
                                                } else if system_rows.is_empty() {
                                                    div { class: "q-empty scanning-system-state", "No scan history is available for this system." }
                                                } else {
                                                    div { class: "scan-sys-expand-table-wrap",
                                                        table { class: "scanning-history-table", thead { tr { th { "Commit" } th { "Freshness" } th { "Status" } th { "Findings" } th { "Last scan" } th { span { class: "sr-only", "Actions" } } } }
                                                            tbody { for (index, row) in system_rows.iter().enumerate() { { system_scan_row(row, index) } } }
                                                        }
                                                    }
                                                }
                                            }
                                        } }
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

fn toggle_system(
    system_id: Uuid,
    mut expanded: Signal<Option<Uuid>>,
    scans: Signal<HashMap<Uuid, Vec<ScanningQueueItemResponse>>>,
    errors: Signal<HashMap<Uuid, String>>,
    loading: Signal<Option<Uuid>>,
) {
    if expanded() == Some(system_id) {
        expanded.set(None);
        return;
    }
    expanded.set(Some(system_id));
    if !scans.read().contains_key(&system_id) && !errors.read().contains_key(&system_id) {
        load_system_scans(system_id, scans, errors, loading);
    }
}

fn toggle_system_reload(
    system_id: Uuid,
    scans: Signal<HashMap<Uuid, Vec<ScanningQueueItemResponse>>>,
    mut errors: Signal<HashMap<Uuid, String>>,
    loading: Signal<Option<Uuid>>,
) {
    errors.write().remove(&system_id);
    load_system_scans(system_id, scans, errors, loading);
}

fn load_system_scans(
    system_id: Uuid,
    mut scans: Signal<HashMap<Uuid, Vec<ScanningQueueItemResponse>>>,
    mut errors: Signal<HashMap<Uuid, String>>,
    mut loading: Signal<Option<Uuid>>,
) {
    loading.set(Some(system_id));
    spawn(async move {
        match fetch_scanning_system_scans(&system_id, Some(500)).await {
            Ok(rows) => {
                scans.write().insert(system_id, rows);
                errors.write().remove(&system_id);
            }
            Err(error) => {
                errors.write().insert(system_id, error.to_string());
            }
        }
        if loading() == Some(system_id) {
            loading.set(None);
        }
    });
}

fn system_scan_row(row: &ScanningQueueItemResponse, index: usize) -> Element {
    let nav = navigator();
    let meta = status_meta(&row.status);
    let needs_build = meta.key == "needs-build";
    let has_important_findings = row.critical_count > 0 || row.high_count > 0;
    let key = row
        .scan_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| format!("{}-{index}", commit_label(&row.commit_hash)));
    rsx! {
        tr { key: "history-{key}", class: "scan-sys-commit-row no-log",
            td { div { class: "scanning-history-commit", if row.is_latest_per_flake { span { class: "latest-star", title: "Latest known commit for this flake", Icon { name: IconName::Star, size: 9 } } } span { class: "mono", title: row.commit_hash.as_deref().unwrap_or("Commit unavailable"), "{commit_label(&row.commit_hash)}" } if row.is_current { span { class: "chip chip-info scanning-current", "current" } } } if let Some(flake) = row.flake_name.as_deref() { div { class: "scanning-history-flake", "{flake}" } } }
            td { { freshness_chip(&row.freshness) } }
            td { span { class: "chip {meta.class}", span { class: "chip-dot", style: "background:{meta.color};" } "{meta.label}" } }
            td { { findings_cell(row) } }
            td { class: "scanning-last-scan", "{last_scan(row)}" }
            td { div { class: "row-actions scanning-row-actions",
                if needs_build { button { class: "btn btn-ghost xs focus-ring", disabled: true, title: "Build and scan is not available from the server", Icon { name: IconName::Cpu, size: 11 } " Build & scan" } }
                else { button { class: "btn-icon focus-ring", disabled: true, title: "Scan logs are not available from the server", aria_label: "Scan log unavailable", Icon { name: IconName::Terminal, size: 13 } } }
                if has_important_findings { button { class: "btn-icon focus-ring", title: "View CVEs", aria_label: "View CVEs", onclick: move |_| { let _ = nav.push(Route::CvesView { query: String::new() }); }, Icon { name: IconName::ArrowRight, size: 13 } } }
            } }
        }
    }
}

fn freshness_chip(freshness: &str) -> Element {
    let (class, label) = match normalize_freshness(freshness) {
        "deployed" => ("chip-healthy", "deployed"),
        "recent" => ("chip-info", "recent"),
        "archived" => ("chip-unknown", "archived"),
        _ => ("chip-unknown", "unknown"),
    };
    rsx! { span { class: "chip {class} scanning-freshness-chip", "{label}" } }
}

fn can_assert_clean(row: &ScanningQueueItemResponse) -> bool {
    status_meta(&row.status).key == "complete"
        && row.critical_count == 0
        && row.high_count == 0
        && row.medium_count == 0
}

fn findings_cell(row: &ScanningQueueItemResponse) -> Element {
    if status_meta(&row.status).key != "complete" {
        return rsx! { span { class: "scanning-unavailable", "—" } };
    }
    rsx! {
        div { class: "scanning-findings",
            if row.critical_count > 0 { span { class: "chip chip-critical", "{row.critical_count}C" } }
            if row.high_count > 0 { span { class: "chip chip-warning", "{row.high_count}H" } }
            if row.medium_count > 0 { span { class: "chip chip-info", "{row.medium_count}M" } }
            if can_assert_clean(row) { span { class: "chip chip-healthy", Icon { name: IconName::Check, size: 9 } " clean" } }
        }
    }
}

fn aggregate_findings(critical: i64, high: i64) -> Element {
    rsx! {
        div { class: "scanning-findings",
            if critical > 0 { span { class: "chip chip-critical", "{critical}C" } }
            if high > 0 { span { class: "chip chip-warning", "{high}H" } }
            if critical == 0 && high == 0 { span { class: "chip chip-healthy", "0 critical/high" } }
        }
    }
}

fn last_scan(row: &ScanningQueueItemResponse) -> String {
    match row.completed_at {
        Some(completed_at) => {
            let age = chrono::Utc::now().signed_duration_since(completed_at);
            if age.num_minutes() < 1 {
                "just now".to_string()
            } else if age.num_hours() < 1 {
                format!("{}m ago", age.num_minutes())
            } else if age.num_days() < 1 {
                format!("{}h ago", age.num_hours())
            } else {
                format!("{}d ago", age.num_days())
            }
        }
        None if status_meta(&row.status).key == "scanning" => "scanning…".to_string(),
        None if status_meta(&row.status).key == "queued" => "pending".to_string(),
        None if status_meta(&row.status).key == "unscanned" => "never".to_string(),
        None => "—".to_string(),
    }
}

fn commit_label(commit_hash: &Option<String>) -> String {
    match commit_hash {
        Some(hash) if !hash.is_empty() => hash.chars().take(12).collect(),
        _ => "unknown".to_string(),
    }
}

fn interval_select(mut value: Signal<String>, disabled: bool) -> Element {
    rsx! {
        select { class: "input focus-ring", aria_label: "Scan interval", disabled, value: value(), oninput: move |event| value.set(event.value()),
            for option in ["1h", "6h", "12h", "24h", "7d", "30d", "168h", "336h", "never"] {
                option { value: "{option}", if option == "never" { "Never" } else { "Every {option}" } }
            }
        }
    }
}

fn schedule_row(title: &str, description: &str, control: Element) -> Element {
    rsx! {
        div { class: "scanning-schedule-row",
            div { div { class: "scanning-schedule-title", "{title}" } div { class: "scanning-schedule-description", "{description}" } }
            div { class: "scanning-schedule-control", {control} }
        }
    }
}

fn stat_card(label: &str, value: &str, meta: Option<&str>, color: &str) -> Element {
    rsx! {
        div { class: "stat", span { class: "stat-accent", style: "--stat-color:{color};" } div { class: "stat-label", "{label}" } div { class: "stat-value", style: "color:{color};", "{value}" } if let Some(meta) = meta { div { class: "stat-meta", "{meta}" } } }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};

    use super::*;

    fn row(
        hostname: &str,
        status: &str,
        freshness: &str,
        critical: i32,
        latest: bool,
    ) -> ScanningQueueItemResponse {
        ScanningQueueItemResponse {
            scan_id: None,
            hostname: hostname.to_string(),
            flake_name: Some("infra".to_string()),
            commit_hash: Some(format!("{hostname}-commit")),
            status: status.to_string(),
            completed_at: Some(Utc::now() - Duration::hours(i64::from(critical + 1))),
            scheduled_at: None,
            critical_count: critical,
            high_count: 0,
            medium_count: 0,
            freshness: freshness.to_string(),
            is_current: false,
            is_latest_per_flake: latest,
            trigger: None,
        }
    }

    #[test]
    fn normalizes_server_status_and_freshness_vocabularies() {
        assert_eq!(status_meta("in_progress").key, "scanning");
        assert_eq!(status_meta("pending").key, "queued");
        assert_eq!(status_meta("completed").key, "complete");
        assert_eq!(status_meta("unexpected").key, "unknown");
        assert_eq!(normalize_freshness("deployed"), "deployed");
        assert_eq!(normalize_freshness(""), "unknown");
    }

    #[test]
    fn marks_only_capped_result_counts_as_bounded() {
        assert_eq!(bounded_count_label(499), "499");
        assert_eq!(bounded_count_label(500), "500+");
        assert_eq!(loaded_count_label(12, 499), "12 of 499");
        assert_eq!(loaded_count_label(12, 500), "12 of 500+ loaded");
    }

    #[test]
    fn filters_across_identity_status_freshness_and_latest_marker() {
        let rows = vec![
            row("atlas", "completed", "deployed", 0, true),
            row("gaia", "failed", "recent", 0, false),
        ];
        assert_eq!(
            filter_and_sort_rows(
                &rows,
                "atl",
                "complete",
                "deployed",
                true,
                ScanSort::Name,
                false
            )
            .iter()
            .map(|row| row.hostname.as_str())
            .collect::<Vec<_>>(),
            vec!["atlas"]
        );
        assert!(
            filter_and_sort_rows(&rows, "gaia", "all", "all", true, ScanSort::Name, false)
                .is_empty()
        );
    }

    #[test]
    fn sorts_status_and_findings_with_stable_hostname_tiebreaker() {
        let rows = vec![
            row("zeta", "completed", "deployed", 0, true),
            row("beta", "failed", "deployed", 0, true),
            row("alpha", "completed", "deployed", 2, true),
        ];
        let by_status =
            filter_and_sort_rows(&rows, "", "all", "all", false, ScanSort::Status, false);
        assert_eq!(
            by_status
                .iter()
                .map(|row| row.hostname.as_str())
                .collect::<Vec<_>>(),
            vec!["beta", "alpha", "zeta"]
        );
        let by_findings =
            filter_and_sort_rows(&rows, "", "all", "all", false, ScanSort::Findings, false);
        assert_eq!(
            by_findings
                .iter()
                .map(|row| row.hostname.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha", "beta", "zeta"]
        );
    }
}
