use std::collections::{HashMap, HashSet};
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use uuid::Uuid;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, closure::Closure};

use crate::api::client::{
    fetch_environments, fetch_scanning_scan_detail, fetch_scanning_scan_records,
    fetch_scanning_schedule, fetch_scanning_stats, fetch_scanning_system_scans,
    fetch_scanning_systems, trigger_cve_derivation_rescan, update_scanning_archive_state,
    update_scanning_schedule,
};
use crate::api::models::{
    ScanSchedulePolicyResponse, ScanningQueueItemResponse, ScanningScanDetailResponse,
    ScanningScanRecordResponse, ScanningScanRecordsResponse, ScanningSystemsItemResponse,
    UpdateScanSchedulePolicyRequest,
};
use crate::components::chips::EnvBadge;
use crate::components::dialog_focus::{
    DialogFocusBoundary, DialogFocusRestore, DialogFocusSentinel, DialogInitialFocus,
};
use crate::components::icon::{Icon, IconName};

const RECORD_LIMIT: i64 = 10_000;
const DETAIL_POLL_MS: u32 = 3_000;
const LIVE_REFRESH_MS: u32 = 15_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScanTab {
    Active,
    Completed,
    Systems,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScanDetailTab {
    Log,
    Details,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScanSort {
    Configuration,
    Revision,
    Status,
    Severity,
    Timestamp,
}

#[derive(Clone, Copy)]
struct StatusMeta {
    key: &'static str,
    class: &'static str,
    color: &'static str,
    label: &'static str,
}

#[derive(Clone, PartialEq, Eq)]
struct ScanActionFeedback {
    message: String,
    success: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct ScanDetailSelection {
    scan_id: Uuid,
    label: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ScanDetailRequest {
    scan_id: Uuid,
    generation: u64,
}

#[derive(Clone, PartialEq)]
enum ScanDetailState {
    Loading,
    Loaded(ScanningScanDetailResponse),
    Error(String),
}

#[derive(Clone, PartialEq)]
struct SystemHistoryData {
    scans: ScanningScanRecordsResponse,
    derivations: Vec<ScanningQueueItemResponse>,
}

#[derive(Clone, PartialEq)]
enum SystemHistoryEntry {
    Scan(ScanningScanRecordResponse),
    NoScan(ScanningQueueItemResponse),
}

fn visible_record_total(response: &ScanningScanRecordsResponse, include_archived: bool) -> i64 {
    if include_archived {
        response.total
    } else {
        response.total.saturating_sub(response.hidden_archived)
    }
}

fn system_history_entries(data: SystemHistoryData) -> Vec<SystemHistoryEntry> {
    let mut entries = data
        .scans
        .items
        .into_iter()
        .map(SystemHistoryEntry::Scan)
        .collect::<Vec<_>>();
    entries.extend(
        data.derivations
            .into_iter()
            .filter(|row| row.scan_id.is_none())
            .map(SystemHistoryEntry::NoScan),
    );
    entries.sort_by(|left, right| match (left, right) {
        (SystemHistoryEntry::Scan(left), SystemHistoryEntry::Scan(right)) => record_time(right)
            .cmp(&record_time(left))
            .then_with(|| left.scan_id.cmp(&right.scan_id)),
        (SystemHistoryEntry::Scan(_), SystemHistoryEntry::NoScan(_)) => std::cmp::Ordering::Less,
        (SystemHistoryEntry::NoScan(_), SystemHistoryEntry::Scan(_)) => std::cmp::Ordering::Greater,
        (SystemHistoryEntry::NoScan(left), SystemHistoryEntry::NoScan(right)) => right
            .is_current
            .cmp(&left.is_current)
            .then_with(|| right.is_latest_per_flake.cmp(&left.is_latest_per_flake))
            .then_with(|| right.derivation_id.cmp(&left.derivation_id)),
    });
    entries
}

fn status_meta(status: &str) -> StatusMeta {
    match status {
        "in_progress" => StatusMeta {
            key: "in_progress",
            class: "chip-info",
            color: "#60a5fa",
            label: "Scanning",
        },
        "pending" => StatusMeta {
            key: "pending",
            class: "chip-info",
            color: "#a78bfa",
            label: "Queued",
        },
        "awaiting_build" => StatusMeta {
            key: "awaiting_build",
            class: "chip-warning",
            color: "#f59e0b",
            label: "Awaiting build",
        },
        "awaiting_closure" => StatusMeta {
            key: "awaiting_closure",
            class: "chip-unknown",
            color: "#94a3b8",
            label: "Awaiting closure",
        },
        "completed" => StatusMeta {
            key: "completed",
            class: "chip-healthy",
            color: "#34d399",
            label: "Completed",
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
        "needs_build" | "needs-build" => StatusMeta {
            key: "needs_build",
            class: "chip-warning",
            color: "#f59e0b",
            label: "Needs build",
        },
        "never_scanned" | "unscanned" => StatusMeta {
            key: "never_scanned",
            class: "chip-unknown",
            color: "#9ca3af",
            label: "Never scanned",
        },
        _ => StatusMeta {
            key: "unknown",
            class: "chip-unknown",
            color: "#9ca3af",
            label: "Unknown",
        },
    }
}

fn status_rank(status: &str) -> u8 {
    match status_meta(status).key {
        "failed" => 0,
        "awaiting_build" => 1,
        "awaiting_closure" => 2,
        "in_progress" => 3,
        "pending" => 4,
        "stale" => 5,
        "completed" => 6,
        "needs_build" => 7,
        "never_scanned" => 8,
        _ => 9,
    }
}

fn record_time(row: &ScanningScanRecordResponse) -> DateTime<Utc> {
    row.completed_at
        .or(row.scheduled_at)
        .unwrap_or(row.created_at)
}

fn severity_counts(row: &ScanningScanRecordResponse) -> (i32, i32, i32, i32) {
    (
        row.critical_count,
        row.high_count,
        row.medium_count,
        row.low_count,
    )
}

fn revision_class(row: &ScanningScanRecordResponse) -> &'static str {
    if row.is_current {
        "deployed"
    } else if row.is_latest_per_flake {
        "recent"
    } else {
        "superseded"
    }
}

#[allow(clippy::too_many_arguments)]
fn filter_and_sort_records(
    rows: &[ScanningScanRecordResponse],
    query: &str,
    status: &str,
    revision: &str,
    latest_only: bool,
    sort: ScanSort,
    descending: bool,
) -> Vec<ScanningScanRecordResponse> {
    let query = query.trim().to_ascii_lowercase();
    let mut filtered = rows
        .iter()
        .filter(|row| {
            let identity = format!(
                "{} {} {} {} {}",
                row.hostname,
                row.flake_name.as_deref().unwrap_or_default(),
                row.commit_hash.as_deref().unwrap_or_default(),
                row.scan_id,
                row.derivation_id
            )
            .to_ascii_lowercase();
            (query.is_empty() || identity.contains(&query))
                && (status == "all" || status_meta(&row.status).key == status)
                && (revision == "all" || revision_class(row) == revision)
                && (!latest_only || row.is_latest_per_flake)
        })
        .cloned()
        .collect::<Vec<_>>();

    filtered.sort_by(|left, right| {
        let order = match sort {
            ScanSort::Configuration => left
                .hostname
                .to_ascii_lowercase()
                .cmp(&right.hostname.to_ascii_lowercase()),
            ScanSort::Revision => left
                .commit_hash
                .as_deref()
                .unwrap_or_default()
                .cmp(right.commit_hash.as_deref().unwrap_or_default()),
            ScanSort::Status => status_rank(&left.status).cmp(&status_rank(&right.status)),
            ScanSort::Severity => severity_counts(left).cmp(&severity_counts(right)),
            ScanSort::Timestamp => record_time(left).cmp(&record_time(right)),
        };
        let order = if descending { order.reverse() } else { order };
        order
            .then_with(|| left.hostname.cmp(&right.hostname))
            .then_with(|| left.commit_hash.cmp(&right.commit_hash))
            .then_with(|| left.scan_id.cmp(&right.scan_id))
    });
    filtered
}

fn first_failed(rows: &[ScanningScanRecordResponse]) -> Option<ScanDetailSelection> {
    let mut failures = rows
        .iter()
        .filter(|row| row.status == "failed")
        .collect::<Vec<_>>();
    failures.sort_by(|left, right| {
        record_time(right)
            .cmp(&record_time(left))
            .then_with(|| left.scan_id.cmp(&right.scan_id))
    });
    failures.first().map(|row| ScanDetailSelection {
        scan_id: row.scan_id,
        label: format!("{} · {}", row.hostname, commit_label(&row.commit_hash)),
    })
}

fn scan_detail_request_is_current(
    request: ScanDetailRequest,
    selected: Option<&ScanDetailSelection>,
    generation: u64,
) -> bool {
    request.generation == generation
        && selected.map(|selection| selection.scan_id) == Some(request.scan_id)
}

fn load_scan_detail(
    selection: ScanDetailSelection,
    mut selected: Signal<Option<ScanDetailSelection>>,
    mut state: Signal<ScanDetailState>,
    mut generation: Signal<u64>,
) {
    let request = ScanDetailRequest {
        scan_id: selection.scan_id,
        generation: generation().wrapping_add(1),
    };
    generation.set(request.generation);
    selected.set(Some(selection.clone()));
    state.set(ScanDetailState::Loading);
    spawn(async move {
        let result = fetch_scanning_scan_detail(&selection.scan_id).await;
        if !scan_detail_request_is_current(request, selected.peek().as_ref(), generation()) {
            return;
        }
        state.set(match result {
            Ok(detail) => ScanDetailState::Loaded(detail),
            Err(error) => ScanDetailState::Error(error.to_string()),
        });
    });
}

fn close_scan_detail(
    mut selected: Signal<Option<ScanDetailSelection>>,
    mut generation: Signal<u64>,
) {
    generation.set(generation().wrapping_add(1));
    selected.set(None);
}

fn retry_exact_scan(
    derivation_id: i32,
    label: String,
    mut pending: Signal<HashSet<i32>>,
    mut feedback: Signal<Option<ScanActionFeedback>>,
    mut refresh: Signal<u64>,
) {
    if pending.read().contains(&derivation_id) {
        return;
    }
    pending.write().insert(derivation_id);
    feedback.set(None);
    spawn(async move {
        let result = trigger_cve_derivation_rescan(derivation_id).await;
        pending.write().remove(&derivation_id);
        match result {
            Ok(response) => {
                feedback.set(Some(ScanActionFeedback {
                    message: format!(
                        "{label}: {} exact scan {}.",
                        if response.enqueued {
                            "queued"
                        } else {
                            "reused"
                        },
                        response.scan_id
                    ),
                    success: true,
                }));
                refresh.set(refresh().wrapping_add(1));
            }
            Err(error) => feedback.set(Some(ScanActionFeedback {
                message: format!("{label}: exact retry failed: {error}"),
                success: false,
            })),
        }
    });
}

/// Renders authoritative CVE scan lifecycle administration.
///
/// The view does not expose cancellation because the server reports every scan
/// as non-cancellable. Archive actions only target terminal scan identities.
#[component]
pub fn ScanningView() -> Element {
    let mut tab = use_signal(|| ScanTab::Active);
    let mut refresh = use_signal(|| 0_u64);
    let mut live_refresh = use_signal(|| 0_u64);
    let mut include_archived = use_signal(|| false);
    let mut query = use_signal(String::new);
    let mut status_filter = use_signal(|| "all".to_string());
    let mut revision_filter = use_signal(|| "all".to_string());
    let mut latest_only = use_signal(|| false);
    let mut sort = use_signal(|| ScanSort::Timestamp);
    let mut descending = use_signal(|| true);
    let mut selected_rows = use_signal(HashSet::<Uuid>::new);
    let mut archive_pending = use_signal(|| false);
    let mut exact_retry_pending = use_signal(HashSet::<i32>::new);
    let mut action_feedback = use_signal(|| Option::<ScanActionFeedback>::None);
    let mut schedule_open = use_signal(|| false);
    let mut schedule_refresh = use_signal(|| 0_u64);
    let mut selected_scan = use_signal(|| Option::<ScanDetailSelection>::None);
    let mut detail_state = use_signal(|| ScanDetailState::Loading);
    let mut detail_generation = use_signal(|| 0_u64);
    let mut system_query = use_signal(String::new);
    let mut system_environment = use_signal(|| "all".to_string());
    let mut expanded_system = use_signal(|| Option::<Uuid>::None);
    let mut system_histories = use_signal(HashMap::<(Uuid, bool), SystemHistoryData>::new);
    let mut system_errors = use_signal(HashMap::<(Uuid, bool), String>::new);
    let mut loading_system = use_signal(|| Option::<(Uuid, bool)>::None);
    let mut open_failed_when_loaded = use_signal(|| false);

    let mut policy_on_build = use_signal(|| true);
    let mut policy_deployed_interval = use_signal(|| "24h".to_string());
    let mut policy_recent_interval = use_signal(|| "24h".to_string());
    let mut policy_archived_interval = use_signal(|| "168h".to_string());
    let mut policy_archived_enabled = use_signal(|| true);
    let mut policy_rebuild_to_scan = use_signal(|| false);
    let mut schedule_save_error = use_signal(|| Option::<String>::None);
    let mut schedule_saving = use_signal(|| false);

    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(LIVE_REFRESH_MS).await;
            live_refresh.set(live_refresh().wrapping_add(1));
        }
    });

    #[cfg(target_arch = "wasm32")]
    {
        let keydown_listener = use_hook(move || {
            let callback = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
                move |event: web_sys::KeyboardEvent| {
                    if event.key() == "Escape" {
                        if selected_scan.peek().is_some() {
                            close_scan_detail(selected_scan, detail_generation);
                        } else if schedule_open() {
                            schedule_open.set(false);
                        }
                    }
                },
            );
            if let Some(window) = web_sys::window() {
                let _ = window
                    .add_event_listener_with_callback("keydown", callback.as_ref().unchecked_ref());
            }
            Rc::new(callback)
        });
        let listener_for_drop = keydown_listener.clone();
        use_drop(move || {
            if let Some(window) = web_sys::window() {
                let _ = window.remove_event_listener_with_callback(
                    "keydown",
                    listener_for_drop.as_ref().as_ref().unchecked_ref(),
                );
            }
        });
    }

    let mut stats = use_resource(move || {
        let _ = refresh();
        let _ = live_refresh();
        async { fetch_scanning_stats().await }
    });
    let mut active = use_resource(move || {
        let _ = refresh();
        let _ = live_refresh();
        async { fetch_scanning_scan_records("active", false, None, Some(RECORD_LIMIT)).await }
    });
    let mut completed = use_resource(move || {
        let _ = refresh();
        let include_archived = include_archived();
        async move {
            fetch_scanning_scan_records("completed", include_archived, None, Some(RECORD_LIMIT))
                .await
        }
    });
    let mut completed_with_archived = use_resource(move || {
        let _ = refresh();
        let _ = live_refresh();
        async { fetch_scanning_scan_records("completed", true, None, Some(RECORD_LIMIT)).await }
    });
    let mut systems = use_resource(move || {
        let _ = refresh();
        async { fetch_scanning_systems(Some(RECORD_LIMIT)).await }
    });
    let environments = use_resource(|| async { fetch_environments().await });
    let mut schedule = use_resource(move || {
        let _ = schedule_refresh();
        async { fetch_scanning_schedule().await }
    });

    use_effect(move || {
        if !schedule_open() {
            return;
        }
        if let Some(Ok(policy)) = schedule.read().as_ref() {
            policy_on_build.set(policy.on_build);
            policy_deployed_interval.set(policy.deployed_interval.clone());
            policy_recent_interval.set(policy.recent_interval.clone());
            policy_archived_interval.set(policy.archived_interval.clone());
            policy_archived_enabled.set(policy.archived_enabled);
            policy_rebuild_to_scan.set(policy.rebuild_to_scan);
        }
    });

    use_effect(move || {
        let _ = tab();
        query.set(String::new());
        status_filter.set("all".to_string());
        revision_filter.set("all".to_string());
        latest_only.set(false);
        sort.set(ScanSort::Timestamp);
        descending.set(true);
        selected_rows.write().clear();
    });

    use_effect(move || {
        let _ = refresh();
        let archived = include_archived();
        system_histories.write().clear();
        system_errors.write().clear();
        if tab() == ScanTab::Systems
            && let Some(system_id) = expanded_system()
        {
            reload_system_history(
                system_id,
                system_histories,
                system_errors,
                loading_system,
                archived,
            );
        }
    });

    let active_value = resource_value(&active);
    let completed_value = resource_value(&completed);
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
    let failed_rows = resource_value(&completed_with_archived).items;
    let schedule_for_button = schedule_value.clone();

    use_effect(move || {
        if !open_failed_when_loaded() {
            return;
        }
        let outcome = {
            let resource = completed_with_archived.read();
            match resource.as_ref() {
                Some(Ok(response)) => Some(Ok(first_failed(&response.items))),
                Some(Err(error)) => Some(Err(error.to_string())),
                None => None,
            }
        };
        let Some(outcome) = outcome else {
            return;
        };
        open_failed_when_loaded.set(false);
        match outcome {
            Ok(selection) => {
                if let Some(selection) = selection {
                    load_scan_detail(selection, selected_scan, detail_state, detail_generation);
                } else {
                    action_feedback.set(Some(ScanActionFeedback {
                        message: "No failed scan is available in the retained history.".to_string(),
                        success: false,
                    }));
                }
            }
            Err(error) => action_feedback.set(Some(ScanActionFeedback {
                message: format!("The newest failed scan could not be loaded: {error}"),
                success: false,
            })),
        }
    });

    rsx! {
        div { class: "scanning-view",
            div { class: "page-head scanning-head",
                div {
                    div { class: "scanning-title-line",
                        h1 { class: "page-title", "Scanning" }
                        span { class: "scanning-live", title: "Active scans and fleet totals refresh every 15 seconds", span { class: "scan-pulse" } "Live" }
                    }
                    p { class: "page-subtitle", "Exact CVE scan lifecycles and bounded vulnix diagnostics" }
                }
                div { class: "scanning-head-actions",
                    button {
                        class: "btn btn-ghost focus-ring",
                        onclick: move |_| {
                            if let Some(policy) = schedule_for_button.clone() {
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
                }
            }

            if let Some(feedback) = action_feedback() {
                div { role: if feedback.success { "status" } else { "alert" }, class: if feedback.success { "sd-callout sd-callout-success scanning-alert" } else { "sd-callout sd-callout-danger scanning-alert" },
                    div { "{feedback.message}" }
                    button { class: "btn btn-ghost xs focus-ring", onclick: move |_| action_feedback.set(None), "Dismiss" }
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
                    { stat_card("Scanning now", &summary.scanning.to_string(), Some(&format!("{} queued · {} awaiting build · {} awaiting closure", summary.queued, summary.awaiting_build, summary.awaiting_closure)), "#60a5fa") }
                    { stat_card("Stale", &summary.stale.to_string(), Some("past rescan interval"), "#fbbf24") }
                    { stat_card("Never scanned", &summary.never_scanned.to_string(), None, "#9ca3af") }
                    if summary.failed > 0 {
                        button {
                            class: "stat scanning-stat-button focus-ring",
                            aria_label: "Open the newest failed scan",
                            onclick: move |_| {
                                tab.set(ScanTab::Completed);
                                if let Some(selection) = first_failed(&failed_rows) {
                                    load_scan_detail(selection, selected_scan, detail_state, detail_generation);
                                } else {
                                    open_failed_when_loaded.set(true);
                                    completed_with_archived.restart();
                                }
                            },
                            span { class: "stat-accent", style: "--stat-color:#f87171;" }
                            div { class: "stat-label", "Failed" }
                            div { class: "stat-value", style: "color:#f87171;", "{summary.failed}" }
                            div { class: "stat-meta", "Open newest failure" }
                        }
                    } else {
                        { stat_card("Failed", "0", None, "#34d399") }
                    }
                    { stat_card("Coverage", &format!("{}%", summary.coverage_percent), Some("configs with results"), "#34d399") }
                } else {
                    for (label, color) in [("Scanning now", "#60a5fa"), ("Stale", "#fbbf24"), ("Never scanned", "#9ca3af"), ("Failed", "#f87171"), ("Coverage", "#34d399")] {
                        { stat_card(label, "—", None, color) }
                    }
                }
            }

            section { class: "card scanning-card", aria_label: "CVE scans",
                div { class: "sd-tabs scanning-tabs", role: "tablist", aria_label: "Scan views",
                    onkeydown: move |event| {
                        let next = match event.key() {
                            Key::ArrowRight => match tab() { ScanTab::Active => ScanTab::Completed, ScanTab::Completed => ScanTab::Systems, ScanTab::Systems => ScanTab::Active },
                            Key::ArrowLeft => match tab() { ScanTab::Active => ScanTab::Systems, ScanTab::Completed => ScanTab::Active, ScanTab::Systems => ScanTab::Completed },
                            Key::Home => ScanTab::Active,
                            Key::End => ScanTab::Systems,
                            _ => return,
                        };
                        event.prevent_default();
                        tab.set(next);
                        focus_element_by_id(scan_tab_id(next));
                    },
                    { scan_tab_button(tab, ScanTab::Active, "Active", active_value.total, "scan-active-panel") }
                    { scan_tab_button(tab, ScanTab::Completed, "Completed", visible_record_total(&completed_value, include_archived()), "scan-completed-panel") }
                    { scan_tab_button(tab, ScanTab::Systems, "By system", systems_value.len() as i64, "scan-systems-panel") }
                }
                match tab() {
                    ScanTab::Active => rsx! {
                        div { id: "scan-active-panel", role: "tabpanel", aria_labelledby: "scan-active-tab",
                            { records_panel(
                                active_value.clone(),
                                active.read().is_none(),
                                resource_error(&active),
                                false,
                                include_archived,
                                query,
                                status_filter,
                                revision_filter,
                                latest_only,
                                sort,
                                descending,
                                selected_rows,
                                archive_pending,
                                exact_retry_pending,
                                action_feedback,
                                refresh,
                                selected_scan,
                                detail_state,
                                detail_generation,
                                move || active.restart(),
                            ) }
                        }
                    },
                    ScanTab::Completed => rsx! {
                        div { id: "scan-completed-panel", role: "tabpanel", aria_labelledby: "scan-completed-tab",
                            { records_panel(
                                completed_value.clone(),
                                completed.read().is_none(),
                                resource_error(&completed),
                                true,
                                include_archived,
                                query,
                                status_filter,
                                revision_filter,
                                latest_only,
                                sort,
                                descending,
                                selected_rows,
                                archive_pending,
                                exact_retry_pending,
                                action_feedback,
                                refresh,
                                selected_scan,
                                detail_state,
                                detail_generation,
                                move || completed.restart(),
                            ) }
                        }
                    },
                    ScanTab::Systems => rsx! {
                        div { id: "scan-systems-panel", role: "tabpanel", aria_labelledby: "scan-systems-tab",
                            { systems_panel(
                                systems_value.clone(),
                                systems.read().is_none(),
                                systems.read().as_ref().and_then(|result| result.as_ref().err()).map(ToString::to_string),
                                env_colors.clone(),
                                system_query,
                                system_environment,
                                expanded_system,
                                system_histories,
                                system_errors,
                                loading_system,
                                include_archived,
                                exact_retry_pending,
                                action_feedback,
                                refresh,
                                selected_scan,
                                detail_state,
                                detail_generation,
                                move || systems.restart(),
                            ) }
                        }
                    },
                }
            }

            if schedule_open() {
                { schedule_modal(
                    schedule_value.clone(),
                    schedule.read().as_ref().and_then(|result| result.as_ref().err()).map(ToString::to_string),
                    schedule_open,
                    policy_on_build,
                    policy_deployed_interval,
                    policy_recent_interval,
                    policy_archived_interval,
                    policy_archived_enabled,
                    policy_rebuild_to_scan,
                    schedule_save_error,
                    schedule_saving,
                    schedule_refresh,
                    selected_scan,
                    move || schedule.restart(),
                ) }
            }
            if let Some(selection) = selected_scan() {
                ScanDetailDrawer {
                    selection,
                    selected: selected_scan,
                    state: detail_state,
                    generation: detail_generation,
                    retry_pending: exact_retry_pending,
                    feedback: action_feedback,
                    refresh,
                }
            }
        }
    }
}

fn resource_value(
    resource: &Resource<Result<ScanningScanRecordsResponse, crate::api::client::ApiClientError>>,
) -> ScanningScanRecordsResponse {
    resource
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned()
        .unwrap_or(ScanningScanRecordsResponse {
            items: Vec::new(),
            total: 0,
            hidden_archived: 0,
        })
}

fn resource_error(
    resource: &Resource<Result<ScanningScanRecordsResponse, crate::api::client::ApiClientError>>,
) -> Option<String> {
    resource
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().err())
        .map(ToString::to_string)
}

fn scan_tab_button(
    mut tab: Signal<ScanTab>,
    value: ScanTab,
    label: &'static str,
    count: i64,
    controls: &'static str,
) -> Element {
    let selected = tab() == value;
    rsx! {
        button {
            id: scan_tab_id(value),
            class: if selected { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
            role: "tab",
            aria_selected: selected,
            aria_controls: controls,
            tabindex: if selected { "0" } else { "-1" },
            onclick: move |_| tab.set(value),
            "{label}"
            span { class: "sd-tab-badge", "{count}" }
        }
    }
}

fn scan_tab_id(tab: ScanTab) -> &'static str {
    match tab {
        ScanTab::Active => "scan-active-tab",
        ScanTab::Completed => "scan-completed-tab",
        ScanTab::Systems => "scan-systems-tab",
    }
}

fn focus_element_by_id(id: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(id))
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = element.focus();
    }

    #[cfg(not(target_arch = "wasm32"))]
    let _ = id;
}

#[allow(clippy::too_many_arguments)]
fn records_panel(
    response: ScanningScanRecordsResponse,
    loading: bool,
    error: Option<String>,
    completed: bool,
    mut include_archived: Signal<bool>,
    mut query: Signal<String>,
    mut status_filter: Signal<String>,
    mut revision_filter: Signal<String>,
    mut latest_only: Signal<bool>,
    mut sort: Signal<ScanSort>,
    mut descending: Signal<bool>,
    mut selected_rows: Signal<HashSet<Uuid>>,
    archive_pending: Signal<bool>,
    exact_retry_pending: Signal<HashSet<i32>>,
    action_feedback: Signal<Option<ScanActionFeedback>>,
    refresh: Signal<u64>,
    selected_scan: Signal<Option<ScanDetailSelection>>,
    detail_state: Signal<ScanDetailState>,
    detail_generation: Signal<u64>,
    retry: impl FnMut() + 'static,
) -> Element {
    let mut retry = retry;
    let rows = filter_and_sort_records(
        &response.items,
        &query(),
        &status_filter(),
        &revision_filter(),
        latest_only(),
        sort(),
        descending(),
    );
    let statuses = response
        .items
        .iter()
        .map(|row| status_meta(&row.status).key)
        .collect::<HashSet<_>>();
    let selected = selected_rows();
    let archive_ids = response
        .items
        .iter()
        .filter(|row| selected.contains(&row.scan_id) && row.archived_at.is_none())
        .map(|row| row.scan_id)
        .collect::<Vec<_>>();
    let restore_ids = response
        .items
        .iter()
        .filter(|row| selected.contains(&row.scan_id) && row.archived_at.is_some())
        .map(|row| row.scan_id)
        .collect::<Vec<_>>();
    let filtered = rows.len();
    let loaded = response.items.len();
    let available = visible_record_total(&response, include_archived());
    let capped = available > loaded as i64;

    rsx! {
        div { class: "scan-toolbar",
            div { class: "q-search scanning-search",
                Icon { name: IconName::Search, size: 13 }
                input { class: "q-search-input", aria_label: "Search scans by configuration, flake, revision, scan ID, or derivation ID", placeholder: "Search scans…", value: query(), oninput: move |event| query.set(event.value()) }
                if !query().is_empty() { button { class: "btn-icon xs focus-ring", aria_label: "Clear scan search", onclick: move |_| query.set(String::new()), Icon { name: IconName::X, size: 13 } } }
            }
            select { class: "input filter-select focus-ring", aria_label: "Filter by scan status", value: status_filter(), oninput: move |event| status_filter.set(event.value()),
                option { value: "all", "All statuses" }
                for key in ["in_progress", "pending", "awaiting_build", "awaiting_closure", "failed", "completed"] {
                    if statuses.contains(key) { option { value: key, "{status_meta(key).label}" } }
                }
            }
            select { class: "input filter-select focus-ring", aria_label: "Filter by revision freshness", value: revision_filter(), oninput: move |event| revision_filter.set(event.value()),
                option { value: "all", "All revisions" }
                option { value: "deployed", "Deployed" }
                option { value: "recent", "Latest per flake" }
                option { value: "superseded", "Superseded" }
            }
            button { class: if latest_only() { "btn btn-ghost xs focus-ring active-filter" } else { "btn btn-ghost xs focus-ring" }, aria_pressed: latest_only(), onclick: move |_| latest_only.toggle(), Icon { name: IconName::Star, size: 12 } " Latest per flake" }
            span { class: "filter-count", "{filtered} visible · {loaded} loaded" if capped { " · showing the first {loaded} of {available}; search and sorting apply to loaded records" } if response.total != available { " · {response.total} all" } }
            if completed {
                label { class: "scanning-include-archived", input { r#type: "checkbox", checked: include_archived(), onchange: move |event| { include_archived.set(event.checked()); selected_rows.write().clear(); } } " Include archived" }
            }
        }

        if completed {
            div { class: "scanning-history-actions",
                span { "{selected.len()} selected" }
                button { class: "btn btn-ghost xs focus-ring", disabled: archive_ids.is_empty() || archive_pending(), onclick: move |_| apply_archive(archive_ids.clone(), true, selected_rows, archive_pending, action_feedback, refresh), "Archive selected" }
                button { class: "btn btn-ghost xs focus-ring", disabled: restore_ids.is_empty() || archive_pending(), onclick: move |_| apply_archive(restore_ids.clone(), false, selected_rows, archive_pending, action_feedback, refresh), "Restore selected" }
                if response.hidden_archived > 0 { span { class: "scanning-hidden-count", "{response.hidden_archived} archived scans hidden by retention view" } }
            }
        }

        if let Some(error) = error {
            { load_error_state("Scans could not be loaded", &error, move || retry()) }
        } else if loading {
            div { class: "q-empty", role: "status", "Loading exact scan lifecycles…" }
        } else if rows.is_empty() {
            div { class: "q-empty",
                if response.items.is_empty() {
                    if completed && response.hidden_archived > 0 {
                        h3 { "Completed scans are hidden" }
                        p { "The current retention view hides {response.hidden_archived} archived scan(s). Include archived scans to review or restore them." }
                    } else {
                        h3 { if completed { "No completed scan history" } else { "No active scans" } }
                        p { if completed { "Terminal scan lifecycles will remain here as history." } else { "Queued, scanning, and prerequisite wait states will appear here." } }
                    }
                } else {
                    h3 { "No scans match these filters" }
                    p { if capped { "No loaded scans match these filters. Additional records exist beyond the loaded cap." } else { "The result count reflects the current client-side filters." } }
                    button { class: "btn btn-ghost xs focus-ring", onclick: move |_| reset_filters(query, status_filter, revision_filter, latest_only), "Reset filters" }
                }
            }
        } else {
            div { class: "scanning-table-wrap",
                table { class: "sys-table scanning-table",
                    thead { tr {
                        if completed { th { span { class: "sr-only", "Select" } } }
                        { sortable_header("Configuration", ScanSort::Configuration, sort, descending) }
                        { sortable_header("Revision", ScanSort::Revision, sort, descending) }
                        { sortable_header("Status", ScanSort::Status, sort, descending) }
                        { sortable_header("Findings", ScanSort::Severity, sort, descending) }
                        { sortable_header("Last scan", ScanSort::Timestamp, sort, descending) }
                        th { "Trigger" }
                        th { class: "scanning-actions-heading", span { class: "sr-only", "Actions" } }
                    } }
                    tbody { for row in rows { { record_row(row, completed, selected_rows, exact_retry_pending, action_feedback, refresh, selected_scan, detail_state, detail_generation) } } }
                }
            }
        }
    }
}

fn reset_filters(
    mut query: Signal<String>,
    mut status: Signal<String>,
    mut revision: Signal<String>,
    mut latest: Signal<bool>,
) {
    query.set(String::new());
    status.set("all".to_string());
    revision.set("all".to_string());
    latest.set(false);
}

fn sortable_header(
    label: &'static str,
    key: ScanSort,
    mut sort: Signal<ScanSort>,
    mut descending: Signal<bool>,
) -> Element {
    let active = sort() == key;
    rsx! {
        th { aria_sort: if !active { "none" } else if descending() { "descending" } else { "ascending" },
            button { class: if active { "th-sort focus-ring on" } else { "th-sort focus-ring" }, aria_label: format!("Sort by {label}"), onclick: move |_| { if sort() == key { descending.toggle(); } else { sort.set(key); descending.set(matches!(key, ScanSort::Severity | ScanSort::Timestamp)); } },
                "{label}" Icon { name: if active && descending() { IconName::ChevronDown } else { IconName::ChevronUp }, size: 10 }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn record_row(
    row: ScanningScanRecordResponse,
    selectable: bool,
    mut selected_rows: Signal<HashSet<Uuid>>,
    retry_pending: Signal<HashSet<i32>>,
    feedback: Signal<Option<ScanActionFeedback>>,
    refresh: Signal<u64>,
    selected_scan: Signal<Option<ScanDetailSelection>>,
    detail_state: Signal<ScanDetailState>,
    detail_generation: Signal<u64>,
) -> Element {
    let meta = status_meta(&row.status);
    let selected = selected_rows.read().contains(&row.scan_id);
    let selection = ScanDetailSelection {
        scan_id: row.scan_id,
        label: format!("{} · {}", row.hostname, commit_label(&row.commit_hash)),
    };
    let can_retry = row.status == "failed";
    let relation = revision_class(&row);
    let relation_label = match relation {
        "deployed" => "Deployed",
        "recent" => "Recent",
        _ => "Superseded",
    };
    let configuration_meta = match row.flake_name.as_deref() {
        Some(flake) => format!("{flake} · {}", commit_label(&row.commit_hash)),
        None => commit_label(&row.commit_hash),
    };
    rsx! {
        tr { key: "{row.scan_id}", class: if row.archived_at.is_some() { "scanning-record archived" } else { "scanning-record" },
            if selectable { td { input { r#type: "checkbox", aria_label: format!("Select scan {}", row.scan_id), checked: selected, onchange: move |event| { if event.checked() { selected_rows.write().insert(row.scan_id); } else { selected_rows.write().remove(&row.scan_id); } } } } }
            td { div { class: "scanning-config-name", "{row.hostname}" } div { class: "scanning-history-flake", "{configuration_meta}" } }
            td { span { class: if relation == "deployed" { "chip chip-healthy" } else if relation == "recent" { "chip chip-info" } else { "chip chip-unknown" }, "{relation_label}" } }
            td {
                span { class: "chip {meta.class}", span { class: "chip-dot", style: "background:{meta.color};" } "{meta.label}" }
                if let Some(reason) = row.wait_reason.as_deref() { div { class: "scanning-wait", "Awaiting: {reason}" } }
                if row.archived_at.is_some() { div { class: "scanning-archived-label", "Archived" } }
            }
            td { { findings(row.critical_count, row.high_count, row.medium_count, row.low_count, row.status == "completed") } }
            td { class: "scanning-last-scan", title: "{record_time(&row).to_rfc3339()}", "{relative_time(record_time(&row))}" }
            td { if let Some(trigger) = row.source_trigger.as_deref() { span { class: "chip chip-unknown scanning-trigger", "{trigger}" } } else { span { class: "scanning-unavailable", "Not recorded" } } }
            td { div { class: "row-actions scanning-row-actions",
                if can_retry { button { class: "btn btn-ghost xs focus-ring", disabled: retry_pending.read().contains(&row.derivation_id), onclick: { let label = format!("{} {}", row.hostname, commit_label(&row.commit_hash)); move |_| retry_exact_scan(row.derivation_id, label.clone(), retry_pending, feedback, refresh) }, Icon { name: IconName::Sync, size: 11 } " Retry exact" } }
                button { class: "btn-icon focus-ring", aria_label: format!("Open details for scan {}", row.scan_id), title: "Open exact scan details", onclick: move |_| load_scan_detail(selection.clone(), selected_scan, detail_state, detail_generation), Icon { name: IconName::Terminal, size: 14 } }
            } }
        }
    }
}

fn apply_archive(
    scan_ids: Vec<Uuid>,
    archived: bool,
    mut selected: Signal<HashSet<Uuid>>,
    mut pending: Signal<bool>,
    mut feedback: Signal<Option<ScanActionFeedback>>,
    mut refresh: Signal<u64>,
) {
    if scan_ids.is_empty() || pending() {
        return;
    }
    pending.set(true);
    spawn(async move {
        match update_scanning_archive_state(scan_ids, archived).await {
            Ok(result) => {
                feedback.set(Some(ScanActionFeedback {
                    message: format!(
                        "{} {} of {} requested scan(s).",
                        if archived { "Archived" } else { "Restored" },
                        result.changed,
                        result.requested
                    ),
                    success: true,
                }));
                selected.write().clear();
                refresh.set(refresh().wrapping_add(1));
            }
            Err(error) => feedback.set(Some(ScanActionFeedback {
                message: format!("Archive state could not be updated: {error}"),
                success: false,
            })),
        }
        pending.set(false);
    });
}

#[allow(clippy::too_many_arguments)]
fn systems_panel(
    rows: Vec<ScanningSystemsItemResponse>,
    loading: bool,
    error: Option<String>,
    env_colors: HashMap<String, String>,
    mut query: Signal<String>,
    mut environment: Signal<String>,
    mut expanded: Signal<Option<Uuid>>,
    histories: Signal<HashMap<(Uuid, bool), SystemHistoryData>>,
    errors: Signal<HashMap<(Uuid, bool), String>>,
    loading_system: Signal<Option<(Uuid, bool)>>,
    mut include_archived: Signal<bool>,
    retry_pending: Signal<HashSet<i32>>,
    feedback: Signal<Option<ScanActionFeedback>>,
    refresh: Signal<u64>,
    selected_scan: Signal<Option<ScanDetailSelection>>,
    detail_state: Signal<ScanDetailState>,
    detail_generation: Signal<u64>,
    retry: impl FnMut() + 'static,
) -> Element {
    let mut retry = retry;
    let search = query().trim().to_ascii_lowercase();
    let selected_environment = environment();
    let mut environment_names = rows
        .iter()
        .filter_map(|row| row.environment.clone())
        .collect::<Vec<_>>();
    environment_names.sort();
    environment_names.dedup();
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
        left.hostname
            .to_ascii_lowercase()
            .cmp(&right.hostname.to_ascii_lowercase())
            .then_with(|| left.system_id.cmp(&right.system_id))
    });
    let visible_count = format!("{} visible · {} loaded", visible.len(), rows.len());

    rsx! {
        div { class: "scan-toolbar",
            div { class: "q-search scanning-search", Icon { name: IconName::Search, size: 13 } input { class: "q-search-input", aria_label: "Search systems", placeholder: "Search systems…", value: query(), oninput: move |event| query.set(event.value()) } }
            select { class: "input filter-select focus-ring", aria_label: "Filter systems by environment", value: environment(), oninput: move |event| environment.set(event.value()), option { value: "all", "All environments" } for value in environment_names { option { value: "{value}", "{value}" } } }
            span { class: "filter-count", "{visible_count}" }
            button { class: "btn btn-ghost xs focus-ring", disabled: query().is_empty() && environment() == "all", onclick: move |_| { query.set(String::new()); environment.set("all".to_string()); }, "Reset" }
        }
        if let Some(error) = error { { load_error_state("Systems could not be loaded", &error, move || retry()) } }
        else if loading { div { class: "q-empty", role: "status", "Loading system scan history…" } }
        else if visible.is_empty() { div { class: "q-empty", h3 { if rows.is_empty() { "No system revision history" } else { "No systems match these filters" } } } }
        else { div { class: "scanning-table-wrap",
            table { class: "sys-table scanning-table scanning-systems-table",
                thead { tr { th { "System" } th { "Environment" } th { "Revision coverage" } th { "Current findings" } th { span { class: "sr-only", "Actions" } } } }
                tbody { for system in visible {
                    {
                        let system_id = system.system_id;
                        let open = expanded() == Some(system_id);
                        let archive_state = include_archived();
                        let history = histories.read().get(&(system_id, archive_state)).cloned();
                        let history_error = errors.read().get(&(system_id, archive_state)).cloned();
                        rsx! {
                            tr { key: "system-{system_id}", class: if open { "scanning-system-row expanded" } else { "scanning-system-row" },
                                td { button { class: "scanning-system-toggle focus-ring", aria_expanded: open, onclick: move |_| toggle_system_history(system_id, expanded), Icon { name: if open { IconName::ChevronDown } else { IconName::ChevronRight }, size: 12 } span { class: "scanning-config-name", "{system.hostname}" } } }
                                td { if let Some(name) = system.environment.clone() { if let Some(color) = env_colors.get(&name.to_ascii_lowercase()) { EnvBadge { name, fg: color.clone(), bg: format!("color-mix(in oklab, {color} 14%, var(--cf-card-bg))"), border: color.clone() } } else { EnvBadge { name } } } else { span { class: "scanning-unavailable", "Unassigned" } } }
                                td { div { class: "scanning-system-counts", span { "{system.scanned} scanned" } if system.stale > 0 { span { class: "stale", "{system.stale} stale" } } if system.needs_build > 0 { span { class: "needs", "{system.needs_build} needs build" } } if system.unscanned > 0 { span { "{system.unscanned} never scanned" } } } }
                                td { { findings(system.current_crit as i32, system.current_high as i32, 0, 0, true) } }
                                td { if let Some(derivation_id) = system.current_derivation_id { button { class: "btn btn-ghost xs focus-ring", disabled: retry_pending.read().contains(&derivation_id), title: "Check the exact currently deployed derivation now", onclick: { let label = format!("{} deployed revision", system.hostname); move |_| retry_exact_scan(derivation_id, label.clone(), retry_pending, feedback, refresh) }, Icon { name: IconName::Sync, size: 11 } " Check now" } } }
                            }
                            if open { tr { class: "scan-sys-expand-row", td { colspan: 5,
                                div { class: "scan-sys-expand",
                                    div { class: "scan-sys-expand-head",
                                        span { "Exact revision history · newest first" }
                                        label { class: "scanning-include-archived", input { r#type: "checkbox", checked: include_archived(), onchange: move |event| include_archived.set(event.checked()) } " Include archived" }
                                    }
                                    if let Some(error) = history_error { { load_error_state("Revision history could not be loaded", &error, move || reload_system_history(system_id, histories, errors, loading_system, include_archived())) } }
                                    else if loading_system() == Some((system_id, archive_state)) { div { class: "q-empty scanning-system-state", role: "status", "Loading exact history…" } }
                                    else if let Some(history) = history {
                                        if history.scans.items.is_empty() && history.derivations.iter().all(|row| row.scan_id.is_some()) { div { class: "q-empty scanning-system-state", if history.scans.hidden_archived > 0 { "{history.scans.hidden_archived} archived scan(s) are hidden by the retention view." } else { "No exact scans or unscanned revisions are recorded for this system." } } }
                                        else { { system_history_table(&system, history, retry_pending, feedback, refresh, selected_scan, detail_state, detail_generation) } }
                                    }
                                }
                            } } }
                        }
                    }
                } }
            }
        } }
    }
}

fn toggle_system_history(system_id: Uuid, mut expanded: Signal<Option<Uuid>>) {
    if expanded() == Some(system_id) {
        expanded.set(None);
    } else {
        expanded.set(Some(system_id));
    }
}

fn reload_system_history(
    system_id: Uuid,
    mut histories: Signal<HashMap<(Uuid, bool), SystemHistoryData>>,
    mut errors: Signal<HashMap<(Uuid, bool), String>>,
    mut loading: Signal<Option<(Uuid, bool)>>,
    include_archived: bool,
) {
    let key = (system_id, include_archived);
    loading.set(Some(key));
    errors.write().remove(&key);
    spawn(async move {
        let scans = fetch_scanning_scan_records(
            "history",
            include_archived,
            Some(&system_id),
            Some(RECORD_LIMIT),
        )
        .await;
        let derivations = fetch_scanning_system_scans(&system_id, Some(RECORD_LIMIT)).await;
        match (scans, derivations) {
            (Ok(scans), Ok(derivations)) => {
                histories
                    .write()
                    .insert(key, SystemHistoryData { scans, derivations });
            }
            (Err(error), _) | (_, Err(error)) => {
                errors.write().insert(key, error.to_string());
            }
        }
        if loading() == Some(key) {
            loading.set(None);
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn system_history_table(
    system: &ScanningSystemsItemResponse,
    history: SystemHistoryData,
    retry_pending: Signal<HashSet<i32>>,
    feedback: Signal<Option<ScanActionFeedback>>,
    refresh: Signal<u64>,
    selected_scan: Signal<Option<ScanDetailSelection>>,
    detail_state: Signal<ScanDetailState>,
    detail_generation: Signal<u64>,
) -> Element {
    let hidden_archived = history.scans.hidden_archived;
    let revision_relations = history
        .derivations
        .iter()
        .map(|row| (row.derivation_id, (row.is_current, row.is_latest_per_flake)))
        .collect::<HashMap<_, _>>();
    let rows = system_history_entries(history);
    rsx! {
        if hidden_archived > 0 { div { class: "scanning-hidden-count", "{hidden_archived} archived scan(s) hidden" } }
        div { class: "scan-sys-expand-table-wrap", table { class: "scanning-history-table",
            thead { tr { th { "Revision" } th { "Relation" } th { "Status" } th { "Findings" } th { "Timestamp" } th { span { class: "sr-only", "Actions" } } } }
            tbody { for entry in rows {
                match entry {
                SystemHistoryEntry::Scan(row) => {
                    let relation = match (row.is_current, row.is_latest_per_flake) {
                        (true, _) => "Deployed",
                        (false, true) => "Recent",
                        _ if system.current_derivation_id == Some(row.derivation_id) => "Deployed",
                        _ => match revision_relations.get(&row.derivation_id).copied() {
                            Some((true, _)) => "Deployed",
                            Some((false, true)) => "Recent",
                            _ => "Superseded config",
                        },
                    };
                    let meta = status_meta(&row.status);
                    let selection = ScanDetailSelection { scan_id: row.scan_id, label: format!("{} · {}", row.hostname, commit_label(&row.commit_hash)) };
                    let revision = row.commit_hash.as_deref().unwrap_or("Revision unavailable");
                    rsx! { tr { key: "history-{row.scan_id}", class: if row.archived_at.is_some() { "scanning-record archived" } else { "scanning-record" },
                        td { div { class: "scanning-full-revision mono", "{revision}" } }
                        td { span { class: if relation == "Deployed" { "chip chip-healthy" } else { "chip chip-unknown" }, "{relation}" } }
                        td { span { class: "chip {meta.class}", "{meta.label}" } if let Some(reason) = row.wait_reason.as_deref() { div { class: "scanning-wait", "Awaiting: {reason}" } } if let Some(failure) = row.failure.as_deref() { div { class: "scanning-row-failure", "{failure}" } } }
                        td { { findings(row.critical_count, row.high_count, row.medium_count, row.low_count, row.status == "completed") } }
                        td { class: "scanning-last-scan", title: "{record_time(&row).to_rfc3339()}", "{relative_time(record_time(&row))}" }
                        td { div { class: "row-actions scanning-row-actions",
                            if row.status == "failed" { button { class: "btn btn-ghost xs focus-ring", disabled: retry_pending.read().contains(&row.derivation_id), onclick: { let label = format!("{} {}", row.hostname, commit_label(&row.commit_hash)); move |_| retry_exact_scan(row.derivation_id, label.clone(), retry_pending, feedback, refresh) }, "Retry exact" } }
                            button { class: "btn-icon focus-ring", aria_label: format!("Open details for scan {}", row.scan_id), onclick: move |_| load_scan_detail(selection.clone(), selected_scan, detail_state, detail_generation), Icon { name: IconName::Terminal, size: 13 } }
                        } }
                    } }
                },
                SystemHistoryEntry::NoScan(row) => {
                    let relation = if row.is_current { "Deployed" } else if row.is_latest_per_flake { "Recent" } else { "Superseded config" };
                    let status = if row.rescan_eligible { "never_scanned" } else { "needs_build" };
                    let meta = status_meta(status);
                    let revision = row.commit_hash.as_deref().unwrap_or("Revision unavailable");
                    rsx! { tr { key: "unscanned-{row.derivation_id}", class: "scanning-record",
                        td { div { class: "scanning-full-revision mono", "{revision}" } div { class: "scanning-record-id mono", "drv {row.derivation_id} · no scan" } }
                        td { span { class: if relation == "Deployed" { "chip chip-healthy" } else { "chip chip-unknown" }, "{relation}" } }
                        td { span { class: "chip {meta.class}", "{meta.label}" } }
                        td { span { class: "scanning-unavailable", "Not available" } }
                        td { class: "scanning-last-scan", "Never" }
                        td { div { class: "row-actions scanning-row-actions",
                            if row.rescan_eligible { button { class: "btn btn-ghost xs focus-ring", disabled: retry_pending.read().contains(&row.derivation_id), onclick: { let label = format!("{} {}", row.hostname, commit_label(&row.commit_hash)); move |_| retry_exact_scan(row.derivation_id, label.clone(), retry_pending, feedback, refresh) }, "Check now" } }
                        } }
                    } }
                },
                }
            } }
        } }
    }
}

#[component]
fn ScanDetailDrawer(
    selection: ScanDetailSelection,
    mut selected: Signal<Option<ScanDetailSelection>>,
    mut state: Signal<ScanDetailState>,
    generation: Signal<u64>,
    retry_pending: Signal<HashSet<i32>>,
    feedback: Signal<Option<ScanActionFeedback>>,
    refresh: Signal<u64>,
) -> Element {
    let mut search = use_signal(String::new);
    let mut match_position = use_signal(|| 0_usize);
    let mut now = use_signal(Utc::now);
    let mut tab = use_signal(|| ScanDetailTab::Log);
    let selection_id = selection.scan_id;
    let poll_selection = selection.clone();

    use_effect(move || {
        let scan_id = selection_id;
        search.set(String::new());
        match_position.set(0);
        tab.set(ScanDetailTab::Log);
        let _ = scan_id;
    });
    use_effect(move || {
        let running = matches!(&*state.read(), ScanDetailState::Loaded(detail) if detail.status == "in_progress");
        if running {
            let refresh_selection = poll_selection.clone();
            spawn(async move {
                gloo_timers::future::TimeoutFuture::new(DETAIL_POLL_MS).await;
                if selected.peek().as_ref().map(|item| item.scan_id)
                    == Some(refresh_selection.scan_id)
                    && matches!(&*state.peek(), ScanDetailState::Loaded(detail) if detail.status == "in_progress")
                {
                    load_scan_detail(refresh_selection, selected, state, generation);
                }
            });
        }
    });
    use_effect(move || {
        if matches!(&*state.read(), ScanDetailState::Loaded(detail) if detail.status == "in_progress")
        {
            spawn(async move {
                while selected.peek().is_some()
                    && matches!(&*state.peek(), ScanDetailState::Loaded(detail) if detail.status == "in_progress")
                {
                    gloo_timers::future::TimeoutFuture::new(1_000).await;
                    now.set(Utc::now());
                }
            });
        }
    });

    let detail = match &*state.read() {
        ScanDetailState::Loaded(detail) => Some(detail.clone()),
        _ => None,
    };
    let matches = detail
        .as_ref()
        .map(|detail| diagnostic_matches(detail, &search()))
        .unwrap_or_default();
    let match_event_ids = detail
        .as_ref()
        .map(|detail| {
            matches
                .iter()
                .map(|index| detail.events[*index].id)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let diagnostic_count_label = if search().trim().is_empty() {
        format!(
            "{} events",
            detail.as_ref().map_or(0, |detail| detail.events.len())
        )
    } else if matches.is_empty() {
        "0 matches".to_string()
    } else {
        format!("{} of {} matches", match_position() + 1, matches.len())
    };
    if match_position() >= matches.len() && !matches.is_empty() {
        match_position.set(matches.len() - 1);
    }

    rsx! {
        div { class: "side-panel-backdrop scanning-log-backdrop", tabindex: "-1", onclick: move |_| close_scan_detail(selected, generation),
            aside { id: "scan-diagnostics-dialog", class: "side-panel scanning-log-drawer", role: "dialog", aria_modal: "true", aria_labelledby: "scan-log-title", tabindex: "-1", onclick: move |event| event.stop_propagation(),
                DialogFocusRestore {}
                DialogInitialFocus { dialog_id: "scan-diagnostics-dialog".to_string() }
                DialogFocusSentinel { dialog_id: "scan-diagnostics-dialog".to_string(), boundary: DialogFocusBoundary::Last }
                div { class: "scanning-log-head",
                    div { h2 { id: "scan-log-title", Icon { name: IconName::Shield, size: 14 } " Scan details" } p { "{selection.label}" } code { "{selection.scan_id}" } }
                    div { class: "row-actions",
                        button { class: "btn-icon focus-ring", aria_label: "Refresh exact scan detail", onclick: { let refresh_selection = selection.clone(); move |_| load_scan_detail(refresh_selection.clone(), selected, state, generation) }, Icon { name: IconName::Sync, size: 14 } }
                        button { class: "btn-icon focus-ring", aria_label: "Close exact scan detail", onclick: move |_| close_scan_detail(selected, generation), Icon { name: IconName::X, size: 15 } }
                    }
                }
                match &*state.read() {
                    ScanDetailState::Loading => rsx! { div { class: "scanning-log-body", div { class: "q-empty", role: "status", "Loading exact scan detail…" } } },
                    ScanDetailState::Error(error) => rsx! { div { class: "scanning-log-body", { load_error_state("Exact scan detail could not be loaded", error, { let retry_selection = selection.clone(); move || load_scan_detail(retry_selection.clone(), selected, state, generation) }) } } },
                    ScanDetailState::Loaded(detail) => rsx! {
                        { detail_identity(detail) }
                        { detail_status_strip(detail, now()) }
                        { detail_callout(detail, selected, generation, retry_pending, feedback, refresh) }
                        div { class: "sd-tabs scanning-detail-tabs", role: "tablist", aria_label: "Scan detail sections",
                            onkeydown: move |event| {
                                let next = match event.key() {
                                    Key::ArrowRight | Key::ArrowLeft => match tab() {
                                        ScanDetailTab::Log => ScanDetailTab::Details,
                                        ScanDetailTab::Details => ScanDetailTab::Log,
                                    },
                                    Key::End => ScanDetailTab::Details,
                                    Key::Home => ScanDetailTab::Log,
                                    _ => return,
                                };
                                event.prevent_default();
                                tab.set(next);
                                focus_element_by_id(match next { ScanDetailTab::Log => "scan-detail-log-tab", ScanDetailTab::Details => "scan-detail-details-tab" });
                            },
                            button { id: "scan-detail-log-tab", class: if tab() == ScanDetailTab::Log { "sd-tab focus-ring active" } else { "sd-tab focus-ring" }, role: "tab", tabindex: if tab() == ScanDetailTab::Log { "0" } else { "-1" }, aria_selected: tab() == ScanDetailTab::Log, aria_controls: "scan-detail-log-panel", onclick: move |_| tab.set(ScanDetailTab::Log), "Log" }
                            button { id: "scan-detail-details-tab", class: if tab() == ScanDetailTab::Details { "sd-tab focus-ring active" } else { "sd-tab focus-ring" }, role: "tab", tabindex: if tab() == ScanDetailTab::Details { "0" } else { "-1" }, aria_selected: tab() == ScanDetailTab::Details, aria_controls: "scan-detail-details-panel", onclick: move |_| tab.set(ScanDetailTab::Details), "Details" }
                        }
                        if tab() == ScanDetailTab::Log {
                            div { id: "scan-detail-log-panel", class: "scanning-log-body scanning-detail-panel", role: "tabpanel", aria_labelledby: "scan-detail-log-tab",
                            div { class: "scanning-log-tools",
                                div { class: "q-search scanning-log-search", Icon { name: IconName::Search, size: 12 } input { class: "q-search-input", aria_label: "Search authorized diagnostic content", placeholder: "Search diagnostics…", value: search(), oninput: move |event| { search.set(event.value()); match_position.set(0); } } }
                                span { class: "filter-count", "{diagnostic_count_label}" }
                                button { class: "btn-icon focus-ring", aria_label: "Previous diagnostic match", disabled: matches.is_empty(), onclick: { let event_ids = match_event_ids.clone(); move |_| { if !event_ids.is_empty() { let position = if match_position() == 0 { event_ids.len() - 1 } else { match_position() - 1 }; match_position.set(position); scroll_to_event(event_ids[position]); } } }, Icon { name: IconName::ChevronUp, size: 13 } }
                                button { class: "btn-icon focus-ring", aria_label: "Next diagnostic match", disabled: matches.is_empty(), onclick: { let event_ids = match_event_ids.clone(); move |_| { if !event_ids.is_empty() { let position = (match_position() + 1) % event_ids.len(); match_position.set(position); scroll_to_event(event_ids[position]); } } }, Icon { name: IconName::ChevronDown, size: 13 } }
                                button { class: "btn btn-ghost xs focus-ring", onclick: { let content = diagnostic_export(detail); let filename = format!("scan-{}-diagnostics.txt", detail.scan_id); move |_| { let _ = crate::export::trigger_download(&filename, "text/plain;charset=utf-8", &content); } }, "Export current content" }
                            }
                            if detail.truncated { div { class: "sd-callout sd-callout-warning", role: "status", "The API bounded this authorized response. Export preserves the same ordered content and truncation marker." } }
                            if detail.events.is_empty() { div { class: "q-empty", h3 { "No persisted diagnostic events" } p { "No log output is fabricated for this lifecycle." } } }
                            else { div { class: "sd-log-stream build-log-stream scanning-log-stream", for (index, event) in detail.events.iter().enumerate() {
                                div { id: "scan-event-{event.id}", key: "{event.id}", class: diagnostic_line_class(event.level.as_str(), matches.get(match_position()).copied() == Some(index) && !search().trim().is_empty()),
                                    span { class: "sd-log-t", title: "{event.occurred_at.to_rfc3339()}", "{diagnostic_time(event.occurred_at)}" }
                                    span { class: "sd-log-lvl", "{event.level.to_ascii_uppercase()}" }
                                    span { class: "sd-log-m", span { class: "scanning-log-source", "{event.source}/{event.event_type} · execution {event.execution_id} · attempt {event.attempt_number} · " } { highlighted_diagnostic_message(&event.message, &search()) } }
                                    if event.truncated { span { class: "scanning-log-truncated", " Event output was truncated at the capture boundary." } }
                                }
                            } } }
                            }
                        } else {
                            div { id: "scan-detail-details-panel", class: "scanning-log-body scanning-detail-panel focus-ring", role: "tabpanel", tabindex: "0", aria_labelledby: "scan-detail-details-tab",
                                { detail_summary(detail) }
                            }
                        }
                    },
                }
                DialogFocusSentinel { dialog_id: "scan-diagnostics-dialog".to_string(), boundary: DialogFocusBoundary::First }
            }
        }
    }
}

fn detail_identity(detail: &ScanningScanDetailResponse) -> Element {
    let flake = detail.flake_name.as_deref().unwrap_or("Not recorded");
    let revision = detail.commit_hash.as_deref().unwrap_or("Not recorded");
    let short_revision = revision.chars().take(12).collect::<String>();
    rsx! {
        div { class: "scanning-detail-config",
            div { class: "scanning-detail-config-icon", Icon { name: IconName::Shield, size: 17 } }
            div { class: "scanning-detail-config-copy",
                strong { "{detail.hostname}" }
                span { class: "mono", "{flake} · {short_revision}" }
            }
        }
    }
}

fn detail_status_strip(detail: &ScanningScanDetailResponse, now: DateTime<Utc>) -> Element {
    let meta = status_meta(&detail.status);
    let elapsed = detail_elapsed_seconds(detail, now).map(format_duration);
    let trigger = detail.source_trigger.as_deref().unwrap_or("Not recorded");
    let timestamp = detail
        .completed_at
        .or(detail.started_at)
        .or(detail.scheduled_at)
        .unwrap_or(detail.created_at);
    rsx! {
        div { class: "scanning-detail-status-strip",
            span { class: "chip {meta.class}", span { class: "chip-dot", style: "background:{meta.color};" } "{meta.label}" }
            span { class: "chip chip-unknown scanning-trigger-chip", "{trigger}" }
            time { datetime: "{timestamp.to_rfc3339()}", title: "{timestamp.to_rfc3339()}", "{detail_time(timestamp)}" }
            if let Some(elapsed) = elapsed { span { class: "scanning-detail-elapsed", "Elapsed {elapsed}" } }
            if detail.archived_at.is_some() { span { class: "chip chip-unknown", "Archived" } }
            if detail.total_vulnerabilities > 0 { div { class: "scanning-detail-severity",
                if detail.critical_count > 0 { span { class: "chip chip-critical", "{detail.critical_count}C" } }
                if detail.high_count > 0 { span { class: "chip chip-warning", "{detail.high_count}H" } }
                if detail.medium_count > 0 { span { class: "chip chip-info", "{detail.medium_count}M" } }
            } }
        }
    }
}

fn detail_callout(
    detail: &ScanningScanDetailResponse,
    selected: Signal<Option<ScanDetailSelection>>,
    generation: Signal<u64>,
    retry_pending: Signal<HashSet<i32>>,
    feedback: Signal<Option<ScanActionFeedback>>,
    refresh: Signal<u64>,
) -> Element {
    let build_terminal = matches!(detail.build_status.as_deref(), Some("failed" | "cancelled"));
    let (kind, title, guidance) = if detail.status == "failed" {
        (
            "danger",
            detail
                .failure
                .as_deref()
                .unwrap_or("The vulnerability scan failed."),
            "Review the persisted scanner log. Retry starts a new exact scan for this derivation.",
        )
    } else if detail.status == "awaiting_build" && build_terminal {
        (
            "danger",
            if detail.build_status.as_deref() == Some("cancelled") {
                "The prerequisite build was cancelled"
            } else {
                "The prerequisite build failed"
            },
            "The scan remains queued and has not run Vulnix. Retry or replace the terminal build; this scan intent will continue when build output exists.",
        )
    } else if detail.status == "awaiting_build" {
        (
            "waiting",
            "Waiting on the build",
            "Vulnix needs the realized NixOS output. This scan starts automatically after the associated build succeeds.",
        )
    } else if detail.status == "awaiting_closure" {
        (
            "waiting",
            "Waiting for a reachable closure",
            "The remote build succeeded, but no completed cache publication makes its closure available to a scanner yet.",
        )
    } else {
        return rsx! {};
    };
    rsx! {
        div { class: "scanning-detail-callout scanning-detail-callout-{kind}", role: if kind == "danger" { "alert" } else { "status" },
            Icon { name: if kind == "danger" { IconName::Warn } else { IconName::Clock }, size: 15 }
            div { class: "scanning-detail-callout-copy",
                strong { "{title}" }
                p { "{guidance}" }
                div { class: "row-actions",
                    if let Some(build_job_id) = detail.build_job_id { a { class: "btn btn-ghost xs focus-ring", href: "/builds?job={build_job_id}", Icon { name: IconName::Build, size: 11 } " View build" } }
                    if detail.status == "failed" { button { class: "btn btn-ghost xs focus-ring", disabled: retry_pending.read().contains(&detail.derivation_id), onclick: { let derivation_id = detail.derivation_id; let label = format!("{} {}", detail.hostname, commit_label(&detail.commit_hash)); move |_| { retry_exact_scan(derivation_id, label.clone(), retry_pending, feedback, refresh); close_scan_detail(selected, generation); } }, Icon { name: IconName::Sync, size: 11 } " Retry scan" } }
                }
            }
        }
    }
}

fn detail_summary(detail: &ScanningScanDetailResponse) -> Element {
    let meta = status_meta(&detail.status);
    let flake = detail.flake_name.as_deref().unwrap_or("Not recorded");
    let revision = detail.commit_hash.as_deref().unwrap_or("Not recorded");
    let trigger = detail.source_trigger.as_deref().unwrap_or("Not recorded");
    let executor = detail.executor.as_deref().unwrap_or("Not recorded");
    let scheduled = detail
        .scheduled_at
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| "Not recorded".to_string());
    let started = detail
        .started_at
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| "Not started".to_string());
    let completed = detail
        .completed_at
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| "Not terminal".to_string());
    rsx! {
        div { class: "scanning-detail-summary",
            div { class: "scanning-detail-findings",
                h3 { "Findings" }
                div { class: "scanning-detail-finding-total", strong { "{detail.total_vulnerabilities}" } span { " vulnerabilities across {detail.total_packages} packages" } }
                div { class: "scanning-detail-severity",
                    span { class: "chip chip-critical", "{detail.critical_count} critical" }
                    span { class: "chip chip-warning", "{detail.high_count} high" }
                    span { class: "chip chip-info", "{detail.medium_count} medium" }
                    span { class: "chip chip-unknown", "{detail.low_count} low" }
                }
            }
            dl { class: "scanning-detail-grid",
                dt { "Configuration" } dd { "{detail.hostname}" }
                dt { "Flake" } dd { "{flake}" }
                dt { "Revision" } dd { code { "{revision}" } }
                dt { "Status" } dd { span { class: "chip {meta.class}", "{meta.label}" } }
                dt { "Trigger" } dd { "{trigger}" }
                dt { "Scanner" } dd { "{detail.scanner_name}" if let Some(version) = detail.scanner_version.as_deref() { " {version}" } }
                dt { "Executor" } dd { "{executor}" }
                dt { "Created" } dd { time { datetime: "{detail.created_at.to_rfc3339()}", "{detail.created_at.to_rfc3339()}" } }
                dt { "Scheduled" } dd { "{scheduled}" }
                dt { "Started" } dd { "{started}" }
                dt { "Completed" } dd { "{completed}" }
                dt { "Attempts" } dd { "{detail.attempts}" }
                if let Some(build_job_id) = detail.build_job_id { dt { "Build job" } dd { code { "{build_job_id}" } if let Some(build_status) = detail.build_status.as_deref() { " · {build_status}" } } }
                if let Some(reason) = detail.wait_reason.as_deref() { dt { "Wait" } dd { "{reason}" } }
                if let Some(failure) = detail.failure.as_deref() { dt { "Failure" } dd { class: "scanning-detail-failure", "{failure}" } }
                if let Some(archived_at) = detail.archived_at { dt { "Archived" } dd { "{archived_at.to_rfc3339()}" } }
                dt { "Cancellation" } dd { if detail.cancellable { "Available" } else { "Not supported by execution ownership" } }
                dt { "Derivation" } dd { code { "{detail.derivation_id}" } }
                dt { "Scan ID" } dd { code { "{detail.scan_id}" } }
            }
        }
    }
}

fn diagnostic_line_class(level: &str, active: bool) -> &'static str {
    match (level, active) {
        ("error", true) => "sd-log-line sd-log-error log-line-hit log-line-active",
        ("warning", true) => "sd-log-line sd-log-warn log-line-hit log-line-active",
        (_, true) => "sd-log-line sd-log-info log-line-hit log-line-active",
        ("error", false) => "sd-log-line sd-log-error",
        ("warning", false) => "sd-log-line sd-log-warn",
        _ => "sd-log-line sd-log-info",
    }
}

fn diagnostic_time(occurred_at: DateTime<Utc>) -> String {
    occurred_at.format("%H:%M:%S").to_string()
}

fn detail_time(timestamp: DateTime<Utc>) -> String {
    timestamp.format("%Y-%m-%d %H:%M UTC").to_string()
}

fn highlighted_diagnostic_message(message: &str, query: &str) -> Element {
    let query = query.trim();
    if query.is_empty() {
        return rsx! { "{message}" };
    }
    let lower_message = message.to_ascii_lowercase();
    let lower_query = query.to_ascii_lowercase();
    let mut parts = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = lower_message[cursor..].find(&lower_query) {
        let start = cursor + relative;
        if start > cursor {
            parts.push((false, message[cursor..start].to_string()));
        }
        let end = start + query.len();
        parts.push((true, message[start..end].to_string()));
        cursor = end;
    }
    if cursor < message.len() {
        parts.push((false, message[cursor..].to_string()));
    }
    rsx! { for (highlighted, part) in parts { if highlighted { mark { class: "log-hit", "{part}" } } else { "{part}" } } }
}

fn detail_elapsed_seconds(detail: &ScanningScanDetailResponse, now: DateTime<Utc>) -> Option<i64> {
    if detail.status == "in_progress" {
        detail
            .started_at
            .map(|started_at| now.signed_duration_since(started_at).num_seconds().max(0))
    } else {
        detail.scan_duration_ms.map(|ms| i64::from(ms) / 1_000)
    }
}

fn diagnostic_matches(detail: &ScanningScanDetailResponse, query: &str) -> Vec<usize> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    detail
        .events
        .iter()
        .enumerate()
        .filter_map(|(index, event)| {
            let haystack = format!(
                "{} {} {} {} {} {}",
                event.message,
                event.level,
                event.source,
                event.event_type,
                event.execution_id,
                event.attempt_number
            )
            .to_ascii_lowercase();
            haystack.contains(&query).then_some(index)
        })
        .collect()
}

fn diagnostic_export(detail: &ScanningScanDetailResponse) -> String {
    let mut lines = vec![
        format!("scan_id: {}", detail.scan_id),
        format!("derivation_id: {}", detail.derivation_id),
        format!("status: {}", detail.status),
        format!(
            "revision: {}",
            detail.commit_hash.as_deref().unwrap_or("not recorded")
        ),
        String::new(),
    ];
    for event in &detail.events {
        lines.push(format!(
            "{} attempt={} level={} source={} type={} execution={}{}\n{}",
            event.occurred_at.to_rfc3339(),
            event.attempt_number,
            event.level,
            event.source,
            event.event_type,
            event.execution_id,
            if event.truncated {
                " truncated=true"
            } else {
                ""
            },
            event.message
        ));
    }
    if detail.truncated {
        lines.push(
            "[response truncated: later authorized events were omitted by the API bound]"
                .to_string(),
        );
    }
    lines.join("\n")
}

#[cfg(target_arch = "wasm32")]
fn scroll_to_event(event_id: i64) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    if let Some(element) = document.get_element_by_id(&format!("scan-event-{event_id}")) {
        element.scroll_into_view();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn scroll_to_event(_event_id: i64) {}

#[allow(clippy::too_many_arguments)]
fn schedule_modal(
    policy: Option<ScanSchedulePolicyResponse>,
    load_error: Option<String>,
    mut open: Signal<bool>,
    policy_on_build: Signal<bool>,
    policy_deployed_interval: Signal<String>,
    policy_recent_interval: Signal<String>,
    policy_archived_interval: Signal<String>,
    policy_archived_enabled: Signal<bool>,
    policy_rebuild_to_scan: Signal<bool>,
    mut save_error: Signal<Option<String>>,
    mut saving: Signal<bool>,
    mut refresh: Signal<u64>,
    selected_scan: Signal<Option<ScanDetailSelection>>,
    retry: impl FnMut() + 'static,
) -> Element {
    let mut retry = retry;
    rsx! {
        div { class: "modal-backdrop", onclick: move |_| open.set(false),
            div { id: "scan-schedule-dialog", class: "modal scanning-schedule-modal", role: "dialog", aria_modal: "true", aria_labelledby: "scan-schedule-title", tabindex: "-1", onclick: move |event| event.stop_propagation(), onkeydown: move |event| if event.key() == Key::Escape && selected_scan.peek().is_none() { event.stop_propagation(); open.set(false); },
                DialogFocusRestore {}
                DialogInitialFocus { dialog_id: "scan-schedule-dialog".to_string() }
                DialogFocusSentinel { dialog_id: "scan-schedule-dialog".to_string(), boundary: DialogFocusBoundary::Last }
                div { class: "modal-head", h2 { id: "scan-schedule-title", Icon { name: IconName::Gear, size: 14 } " Scan schedule" } p { "Edit the persisted server scan policy." } }
                div { class: "modal-body",
                    if let Some(error) = load_error { { load_error_state("The scan schedule could not be loaded", &error, move || retry()) } }
                    else if policy.is_none() { div { class: "scanning-modal-state", role: "status", "Loading schedule…" } }
                    else { div { class: "scanning-schedule-rows",
                        if let Some(error) = save_error() { div { class: "sd-callout sd-callout-danger", role: "alert", "{error}" } }
                        { schedule_row("Scan on build", "Scan a freshly built exact configuration before deployment.", bool_control(policy_on_build, "Scan on build")) }
                        { schedule_row("Deployed configs", "Rescan currently running configurations.", interval_select(policy_deployed_interval, false, "Deployed configs scan interval")) }
                        { schedule_row("Recent configs", "Rescan recent configurations that are not deployed.", interval_select(policy_recent_interval, false, "Recent configs scan interval")) }
                        { schedule_row("Superseded configs", "Reduce work for superseded configurations.", archive_control(policy_archived_enabled, policy_archived_interval)) }
                        { schedule_row("Rebuild to scan old configs", "Permit policy-driven rebuilds when an archived closure is unavailable.", bool_control(policy_rebuild_to_scan, "Rebuild to scan old configs")) }
                    } }
                }
                div { class: "modal-foot",
                    button { class: "btn btn-ghost focus-ring", disabled: saving(), onclick: move |_| open.set(false), "Cancel" }
                    button { class: "btn btn-primary focus-ring", disabled: policy.is_none() || saving(), onclick: move |_| {
                        let request = UpdateScanSchedulePolicyRequest { on_build: policy_on_build(), deployed_interval: policy_deployed_interval(), recent_interval: policy_recent_interval(), archived_interval: policy_archived_interval(), archived_enabled: policy_archived_enabled(), rebuild_to_scan: policy_rebuild_to_scan() };
                        save_error.set(None); saving.set(true);
                        spawn(async move { match update_scanning_schedule(&request).await { Ok(_) => { refresh.set(refresh().wrapping_add(1)); open.set(false); }, Err(error) => save_error.set(Some(format!("The scan schedule could not be saved: {error}"))) } saving.set(false); });
                    }, Icon { name: IconName::Check, size: 13 } if saving() { " Saving…" } else { " Save schedule" } }
                }
                DialogFocusSentinel { dialog_id: "scan-schedule-dialog".to_string(), boundary: DialogFocusBoundary::First }
            }
        }
    }
}

fn bool_control(mut value: Signal<bool>, label: &'static str) -> Element {
    rsx! { label { class: "scanning-toggle", input { r#type: "checkbox", aria_label: "{label}", checked: value(), onchange: move |event| value.set(event.checked()) } span { if value() { "On" } else { "Off" } } } }
}

fn archive_control(enabled: Signal<bool>, interval: Signal<String>) -> Element {
    rsx! { div { class: "scanning-archive-control", { bool_control(enabled, "Scan superseded configs") } { interval_select(interval, !enabled(), "Superseded configs scan interval") } } }
}

fn interval_select(mut value: Signal<String>, disabled: bool, label: &'static str) -> Element {
    rsx! { select { class: "input focus-ring", aria_label: "{label}", disabled, value: value(), oninput: move |event| value.set(event.value()), for option in ["1h", "6h", "12h", "24h", "7d", "30d", "168h", "336h", "never"] { option { value: "{option}", if option == "never" { "Never" } else { "Every {option}" } } } } }
}

fn schedule_row(title: &str, description: &str, control: Element) -> Element {
    rsx! { div { class: "scanning-schedule-row", div { div { class: "scanning-schedule-title", "{title}" } div { class: "scanning-schedule-description", "{description}" } } div { class: "scanning-schedule-control", {control} } } }
}

fn load_error_state(title: &str, error: &str, retry: impl FnMut() + 'static) -> Element {
    let mut retry = retry;
    rsx! { div { class: "q-empty", role: "alert", Icon { name: IconName::Warn, size: 20 } h3 { "{title}" } p { "{error}" } button { class: "btn btn-ghost xs focus-ring", onclick: move |_| retry(), "Retry" } } }
}

fn findings(critical: i32, high: i32, medium: i32, low: i32, authoritative: bool) -> Element {
    if !authoritative {
        return rsx! { span { class: "scanning-unavailable", "Not available" } };
    }
    rsx! { div { class: "scanning-findings",
        if critical > 0 { span { class: "chip chip-critical", "{critical}C" } }
        if high > 0 { span { class: "chip chip-warning", "{high}H" } }
        if medium > 0 { span { class: "chip chip-info", "{medium}M" } }
        if low > 0 { span { class: "chip chip-unknown", "{low}L" } }
        if critical + high + medium + low == 0 { span { class: "chip chip-healthy", Icon { name: IconName::Check, size: 9 } " clean" } }
    } }
}

fn relative_time(timestamp: DateTime<Utc>) -> String {
    let age = Utc::now().signed_duration_since(timestamp);
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

fn format_duration(seconds: i64) -> String {
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours}h {minutes:02}m {seconds:02}s")
    } else {
        format!("{minutes}m {seconds:02}s")
    }
}

fn commit_label(commit_hash: &Option<String>) -> String {
    commit_hash
        .as_deref()
        .filter(|hash| !hash.is_empty())
        .map(|hash| hash.chars().take(12).collect())
        .unwrap_or_else(|| "unknown".to_string())
}

fn coverage_width(count: i64, total: i64) -> String {
    if total <= 0 {
        return "0%".to_string();
    }
    format!("{:.2}%", (count.max(0) as f64 / total as f64) * 100.0)
}

fn stat_card(label: &str, value: &str, meta: Option<&str>, color: &str) -> Element {
    rsx! { div { class: "stat", span { class: "stat-accent", style: "--stat-color:{color};" } div { class: "stat-label", "{label}" } div { class: "stat-value", style: "color:{color};", "{value}" } if let Some(meta) = meta { div { class: "stat-meta", "{meta}" } } } }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;
    use crate::api::models::ScanningScanDiagnosticEventResponse;

    fn row(
        hostname: &str,
        status: &str,
        hours_ago: i64,
        critical: i32,
    ) -> ScanningScanRecordResponse {
        let timestamp = Utc::now() - Duration::hours(hours_ago);
        ScanningScanRecordResponse {
            scan_id: Uuid::new_v4(),
            derivation_id: critical + hours_ago as i32 + 1,
            hostname: hostname.to_string(),
            flake_name: Some("infra".to_string()),
            commit_hash: Some(format!("{hostname}-{hours_ago:02}-full-revision")),
            is_current: false,
            is_latest_per_flake: false,
            status: status.to_string(),
            source_trigger: Some("manual".to_string()),
            created_at: timestamp,
            scheduled_at: Some(timestamp),
            started_at: (status == "in_progress").then_some(timestamp),
            completed_at: matches!(status, "completed" | "failed").then_some(timestamp),
            scanner_name: "vulnix".to_string(),
            scanner_version: None,
            executor: None,
            failure: (status == "failed").then(|| "failure".to_string()),
            wait_reason: status
                .starts_with("awaiting")
                .then(|| "prerequisite".to_string()),
            total_packages: 10,
            total_vulnerabilities: critical,
            critical_count: critical,
            high_count: 0,
            medium_count: 0,
            low_count: 0,
            scan_duration_ms: Some(1_000),
            attempts: 1,
            archived_at: None,
            cancellable: false,
        }
    }

    fn detail(
        events: Vec<ScanningScanDiagnosticEventResponse>,
        truncated: bool,
    ) -> ScanningScanDetailResponse {
        let row = row("atlas", "failed", 1, 1);
        ScanningScanDetailResponse {
            scan_id: row.scan_id,
            derivation_id: row.derivation_id,
            hostname: row.hostname,
            flake_name: row.flake_name,
            commit_hash: row.commit_hash,
            status: row.status,
            scanner_name: row.scanner_name,
            scanner_version: row.scanner_version,
            source_trigger: row.source_trigger,
            created_at: row.created_at,
            scheduled_at: row.scheduled_at,
            started_at: row.started_at,
            completed_at: row.completed_at,
            scan_duration_ms: row.scan_duration_ms,
            attempts: row.attempts,
            total_packages: row.total_packages,
            total_vulnerabilities: row.total_vulnerabilities,
            critical_count: row.critical_count,
            high_count: row.high_count,
            medium_count: row.medium_count,
            low_count: row.low_count,
            failure: row.failure,
            wait_reason: row.wait_reason,
            build_job_id: Some(Uuid::new_v4()),
            build_status: Some("failed".to_string()),
            executor: row.executor,
            archived_at: row.archived_at,
            cancellable: false,
            events,
            truncated,
        }
    }

    fn unscanned_derivation(rescan_eligible: bool) -> ScanningQueueItemResponse {
        ScanningQueueItemResponse {
            derivation_id: 42,
            rescan_eligible,
            scan_id: None,
            hostname: "atlas".to_string(),
            flake_name: Some("infra".to_string()),
            commit_hash: Some("full-unscanned-revision".to_string()),
            status: "never_scanned".to_string(),
            completed_at: None,
            scheduled_at: None,
            critical_count: 0,
            high_count: 0,
            medium_count: 0,
            freshness: "archived".to_string(),
            is_current: true,
            is_latest_per_flake: true,
            source_trigger: None,
        }
    }

    #[test]
    fn classifies_all_authoritative_active_states() {
        assert_eq!(status_meta("in_progress").label, "Scanning");
        assert_eq!(status_meta("pending").label, "Queued");
        assert_eq!(status_meta("awaiting_build").label, "Awaiting build");
        assert_eq!(status_meta("awaiting_closure").label, "Awaiting closure");
    }

    #[test]
    fn filters_and_sorts_with_domain_values_and_stable_identity() {
        let rows = vec![row("zeta", "completed", 3, 0), row("atlas", "failed", 1, 2)];
        let filtered = filter_and_sort_records(
            &rows,
            "atlas",
            "failed",
            "all",
            false,
            ScanSort::Timestamp,
            true,
        );
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].hostname, "atlas");
        let sorted =
            filter_and_sort_records(&rows, "", "all", "all", false, ScanSort::Severity, true);
        assert_eq!(sorted[0].hostname, "atlas");
    }

    #[test]
    fn latest_filter_uses_server_revision_authority_not_scan_time() {
        let mut older_commit_rescanned_today = row("older", "completed", 0, 0);
        older_commit_rescanned_today.is_latest_per_flake = false;
        let mut newer_commit_scanned_yesterday = row("newer", "completed", 24, 0);
        newer_commit_scanned_yesterday.is_latest_per_flake = true;

        let filtered = filter_and_sort_records(
            &[
                older_commit_rescanned_today,
                newer_commit_scanned_yesterday.clone(),
            ],
            "",
            "all",
            "all",
            true,
            ScanSort::Timestamp,
            true,
        );

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].scan_id, newer_commit_scanned_yesterday.scan_id);
    }

    #[test]
    fn severity_sort_compares_each_severity_without_weight_collisions() {
        let mut one_critical = row("critical", "completed", 1, 1);
        one_critical.high_count = 0;
        let mut many_high = row("high", "completed", 1, 0);
        many_high.high_count = 1_000;

        let sorted = filter_and_sort_records(
            &[many_high, one_critical],
            "",
            "all",
            "all",
            false,
            ScanSort::Severity,
            true,
        );

        assert_eq!(sorted[0].hostname, "critical");
    }

    #[test]
    fn failed_stat_selects_newest_failure_deterministically() {
        let older = row("older", "failed", 4, 0);
        let newer = row("newer", "failed", 1, 0);
        assert_eq!(
            first_failed(&[older, newer.clone()]).unwrap().scan_id,
            newer.scan_id
        );
    }

    #[test]
    fn diagnostic_search_and_export_preserve_authorized_order_and_bounds() {
        let event = |id, message: &str| ScanningScanDiagnosticEventResponse {
            id,
            execution_id: Uuid::new_v4(),
            attempt_number: 1,
            occurred_at: Utc::now(),
            level: "error".to_string(),
            source: "vulnix".to_string(),
            event_type: "output".to_string(),
            message: message.to_string(),
            truncated: false,
        };
        let detail = detail(vec![event(1, "first needle"), event(2, "second")], true);
        assert_eq!(diagnostic_matches(&detail, "needle"), vec![0]);
        let export = diagnostic_export(&detail);
        assert!(export.find("first needle").unwrap() < export.find("second").unwrap());
        assert!(export.contains("response truncated"));
    }

    #[test]
    fn detail_requests_reject_stale_generations() {
        let scan_id = Uuid::new_v4();
        let selection = ScanDetailSelection {
            scan_id,
            label: "scan".to_string(),
        };
        assert!(!scan_detail_request_is_current(
            ScanDetailRequest {
                scan_id,
                generation: 1
            },
            Some(&selection),
            2
        ));
        assert!(scan_detail_request_is_current(
            ScanDetailRequest {
                scan_id,
                generation: 2
            },
            Some(&selection),
            2
        ));
    }

    #[test]
    fn archive_aware_totals_distinguish_visible_and_all_rows() {
        let response = ScanningScanRecordsResponse {
            items: vec![row("atlas", "completed", 1, 0)],
            total: 12,
            hidden_archived: 5,
        };
        assert_eq!(visible_record_total(&response, false), 7);
        assert_eq!(visible_record_total(&response, true), 12);
    }

    #[test]
    fn system_history_includes_unscanned_derivations_without_scan_identity() {
        let entry = system_history_entries(SystemHistoryData {
            scans: ScanningScanRecordsResponse {
                items: Vec::new(),
                total: 0,
                hidden_archived: 0,
            },
            derivations: vec![unscanned_derivation(false)],
        })
        .pop()
        .expect("the no-scan derivation should remain visible");
        assert!(
            matches!(entry, SystemHistoryEntry::NoScan(row) if row.scan_id.is_none() && !row.rescan_eligible)
        );
    }

    #[test]
    fn running_elapsed_requires_authoritative_start_time() {
        let mut running = detail(Vec::new(), false);
        running.status = "in_progress".to_string();
        running.created_at = Utc::now() - Duration::hours(3);
        running.started_at = None;
        assert_eq!(detail_elapsed_seconds(&running, Utc::now()), None);

        let now = Utc::now();
        running.started_at = Some(now - Duration::seconds(75));
        assert_eq!(detail_elapsed_seconds(&running, now), Some(75));
    }
}
