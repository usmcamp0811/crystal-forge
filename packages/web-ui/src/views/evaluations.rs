//! Evaluations queue view.

use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use gloo_timers::future::TimeoutFuture;

use crate::api::{
    client::{fetch_eval_queue, reorder_eval_queue},
    models::{EvalQueueItem, EvalQueueSummary},
};
use crate::components::layout::Card;
use crate::hooks::websocket::{ConnectionState, SystemEvalStatus, use_websocket_eval_stream};
use crate::theme;

#[component]
pub fn EvaluationsView() -> Element {
    rsx! { EvaluationsPage { initial_commit_id: None } }
}

#[component]
pub fn EvaluationsCommitView(commit_id: i32) -> Element {
    rsx! { EvaluationsPage { initial_commit_id: Some(commit_id) } }
}

#[component]
fn EvaluationsPage(initial_commit_id: Option<i32>) -> Element {
    let mut selected_commit_id = use_signal(|| initial_commit_id);
    let mut queue_items = use_signal(Vec::<EvalQueueItem>::new);
    let mut refresh = use_signal(|| 0_u64);
    let mut reorder_error = use_signal(|| None::<String>);
    let mut drag_commit_id = use_signal(|| None::<i32>);

    let queue_resource = use_resource(move || async move {
        let _ = refresh();
        fetch_eval_queue().await
    });

    {
        let mut refresh = refresh.clone();
        use_future(move || async move {
            loop {
                #[cfg(target_arch = "wasm32")]
                {
                    TimeoutFuture::new(3000).await;
                    refresh.set(refresh() + 1);
                }

                #[cfg(not(target_arch = "wasm32"))]
                {
                    let _ = refresh;
                    break;
                }
            }
        });
    }

    use_effect(move || {
        if let Some(Ok(summary)) = &*queue_resource.read() {
            queue_items.set(summary.items.clone());
            let in_progress = summary
                .items
                .iter()
                .find(|item| is_in_progress_eval_status(&item.evaluation_status))
                .map(|item| item.commit_id);
            let selected_exists = selected_commit_id
                .read()
                .as_ref()
                .map(|selected| summary.items.iter().any(|item| item.commit_id == *selected))
                .unwrap_or(false);

            if !selected_exists {
                let first_active = summary
                    .items
                    .iter()
                    .find(|item| is_active_eval_status(&item.evaluation_status))
                    .or_else(|| summary.items.first())
                    .map(|item| item.commit_id);
                selected_commit_id.set(in_progress.or(first_active));
            }
        }
    });

    let active_items = queue_items
        .read()
        .iter()
        .filter(|item| is_active_eval_status(&item.evaluation_status))
        .cloned()
        .collect::<Vec<_>>();
    let completed_items = queue_items
        .read()
        .iter()
        .filter(|item| !is_active_eval_status(&item.evaluation_status))
        .cloned()
        .collect::<Vec<_>>();

    let selected_item = queue_items
        .read()
        .iter()
        .find(|item| Some(item.commit_id) == *selected_commit_id.read())
        .cloned();
    let live_item = queue_items
        .read()
        .iter()
        .find(|item| is_in_progress_eval_status(&item.evaluation_status))
        .cloned();

    let selected_commit_str = (*selected_commit_id.read())
        .map(|id| id.to_string())
        .unwrap_or_else(|| "0".to_string());
    let (eval_logs, system_status, connection_state, _reconnect) =
        use_websocket_eval_stream(&selected_commit_str);

    let summary_snapshot = queue_resource
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned();

    let in_progress_count = queue_items
        .read()
        .iter()
        .filter(|item| is_in_progress_eval_status(&item.evaluation_status))
        .count();
    let pending_count = queue_items
        .read()
        .iter()
        .filter(|item| is_pending_eval_status(&item.evaluation_status))
        .count();

    if initial_commit_id.is_none() {
        if let Some(live) = live_item.as_ref() {
            if Some(live.commit_id) != *selected_commit_id.read() {
                selected_commit_id.set(Some(live.commit_id));
            }
        }
    }

    // State for log modal
    let mut log_modal_open = use_signal(|| false);

    rsx! {
        div {
            class: "space-y-6",

            header {
                class: "flex flex-col gap-4 xl:flex-row xl:items-center xl:justify-between",
                div {
                    h1 { class: "{theme::typography::PAGE_TITLE}", "Evaluations" }
                    p {
                        class: "text-sm {theme::text::SECONDARY}",
                        "Track commit evaluation order, reorder priority, and monitor policy outcomes in real-time."
                    }
                }
                div {
                    class: "flex flex-wrap items-center gap-2",
                    button {
                        class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN} {theme::interactive::FOCUS_RING}",
                        onclick: move |_| refresh.set(refresh() + 1),
                        "Refresh"
                    }
                }
            }

            if let Some(error) = reorder_error.read().clone() {
                p {
                    class: "text-xs px-3 py-2 rounded-lg border text-red-100",
                    style: "background-color: #4A252D; border-color: #7A3D48;",
                    "{error}"
                }
            }

            if let Some(Err(error)) = &*queue_resource.read() {
                p {
                    class: "text-xs px-3 py-2 rounded-lg border text-red-100",
                    style: "background-color: #4A252D; border-color: #7A3D48;",
                    "Failed to refresh evaluation queue: {error}"
                }
            }

            if in_progress_count == 0 && pending_count > 0 {
                p {
                    class: "text-xs px-3 py-2 rounded-lg border text-amber-100",
                    style: "background-color: #493E26; border-color: #8C7041;",
                    "No evaluations are currently running. {pending_count} commit(s) are pending in queue. If this persists, the eval worker loop may be stalled."
                }
            }

            // Full-width Evaluation Logs at the top
            Card {
                title: Some("Evaluation Logs".to_string()),
                children: rsx! {
                    div { class: "space-y-2",
                        div { class: "flex items-center justify-between",
                            p {
                                class: "text-xs",
                                span {
                                    class: "inline-flex items-center px-2 py-0.5 rounded border {connection_badge_class(&connection_state.read())}",
                                    "{connection_badge_text(&connection_state.read())}"
                                }
                            }
                            button {
                                class: "px-3 py-1.5 text-xs rounded text-white {theme::interactive::PRIMARY_BTN} {theme::interactive::FOCUS_RING}",
                                onclick: move |_| log_modal_open.set(true),
                                "⛶ Maximize"
                            }
                        }
                        div {
                            class: "h-[30vh] overflow-auto rounded border border-gray-700 bg-gray-950/60 p-3 space-y-1",
                            for line in eval_logs.read().iter().rev().take(200).rev() {
                                p { class: "text-xs font-mono text-gray-300", "{line}" }
                            }
                            if eval_logs.read().is_empty() {
                                p { class: "text-sm text-gray-500", "No log messages yet for selected commit." }
                            }
                        }
                    }
                }
            }

            div {
                class: "cf-builds-split",

                div {
                    class: "space-y-6",

                    Card {
                        title: Some("Active Eval Queue".to_string()),
                        children: rsx! {
                            div { class: "space-y-2",
                                for (index , item) in active_items.iter().enumerate() {
                                    {
                                        let item = item.clone();
                                        let commit_id = item.commit_id;
                                        let is_selected = Some(item.commit_id) == *selected_commit_id.read();
                                        let can_move_up = index > 0;
                                        let can_move_down = index + 1 < active_items.len();
                                        let progress_text = progress_label(
                                            &item,
                                            if is_selected {
                                                Some(system_status.read().clone())
                                            } else {
                                                None
                                            },
                                        );

                                        let mut drag_start_signal = drag_commit_id.clone();
                                        let mut drag_drop_signal = drag_commit_id.clone();
                                        let mut selected_commit_signal = selected_commit_id.clone();

                                        let mut queue_items_drop = queue_items.clone();
                                        let reorder_error_drop = reorder_error.clone();

                                        let mut queue_items_up = queue_items.clone();
                                        let reorder_error_up = reorder_error.clone();

                                        let mut queue_items_down = queue_items.clone();
                                        let reorder_error_down = reorder_error.clone();

                                        rsx! {
                                            button {
                                                key: "active-{commit_id}",
                                                draggable: "true",
                                                class: "{active_row_class(is_selected)}",
                                                ondragstart: move |_| drag_start_signal.set(Some(commit_id)),
                                                ondragover: move |evt| evt.prevent_default(),
                                                ondrop: move |evt| {
                                                    evt.prevent_default();
                                                    if let Some(source_id) = *drag_drop_signal.read() {
                                                        if source_id != commit_id {
                                                            let mut reordered = queue_items_drop
                                                                .read()
                                                                .iter()
                                                                .filter(|entry| is_active_eval_status(&entry.evaluation_status))
                                                                .cloned()
                                                                .collect::<Vec<_>>();
                                                            reorder_commit_list(&mut reordered, source_id, commit_id);
                                                            queue_items_drop.with_mut(|all| apply_active_reorder(all, &reordered));
                                                            let ordered_ids = reordered.iter().map(|entry| entry.commit_id).collect::<Vec<_>>();
                                                            spawn_reorder_request(ordered_ids, reorder_error_drop);
                                                        }
                                                    }
                                                    drag_drop_signal.set(None);
                                                },
                                                onclick: move |_| selected_commit_signal.set(Some(commit_id)),

                                                div {
                                                    class: "flex items-start justify-between gap-4",
                                                    div {
                                                        p { class: "text-sm font-semibold text-white", "{item.flake_name}" }
                                                        p { class: "text-xs text-gray-400 mt-1 font-mono", "{item.commit_hash.chars().take(8).collect::<String>()} · {item.branch}" }
                                                    }
                                                    div {
                                                        class: "text-right",
                                                        p {
                                                            class: "text-xs text-gray-300",
                                                            title: "Policy passed / total configurations",
                                                            "{progress_text}"
                                                        }
                                                        span {
                                                            class: "inline-flex mt-1 px-2 py-0.5 text-[10px] rounded border {eval_status_class(&item.evaluation_status)}",
                                                            title: "{eval_status_help(&item.evaluation_status)}",
                                                            "{item.evaluation_status}"
                                                        }
                                                    }
                                                }

                                                div {
                                                    class: "mt-3 flex items-center justify-between gap-2",
                                                    p {
                                                        class: "text-xs text-gray-400",
                                                        "{item.system_count} systems · {item.policy_failed_count} policy failed · {item.eval_failed_count} eval failed"
                                                    }
                                                    div { class: "inline-flex items-center gap-1",
                                                        button {
                                                            class: "px-2 py-1 text-xs rounded text-white {theme::interactive::PRIMARY_BTN} {theme::interactive::FOCUS_RING} disabled:opacity-40 disabled:cursor-not-allowed",
                                                            disabled: !can_move_up,
                                                            onclick: move |evt| {
                                                                evt.stop_propagation();
                                                                if !can_move_up {
                                                                    return;
                                                                }
                                                                let mut reordered = queue_items_up
                                                                    .read()
                                                                    .iter()
                                                                    .filter(|entry| is_active_eval_status(&entry.evaluation_status))
                                                                    .cloned()
                                                                    .collect::<Vec<_>>();
                                                                if let Some(current_index) = reordered
                                                                    .iter()
                                                                    .position(|entry| entry.commit_id == commit_id)
                                                                {
                                                                    if current_index > 0 {
                                                                        reordered.swap(current_index - 1, current_index);
                                                                        queue_items_up.with_mut(|all| apply_active_reorder(all, &reordered));
                                                                        let ordered_ids = reordered
                                                                            .iter()
                                                                            .map(|entry| entry.commit_id)
                                                                            .collect::<Vec<_>>();
                                                                        spawn_reorder_request(ordered_ids, reorder_error_up);
                                                                    }
                                                                }
                                                            },
                                                            "Up"
                                                        }
                                                        button {
                                                            class: "px-2 py-1 text-xs rounded text-white {theme::interactive::SUCCESS_BTN} {theme::interactive::FOCUS_RING} disabled:opacity-40 disabled:cursor-not-allowed",
                                                            disabled: !can_move_down,
                                                            onclick: move |evt| {
                                                                evt.stop_propagation();
                                                                if !can_move_down {
                                                                    return;
                                                                }
                                                                let mut reordered = queue_items_down
                                                                    .read()
                                                                    .iter()
                                                                    .filter(|entry| is_active_eval_status(&entry.evaluation_status))
                                                                    .cloned()
                                                                    .collect::<Vec<_>>();
                                                                if let Some(current_index) = reordered
                                                                    .iter()
                                                                    .position(|entry| entry.commit_id == commit_id)
                                                                {
                                                                    if current_index + 1 < reordered.len() {
                                                                        reordered.swap(current_index, current_index + 1);
                                                                        queue_items_down.with_mut(|all| apply_active_reorder(all, &reordered));
                                                                        let ordered_ids = reordered
                                                                            .iter()
                                                                            .map(|entry| entry.commit_id)
                                                                            .collect::<Vec<_>>();
                                                                        spawn_reorder_request(ordered_ids, reorder_error_down);
                                                                    }
                                                                }
                                                            },
                                                            "Down"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if active_items.is_empty() {
                                    p { class: "text-sm text-gray-400", "No active evaluations in queue." }
                                }
                            }
                        }
                    }
                }

                div {
                    class: "space-y-6",
                    if let Some(summary) = summary_snapshot {
                        MetricsStrip {
                            summary,
                        }
                    }

                    if let Some(live) = live_item.clone() {
                        div {
                            class: "text-xs px-3 py-2 rounded-lg border cf-chip-info text-blue-100 flex items-center justify-between gap-2",
                            div {
                                "Live eval: {live.flake_name} · {live.commit_hash.chars().take(8).collect::<String>()}"
                            }
                            if Some(live.commit_id) != *selected_commit_id.read() {
                                button {
                                    class: "px-2 py-1 rounded text-xs text-white {theme::interactive::PRIMARY_BTN} {theme::interactive::FOCUS_RING}",
                                    onclick: move |_| selected_commit_id.set(Some(live.commit_id)),
                                    "View live"
                                }
                            }
                        }
                    }

                    Card {
                        title: Some("Selected Commit".to_string()),
                        children: rsx! {
                            if let Some(item) = selected_item.clone() {
                                div { class: "space-y-3",
                                    p { class: "text-sm text-white font-semibold", "{item.flake_name}" }
                                    p { class: "text-xs text-gray-400 font-mono", "{item.commit_hash}" }
                                    p {
                                        class: "text-xs text-gray-300",
                                        title: "Policy passed / total configurations",
                                        "{progress_label(&item, Some(system_status.read().clone()))}"
                                    }
                                    div { class: "flex flex-wrap gap-2",
                                        for system in item.systems {
                                            {
                                                let status = system_status.read().get(&system).cloned();
                                                rsx! {
                                                    span {
                                                        key: "{system}",
                                                        class: "px-2 py-1 rounded border text-xs font-mono {system_chip_class(status.as_ref())}",
                                                        title: "{system_chip_help(status.as_ref())}",
                                                        "{system}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                p { class: "text-sm text-gray-400", "Select a queue item to inspect systems and logs." }
                            }
                        }
                    }

                    Card {
                        title: Some("Completed Evaluations".to_string()),
                        children: rsx! {
                            div { class: "space-y-2 max-h-[24vh] overflow-auto pr-1",
                                for item in completed_items.iter().cloned() {
                                    button {
                                        key: "done-{item.commit_id}",
                                        class: "w-full rounded-lg border border-gray-700 bg-gray-900/40 px-3 py-2 text-left",
                                        onclick: move |_| selected_commit_id.set(Some(item.commit_id)),
                                        div { class: "flex items-center justify-between gap-2",
                                            p { class: "text-sm text-gray-200", "{item.flake_name} · {item.commit_hash.chars().take(8).collect::<String>()}" }
                                            span {
                                                class: "inline-flex px-2 py-0.5 text-[10px] rounded border {eval_status_class(&item.evaluation_status)}",
                                                title: "{eval_status_help(&item.evaluation_status)}",
                                                "{item.evaluation_status}"
                                            }
                                        }
                                    }
                                }
                                if completed_items.is_empty() {
                                    p { class: "text-sm text-gray-400", "No completed evaluations yet." }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Modal for maximized logs - OUTSIDE main content div for proper z-index layering
        if *log_modal_open.read() {
            div {
                style: "position: fixed; inset: 0; z-index: 9999; display: flex; align-items: center; justify-content: center; background-color: rgba(0, 0, 0, 0.8); padding: 1rem;",
                onclick: move |_| log_modal_open.set(false),
                div {
                    style: "width: 100%; max-width: 90rem; max-height: 90vh; display: flex; flex-direction: column; background-color: rgb(17, 24, 39); border-radius: 0.5rem; border: 1px solid rgb(55, 65, 81); box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.5);",
                    onclick: move |evt| evt.stop_propagation(),
                    
                    // Header
                    div {
                        style: "display: flex; align-items: center; justify-content: space-between; padding: 0.75rem 1rem; border-bottom: 1px solid rgb(55, 65, 81);",
                        div {
                            h2 { class: "text-lg font-semibold text-white", "Evaluation Logs" }
                            if let Some(item) = selected_item.clone() {
                                p { class: "text-xs text-gray-400 mt-1",
                                    "{item.flake_name} · {item.commit_hash.chars().take(8).collect::<String>()}"
                                }
                            }
                        }
                        div { class: "flex items-center gap-2",
                            p {
                                class: "text-xs",
                                span {
                                    class: "inline-flex items-center px-2 py-0.5 rounded border {connection_badge_class(&connection_state.read())}",
                                    "{connection_badge_text(&connection_state.read())}"
                                }
                            }
                            button {
                                class: "px-3 py-1.5 text-sm rounded text-white {theme::interactive::DANGER_BTN} {theme::interactive::FOCUS_RING}",
                                onclick: move |_| log_modal_open.set(false),
                                "✕ Close"
                            }
                        }
                    }
                    
                    // Logs content
                    div {
                        style: "flex: 1; overflow: auto; padding: 1rem; background-color: rgba(3, 7, 18, 0.6);",
                        for line in eval_logs.read().iter() {
                            p { 
                                class: "text-sm font-mono text-gray-300",
                                style: "margin-bottom: 0.25rem;",
                                "{line}" 
                            }
                        }
                        if eval_logs.read().is_empty() {
                            p { class: "text-sm text-gray-500", "No log messages yet for selected commit." }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn MetricsStrip(summary: EvalQueueSummary) -> Element {
    rsx! {
        div { class: "grid grid-cols-1 sm:grid-cols-3 gap-3",
            StatCard { label: "Active", value: summary.active_count.to_string(), tone: "blue" }
            StatCard { label: "Completed", value: summary.completed_count.to_string(), tone: "green" }
            StatCard { label: "Total", value: summary.items.len().to_string(), tone: "slate" }
        }
    }
}

#[component]
fn StatCard(label: String, value: String, tone: &'static str) -> Element {
    let style = match tone {
        "blue" => "background-color: rgba(23, 37, 84, 0.6); border-color: rgba(59, 130, 246, 0.5); color: rgb(219, 234, 254);",
        "green" => "background-color: rgba(6, 78, 59, 0.6); border-color: rgba(16, 185, 129, 0.5); color: rgb(209, 250, 229);",
        _ => "background-color: rgba(30, 41, 59, 0.8); border-color: rgb(71, 85, 105); color: rgb(241, 245, 249);",
    };
    rsx! {
        div {
            class: "rounded-lg border px-3 py-2",
            style: "{style}",
            p { 
                class: "text-[11px] uppercase tracking-wide",
                style: "opacity: 0.8;",
                "{label}" 
            }
            p { class: "text-lg font-semibold", "{value}" }
        }
    }
}

fn is_active_eval_status(status: &str) -> bool {
    let normalized = status.trim().to_ascii_lowercase();
    matches!(normalized.as_str(), "pending" | "in_progress" | "in-progress" | "in progress")
}

fn is_in_progress_eval_status(status: &str) -> bool {
    let normalized = status.trim().to_ascii_lowercase();
    matches!(normalized.as_str(), "in_progress" | "in-progress" | "in progress")
}

fn is_pending_eval_status(status: &str) -> bool {
    status.trim().eq_ignore_ascii_case("pending")
}

fn active_row_class(is_selected: bool) -> &'static str {
    if is_selected {
        "w-full rounded-lg border border-blue-500/60 bg-blue-900/20 px-3 py-3 text-left transition"
    } else {
        "w-full rounded-lg border border-gray-700 bg-gray-900/40 px-3 py-3 text-left transition hover:border-gray-500"
    }
}

fn eval_status_class(status: &str) -> &'static str {
    let normalized = status.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "in_progress" | "in-progress" => "cf-eval-chip-active",
        "pending" => "cf-eval-chip-pending",
        "complete" => "cf-eval-chip-complete",
        "failed" => "cf-eval-chip-failed",
        _ => "cf-chip-slate",
    }
}

fn eval_status_help(status: &str) -> &'static str {
    let normalized = status.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "pending" => "Waiting in queue",
        "in-progress" => "Evaluation currently running",
        "in_progress" => "Evaluation currently running",
        "complete" => "Evaluation completed",
        "failed" => "Evaluation failed",
        _ => "Unknown evaluation state",
    }
}

fn connection_badge_class(state: &ConnectionState) -> &'static str {
    match state {
        ConnectionState::Connected => "border-emerald-500/40 bg-emerald-900/20 text-emerald-100",
        ConnectionState::Connecting => "border-blue-500/40 bg-blue-900/20 text-blue-100",
        ConnectionState::Disconnected => "border-slate-600 bg-slate-800/30 text-slate-300",
        ConnectionState::Error(_) => "border-red-500/40 bg-red-900/20 text-red-100",
    }
}

fn connection_badge_text(state: &ConnectionState) -> &'static str {
    match state {
        ConnectionState::Connected => "Live log stream connected",
        ConnectionState::Connecting => "Connecting log stream...",
        ConnectionState::Disconnected => "Log stream disconnected",
        ConnectionState::Error(_) => "Log stream error",
    }
}

fn system_chip_class(status: Option<&SystemEvalStatus>) -> &'static str {
    match status {
        Some(SystemEvalStatus::Evaluating) => "cf-eval-system-evaluating animate-pulse",
        Some(SystemEvalStatus::QueuedForBuild) => "cf-eval-system-success",
        Some(SystemEvalStatus::PolicyFailed) => "cf-eval-system-policy-failed",
        Some(SystemEvalStatus::Success) => "cf-eval-system-success",
        Some(SystemEvalStatus::Failed) => "cf-eval-system-failed",
        _ => "cf-eval-system-pending",
    }
}

fn system_chip_help(status: Option<&SystemEvalStatus>) -> &'static str {
    match status {
        Some(SystemEvalStatus::Pending) | None => "Pending evaluation",
        Some(SystemEvalStatus::Evaluating) => "Currently evaluating this system",
        Some(SystemEvalStatus::QueuedForBuild) => {
            "Policy passed (CF enabled); system was added to build queue"
        }
        Some(SystemEvalStatus::PolicyFailed) => {
            "Policy failed: CF agent is not enabled, so this system is not queued for build"
        }
        Some(SystemEvalStatus::Success) => "Evaluation succeeded",
        Some(SystemEvalStatus::Failed) => "Evaluation failed",
    }
}

fn progress_label(
    item: &EvalQueueItem,
    system_status: Option<std::collections::HashMap<String, SystemEvalStatus>>,
) -> String {
    if let Some(statuses) = system_status {
        let passed = item
            .systems
            .iter()
            .filter(|name| {
                matches!(
                    statuses.get(*name),
                    Some(SystemEvalStatus::QueuedForBuild) | Some(SystemEvalStatus::Success)
                )
            })
            .count() as i64;
        let total = item.system_count.max(item.systems.len() as i64);
        return format!("{passed}/{total} passed");
    }

    format!("{}/{} passed", item.passed_count, item.system_count)
}

fn spawn_reorder_request(ordered_ids: Vec<i32>, mut reorder_error: Signal<Option<String>>) {
    spawn(async move {
        if let Err(err) = reorder_eval_queue(&ordered_ids).await {
            reorder_error.set(Some(err.to_string()));
        } else {
            reorder_error.set(None);
        }
    });
}

fn reorder_commit_list(
    items: &mut Vec<EvalQueueItem>,
    source_commit_id: i32,
    target_commit_id: i32,
) {
    let Some(source_index) = items
        .iter()
        .position(|item| item.commit_id == source_commit_id)
    else {
        return;
    };
    let Some(target_index) = items
        .iter()
        .position(|item| item.commit_id == target_commit_id)
    else {
        return;
    };

    let moved = items.remove(source_index);
    items.insert(target_index, moved);
}

fn apply_active_reorder(all_items: &mut [EvalQueueItem], reordered_active: &[EvalQueueItem]) {
    let mut reordered_lookup = std::collections::HashMap::new();
    for (index, item) in reordered_active.iter().enumerate() {
        reordered_lookup.insert(item.commit_id, index as i64 + 1);
    }

    for item in all_items.iter_mut() {
        if let Some(position) = reordered_lookup.get(&item.commit_id) {
            item.queue_position = *position;
        }
    }

    all_items.sort_by(|a, b| {
        let a_active = is_active_eval_status(&a.evaluation_status);
        let b_active = is_active_eval_status(&b.evaluation_status);
        match (a_active, b_active) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.queue_position.cmp(&b.queue_position),
        }
    });
}
