//! Builds control center view.

use chrono::{Duration, Utc};
use dioxus::prelude::*;

use crate::api::{
    self,
    client::{
        fetch_build_queue_paginated, fetch_recent_build_jobs, move_build_job_down,
        move_build_job_up,
    },
    models::{BuildQueueParams, BuildStatus as ApiBuildStatus, BuilderStatus},
};
use crate::components::builds::{
    BuildAction, BuildDetailPane, BuildItem, BuildQueuePane, BuildStatus, ConfirmActionModal,
    DetailTab, MetricsRow, PendingAction, QueueAction, QueueActionButton, WorkerAction, WorkerItem,
    WorkerStatus, WorkerStrip, extract_system_name, selected_build_data,
};
use crate::state::app_state::AppState;
use crate::state::auth;
use crate::theme;

const PAGE_SIZE: i64 = 50;

#[derive(Clone, Copy, PartialEq, Eq)]
enum BuildsTab {
    ActiveQueue,
    Completed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CompletedStatusFilter {
    All,
    Complete,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CompletedSortOrder {
    NewestFirst,
    OldestFirst,
}

fn format_completed_at(item: &BuildItem) -> String {
    item.completed_at
        .map(|ts| ts.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Format seconds into human-readable duration (e.g., "2m 15s", "1h 30m").
fn format_human_duration(seconds: i64) -> String {
    let secs = seconds.max(0);
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let remaining_secs = secs % 60;

    if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, remaining_secs)
    } else {
        format!("{}s", remaining_secs)
    }
}

fn format_duration(item: &BuildItem) -> String {
    item.duration_secs
        .map(format_human_duration)
        .or_else(|| item.runtime.clone())
        .unwrap_or_else(|| "-".to_string())
}

fn format_environment(item: &BuildItem) -> String {
    item.environment.clone().unwrap_or_else(|| "-".to_string())
}

/// Truncate a string with ellipsis for display.
fn truncate_with_ellipsis(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

/// Render a single log line with structured formatting (timestamp, level, message).
/// JSX structure: <div className="sd-log-line sd-log-${lvl}">
fn render_log_line(line: &str) -> Element {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return rsx! { div { class: "h-1" } };
    }

    // Parse timestamp if present (format: HH:MM:SS or similar at start)
    let (timestamp, rest) = if let Some(pos) = trimmed.find(|c: char| c != ':' && !c.is_numeric()) {
        if pos > 0 && pos < 12 && trimmed[..pos].contains(':') {
            (&trimmed[..pos], trimmed[pos..].trim())
        } else {
            ("", trimmed)
        }
    } else {
        ("", trimmed)
    };

    // Simple heuristic: look for log level keywords
    let (log_level_class, level_label) = if rest.contains("error") || rest.contains("ERROR") {
        ("sd-log-error", "ERROR")
    } else if rest.contains("warn") || rest.contains("WARN") {
        ("sd-log-warn", "WARN")
    } else if rest.contains("info") || rest.contains("INFO") {
        ("sd-log-info", "INFO")
    } else {
        ("sd-log-info", "INFO") // Default to info level
    };

    // JSX: <div className="sd-log-line sd-log-${lvl}">
    //        <span className="sd-log-t">{t}</span>
    //        <span className="sd-log-lvl">{lvl.toUpperCase()}</span>
    //        <span className="sd-log-m">{m}</span>
    //      </div>
    rsx! {
        div {
            class: "sd-log-line {log_level_class}",
            // JSX: <span className="sd-log-t">{t}</span>
            span { class: "sd-log-t", "{timestamp}" }
            // JSX: <span className="sd-log-lvl">{lvl.toUpperCase()}</span>
            span { class: "sd-log-lvl", "{level_label}" }
            // JSX: <span className="sd-log-m">{m}</span>
            span { class: "sd-log-m", "{rest}" }
        }
    }
}

/// Map a raw `BuildQueueItem` from API into the UI `BuildItem`.
fn map_queue_item(item: &crate::api::models::BuildQueueItem, idx: usize) -> BuildItem {
    // JSX shows simple relative time like "5m" in queuedAt column
    let queued_for = {
        let ago = (Utc::now() - item.queued_at).num_seconds().max(0);
        format_human_duration(ago)
    };

    BuildItem {
        id: (idx + 1) as i32,
        job_id: item.job_id,
        system_id: item.system_id,
        hostname: item.hostname.clone(),
        environment: item.environment.clone(),
        flake: item.flake_name.clone(),
        commit: item.commit_hash.clone(),
        branch: "main".to_string(),
        arch: "x86_64-linux".to_string(),
        worker_id: item
            .builder_name
            .clone()
            .unwrap_or_else(|| "unassigned".to_string()),
        queued_at: item.queued_at,
        queued_for,
        runtime: item.elapsed_secs.map(format_human_duration),
        duration_secs: item.elapsed_secs,
        completed_at: None,
        started_by: "scheduler".to_string(),
        logs: item.logs.clone(),
        status: match item.status {
            ApiBuildStatus::Queued => BuildStatus::Queued,
            ApiBuildStatus::Building => BuildStatus::Building,
            ApiBuildStatus::Cancelling => BuildStatus::Stopping,
            ApiBuildStatus::Failed => BuildStatus::Failed,
            ApiBuildStatus::Complete => BuildStatus::Complete,
            ApiBuildStatus::Cancelled => BuildStatus::Cancelled,
            ApiBuildStatus::Idle => BuildStatus::Queued,
        },
        summary: item.commit_message.clone().unwrap_or_else(|| {
            format!(
                "job {}",
                item.job_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            )
        }),
        cached_derivs: item.cached_derivs as usize,
        built_derivs: item.built_derivs as usize,
        total_derivs: item.total_derivs as usize,
        current_pkg: None,
        failed_pkg: None,
        attempts: 1,
    }
}

/// Builds control center page.
#[component]
pub fn BuildsView() -> Element {
    let app_state = use_context::<Signal<AppState>>();
    let can_requeue = auth::is_operator_or_above(&app_state.read().auth);

    let mut workers = use_signal(Vec::<WorkerItem>::new);
    let mut refresh_trigger = use_signal(|| 0_u64);

    // Auto-refresh: bump refresh_trigger every 5 s so active builds, queue
    // positions, and live log snapshots stay current without user interaction.
    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(5_000).await;
            refresh_trigger.set(refresh_trigger() + 1);
        }
    });

    // --- Active queue state ---
    let mut queue_page = use_signal(|| 1_i64);
    let mut queue_total = use_signal(|| 0_i64);
    let mut builds = use_signal(Vec::<BuildItem>::new);

    // Filter state — Active Queue defaults to active jobs only (queued + building + cancelling).
    // Operators can widen to "All" or other statuses via the status dropdown.
    let mut filter_status = use_signal(|| "queued,building,cancelling".to_string());
    let mut filter_commit = use_signal(String::new);
    let mut filter_flake = use_signal(String::new);
    let mut filter_config = use_signal(String::new);
    // Simple time range: "today", "last7d", "" (all)
    let mut filter_time_range = use_signal(String::new);

    // Derived filter signals used to trigger resource re-fetch
    let queue_resource = use_resource(move || async move {
        let _ = refresh_trigger();
        let page = queue_page();
        let status = filter_status();
        let commit = filter_commit();
        let flake = filter_flake();
        let config = filter_config();
        let time = filter_time_range();

        let (queued_after, queued_before) = match time.as_str() {
            "today" => {
                let start = Utc::now()
                    .date_naive()
                    .and_hms_opt(0, 0, 0)
                    .map(|dt| dt.and_utc());
                (start, None)
            }
            "last7d" => {
                let start = Utc::now() - Duration::days(7);
                (Some(start), None)
            }
            _ => (None, None),
        };

        let params = BuildQueueParams {
            page: Some(page),
            limit: Some(PAGE_SIZE),
            status: if status.is_empty() {
                None
            } else {
                Some(status)
            },
            commit_hash: if commit.is_empty() {
                None
            } else {
                Some(commit)
            },
            flake_name: if flake.is_empty() { None } else { Some(flake) },
            config_name: if config.is_empty() {
                None
            } else {
                Some(config)
            },
            queued_after,
            queued_before,
        };
        fetch_build_queue_paginated(&params).await
    });

    let builders = use_resource(move || async move {
        let _ = refresh_trigger();
        api::client::fetch_builders().await
    });
    let recent_builds = use_resource(move || async move {
        let _ = refresh_trigger();
        fetch_recent_build_jobs().await
    });

    let mut build_history = use_signal(Vec::<BuildItem>::new);

    use_effect(move || {
        if let Some(Ok(builder_list)) = &*builders.read() {
            let mapped = builder_list
                .iter()
                .map(|builder| WorkerItem {
                    id: builder.id.to_string(),
                    name: builder.name.clone(),
                    host: Some(format!("{}.builder", builder.name)),
                    arch: Some("x86_64-linux".to_string()),
                    cpu_cores: builder.max_cpu_cores,
                    memory_gb: builder.max_memory_mb.map(|mb| mb / 1024),
                    active_slots: builder.active_jobs.max(0) as usize,
                    total_slots: builder.max_concurrent_jobs.max(1) as usize,
                    queue_depth: builder.queued_jobs.max(0) as usize,
                    status: if !builder.enabled {
                        WorkerStatus::Paused // Disabled builders always show as paused
                    } else {
                        match builder.status {
                            BuilderStatus::Active => WorkerStatus::Running,
                            BuilderStatus::Inactive => WorkerStatus::Paused,
                            BuilderStatus::Offline => WorkerStatus::Paused, // Treat offline as paused in UI
                            BuilderStatus::Draining => WorkerStatus::Draining,
                        }
                    },
                })
                .collect::<Vec<_>>();
            workers.set(mapped);
        }
    });

    use_effect(move || {
        if let Some(Ok(page_resp)) = &*queue_resource.read() {
            let mapped = page_resp
                .items
                .iter()
                .enumerate()
                .map(|(idx, item)| map_queue_item(item, idx))
                .collect::<Vec<_>>();
            builds.set(mapped);
            queue_total.set(page_resp.total);
        }
    });

    use_effect(move || {
        if let Some(Ok(items)) = &*recent_builds.read() {
            let mapped = items
                .iter()
                .enumerate()
                .map(|(idx, item)| {
                    let finished_for = item
                        .elapsed_secs
                        .map(|secs| format!("completed in {}s", secs))
                        .unwrap_or_else(|| "completed".to_string());
                    let completed_at = match (item.started_at, item.elapsed_secs) {
                        (Some(started_at), Some(elapsed_secs)) => {
                            Some(started_at + Duration::seconds(elapsed_secs.max(0)))
                        }
                        _ => Some(item.queued_at),
                    };

                    BuildItem {
                        id: -((idx as i32) + 1),
                        job_id: item.job_id,
                        system_id: item.system_id,
                        hostname: item.hostname.clone(),
                        environment: item.environment.clone(),
                        flake: item.flake_name.clone(),
                        commit: item.commit_hash.clone(),
                        branch: "main".to_string(),
                        arch: "x86_64-linux".to_string(),
                        worker_id: item
                            .builder_name
                            .clone()
                            .unwrap_or_else(|| "unassigned".to_string()),
                        queued_at: item.queued_at,
                        queued_for: finished_for,
                        runtime: item.elapsed_secs.map(format_human_duration),
                        duration_secs: item.elapsed_secs,
                        completed_at,
                        started_by: "scheduler".to_string(),
                        logs: item.logs.clone(),
                        status: match item.status {
                            ApiBuildStatus::Failed => BuildStatus::Failed,
                            ApiBuildStatus::Complete => BuildStatus::Complete,
                            ApiBuildStatus::Cancelled => BuildStatus::Cancelled,
                            ApiBuildStatus::Building => BuildStatus::Building,
                            ApiBuildStatus::Cancelling => BuildStatus::Stopping,
                            ApiBuildStatus::Queued => BuildStatus::Queued,
                            ApiBuildStatus::Idle => BuildStatus::Queued,
                        },
                        summary: item.commit_message.clone().unwrap_or_else(|| {
                            format!(
                                "job {}",
                                item.job_id
                                    .map(|id| id.to_string())
                                    .unwrap_or_else(|| "unknown".to_string())
                            )
                        }),
                        cached_derivs: 0,
                        built_derivs: 0,
                        total_derivs: 0,
                        current_pkg: None,
                        failed_pkg: None,
                        attempts: 1,
                    }
                })
                .collect::<Vec<_>>();
            build_history.set(mapped);
        }
    });

    let mut selected_build = use_signal(|| None::<i32>);
    let mut log_open = use_signal(|| false);
    let mut active_view = use_signal(|| BuildsTab::ActiveQueue);
    let mut active_tab = use_signal(|| DetailTab::Details);
    let mut completed_status_filter = use_signal(|| CompletedStatusFilter::All);
    let mut completed_sort_order = use_signal(|| CompletedSortOrder::NewestFirst);

    // Search/filter state (JSX: query)
    let mut search_query = use_signal(String::new);
    // Attention flash for failed rows on first Completed tab open (JSX: flashHistRows)
    let mut acked_hist = use_signal(|| false);
    let mut flash_hist_rows = use_signal(|| false);

    let follow_logs = use_signal(|| true);
    let pause_logs = use_signal(|| false);
    let wrap_logs = use_signal(|| false);
    let log_query = use_signal(String::new);

    let mut pending_action = use_signal(|| None::<PendingAction>);
    let mut last_action_note = use_signal(|| None::<String>);
    let mut action_error = use_signal(|| None::<String>);

    let queue_data = builds.read().clone();
    let worker_data = workers.read().clone();

    let mut completed_rows = build_history.read().clone();
    completed_rows.retain(|item| {
        matches!(
            item.status,
            BuildStatus::Complete | BuildStatus::Failed | BuildStatus::Cancelled
        ) && match completed_status_filter() {
            CompletedStatusFilter::All => true,
            CompletedStatusFilter::Complete => item.status == BuildStatus::Complete,
            CompletedStatusFilter::Failed => item.status == BuildStatus::Failed,
            CompletedStatusFilter::Cancelled => item.status == BuildStatus::Cancelled,
        }
    });
    completed_rows.sort_by(|left, right| {
        let left_key = left.completed_at.unwrap_or_else(Utc::now);
        let right_key = right.completed_at.unwrap_or_else(Utc::now);
        match completed_sort_order() {
            CompletedSortOrder::NewestFirst => right_key.cmp(&left_key),
            CompletedSortOrder::OldestFirst => left_key.cmp(&right_key),
        }
    });

    let visible_rows = if active_view() == BuildsTab::ActiveQueue {
        queue_data.clone()
    } else {
        completed_rows.clone()
    };
    let selected = selected_build_data(selected_build.read().to_owned(), &visible_rows);

    // Search filter: matches system, flake, commit, worker, arch, status label.
    let search_q = search_query.read().trim().to_lowercase();
    let match_build = |b: &BuildItem| -> bool {
        if search_q.is_empty() {
            return true;
        }
        [
            Some(extract_system_name(&b.hostname).to_lowercase()),
            Some(b.flake.to_lowercase()),
            Some(b.commit.to_lowercase()),
            if b.worker_id == "unassigned" {
                None
            } else {
                Some(b.worker_id.to_lowercase())
            },
            Some(b.arch.to_lowercase()),
            Some(b.status.label().to_lowercase()),
        ]
        .into_iter()
        .flatten()
        .any(|v| v.contains(&search_q))
    };

    let (base_list, filtered_list): (Vec<BuildItem>, Vec<BuildItem>) =
        if active_view() == BuildsTab::ActiveQueue {
            let f: Vec<BuildItem> = queue_data
                .iter()
                .filter(|b| match_build(b))
                .cloned()
                .collect();
            (queue_data.clone(), f)
        } else {
            let f: Vec<BuildItem> = completed_rows
                .iter()
                .filter(|b| match_build(b))
                .cloned()
                .collect();
            (completed_rows.clone(), f)
        };
    let base_len = base_list.len();
    let filtered_len = filtered_list.len();

    let total_pages = {
        let t = queue_total();
        if t == 0 {
            1
        } else {
            (t + PAGE_SIZE - 1) / PAGE_SIZE
        }
    };

    rsx! {
        // JSX: <div style={{ display:"flex", flexDirection:"column", gap:16 }}>
        // gap:16 = 16px = space-y-4 (1rem = 16px)
        div {
            class: "space-y-4",

            // JSX: <div className="page-head">
            header {
                class: "page-head",
                div {
                    // JSX: <h1 className="page-title">
                    h1 { class: "page-title", "Builds" }
                    // JSX: <p className="page-subtitle">
                    p {
                        class: "page-subtitle",
                        "{queue_data.iter().filter(|b| matches!(b.status, BuildStatus::Building | BuildStatus::Stopping)).count()} building · {queue_data.iter().filter(|b| b.status == BuildStatus::Queued).count()} queued · {worker_data.iter().filter(|w| w.status == WorkerStatus::Running).count()}/{worker_data.len()} workers active"
                    }
                }
                // JSX: <LiveIndicator />
                LiveIndicator {}
            }

            MetricsRow {
                workers: worker_data.clone(),
                builds: queue_data.clone(),
                history_builds: build_history.read().clone(),
            }

            section {
                div {
                    class: "text-[12px] font-semibold uppercase tracking-[0.08em] {theme::text::MUTED} mb-[10px]",
                    "Build Workers"
                }

                WorkerStrip {
                    workers: worker_data.clone(),
                    on_action: move |(worker_id, action)| {
                        pending_action.set(Some(PendingAction::Worker { worker_id, action }))
                    },
                }
            }

            // JSX: <div className="card" style={{ overflow:"hidden" }}>
            div {
                class: "card overflow-hidden",
                // JSX: <div className="sd-tabs q-tabbar" style={{ padding:"0 16px", borderBottom:"1px solid var(--cf-card-border)" }}>
                div {
                    class: "sd-tabs q-tabbar",
                    style: "padding: 0 16px; border-bottom: 1px solid var(--cf-card-border);",
                    button {
                        class: if active_view() == BuildsTab::ActiveQueue {
                            "sd-tab focus-ring active"
                        } else {
                            "sd-tab focus-ring"
                        },
                        onclick: move |_| {
                            active_view.set(BuildsTab::ActiveQueue);
                            selected_build.set(None);
                            log_open.set(false);
                            search_query.set(String::new());
                        },
                        "Active "
                        span { class: "sd-tab-badge", "{queue_data.len()}" }
                    }
                    button {
                        class: if active_view() == BuildsTab::Completed {
                            "sd-tab focus-ring active"
                        } else {
                            "sd-tab focus-ring"
                        },
                        onclick: move |_| {
                            let prev = active_view();
                            active_view.set(BuildsTab::Completed);
                            selected_build.set(None);
                            log_open.set(false);
                            search_query.set(String::new());
                            // JSX: flash failed rows on first history open
                            if prev != BuildsTab::Completed && !acked_hist() {
                                acked_hist.set(true);
                                flash_hist_rows.set(true);
                                let mut fh = flash_hist_rows;
                                spawn(async move {
                                    gloo_timers::future::TimeoutFuture::new(3_200).await;
                                    fh.set(false);
                                });
                            }
                        },
                        "Completed "
                        span { class: "sd-tab-badge", "{build_history.read().len()}" }
                    }
                    // JSX: {tab==="active" && cancellable.length > 0 && <MultiSelectHint />}
                    if active_view() == BuildsTab::ActiveQueue
                        && queue_data.iter().any(|b| matches!(b.status, BuildStatus::Queued | BuildStatus::Building | BuildStatus::Stopping))
                    {
                        span {
                            class: "ms-hint",
                            title: "Shift-click to toggle cancellable rows",
                            kbd { "⇧" }
                            "-click to select"
                        }
                    }
                    // JSX: search bar
                    div {
                        class: "q-search",
                        // search icon
                        svg {
                            width: "13", height: "13",
                            view_box: "0 0 24 24",
                            fill: "none", stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round", stroke_linejoin: "round",
                            circle { cx: "11", cy: "11", r: "8" }
                            line { x1: "21", y1: "21", x2: "16.65", y2: "16.65" }
                        }
                        input {
                            class: "q-search-input",
                            placeholder: if active_view() == BuildsTab::ActiveQueue {
                                "Search active builds\u{2026}"
                            } else {
                                "Search completed builds\u{2026}"
                            },
                            value: "{search_query}",
                            oninput: move |evt| search_query.set(evt.value()),
                        }
                        if !search_query.read().is_empty() {
                            span {
                                class: "q-search-count",
                                "{filtered_len} of {base_len}"
                            }
                            button {
                                class: "btn-icon xs focus-ring",
                                title: "Clear search",
                                onclick: move |_| search_query.set(String::new()),
                                svg {
                                    width: "13", height: "13",
                                    view_box: "0 0 24 24",
                                    fill: "none", stroke: "currentColor",
                                    stroke_width: "2",
                                    line { x1: "18", y1: "6", x2: "6", y2: "18" }
                                    line { x1: "6", y1: "6", x2: "18", y2: "18" }
                                }
                            }
                        }
                    }
                }
                // JSX: {filteredList.length === 0 ? <EmptyState/> : <BuildQueueTable .../>}
                if !search_q.is_empty() && filtered_list.is_empty() {
                    div {
                        class: "q-empty",
                        svg {
                            width: "20", height: "20",
                            view_box: "0 0 24 24",
                            fill: "none", stroke: "currentColor",
                            stroke_width: "2",
                            circle { cx: "11", cy: "11", r: "8" }
                            line { x1: "21", y1: "21", x2: "16.65", y2: "16.65" }
                        }
                        div { "No builds match \"{search_q}\"." }
                        button {
                            class: "btn btn-ghost xs focus-ring",
                            onclick: move |_| search_query.set(String::new()),
                            "Clear search"
                        }
                    }
                } else {
                    BuildQueuePane {
                        builds: filtered_list.clone(),
                        selected_id: selected_build,
                        flash_failed: flash_hist_rows(),
                        can_requeue,
                        on_build_action: move |(build_id, action)| {
                            match action {
                                BuildAction::MoveUp | BuildAction::MoveDown => {
                                    let queue_snapshot = builds.read().clone();
                                    let mut action_error = action_error;
                                    let mut last_action_note = last_action_note;
                                    let mut refresh_trigger = refresh_trigger;
                                    spawn(async move {
                                        let selected = queue_snapshot.iter().find(|b| b.id == build_id);
                                        let Some(selected) = selected else {
                                            action_error.set(Some(format!("Build row #{} not found", build_id)));
                                            return;
                                        };
                                        let Some(job_id) = selected.job_id else {
                                            action_error.set(Some("Queue item has no job id; cannot reorder".to_string()));
                                            return;
                                        };

                                        let result = if action == BuildAction::MoveUp {
                                            move_build_job_up(&job_id).await
                                        } else {
                                            move_build_job_down(&job_id).await
                                        };

                                        match result {
                                            Ok(_) => {
                                                action_error.set(None);
                                                last_action_note.set(Some(
                                                    if action == BuildAction::MoveUp {
                                                        format!("Moved job {} up", job_id)
                                                    } else {
                                                        format!("Moved job {} down", job_id)
                                                    },
                                                ));
                                                refresh_trigger.set(refresh_trigger() + 1);
                                            }
                                            Err(e) => action_error.set(Some(format!("Failed to reorder: {}", e))),
                                        }
                                    });
                                }
                                _ => pending_action.set(Some(PendingAction::Build { build_id, action })),
                            }
                        },
                        on_log: move |build_id| {
                            // JSX parity: open the tray on its Log tab (not a separate modal).
                            selected_build.set(Some(build_id));
                            active_tab.set(DetailTab::Logs);
                            log_open.set(false);
                        },
                        on_bulk_rerun: {
                            move |build_ids: Vec<i32>| {
                                let mut action_error = action_error;
                                let mut last_action_note = last_action_note;
                                let mut refresh_trigger = refresh_trigger;
                                let filtered = filtered_list.clone();
                                spawn(async move {
                                    let count = build_ids.len();
                                    for id in &build_ids {
                                        if let Some(build) = filtered.iter().find(|b| b.id == *id) {
                                            if let Some(jid) = build.job_id {
                                                let _ = api::client::requeue_build_job(&jid).await;
                                            }
                                        }
                                    }
                                    action_error.set(None);
                                    let suffix = if count == 1 { "" } else { "s" };
                                    last_action_note.set(Some(format!("Re-queued {count} build{suffix}")));
                                    refresh_trigger.set(refresh_trigger() + 1);
                                });
                            }
                        },
                        on_bulk_download_logs: move |_build_ids| {
                            // TODO: Download logs as a single archive
                            action_error.set(Some("Download logs not yet implemented".to_string()));
                        },
                        on_bulk_delete: move |_build_ids| {
                            // TODO: Delete build history entries
                            action_error.set(Some("Delete builds not yet implemented".to_string()));
                        },
                    }
                }
            }

            if selected.is_some() {
                // JSX: <div className="fl-tray-backdrop" onClick={onClose} />
                div {
                    class: "fl-tray-backdrop",
                    onclick: move |_| {
                        selected_build.set(None);
                        log_open.set(false);
                    },
                }
                // JSX: <aside className="fl-tray build-log-tray">
                aside {
                    class: "fl-tray build-log-tray",
                    onclick: |evt| evt.stop_propagation(),
                    {
                        let selected_for_action = selected.clone();
                        rsx! {
                    BuildDetailPane {
                        selected: selected.clone(),
                        can_requeue,
                        on_close: move |_| {
                            selected_build.set(None);
                            log_open.set(false);
                        },
                        on_log: move |_| {
                            active_tab.set(DetailTab::Logs);
                            log_open.set(false);
                        },
                        on_build_action: move |action| {
                            if let Some(build) = selected_for_action.clone() {
                                pending_action.set(Some(PendingAction::Build {
                                    build_id: build.id,
                                    action,
                                }));
                            }
                        },
                        tab: active_tab,
                        on_tab_change: move |tab| active_tab.set(tab),
                        follow_logs: follow_logs,
                        pause_logs: pause_logs,
                        wrap_logs: wrap_logs,
                        log_query: log_query,
                    }
                        }
                    }
                }
            }

            if selected.is_some() && log_open() {
                // JSX: <div className="modal-backdrop" onClick={onClose}>
                div {
                    class: "modal-backdrop",
                    onclick: move |_| log_open.set(false),
                    // JSX: <div className="modal" style={{ width:"min(800px,98vw)" }}>
                    div {
                        class: "modal",
                        style: "width: min(800px, 98vw);",
                        onclick: |evt| evt.stop_propagation(),
                        // JSX: <div className="modal-head">
                        div {
                            class: "modal-head",
                            style: "display: flex; justify-content: space-between; align-items: center;",
                            div {
                                h2 {
                                    style: "margin: 0; font-size: 15px;",
                                    // JSX: Build log — <span className="mono">{b.pkg}</span>
                                    "Build log — "
                                    span { class: "mono", "{selected.clone().unwrap().pkg()}" }
                                }
                                // JSX: <p style={{ margin:"4px 0 0", fontSize:12, color:"var(--cf-text-muted)" }}>{b.drv.slice(0,50)}…</p>
                                p {
                                    style: "margin: 4px 0 0; font-size: 12px; color: var(--cf-text-muted);",
                                    "{truncate_with_ellipsis(&selected.clone().unwrap().drv(), 50)}"
                                }
                            }
                            // JSX: <button className="btn-icon focus-ring"><Icon name="x" size={16} /></button>
                            button {
                                class: "btn-icon focus-ring",
                                onclick: move |_| log_open.set(false),
                                svg {
                                    width: "16",
                                    height: "16",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    line { x1: "18", y1: "6", x2: "6", y2: "18" }
                                    line { x1: "6", y1: "6", x2: "18", y2: "18" }
                                }
                            }
                        }
                        // JSX: <pre ref={ref} className="sd-log-stream" style={{ minHeight:340, maxHeight:480 }}>
                        pre {
                            class: "sd-log-stream",
                            style: "min-height: 340px; max-height: 480px;",
                            if let Some(build) = selected.clone() {
                                if let Some(logs) = build.logs.clone() {
                                    // Parse and render structured log lines
                                    for line in logs.lines() {
                                        {render_log_line(line)}
                                    }
                                } else {
                                    div { class: "text-gray-500 italic", "No logs available" }
                                }
                                // If the job is actively building, show a live status line so
                                // the user knows the build is running and not hung. The queue
                                // auto-refreshes every 5 s so this will update with real log
                                // output as it arrives from the builder.
                                if matches!(build.status, BuildStatus::Building | BuildStatus::Stopping) {
                                    div {
                                        style: "margin-top: 6px; font-size: 11px; color: var(--cf-text-muted); display: flex; align-items: center; gap: 6px;",
                                        span { class: "ed-pulse", style: "position: static; margin: 0;" }
                                        span { "Build running on {build.worker_id} · log refreshes every 5 s" }
                                    }
                                }
                            }
                            // JSX: <div className="sd-log-caret">▍</div>
                            div { class: "sd-log-caret", "▍" }
                        }
                        // JSX: <div className="modal-foot">
                        div {
                            class: "modal-foot",
                            // JSX: <button className="btn btn-ghost focus-ring xs">
                            button {
                                class: "btn btn-ghost focus-ring xs",
                                span { style: "font-size: 12px;", "↓" }
                                " Download"
                            }
                            // JSX: <button className="btn btn-primary focus-ring">
                            button {
                                class: "btn btn-primary focus-ring",
                                onclick: move |_| log_open.set(false),
                                "Close"
                            }
                        }
                    }
                }
            }

            if let Some(action) = pending_action.read().clone() {
                ConfirmActionModal {
                    action: action.clone(),
                    on_cancel: move |_| pending_action.set(None),
                    on_confirm: move |_| {
                        if let Some(next_action) = pending_action.read().clone() {
                            match next_action.clone() {
                                PendingAction::Queue(queue_action) => {
                                    let builders_snapshot = workers.read().clone();
                                    let mut last_action_note = last_action_note;
                                    let mut action_error = action_error;
                                    let mut refresh_trigger = refresh_trigger;
                                    spawn(async move {
                                        let target_status = match queue_action {
                                            QueueAction::StartAll => BuilderStatus::Active,
                                            QueueAction::PauseAll | QueueAction::DrainAll => BuilderStatus::Inactive,
                                        };

                                        for worker in builders_snapshot {
                                            let request = crate::api::models::UpdateBuilderRequest {
                                                name: None,
                                                host: None,
                                                arch: None,
                                                status: Some(target_status.clone()),
                                                max_cpu_cores: None,
                                                max_memory_mb: None,
                                                max_concurrent_jobs: None,
                                                enabled: None,
                                            };

                                            let builder_id = match uuid::Uuid::parse_str(&worker.id) {
                                                Ok(id) => id,
                                                Err(_) => continue,
                                            };

                                            if let Err(e) = api::client::update_builder(&builder_id, &request).await {
                                                action_error.set(Some(format!("Failed applying {}: {}", queue_action.label(), e)));
                                                return;
                                            }
                                        }

                                        action_error.set(None);
                                        last_action_note.set(Some(format!("Applied {}", queue_action.label())));
                                        refresh_trigger.set(refresh_trigger() + 1);
                                    });
                                }
                                PendingAction::Worker { worker_id, action } => {
                                    let mut last_action_note = last_action_note;
                                    let mut action_error = action_error;
                                    let mut refresh_trigger = refresh_trigger;
                                    spawn(async move {
                                        let target_status = match action {
                                            WorkerAction::Start => BuilderStatus::Active,
                                            WorkerAction::Pause | WorkerAction::Drain => BuilderStatus::Inactive,
                                        };

                                        let builder_id = match uuid::Uuid::parse_str(&worker_id) {
                                            Ok(id) => id,
                                            Err(_) => {
                                                action_error.set(Some(format!("Invalid worker id: {}", worker_id)));
                                                return;
                                            }
                                        };

                                        let request = crate::api::models::UpdateBuilderRequest {
                                            name: None,
                                            host: None,
                                            arch: None,
                                            status: Some(target_status),
                                            max_cpu_cores: None,
                                            max_memory_mb: None,
                                            max_concurrent_jobs: None,
                                            enabled: None,
                                        };

                                        match api::client::update_builder(&builder_id, &request).await {
                                            Ok(_) => {
                                                action_error.set(None);
                                                last_action_note.set(Some(format!("Applied {} on {}", action.label(), worker_id)));
                                                refresh_trigger.set(refresh_trigger() + 1);
                                            }
                                            Err(e) => action_error.set(Some(format!("Failed applying {} on {}: {}", action.label(), worker_id, e))),
                                        }
                                    });
                                }
                                PendingAction::Build { build_id, action } => {
                                    let queue_snapshot = builds.read().clone();
                                    let history_snapshot = build_history.read().clone();
                                    let mut action_error = action_error;
                                    let mut last_action_note = last_action_note;
                                    let mut refresh_trigger = refresh_trigger;
                                    spawn(async move {
                                        // Check both active queue and completed history
                                        let selected = queue_snapshot.iter().find(|b| b.id == build_id)
                                            .or_else(|| history_snapshot.iter().find(|b| b.id == build_id));
                                        let Some(selected) = selected else {
                                            action_error.set(Some(format!("Build row #{} not found", build_id)));
                                            return;
                                        };

                                        match action {
                                            BuildAction::RunNext => {
                                                let Some(job_id) = selected.job_id else {
                                                    action_error.set(Some("Queue item has no job id; cannot prioritize".to_string()));
                                                    return;
                                                };

                                                match api::client::prioritize_build_job(&job_id).await {
                                                    Ok(_) => {
                                                        action_error.set(None);
                                                        last_action_note.set(Some(format!("Prioritized job {}", job_id)));
                                                        refresh_trigger.set(refresh_trigger() + 1);
                                                    }
                                                    Err(e) => {
                                                        action_error.set(Some(format!("Failed to prioritize: {}", e)));
                                                    }
                                                }
                                            }
                                            BuildAction::Restart => {
                                                // Prefer direct requeue if we have a job_id (terminal statuses).
                                                // Fall back to system sync for statuses without a job_id.
                                                if let Some(ref jid) = selected.job_id {
                                                    match api::client::requeue_build_job(jid).await {
                                                        Ok(_) => {
                                                            action_error.set(None);
                                                            last_action_note.set(Some("Build re-queued".to_string()));
                                                            refresh_trigger.set(refresh_trigger() + 1);
                                                        }
                                                        Err(e) => {
                                                            action_error.set(Some(format!("Failed to requeue: {}", e)));
                                                        }
                                                    }
                                                } else {
                                                    let Some(system_id) = selected.system_id else {
                                                        action_error.set(Some("Queue item has no system id; cannot trigger build".to_string()));
                                                        return;
                                                    };
                                                    match api::client::request_system_sync(&system_id).await {
                                                        Ok(_) => {
                                                            action_error.set(None);
                                                            last_action_note.set(Some(format!("Triggered build sync for system {}", system_id)));
                                                            refresh_trigger.set(refresh_trigger() + 1);
                                                        }
                                                        Err(e) => {
                                                            action_error.set(Some(format!("Failed to trigger build: {}", e)));
                                                        }
                                                    }
                                                }
                                            }
                                            BuildAction::Stop => {
                                                let Some(job_id) = selected.job_id else {
                                                    action_error.set(Some("Queue item has no job id; cannot stop".to_string()));
                                                    return;
                                                };

                                                match api::client::cancel_build_job(&job_id).await {
                                                    Ok(_) => {
                                                        action_error.set(None);
                                                        last_action_note.set(Some(format!("Cancelled job {}", job_id)));
                                                        refresh_trigger.set(refresh_trigger() + 1);
                                                    }
                                                    Err(e) => {
                                                        action_error.set(Some(format!("Failed to stop: {}", e)));
                                                    }
                                                }
                                            }
                                            BuildAction::ForceCancel => {
                                                let Some(job_id) = selected.job_id else {
                                                    action_error.set(Some("Queue item has no job id; cannot force cancel".to_string()));
                                                    return;
                                                };

                                                match api::client::force_cancel_build_job(&job_id).await {
                                                    Ok(_) => {
                                                        action_error.set(None);
                                                        last_action_note.set(Some(format!("Force-cancelled job {}", job_id)));
                                                        refresh_trigger.set(refresh_trigger() + 1);
                                                    }
                                                    Err(e) => {
                                                        action_error.set(Some(format!("Failed to force cancel: {}", e)));
                                                    }
                                                }
                                            }
                                            BuildAction::MoveUp | BuildAction::MoveDown => {
                                                // Queue reorder is handled directly at the table action site.
                                            }
                                          }
                                    });
                                }
                            }
                        }
                        pending_action.set(None);
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// LiveIndicator — pulsing dot + "updated Ns ago" counter
// JSX: function LiveIndicator({ label = "Live" })
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn LiveIndicator() -> Element {
    let mut secs = use_signal(|| 0_u32);

    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(1_000).await;
            secs.set((secs() + 1) % 60);
        }
    });

    rsx! {
        div {
            style: "display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--cf-text-muted);",
            span {
                style: "display: inline-flex; align-items: center; gap: 6px;",
                span { class: "ed-pulse", style: "position: static; margin: 0;" }
                span { style: "color: #34d399; font-weight: 600;", "Live" }
            }
            {
                let s = secs();
                if s == 0 {
                    rsx! { span { "· updated just now" } }
                } else {
                    rsx! { span { "· updated {s}s ago" } }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Full-width queue table (table view mode)
// ─────────────────────────────────────────────────────────────────────────────

use crate::components::builds::{build_status_badge_class, queue_sort_rank, short_commit};

/// Full-width table view of the active build queue.
///
/// Shown when the operator selects "Table" in the view-mode toggle.  Fills the
/// entire content area (less the sidebar) — no detail pane split.
#[component]
fn BuildQueueFullTable(
    builds: Vec<BuildItem>,
    selected_id: Signal<Option<i32>>,
    on_build_action: EventHandler<(i32, BuildAction)>,
) -> Element {
    let app_state = use_context::<Signal<AppState>>();
    let can_requeue = auth::is_operator_or_above(&app_state.read().auth);

    let mut sorted = builds;
    sorted.sort_by_key(|b| queue_sort_rank(b.status));

    rsx! {
        div {
            class: "w-full overflow-x-auto rounded-xl border border-slate-700",
            "data-testid": "build-queue-table",
            table {
                class: "w-full text-xs",
                thead {
                    class: "sticky top-0 bg-slate-900 border-b border-slate-700",
                    tr {
                        th { class: "text-left px-3 py-2 text-slate-400 font-medium", "Status" }
                        th { class: "text-left px-3 py-2 text-slate-400 font-medium", "System" }
                        th { class: "text-left px-3 py-2 text-slate-400 font-medium", "Flake" }
                        th { class: "text-left px-3 py-2 text-slate-400 font-medium", "Commit" }
                        th { class: "text-left px-3 py-2 text-slate-400 font-medium", "Builder" }
                        th { class: "text-left px-3 py-2 text-slate-400 font-medium", "Time" }
                        th { class: "text-right px-3 py-2 text-slate-400 font-medium", "Actions" }
                    }
                }
                tbody {
                    for build in sorted {
                        {
                            let is_selected = *selected_id.read() == Some(build.id);
                            let row_bg = if is_selected {
                                "bg-cyan-900/20"
                            } else {
                                "hover:bg-white/5"
                            };
                            rsx! {
                                tr {
                                    key: "{build.id}",
                                    class: "{row_bg} border-b border-slate-800 cursor-pointer transition-colors",
                                    "data-testid": "build-queue-row",
                                    onclick: move |_| selected_id.set(Some(build.id)),
                                    td { class: "px-3 py-2",
                                        span {
                                            class: "inline-flex px-2 py-0.5 text-[10px] uppercase rounded border {build_status_badge_class(build.status)}",
                                            "{build.status.label()}"
                                        }
                                    }
                                    td {
                                        class: "px-3 py-2 text-slate-200 font-medium truncate max-w-[160px]",
                                        title: "{extract_system_name(&build.hostname)}",
                                        "{extract_system_name(&build.hostname)}"
                                    }
                                    td { class: "px-3 py-2",
                                        span {
                                            class: "inline-flex px-2 py-0.5 text-[10px] rounded border cf-chip-blue",
                                            "{build.flake}"
                                        }
                                    }
                                    td {
                                        class: "px-3 py-2 font-mono text-slate-400",
                                        title: "{build.commit}",
                                        "{short_commit(&build.commit)}"
                                    }
                                    td { class: "px-3 py-2 text-slate-500", "{build.worker_id}" }
                                    td { class: "px-3 py-2 text-slate-400 whitespace-nowrap",
                                        if let Some(ref rt) = build.runtime {
                                            span { class: "text-teal-400", "{rt}" }
                                        } else {
                                            "{build.queued_for}"
                                        }
                                    }
                                    td { class: "px-3 py-2 text-right",
                                        div { class: "inline-flex items-center gap-1",
                                            if matches!(build.status, BuildStatus::Building) {
                                                button {
                                                    class: "text-[10px] text-red-400 hover:text-red-300 px-2 py-1 rounded hover:bg-red-500/10 transition-colors",
                                                    onclick: move |evt| {
                                                        evt.stop_propagation();
                                                        on_build_action.call((build.id, BuildAction::Stop));
                                                    },
                                                    "Stop"
                                                }
                                            }
                                            if matches!(build.status, BuildStatus::Stopping) {
                                                button {
                                                    class: "text-[10px] text-orange-400 hover:text-orange-300 px-2 py-1 rounded hover:bg-orange-500/10 transition-colors",
                                                    onclick: move |evt| {
                                                        evt.stop_propagation();
                                                        on_build_action.call((build.id, BuildAction::ForceCancel));
                                                    },
                                                    "Force Cancel"
                                                }
                                            }
                                            if can_requeue && matches!(build.status, BuildStatus::Failed | BuildStatus::Complete | BuildStatus::Cancelled) {
                                                button {
                                                    class: "text-[10px] px-2 py-1 rounded transition-colors cf-action-link",
                                                    onclick: move |evt| {
                                                        evt.stop_propagation();
                                                        on_build_action.call((build.id, BuildAction::Restart));
                                                    },
                                                    "Requeue"
                                                }
                                            }
                                            if build.status == BuildStatus::Queued {
                                                button {
                                                    class: "text-[10px] text-cyan-300 hover:text-cyan-200 px-2 py-1 rounded hover:bg-cyan-500/10 transition-colors",
                                                    onclick: move |evt| {
                                                        evt.stop_propagation();
                                                        on_build_action.call((build.id, BuildAction::RunNext));
                                                    },
                                                    "Run Next"
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
