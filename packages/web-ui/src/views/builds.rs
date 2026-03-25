//! Builds control center view.

use chrono::{Duration, Utc};
use dioxus::prelude::*;

use crate::api::{
    self,
    client::fetch_recent_build_jobs,
    models::{BuildStatus as ApiBuildStatus, BuilderStatus},
};
use crate::components::builds::{
    extract_system_name, selected_build_data, BuildAction, BuildDetailPane, BuildItem,
    BuildQueuePane, BuildStatus, ConfirmActionModal, DetailTab, MetricsRow, PendingAction,
    QueueAction, QueueActionButton, WorkerAction, WorkerItem, WorkerStatus, WorkerStrip,
};
use crate::components::layout::Card;
use crate::theme;

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

fn format_duration(item: &BuildItem) -> String {
    item.duration_secs
        .map(|secs| format!("{}s", secs.max(0)))
        .or_else(|| item.runtime.clone())
        .unwrap_or_else(|| "-".to_string())
}

fn format_environment(item: &BuildItem) -> String {
    item.environment.clone().unwrap_or_else(|| "-".to_string())
}

/// Builds control center page.
#[component]
pub fn BuildsView() -> Element {
    let mut workers = use_signal(Vec::<WorkerItem>::new);
    let mut builds = use_signal(Vec::<BuildItem>::new);
    let refresh_trigger = use_signal(|| 0_u64);
    let builders = use_resource(move || async move {
        let _ = refresh_trigger();
        api::client::fetch_builders().await
    });
    let recent_builds = use_resource(move || async move {
        let _ = refresh_trigger();
        fetch_recent_build_jobs().await
    });
    let dashboard = use_resource(move || async move {
        let _ = refresh_trigger();
        api::client::fetch_dashboard().await
    });

    let mut build_history = use_signal(Vec::<BuildItem>::new);

    use_effect(move || {
        if let Some(Ok(builder_list)) = &*builders.read() {
            let mapped = builder_list
                .iter()
                .map(|builder| WorkerItem {
                    id: builder.id.to_string(),
                    name: builder.name.clone(),
                    active_slots: builder.active_jobs.max(0) as usize,
                    total_slots: builder.max_concurrent_jobs.max(1) as usize,
                    queue_depth: builder.queued_jobs.max(0) as usize,
                    status: match builder.status {
                        BuilderStatus::Active => WorkerStatus::Running,
                        BuilderStatus::Inactive => WorkerStatus::Paused,
                        BuilderStatus::Offline => WorkerStatus::Draining,
                    },
                })
                .collect::<Vec<_>>();
            workers.set(mapped);
        }
    });

    use_effect(move || {
        if let Some(Ok(summary)) = &*dashboard.read() {
            if let Some(queue) = &summary.build_queue {
                let mapped = queue
                    .items
                    .iter()
                    .enumerate()
                    .map(|(idx, item)| {
                        let queued_for = if item.status == ApiBuildStatus::Building {
                            format!("running {}s", item.elapsed_secs.unwrap_or(0))
                        } else {
                            let ago = (Utc::now() - item.queued_at).num_seconds().max(0);
                            format!("queued {}s ago", ago)
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
                            worker_id: item
                                .builder_name
                                .clone()
                                .unwrap_or_else(|| "unassigned".to_string()),
                            queued_for,
                            runtime: item.elapsed_secs.map(|secs| format!("{}s", secs)),
                            duration_secs: item.elapsed_secs,
                            completed_at: None,
                            started_by: "scheduler".to_string(),
                            status: match item.status {
                                ApiBuildStatus::Queued => BuildStatus::Queued,
                                ApiBuildStatus::Building => BuildStatus::Building,
                                ApiBuildStatus::Failed => BuildStatus::Failed,
                                ApiBuildStatus::Complete => BuildStatus::Complete,
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
                        }
                    })
                    .collect::<Vec<_>>();
                builds.set(mapped);
            }
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
                        worker_id: item
                            .builder_name
                            .clone()
                            .unwrap_or_else(|| "unassigned".to_string()),
                        queued_for: finished_for,
                        runtime: item.elapsed_secs.map(|secs| format!("{}s", secs)),
                        duration_secs: item.elapsed_secs,
                        completed_at,
                        started_by: "scheduler".to_string(),
                        status: match item.status {
                            ApiBuildStatus::Failed => BuildStatus::Failed,
                            ApiBuildStatus::Complete => BuildStatus::Complete,
                            ApiBuildStatus::Building => BuildStatus::Building,
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
                    }
                })
                .collect::<Vec<_>>();
            build_history.set(mapped);
        }
    });

    let mut selected_build = use_signal(|| Some(1_i32));
    let mut active_view = use_signal(|| BuildsTab::ActiveQueue);
    let mut active_tab = use_signal(|| DetailTab::Logs);
    let mut completed_status_filter = use_signal(|| CompletedStatusFilter::All);
    let mut completed_sort_order = use_signal(|| CompletedSortOrder::NewestFirst);

    let follow_logs = use_signal(|| true);
    let pause_logs = use_signal(|| false);
    let wrap_logs = use_signal(|| false);
    let log_query = use_signal(String::new);

    let mut pending_action = use_signal(|| None::<PendingAction>);
    let mut last_action_note = use_signal(|| None::<String>);
    let mut action_error = use_signal(|| None::<String>);

    let queue_data = builds.read().clone();
    let worker_data = workers.read().clone();
    let selected = selected_build_data(selected_build.read().to_owned(), &queue_data);

    let mut completed_rows = build_history.read().clone();
    completed_rows.retain(|item| {
        matches!(item.status, BuildStatus::Complete | BuildStatus::Failed)
            && match completed_status_filter() {
                CompletedStatusFilter::All => true,
                CompletedStatusFilter::Complete => item.status == BuildStatus::Complete,
                CompletedStatusFilter::Failed => item.status == BuildStatus::Failed,
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

    rsx! {
        div {
            class: "space-y-6",

            header {
                class: "flex flex-col gap-4 xl:flex-row xl:items-center xl:justify-between",
                div {
                    h1 { class: "{theme::typography::PAGE_TITLE}", "Builds" }
                    p {
                        class: "text-sm {theme::text::SECONDARY}",
                        "Control active build workers, inspect queue state, and monitor live build logs."
                    }
                }
                div {
                    class: "flex flex-wrap items-center gap-2",
                    span {
                        class: "inline-flex items-center px-2 py-1 text-xs rounded border text-emerald-100 cf-worker-status-running",
                        span { class: "w-2 h-2 rounded-full bg-emerald-300 mr-2 animate-pulse", }
                        "Live"
                    }
                    QueueActionButton {
                        label: "Start All",
                        onclick: move |_| pending_action.set(Some(PendingAction::Queue(QueueAction::StartAll))),
                    }
                    QueueActionButton {
                        label: "Pause All",
                        onclick: move |_| pending_action.set(Some(PendingAction::Queue(QueueAction::PauseAll))),
                    }
                    QueueActionButton {
                        label: "Drain All",
                        onclick: move |_| pending_action.set(Some(PendingAction::Queue(QueueAction::DrainAll))),
                    }
                }
            }

            MetricsRow {
                workers: worker_data.clone(),
                builds: queue_data.clone(),
            }

            WorkerStrip {
                workers: worker_data.clone(),
                on_action: move |(worker_id, action)| {
                    pending_action.set(Some(PendingAction::Worker { worker_id, action }))
                },
            }

            div {
                class: "flex border-b border-slate-700",
                button {
                    class: if active_view() == BuildsTab::ActiveQueue {
                        "px-4 py-2 border-b-2 border-blue-500 text-blue-400 font-medium"
                    } else {
                        "px-4 py-2 border-b-2 border-transparent text-slate-400 hover:text-white transition-colors"
                    },
                    onclick: move |_| active_view.set(BuildsTab::ActiveQueue),
                    "Active Queue"
                }
                button {
                    class: if active_view() == BuildsTab::Completed {
                        "px-4 py-2 border-b-2 border-blue-500 text-blue-400 font-medium"
                    } else {
                        "px-4 py-2 border-b-2 border-transparent text-slate-400 hover:text-white transition-colors"
                    },
                    onclick: move |_| active_view.set(BuildsTab::Completed),
                    "Completed Builds"
                }
            }

            if let Some(note) = last_action_note.read().clone() {
                p {
                    class: "text-xs px-3 py-2 rounded-lg border text-blue-100 cf-chip-info",
                    "{note}"
                }
            }

            if let Some(err) = action_error.read().clone() {
                p {
                    class: "text-xs px-3 py-2 rounded-lg border text-red-100",
                    style: "background-color: #4A252D; border-color: #7A3D48;",
                    "{err}"
                }
            }

            if active_view() == BuildsTab::ActiveQueue {
                div {
                    class: "cf-builds-split",
                    div {
                        BuildQueuePane {
                            builds: queue_data.clone(),
                            selected_id: selected_build,
                            on_build_action: move |(build_id, action)| {
                                pending_action.set(Some(PendingAction::Build { build_id, action }))
                            },
                        }
                    }

                    div {
                        BuildDetailPane {
                            selected: selected,
                            tab: active_tab,
                            on_tab_change: move |tab| active_tab.set(tab),
                            follow_logs: follow_logs,
                            pause_logs: pause_logs,
                            wrap_logs: wrap_logs,
                            log_query: log_query,
                        }
                    }
                }
            } else {
                Card {
                    title: Some("Completed Builds".to_string()),
                    children: rsx! {
                        div {
                            class: "flex flex-wrap items-center gap-3 pb-3",
                            label { class: "text-xs {theme::text::SECONDARY}", "Status" }
                            select {
                                class: "px-2 py-1 rounded border border-slate-600 bg-slate-900 text-xs text-slate-200",
                                value: match completed_status_filter() {
                                    CompletedStatusFilter::All => "all",
                                    CompletedStatusFilter::Complete => "complete",
                                    CompletedStatusFilter::Failed => "failed",
                                },
                                onchange: move |event| {
                                    let value = event.value();
                                    let next = match value.as_str() {
                                        "complete" => CompletedStatusFilter::Complete,
                                        "failed" => CompletedStatusFilter::Failed,
                                        _ => CompletedStatusFilter::All,
                                    };
                                    completed_status_filter.set(next);
                                },
                                option { value: "all", "All" }
                                option { value: "complete", "Complete" }
                                option { value: "failed", "Failed" }
                            }

                            label { class: "text-xs {theme::text::SECONDARY}", "Sort" }
                            select {
                                class: "px-2 py-1 rounded border border-slate-600 bg-slate-900 text-xs text-slate-200",
                                value: match completed_sort_order() {
                                    CompletedSortOrder::NewestFirst => "newest",
                                    CompletedSortOrder::OldestFirst => "oldest",
                                },
                                onchange: move |event| {
                                    let next = if event.value() == "oldest" {
                                        CompletedSortOrder::OldestFirst
                                    } else {
                                        CompletedSortOrder::NewestFirst
                                    };
                                    completed_sort_order.set(next);
                                },
                                option { value: "newest", "Newest completion first" }
                                option { value: "oldest", "Oldest completion first" }
                            }
                        }

                        if completed_rows.is_empty() {
                            p { class: "text-sm {theme::text::SECONDARY}", "No completed builds yet." }
                        } else {
                            div {
                                class: "overflow-x-auto",
                                table {
                                    class: "w-full text-xs",
                                    thead {
                                        tr { class: "text-left border-b border-slate-700 text-slate-300",
                                            th { class: "py-2 pr-3", "System" }
                                            th { class: "py-2 pr-3", "Environment" }
                                            th { class: "py-2 pr-3", "Status" }
                                            th { class: "py-2 pr-3", "Completion Time" }
                                            th { class: "py-2 pr-3", "Duration" }
                                            th { class: "py-2 pr-3", "Commit" }
                                        }
                                    }
                                    tbody {
                                        for item in completed_rows.iter() {
                                            {
                                                let status_class = match item.status {
                                                    BuildStatus::Complete => "px-2 py-1 text-[10px] rounded border cf-build-status-complete",
                                                    BuildStatus::Failed => "px-2 py-1 text-[10px] rounded border cf-build-status-failed",
                                                    _ => "px-2 py-1 text-[10px] rounded border cf-chip-slate",
                                                };
                                                rsx! {
                                                    tr { key: "completed-{item.id}", class: "border-b border-slate-800/70",
                                                        td { class: "py-2 pr-3 font-mono text-slate-200", "{extract_system_name(&item.hostname)}" }
                                                        td { class: "py-2 pr-3 text-slate-300", "{format_environment(item)}" }
                                                        td { class: "py-2 pr-3",
                                                            span { class: "{status_class}", "{item.status_label()}" }
                                                        }
                                                        td { class: "py-2 pr-3 text-slate-300", "{format_completed_at(item)}" }
                                                        td { class: "py-2 pr-3 text-slate-300", "{format_duration(item)}" }
                                                        td { class: "py-2 pr-3 text-slate-400 font-mono", "{item.commit.chars().take(8).collect::<String>()}" }
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
                                                status: Some(target_status.clone()),
                                                max_cpu_cores: None,
                                                max_memory_mb: None,
                                                max_concurrent_jobs: None,
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
                                            status: Some(target_status),
                                            max_cpu_cores: None,
                                            max_memory_mb: None,
                                            max_concurrent_jobs: None,
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
                                    let mut action_error = action_error;
                                    let mut last_action_note = last_action_note;
                                    let mut refresh_trigger = refresh_trigger;
                                    spawn(async move {
                                        let selected = queue_snapshot.iter().find(|b| b.id == build_id);
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
                                            BuildAction::Stop => {
                                                action_error.set(Some("Stop build is not implemented by API yet".to_string()));
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
