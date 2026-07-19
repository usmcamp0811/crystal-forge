//! Builds control center view.

use chrono::{Duration, Utc};
use dioxus::prelude::*;

use crate::alerts::{acknowledge_with_cursor_and_ids_async, NAV_BADGES};

use crate::api::{
    self,
    client::{
        fetch_build_queue_paginated, fetch_recent_build_jobs, move_build_job_down,
        move_build_job_up,
    },
    models::{BuildQueueParams, BuildStatus as ApiBuildStatus, BuilderStatus},
};
use crate::components::builds::{
    extract_system_name, selected_build_data, BuildAction, BuildDetailPane, BuildItem,
    BuildQueuePane, BuildStatus, ConfirmActionModal, DetailTab, MetricsRow, PendingAction,
    QueueAction, QueueActionButton, WorkerAction, WorkerItem, WorkerStatus, WorkerStrip,
};
use crate::hooks::use_infinite_scroll;
use crate::state::app_state::AppState;
use crate::state::auth;
use crate::state::navigation_focus::{FocusTarget, NavigationFocus};
use crate::theme;

const PAGE_SIZE: i64 = 50;
const FETCH_LIMIT_MAX: i64 = 10_000; // must match backend LIMIT_MAX

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
    let mut navigation_focus = use_context::<Signal<Option<NavigationFocus>>>();
    let can_requeue = auth::is_operator_or_above(&app_state.read().auth);

    let mut workers = use_signal(Vec::<WorkerItem>::new);
    let mut active_refresh_trigger = use_signal(|| 0_u64);
    let mut history_refresh_trigger = use_signal(|| 0_u64);
    let mut active_view = use_signal(|| BuildsTab::ActiveQueue);
    let mut selected_build = use_signal(|| None::<uuid::Uuid>);
    let mut log_open = use_signal(|| false);

    // Auto-refresh: bump the relevant refresh signal every 5 s depending on
    // which tab is active.  When viewing the Active queue we only poll queue
    // + builder data; when viewing Completed history we only poll the history
    // endpoint.  This prevents unbounded polling cost growth — after scrolling
    // through thousands of completed builds we do *not* re-request every
    // loaded row (including logs) on every tick.
    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(5_000).await;
            if active_view() == BuildsTab::ActiveQueue {
                active_refresh_trigger.set(active_refresh_trigger() + 1);
            } else {
                history_refresh_trigger.set(history_refresh_trigger() + 1);
            }
        }
    });

    // --- Active queue state ---
    let mut fetch_limit = use_signal(|| PAGE_SIZE);
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

    // Reset to page 1 limit whenever filters change so accumulated rows are cleared.
    // NB: `refresh_trigger` is deliberately excluded — polling ticks every 5 s
    // and must not erase previously loaded rows (review finding #8).
    use_effect(move || {
        let _ = (
            filter_status(),
            filter_commit(),
            filter_flake(),
            filter_config(),
            filter_time_range(),
        );
        fetch_limit.set(PAGE_SIZE);
    });

    // Derived filter signals used to trigger resource re-fetch
    let queue_resource = use_resource(move || async move {
        let _ = active_refresh_trigger();
        let limit = fetch_limit();
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
            page: Some(1),
            limit: Some(limit),
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
        let _ = active_refresh_trigger();
        api::client::fetch_builders().await
    });
    let mut build_history_fetch_limit = use_signal(|| 100_i64);
    let mut build_history_total = use_signal(|| 0_i64);
    let recent_builds = use_resource(move || async move {
        let _ = history_refresh_trigger();
        fetch_recent_build_jobs(build_history_fetch_limit()).await
    });

    let mut build_history = use_signal(Vec::<BuildItem>::new);
    let mut build_history_ack_cursor = use_signal(|| None::<String>);
    let mut builds_ack_sent = use_signal(|| false);

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
        if let Some(Ok(page_resp)) = &*recent_builds.read() {
            build_history_ack_cursor.set(NAV_BADGES.read_unchecked().observed_at.clone());
            let mapped = page_resp
                .items
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
            build_history_total.set(page_resp.total);
        }
    });

    let mut active_tab = use_signal(|| DetailTab::Details);
    let mut completed_status_filter = use_signal(|| CompletedStatusFilter::All);
    let mut completed_sort_order = use_signal(|| CompletedSortOrder::NewestFirst);

    use_effect(move || {
        let Some(focus) = navigation_focus() else {
            return;
        };
        if focus.target != FocusTarget::Builds {
            return;
        }

        let status = focus.status.as_deref().unwrap_or_default();
        if matches!(status, "failed" | "complete" | "cancelled") {
            active_view.set(BuildsTab::Completed);
            completed_status_filter.set(CompletedStatusFilter::All);
        } else {
            active_view.set(BuildsTab::ActiveQueue);
            filter_status.set("queued,building,cancelling".to_string());
        }

        filter_commit.set(focus.commit_sha.unwrap_or_default());
        filter_flake.set(focus.flake_name.unwrap_or_default());
        selected_build.set(None);
    });

    // Search/filter state (JSX: query)
    let mut search_query = use_signal(String::new);
    // Attention flash for failed rows on first Completed tab open (JSX: flashHistRows)
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
    let completed_failed_count = build_history
        .read()
        .iter()
        .filter(|item| item.status == BuildStatus::Failed)
        .count();
    use_effect(move || {
        if active_view() == BuildsTab::Completed
            && !builds_ack_sent()
            && recent_builds.read().as_ref().is_some_and(|r| r.is_ok())
        {
            let Some(cursor) = build_history_ack_cursor.read().clone() else {
                return;
            };
            let alert_ids = build_history
                .read()
                .iter()
                .filter(|item| item.status == BuildStatus::Failed)
                .filter_map(|item| item.job_id.map(|id| id.to_string()))
                .collect::<Vec<_>>();
            spawn(async move {
                if acknowledge_with_cursor_and_ids_async(
                    "builds",
                    completed_failed_count as i64,
                    cursor,
                    None,
                    Some(alert_ids),
                )
                .await
                {
                    builds_ack_sent.set(true);
                }
            });
        }
    });
    // Server-computed "new failed builds since last acknowledgment" (persists
    // across page refresh/re-login — see alerts::NAV_BADGES). Drives both the
    // Completed tab's badge count and its attention-flash-tab pulse, replacing
    // a raw total that would otherwise reappear identically on every reload.
    let builds_failed_new = NAV_BADGES().builds_failed_new;
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
    let focus_visible_rows = visible_rows.clone();

    use_effect(move || {
        let Some(focus) = navigation_focus() else {
            return;
        };
        if focus.target != FocusTarget::Builds {
            return;
        }

        let matching_job_id = focus_visible_rows.iter().find_map(|item| {
            let commit_matches = match focus.commit_sha.as_deref() {
                Some(commit_sha) => item.commit == commit_sha,
                None => true,
            };
            let flake_matches = match focus.flake_name.as_deref() {
                Some(flake_name) => item.flake == flake_name,
                None => true,
            };
            if commit_matches && flake_matches {
                item.job_id
            } else {
                None
            }
        });

        if let Some(job_id) = matching_job_id {
            selected_build.set(Some(job_id));
            active_tab.set(DetailTab::Details);
            navigation_focus.set(None);
        }
    });

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

    // Infinite-scroll paging over the client-side filtered list.
    let tab_key = if active_view() == BuildsTab::ActiveQueue {
        "active"
    } else {
        "completed"
    };
    let paging = use_infinite_scroll(format!("{}|{}", tab_key, search_q), 20);
    let paged_list: Vec<BuildItem> = filtered_list.iter().take(paging.count()).cloned().collect();
    // has_more is true when:
    //   (a) there are more client-side rows in the filtered list, OR
    //   (b) the active/completed backing resource still has unloaded rows.
    // The server caps all list requests at FETCH_LIMIT_MAX, so totals
    // beyond that cap are unreachable — stop advertising "more" at the cap.
    let loaded_active_len = queue_data.len();
    let active_server_has_more = (loaded_active_len as i64) < queue_total().min(FETCH_LIMIT_MAX);
    let completed_server_has_more =
        (build_history.read().len() as i64) < build_history_total().min(FETCH_LIMIT_MAX);
    let has_more = paging.count() < filtered_list.len()
        || (active_view() == BuildsTab::ActiveQueue && active_server_has_more)
        || (active_view() == BuildsTab::Completed && completed_server_has_more);

    // Grow the server fetch limit when the client-side paging count has caught
    // up with all visible (search-filtered) loaded rows and the server still
    // has more to fetch. All signal reads are *inside* the closure so Dioxus
    // subscribes to them reactively (review finding #1).
    // The threshold is the number of loaded items that match the search filter,
    // ensuring that client-side searching does not block server-page fetches
    // (review finding #5).
    use_effect(move || {
        if active_view() == BuildsTab::ActiveQueue {
            let loaded_len = builds.read().len();
            let total = queue_total();
            let requested_len = fetch_limit();
            let paging_count = paging.count();

            // Determine how many loaded items pass the search filter.
            let sq = search_query();
            let q = sq.trim().to_lowercase();
            let threshold = if q.is_empty() {
                loaded_len
            } else {
                builds
                    .read()
                    .iter()
                    .filter(|b| {
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
                        .any(|v| v.contains(&q))
                    })
                    .count()
            };

            // NB: no `threshold > 0` guard — a zero-match search must still
            // advance through server pages until matches appear or the server
            // is exhausted (review finding #3).
            let reachable_total = total.min(FETCH_LIMIT_MAX);
            let server_has_more = (loaded_len as i64) < reachable_total;
            if (loaded_len as i64) >= requested_len
                && paging_count >= threshold
                && server_has_more
                && requested_len < FETCH_LIMIT_MAX
            {
                fetch_limit.set((requested_len + PAGE_SIZE).min(FETCH_LIMIT_MAX));
            }
            // Re-evaluate the sentinel after the list may have grown.
            paging.recheck(paging.count().min(threshold));
        }
    });

    use_effect(move || {
        if active_view() == BuildsTab::Completed {
            let loaded_len = build_history.read().len();
            let total = build_history_total();
            let requested_len = build_history_fetch_limit();
            let paging_count = paging.count();

            // Calculate threshold using both status filter AND search query,
            // matching the same predicates used to produce the rendered list
            // (review finding #2). The observer recheck needs the actual
            // displayed row count, not the backing list size.
            let q = search_query().trim().to_lowercase();
            let status_filter = completed_status_filter();
            let threshold = build_history
                .read()
                .iter()
                .filter(|b| {
                    // Apply status filter first.
                    matches!(
                        b.status,
                        BuildStatus::Complete | BuildStatus::Failed | BuildStatus::Cancelled
                    ) && match status_filter {
                        CompletedStatusFilter::All => true,
                        CompletedStatusFilter::Complete => b.status == BuildStatus::Complete,
                        CompletedStatusFilter::Failed => b.status == BuildStatus::Failed,
                        CompletedStatusFilter::Cancelled => b.status == BuildStatus::Cancelled,
                    }
                })
                .filter(|b| {
                    // Then apply search query.
                    if q.is_empty() {
                        true
                    } else {
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
                        .any(|v| v.contains(&q))
                    }
                })
                .count();

            let reachable_total = total.min(FETCH_LIMIT_MAX);
            let server_has_more = (loaded_len as i64) < reachable_total;
            if (loaded_len as i64) >= requested_len
                && paging_count >= threshold
                && server_has_more
                && requested_len < FETCH_LIMIT_MAX
            {
                build_history_fetch_limit.set((requested_len + 100).min(FETCH_LIMIT_MAX));
            }
            // Re-evaluate the sentinel after the list may have grown.
            paging.recheck(paging.count().min(threshold));
        }
    });

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
                        "Active"
                        span { class: "sd-tab-badge", "{queue_total()}" }
                    }
                    button {
                        // JSX: `sd-tab focus-ring${tab===t.k?" active":""}${flashTab && t.k==="history"?" attention-flash-tab":""}`
                        class: if active_view() == BuildsTab::Completed {
                            "sd-tab focus-ring active"
                        } else if builds_failed_new > 0 {
                            "sd-tab focus-ring attention-flash-tab"
                        } else {
                            "sd-tab focus-ring"
                        },
                        onclick: move |_| {
                            let prev = active_view();
                            active_view.set(BuildsTab::Completed);
                            selected_build.set(None);
                            log_open.set(false);
                            search_query.set(String::new());
                            // JSX: flash failed rows on first history open.
                            // Only flash if there's something genuinely new
                            // since the user's last acknowledgment of Builds.
                            if prev != BuildsTab::Completed && builds_failed_new > 0 {
                                flash_hist_rows.set(true);
                                let mut fh = flash_hist_rows;
                                spawn(async move {
                                    gloo_timers::future::TimeoutFuture::new(3_200).await;
                                    fh.set(false);
                                });
                            }
                            // Acknowledge the "builds" sidebar/tab badge when this tab is opened
                            // (persists server-side — TASK-385 follow-up).
                            builds_ack_sent.set(false);
                        },
                        "Completed"
                        span { class: "sd-tab-badge", "{build_history_total()}" }
                    }
                    // JSX: {selectableIds.length > 0 && <MultiSelectHint />}
                    // selectableIds = cancellable builds on Active, filteredList on Completed.
                    if if active_view() == BuildsTab::ActiveQueue {
                        queue_data.iter().any(|b| matches!(b.status, BuildStatus::Queued | BuildStatus::Building | BuildStatus::Stopping))
                    } else {
                        filtered_len > 0
                    } {
                        // JSX: <span className="ms-hint" title="⌘/Ctrl-click to toggle rows · Shift-click to select a range">
                        //        <kbd>⌘</kbd>/<kbd>⇧</kbd>-click to select
                        //      </span>
                        span {
                            class: "ms-hint",
                            title: "⌘/Ctrl-click to toggle rows · Shift-click to select a range",
                            kbd { "⌘" }
                            "/"
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
                        builds: paged_list.clone(),
                        selected_id: selected_build,
                        flash_failed: flash_hist_rows(),
                        can_requeue,
                        on_build_action: move |(job_id, action)| {
                            match action {
                                  BuildAction::MoveUp | BuildAction::MoveDown => {
                                    #[cfg(target_arch = "wasm32")]
                                    web_sys::console::log_1(&format!("Move action triggered: {:?} for job_id {}", action, job_id).into());

                                    let queue_snapshot = builds.read().clone();
                                    let mut action_error = action_error;
                                    let mut last_action_note = last_action_note;
                                    let mut active_refresh_trigger = active_refresh_trigger;
                                    spawn(async move {
                                        #[cfg(target_arch = "wasm32")]
                                        web_sys::console::log_1(&format!("Move action: searching for job_id {} in {} builds", job_id, queue_snapshot.len()).into());

                                        let selected = queue_snapshot.iter().find(|b| b.job_id == Some(job_id));
                                        let Some(selected) = selected else {
                                            let err = format!("Build job {} not found", job_id);
                                            #[cfg(target_arch = "wasm32")]
                                            web_sys::console::error_1(&err.clone().into());
                                            action_error.set(Some(err));
                                            return;
                                        };

                                        #[cfg(target_arch = "wasm32")]
                                        web_sys::console::log_1(&format!("Found build, job_id: {:?}, status: {:?}", selected.job_id, selected.status).into());

                                        #[cfg(target_arch = "wasm32")]
                                        web_sys::console::log_1(&format!("Calling API to move job {} {:?}", job_id, action).into());

                                        let result = if action == BuildAction::MoveUp {
                                            move_build_job_up(&job_id).await
                                        } else {
                                            move_build_job_down(&job_id).await
                                        };

                                        match result {
                                            Ok(_) => {
                                                #[cfg(target_arch = "wasm32")]
                                                web_sys::console::log_1(&format!("Move succeeded for job {}", job_id).into());

                                                action_error.set(None);
                                                last_action_note.set(Some(
                                                    if action == BuildAction::MoveUp {
                                                        format!("Moved job {} up", job_id)
                                                    } else {
                                                        format!("Moved job {} down", job_id)
                                                    },
                                                ));
                                                active_refresh_trigger.set(active_refresh_trigger() + 1);
                                            }
                                            Err(e) => {
                                                let err = format!("Failed to reorder: {}", e);
                                                #[cfg(target_arch = "wasm32")]
                                                web_sys::console::error_1(&err.clone().into());
                                                action_error.set(Some(err));
                                            }
                                        }
                                    });
                                }
                                _ => pending_action.set(Some(PendingAction::Build { job_id, action })),
                            }
                        },
                        on_log: move |job_id| {
                            // JSX parity: open the tray on its Log tab (not a separate modal).
                            selected_build.set(Some(job_id));
                            active_tab.set(DetailTab::Logs);
                            log_open.set(false);
                        },
                        on_bulk_rerun: {
                            move |build_ids: Vec<uuid::Uuid>| {
                                let mut action_error = action_error;
                                let mut last_action_note = last_action_note;
                                let mut active_refresh_trigger = active_refresh_trigger;
                                let filtered = filtered_list.clone();
                                spawn(async move {
                                    let count = build_ids.len();
                                    for id in &build_ids {
                                        if let Some(build) = filtered.iter().find(|b| b.job_id == Some(*id)) {
                                            if let Some(jid) = build.job_id {
                                                let _ = api::client::requeue_build_job(&jid).await;
                                            }
                                        }
                                    }
                                    action_error.set(None);
                                    let suffix = if count == 1 { "" } else { "s" };
                                    last_action_note.set(Some(format!("Re-queued {count} build{suffix}")));
                                    active_refresh_trigger.set(active_refresh_trigger() + 1);
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
                    // Infinite-scroll sentinel — grows the paged slice when scrolled into view.
                    if has_more {
                        div {
                            class: "infinite-sentinel",
                            "data-sentinel": paging.sentinel_id(),
                            onmounted: move |_| paging.check_and_register(),
                            "Loading more builds…"
                        }
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
                                if let Some(job_id) = build.job_id {
                                    pending_action.set(Some(PendingAction::Build { job_id, action }));
                                }
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
                                    let mut active_refresh_trigger = active_refresh_trigger;
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
                                        active_refresh_trigger.set(active_refresh_trigger() + 1);
                                    });
                                }
                                PendingAction::Worker { worker_id, action } => {
                                    let mut last_action_note = last_action_note;
                                    let mut action_error = action_error;
                                    let mut active_refresh_trigger = active_refresh_trigger;
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
                                                active_refresh_trigger.set(active_refresh_trigger() + 1);
                                            }
                                            Err(e) => action_error.set(Some(format!("Failed applying {} on {}: {}", action.label(), worker_id, e))),
                                        }
                                    });
                                }
                                PendingAction::Build { job_id, action } => {
                                    let queue_snapshot = builds.read().clone();
                                    let history_snapshot = build_history.read().clone();
                                    let mut action_error = action_error;
                                    let mut last_action_note = last_action_note;
                                    let mut active_refresh_trigger = active_refresh_trigger;
                                    spawn(async move {
                                        // Check both active queue and completed history
                                        let selected = queue_snapshot.iter().find(|b| b.job_id == Some(job_id))
                                            .or_else(|| history_snapshot.iter().find(|b| b.job_id == Some(job_id)));
                                        let Some(selected) = selected else {
                                            action_error.set(Some(format!("Build job {} not found", job_id)));
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
                                                        active_refresh_trigger.set(active_refresh_trigger() + 1);
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
                                                            active_refresh_trigger.set(active_refresh_trigger() + 1);
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
                                                            active_refresh_trigger.set(active_refresh_trigger() + 1);
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
                                                        active_refresh_trigger.set(active_refresh_trigger() + 1);
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
                                                        active_refresh_trigger.set(active_refresh_trigger() + 1);
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
    selected_id: Signal<Option<uuid::Uuid>>,
    on_build_action: EventHandler<(uuid::Uuid, BuildAction)>,
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
                            let is_selected = build.job_id.is_some_and(|id| *selected_id.read() == Some(id));
                            let row_key = build
                                .job_id
                                .map(|id| id.to_string())
                                .unwrap_or_else(|| format!("legacy-{}", build.id));
                            let row_bg = if is_selected {
                                "bg-cyan-900/20"
                            } else {
                                "hover:bg-white/5"
                            };
                            rsx! {
                                tr {
                                    key: "{row_key}",
                                    class: "{row_bg} border-b border-slate-800 cursor-pointer transition-colors",
                                    "data-testid": "build-queue-row",
                                    onclick: move |_| {
                                        if let Some(job_id) = build.job_id {
                                            selected_id.set(Some(job_id));
                                        }
                                    },
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
                                                        if let Some(job_id) = build.job_id {
                                                            on_build_action.call((job_id, BuildAction::Stop));
                                                        }
                                                    },
                                                    "Stop"
                                                }
                                            }
                                            if matches!(build.status, BuildStatus::Stopping) {
                                                button {
                                                    class: "text-[10px] text-orange-400 hover:text-orange-300 px-2 py-1 rounded hover:bg-orange-500/10 transition-colors",
                                                    onclick: move |evt| {
                                                        evt.stop_propagation();
                                                        if let Some(job_id) = build.job_id {
                                                            on_build_action.call((job_id, BuildAction::ForceCancel));
                                                        }
                                                    },
                                                    "Force Cancel"
                                                }
                                            }
                                            if can_requeue && matches!(build.status, BuildStatus::Failed | BuildStatus::Complete | BuildStatus::Cancelled) {
                                                button {
                                                    class: "text-[10px] px-2 py-1 rounded transition-colors cf-action-link",
                                                    onclick: move |evt| {
                                                        evt.stop_propagation();
                                                        if let Some(job_id) = build.job_id {
                                                            on_build_action.call((job_id, BuildAction::Restart));
                                                        }
                                                    },
                                                    "Requeue"
                                                }
                                            }
                                            if build.status == BuildStatus::Queued {
                                                button {
                                                    class: "text-[10px] text-cyan-300 hover:text-cyan-200 px-2 py-1 rounded hover:bg-cyan-500/10 transition-colors",
                                                    onclick: move |evt| {
                                                        evt.stop_propagation();
                                                        if let Some(job_id) = build.job_id {
                                                            on_build_action.call((job_id, BuildAction::RunNext));
                                                        }
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
