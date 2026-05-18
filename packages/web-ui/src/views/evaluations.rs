//! Evaluations view - rebuilt to match JSX mockup design exactly.

use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use gloo_timers::future::TimeoutFuture;

use crate::api::{
    client::{
        ApiClientError, cancel_commit_evaluation, fetch_eval_history, fetch_eval_queue,
        force_cancel_commit_evaluation, re_evaluate_commit, reorder_eval_queue,
    },
    models::{EvalHistoryItem, EvalHistoryPage, EvalQueueItem},
};
use crate::components::{EvalLogModal, Icon, IconName};

#[derive(Clone, Copy, PartialEq, Eq)]
enum EvaluationsTab {
    ActiveQueue,
    History,
}

#[derive(Clone, PartialEq)]
enum EvalDrawerTarget {
    Queue(EvalQueueItem),
    History(EvalHistoryItem),
}

#[component]
pub fn EvaluationsView() -> Element {
    rsx! { EvaluationsPage {} }
}

#[component]
pub fn EvaluationsCommitView(commit_id: i32) -> Element {
    let _ = commit_id;
    rsx! { EvaluationsPage {} }
}

#[component]
fn EvaluationsPage() -> Element {
    let mut queue_items = use_signal(Vec::<EvalQueueItem>::new);
    let mut refresh = use_signal(|| 0_u64);
    let mut active_tab = use_signal(|| EvaluationsTab::ActiveQueue);
    let mut log_modal_target = use_signal(|| None::<EvalQueueItem>);
    let mut history_log_modal_target = use_signal(|| None::<EvalHistoryItem>);
    let mut drawer_target = use_signal(|| None::<EvalDrawerTarget>);
    let mut history_selected_ids = use_signal(std::collections::HashSet::<i32>::new);

    // History tab state
    let mut history_page = use_signal(|| 1_i64);
    let mut history_status_filter = use_signal(|| String::from("all"));
    let mut history_flake_filter = use_signal(|| String::from("all"));

    let history_resource = use_resource(move || async move {
        let _ = refresh();
        let page = history_page();
        let status = history_status_filter();
        let flake = history_flake_filter();
        fetch_eval_history(
            page,
            50,
            if status.is_empty() || status == "all" {
                None
            } else {
                Some(status.as_str())
            },
            if flake.is_empty() || flake == "all" {
                None
            } else {
                Some(flake.as_str())
            },
        )
        .await
    });

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
        }
    });

    let active_items = queue_items
        .read()
        .iter()
        .filter(|item| is_active_eval_status(&item.evaluation_status))
        .cloned()
        .collect::<Vec<_>>();

    let summary_snapshot = queue_resource
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned();

    let active_count = active_items.len() as i64;
    let completed_count = summary_snapshot
        .as_ref()
        .map(|s| s.completed_count)
        .unwrap_or(0);
    let failed_count = queue_items
        .read()
        .iter()
        .filter(|item| item.evaluation_status == "failed")
        .count() as i64;
    let total_count = queue_items.read().len() as i64;
    let selected_count = history_selected_ids.read().len();
    let selected_history_rows = history_resource
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .map(|page| {
            page.items
                .iter()
                .filter(|item| history_selected_ids.read().contains(&item.commit_id))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let same_flake_pair = selected_history_rows.len() == 2
        && selected_history_rows[0].flake_name == selected_history_rows[1].flake_name;
    let compare_disabled = selected_count != 2 || !same_flake_pair;
    let compare_title = if selected_count != 2 {
        "Select exactly 2 evaluations to compare"
    } else if !same_flake_pair {
        "Compare only works for two evaluations of the same flake"
    } else {
        "Compare selected evaluations"
    };

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 16px;",

            // Page head
            div {
                class: "page-head",
                div {
                    h1 { class: "page-title", "Evaluations" }
                    p {
                        class: "page-subtitle",
                        "{active_count} active · {completed_count} completed · {failed_count} failed"
                    }
                }
                div {
                    style: "display: flex; gap: 8px;",
                    button {
                        class: "btn btn-ghost focus-ring",
                        title: "Sync flakes",
                        Icon { name: IconName::Sync, size: 14 }
                        " Sync flakes"
                    }
                    button {
                        class: "btn btn-primary focus-ring",
                        onclick: move |_| refresh.set(refresh() + 1),
                        title: "Queue eval",
                        Icon { name: IconName::Plus, size: 14 }
                        " Queue eval"
                    }
                }
            }

            // Stat strip
            div {
                class: "stat-strip",
                div {
                    class: "stat",
                    span {
                        class: "stat-accent",
                        style: "--stat-color: #60a5fa;"
                    }
                    div { class: "stat-label", "Active" }
                    div {
                        class: "stat-value",
                        style: "color: #60a5fa;",
                        "{active_count}"
                    }
                }
                div {
                    class: "stat",
                    span {
                        class: "stat-accent",
                        style: "--stat-color: #34d399;"
                    }
                    div { class: "stat-label", "Completed" }
                    div {
                        class: "stat-value",
                        style: "color: #34d399;",
                        "{completed_count}"
                    }
                }
                div {
                    class: "stat",
                    span {
                        class: "stat-accent",
                        style: "--stat-color: #f87171;"
                    }
                    div { class: "stat-label", "Failed" }
                    div {
                        class: "stat-value",
                        style: "color: #f87171;",
                        "{failed_count}"
                    }
                }
                div {
                    class: "stat",
                    span {
                        class: "stat-accent",
                        style: "--stat-color: var(--cf-text-secondary);"
                    }
                    div { class: "stat-label", "Total" }
                    div { class: "stat-value", "{total_count}" }
                }
            }

            // Tabs
            div {
                class: "card",
                style: "overflow: hidden;",

                div {
                    class: "sd-tabs",
                    style: "padding: 0 16px; border-bottom: 1px solid var(--cf-card-border);",
                    button {
                        class: if active_tab() == EvaluationsTab::ActiveQueue {
                            "sd-tab focus-ring active"
                        } else {
                            "sd-tab focus-ring"
                        },
                        onclick: move |_| active_tab.set(EvaluationsTab::ActiveQueue),
                        "Active Queue "
                        span { class: "sd-tab-badge", "{active_count}" }
                    }
                    button {
                        class: if active_tab() == EvaluationsTab::History {
                            "sd-tab focus-ring active"
                        } else {
                            "sd-tab focus-ring"
                        },
                        onclick: move |_| active_tab.set(EvaluationsTab::History),
                        "History"
                    }
                }

                if active_tab() == EvaluationsTab::ActiveQueue {
                    EvalActiveQueue {
                        evals: active_items.clone(),
                        refresh: refresh,
                        queue_items: queue_items,
                        log_modal_target: log_modal_target,
                        drawer_target: drawer_target,
                    }
                }

                if active_tab() == EvaluationsTab::History {
                    if !history_selected_ids.read().is_empty() {
                        div {
                            class: "ed-bulkbar",
                            span {
                                style: "font-size: 13px; font-weight: 600;",
                                "{selected_count} selected"
                            }
                            button {
                                class: "btn btn-ghost focus-ring xs",
                                onclick: move |_| {
                                    let selected_ids: Vec<i32> = history_selected_ids.read().iter().copied().collect();
                                    let mut refresh_sig = refresh.clone();
                                    let mut selected_sig = history_selected_ids.clone();
                                    spawn(async move {
                                        for commit_id in selected_ids {
                                            let _ = re_evaluate_commit(commit_id).await;
                                        }
                                        selected_sig.write().clear();
                                        refresh_sig.set(refresh_sig() + 1);
                                    });
                                },
                                Icon { name: IconName::Sync, size: 11 }
                                " Re-evaluate"
                            }
                            button {
                                class: "btn btn-ghost focus-ring xs",
                                disabled: compare_disabled,
                                title: "{compare_title}",
                                style: if compare_disabled {
                                    "opacity: 0.4; cursor: not-allowed;"
                                } else {
                                    ""
                                },
                                "Compare"
                            }
                            button {
                                class: "btn btn-ghost focus-ring xs",
                                Icon { name: IconName::Download, size: 11 }
                                " Download logs"
                            }
                            div { style: "flex: 1;" }
                            button {
                                class: "btn-icon focus-ring",
                                onclick: move |_| {
                                    history_selected_ids.write().clear();
                                },
                                title: "Clear",
                                Icon { name: IconName::X, size: 14 }
                            }
                        }
                    }

                    EvalHistory {
                        history_resource: history_resource,
                        history_status_filter: history_status_filter,
                        history_flake_filter: history_flake_filter,
                        history_page: history_page,
                        refresh: refresh,
                        history_log_modal_target: history_log_modal_target,
                        history_selected_ids: history_selected_ids,
                        drawer_target: drawer_target,
                    }
                }
            }

            if let Some(target) = drawer_target.read().clone() {
                EvalDrawer {
                    target: target,
                    refresh: refresh,
                    on_close: move |_| drawer_target.set(None),
                    open_queue_logs: move |item: EvalQueueItem| log_modal_target.set(Some(item)),
                    open_history_logs: move |item: EvalHistoryItem| history_log_modal_target.set(Some(item)),
                }
            }

            // Log modal (from active queue)
            if let Some(target) = log_modal_target.read().clone() {
                EvalLogModal {
                    commit_id: target.commit_id,
                    commit_hash: target.commit_hash,
                    evaluation_status: target.evaluation_status,
                    on_close: move |_| log_modal_target.set(None),
                }
            }

            // Log modal (from history tab)
            if let Some(target) = history_log_modal_target.read().clone() {
                EvalLogModal {
                    commit_id: target.commit_id,
                    commit_hash: target.commit_hash,
                    evaluation_status: target.evaluation_status,
                    on_close: move |_| history_log_modal_target.set(None),
                }
            }

            if log_modal_target.read().is_none() && history_log_modal_target.read().is_none() {
                div {
                    class: "ed-kbd-hint",
                    span {
                        kbd { "j" }
                        kbd { "k" }
                        " navigate"
                    }
                    span {
                        kbd { "↵" }
                        " open"
                    }
                    if active_tab() == EvaluationsTab::ActiveQueue {
                        span {
                            kbd { "c" }
                            " cancel"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EvalActiveQueue(
    evals: Vec<EvalQueueItem>,
    mut refresh: Signal<u64>,
    queue_items: Signal<Vec<EvalQueueItem>>,
    mut log_modal_target: Signal<Option<EvalQueueItem>>,
    mut drawer_target: Signal<Option<EvalDrawerTarget>>,
) -> Element {
    if evals.is_empty() {
        return rsx! {
            div {
                class: "empty",
                style: "margin: 24px;",
                h3 { "No active evaluations" }
                div { "All flake evaluations are complete." }
            }
        };
    }

    rsx! {
        table {
            class: "sys-table",
            thead {
                tr {
                    th { style: "width: 40px;", "#" }
                    th { "Flake · commit" }
                    th { "Branch" }
                    th { "Status" }
                    th { "Systems" }
                    th { "Policy" }
                    th { "Started" }
                    th { style: "text-align: right;", "Actions" }
                }
            }
            tbody {
                for (i , ev) in evals.iter().enumerate() {
                    {
                        let ev_clone = ev.clone();
                        let commit_id = ev.commit_id;
                        let status_meta = eval_status_meta(&ev.evaluation_status);
                        let can_cancel = matches!(ev.evaluation_status.as_str(), "pending" | "in_progress");
                        let can_force_cancel = ev.evaluation_status == "cancelling";
                        let is_first = i == 0;
                        let is_last = i == evals.len() - 1;
                        let ev_for_row = ev_clone.clone();
                        let ev_for_log_button = ev_clone.clone();

                        rsx! {
                            tr {
                                key: "{commit_id}",
                                style: "cursor: pointer;",
                                onclick: move |_| drawer_target.set(Some(EvalDrawerTarget::Queue(ev_for_row.clone()))),
                                td {
                                    style: "color: var(--cf-text-muted); font-size: 12px;",
                                    "{ev.queue_position}"
                                }
                                td {
                                    div {
                                        style: "font-weight: 600; font-size: 13px;",
                                        "{ev_clone.flake_name}"
                                    }
                                    div {
                                        class: "mono",
                                        style: "font-size: 11px; color: var(--cf-text-muted);",
                                        "{ev_clone.commit_hash.chars().take(12).collect::<String>()}"
                                    }
                                }
                                td {
                                    span { class: "chip chip-unknown", "{ev_clone.branch}" }
                                }
                                td {
                                    span {
                                        class: "chip {status_meta.cls}",
                                        span {
                                            class: "chip-dot",
                                            style: "background: {status_meta.color};"
                                        }
                                        "{status_meta.label}"
                                    }
                                }
                                td {
                                    style: "font-size: 12px; color: var(--cf-text-secondary);",
                                    "{ev_clone.system_count} hosts"
                                }
                                td {
                                    div {
                                        style: "display: flex; gap: 6px;",
                                        span {
                                            class: "chip chip-healthy",
                                            "{ev_clone.passed_count} ✓"
                                        }
                                        if ev_clone.policy_failed_count > 0 {
                                            span {
                                                class: "chip chip-critical",
                                                "{ev_clone.policy_failed_count} ✗"
                                            }
                                        }
                                    }
                                }
                                td {
                                    style: "font-size: 12px; color: var(--cf-text-muted);",
                                    "{format_relative_time(ev_clone.committed_at)}"
                                }
                                td {
                                    onclick: move |evt| evt.stop_propagation(),
                                    div {
                                        class: "row-actions",
                                        style: "opacity: 1; gap: 4px; justify-content: flex-end;",

                                        button {
                                            class: "btn-icon focus-ring",
                                            title: "Move up",
                                            disabled: is_first,
                                            style: if is_first { "opacity: 0.3;" } else { "" },
                                            onclick: move |_| {
                                                if is_first { return; }
                                                let items = queue_items.clone();
                                                let mut refresh_sig = refresh.clone();

                                                spawn(async move {
                                                    let mut active: Vec<_> = items
                                                        .read()
                                                        .iter()
                                                        .filter(|item| is_active_eval_status(&item.evaluation_status))
                                                        .cloned()
                                                        .collect();

                                                    if let Some(idx) = active.iter().position(|e| e.commit_id == commit_id) {
                                                        if idx > 0 {
                                                            active.swap(idx - 1, idx);
                                                            let ordered_ids: Vec<i32> = active.iter().map(|e| e.commit_id).collect();
                                                            let _ = reorder_eval_queue(&ordered_ids).await;
                                                            refresh_sig.set(refresh_sig() + 1);
                                                        }
                                                    }
                                                });
                                            },
                                            "↑"
                                        }

                                        button {
                                            class: "btn-icon focus-ring",
                                            title: "Move down",
                                            disabled: is_last,
                                            style: if is_last { "opacity: 0.3;" } else { "" },
                                            onclick: move |_| {
                                                if is_last { return; }
                                                let items = queue_items.clone();
                                                let mut refresh_sig = refresh.clone();

                                                spawn(async move {
                                                    let mut active: Vec<_> = items
                                                        .read()
                                                        .iter()
                                                        .filter(|item| is_active_eval_status(&item.evaluation_status))
                                                        .cloned()
                                                        .collect();

                                                    if let Some(idx) = active.iter().position(|e| e.commit_id == commit_id) {
                                                        if idx + 1 < active.len() {
                                                            active.swap(idx, idx + 1);
                                                            let ordered_ids: Vec<i32> = active.iter().map(|e| e.commit_id).collect();
                                                            let _ = reorder_eval_queue(&ordered_ids).await;
                                                            refresh_sig.set(refresh_sig() + 1);
                                                        }
                                                    }
                                                });
                                            },
                                            "↓"
                                        }

                                        button {
                                            class: "btn-icon focus-ring",
                                            title: "View logs",
                                            onclick: move |_| log_modal_target.set(Some(ev_for_log_button.clone())),
                                            Icon { name: IconName::Terminal, size: 14 }
                                        }

                                        if can_force_cancel {
                                            button {
                                                class: "btn btn-danger focus-ring",
                                                style: "padding: 3px 8px; font-size: 11px;",
                                                onclick: move |_| {
                                                    let mut refresh_sig = refresh.clone();
                                                    spawn(async move {
                                                        let _ = force_cancel_commit_evaluation(commit_id).await;
                                                        refresh_sig.set(refresh_sig() + 1);
                                                    });
                                                },
                                                "Force cancel"
                                            }
                                        }

                                        if can_cancel {
                                            button {
                                                class: "btn btn-ghost focus-ring",
                                                style: "padding: 3px 8px; font-size: 11px;",
                                                onclick: move |_| {
                                                    let mut refresh_sig = refresh.clone();
                                                    spawn(async move {
                                                        let _ = cancel_commit_evaluation(commit_id).await;
                                                        refresh_sig.set(refresh_sig() + 1);
                                                    });
                                                },
                                                "Cancel"
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
fn EvalHistory(
    history_resource: Resource<Result<EvalHistoryPage, ApiClientError>>,
    mut history_status_filter: Signal<String>,
    mut history_flake_filter: Signal<String>,
    mut history_page: Signal<i64>,
    mut refresh: Signal<u64>,
    mut history_log_modal_target: Signal<Option<EvalHistoryItem>>,
    mut history_selected_ids: Signal<std::collections::HashSet<i32>>,
    mut drawer_target: Signal<Option<EvalDrawerTarget>>,
) -> Element {
    let history_snapshot = history_resource.read();

    rsx! {
        div {
            // Filter bar
            div {
                style: "padding: 12px 16px; border-bottom: 1px solid var(--cf-divider); display: flex; gap: 10px; flex-wrap: wrap; align-items: center;",

                div {
                    class: "seg",
                    for (label, value) in [("all", ""), ("complete", "complete"), ("failed", "failed"), ("cancelled", "cancelled")] {
                        {
                            let value_str = value.to_string();
                            let is_active = history_status_filter() == value;
                            rsx! {
                                button {
                                    key: "{label}",
                                    class: if is_active { "active" } else { "" },
                                    onclick: move |_| {
                                        history_status_filter.set(value_str.clone());
                                        history_page.set(1);
                                        refresh.set(refresh() + 1);
                                    },
                                    "{label}"
                                }
                            }
                        }
                    }
                }

                select {
                    class: "input filter-select focus-ring",
                    style: "width: auto;",
                    value: "{history_flake_filter()}",
                    onchange: move |evt| {
                        history_flake_filter.set(evt.value().clone());
                        history_page.set(1);
                        refresh.set(refresh() + 1);
                    },
                    option { value: "all", "All flakes" }
                    if let Some(Ok(page_data)) = &*history_snapshot {
                        {
                            let mut flakes: Vec<String> = page_data
                                .items
                                .iter()
                                .map(|item| item.flake_name.clone())
                                .collect();
                            flakes.sort();
                            flakes.dedup();
                            flakes.into_iter().map(|flake_name| rsx! {
                                option {
                                    key: "{flake_name}",
                                    value: "{flake_name}",
                                    "{flake_name}"
                                }
                            })
                        }
                    }
                }

                if let Some(Ok(page_data)) = &*history_snapshot {
                    span {
                        class: "filter-count",
                        "{page_data.items.len()} entries"
                    }
                }
            }

            // History table
            match &*history_snapshot {
                Some(Ok(page_data)) => rsx! {
                    {
                        let commit_ids: Vec<i32> = page_data.items.iter().map(|item| item.commit_id).collect();
                        let all_checked = commit_ids.iter().all(|id| history_selected_ids.read().contains(id))
                            && !commit_ids.is_empty();
                        rsx! {
                    table {
                        class: "sys-table",
                        thead {
                            tr {
                                th { style: "width: 36px;",
                                    input {
                                        r#type: "checkbox",
                                        class: "ed-checkbox",
                                        checked: all_checked,
                                        oninput: move |_| {
                                            if all_checked {
                                                history_selected_ids.write().clear();
                                            } else {
                                                let mut next = history_selected_ids.read().clone();
                                                for id in &commit_ids {
                                                    next.insert(*id);
                                                }
                                                history_selected_ids.set(next);
                                            }
                                        }
                                    }
                                }
                                th { "Flake · commit" }
                                th { "Branch" }
                                th { "Status" }
                                th { "Systems" }
                                th { "Policy" }
                                th { "Duration" }
                                th { "Completed" }
                                th { style: "text-align: right;", " " }
                            }
                        }
                        tbody {
                            for ev in page_data.items.iter() {
                                {
                                    let ev = ev.clone();
                                    let commit_id = ev.commit_id;
                                    let status_meta = eval_status_meta(&ev.evaluation_status);
                                    let ev_for_row = ev.clone();
                                    let ev_for_log_button = ev.clone();

                                    rsx! {
                                        tr {
                                            key: "{commit_id}",
                                            style: "cursor: pointer;",
                                            onclick: move |_| drawer_target.set(Some(EvalDrawerTarget::History(ev_for_row.clone()))),
                                            td {
                                                onclick: move |evt| {
                                                    evt.stop_propagation();
                                                    let mut next = history_selected_ids.read().clone();
                                                    if next.contains(&commit_id) {
                                                        next.remove(&commit_id);
                                                    } else {
                                                        next.insert(commit_id);
                                                    }
                                                    history_selected_ids.set(next);
                                                },
                                                input {
                                                    r#type: "checkbox",
                                                    class: "ed-checkbox",
                                                    checked: history_selected_ids.read().contains(&commit_id),
                                                    readonly: true,
                                                }
                                            }
                                            td {
                                                div {
                                                    style: "font-weight: 600; font-size: 13px;",
                                                    "{ev.flake_name}"
                                                }
                                                div {
                                                    class: "mono",
                                                    style: "font-size: 11px; color: var(--cf-text-muted);",
                                                    "{ev.commit_hash.chars().take(12).collect::<String>()}"
                                                }
                                            }
                                            td {
                                                span { class: "chip chip-unknown", "{ev.branch}" }
                                            }
                                            td {
                                                span {
                                                    class: "chip {status_meta.cls}",
                                                    span {
                                                        class: "chip-dot",
                                                        style: "background: {status_meta.color};"
                                                    }
                                                    "{status_meta.label}"
                                                }
                                            }
                                            td {
                                                style: "font-size: 12px;",
                                                "{ev.system_count}"
                                            }
                                            td {
                                                div {
                                                    style: "display: flex; gap: 6px;",
                                                    span {
                                                        class: "chip chip-healthy",
                                                        style: "font-size: 10px;",
                                                        "{ev.passed_count} ✓"
                                                    }
                                                    if ev.policy_failed_count > 0 {
                                                        span {
                                                            class: "chip chip-critical",
                                                            style: "font-size: 10px;",
                                                            "{ev.policy_failed_count} ✗"
                                                        }
                                                    }
                                                }
                                            }
                                            td {
                                                class: "mono",
                                                style: "font-size: 12px; color: var(--cf-text-secondary);",
                                                "{format_eval_duration(&ev)}"
                                            }
                                            td {
                                                style: "font-size: 12px; color: var(--cf-text-muted);",
                                                "{format_eval_completed_at(&ev)}"
                                            }
                                             td {
                                                div {
                                                    class: "row-actions",
                                                    onclick: move |evt| evt.stop_propagation(),
                                                    button {
                                                        class: "btn-icon focus-ring",
                                                        title: "View evaluation logs",
                                                        onclick: move |_| history_log_modal_target.set(Some(ev_for_log_button.clone())),
                                                        Icon { name: IconName::Terminal, size: 14 }
                                                    }
                                                    if ev.evaluation_status != "complete" {
                                                        button {
                                                            class: "btn-icon focus-ring",
                                                            title: "Re-evaluate",
                                                            onclick: move |_| {
                                                                let mut refresh_sig = refresh.clone();
                                                                spawn(async move {
                                                                    let _ = re_evaluate_commit(commit_id).await;
                                                                    refresh_sig.set(refresh_sig() + 1);
                                                                });
                                                            },
                                                            Icon { name: IconName::Sync, size: 14 }
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
                },
                Some(Err(e)) => rsx! {
                    div {
                        style: "padding: 24px; color: var(--cf-red);",
                        "Failed to load eval history: {e}"
                    }
                },
                None => rsx! {
                    div {
                        style: "padding: 24px; display: flex; align-items: center; gap: 12px; color: var(--cf-text-muted);",
                        "Loading history…"
                    }
                }
            }
        }
    }
}

#[component]
fn EvalDrawer(
    target: EvalDrawerTarget,
    mut refresh: Signal<u64>,
    on_close: EventHandler<MouseEvent>,
    open_queue_logs: EventHandler<EvalQueueItem>,
    open_history_logs: EventHandler<EvalHistoryItem>,
) -> Element {
    let mut drawer_tab = use_signal(|| String::from("log"));

    match target {
        EvalDrawerTarget::Queue(ev) => {
            let status_meta = eval_status_meta(&ev.evaluation_status);
            let can_cancel = matches!(ev.evaluation_status.as_str(), "pending" | "in_progress");
            let can_force_cancel = ev.evaluation_status == "cancelling";
            let is_live = can_cancel || can_force_cancel;
            let open_logs_ev = ev.clone();
            let close_click = move |evt: MouseEvent| on_close.call(evt);

            rsx! {
                div {
                    class: "fl-tray-backdrop",
                    onclick: close_click,
                }
                aside {
                    class: "fl-tray",
                    role: "dialog",
                    "aria-label": "Evaluation detail",
                    header {
                        class: "fl-tray-head",
                        div {
                            style: "display: flex; align-items: center; gap: 12px; min-width: 0; flex: 1;",
                            Icon { name: IconName::Terminal, size: 18 }
                            div { style: "min-width: 0;",
                            h2 {
                                "{ev.flake_name}"
                                span { class: "chip chip-unknown", style: "font-size: 10px;", "{ev.branch}" }
                                span {
                                    class: "chip {status_meta.cls}",
                                    span { class: "chip-dot", style: "background: {status_meta.color};" }
                                    "{status_meta.label}"
                                    if is_live { span { class: "ed-pulse" } }
                                }
                            }
                            div {
                                class: "fqdn",
                                "{ev.commit_hash} · {ev.commit_id}"
                            }
                            }
                        }
                        div {
                            style: "display: flex; gap: 6px; align-items: center;",
                            if can_cancel {
                                button {
                                    class: "btn btn-ghost focus-ring xs",
                                    onclick: move |_| {
                                        let mut refresh_sig = refresh.clone();
                                        let commit_id = ev.commit_id;
                                        spawn(async move {
                                            let _ = cancel_commit_evaluation(commit_id).await;
                                            refresh_sig.set(refresh_sig() + 1);
                                        });
                                    },
                                    "Cancel"
                                }
                            }
                            if can_force_cancel {
                                button {
                                    class: "btn btn-ghost focus-ring xs",
                                    style: "color: #f87171;",
                                    onclick: move |_| {
                                        let mut refresh_sig = refresh.clone();
                                        let commit_id = ev.commit_id;
                                        spawn(async move {
                                            let _ = force_cancel_commit_evaluation(commit_id).await;
                                            refresh_sig.set(refresh_sig() + 1);
                                        });
                                    },
                                    "Force-cancel"
                                }
                            }
                            button {
                                class: "btn-icon focus-ring",
                                onclick: close_click,
                                title: "Close",
                                Icon { name: IconName::X, size: 16 }
                            }
                        }
                    }
                    div {
                        class: "ed-stats",
                        div { class: "ed-stat", div { class: "ed-stat-label", "Started" } div { class: "ed-stat-val", style: "font-size: 13px; font-weight: 600;", "{format_relative_time(ev.committed_at)}" } }
                        div { class: "ed-stat", div { class: "ed-stat-label", "Duration" } div { class: "ed-stat-val mono", "—" } }
                        div { class: "ed-stat", div { class: "ed-stat-label", "Systems" } div { class: "ed-stat-val", "{ev.system_count}" } }
                        div {
                            class: "ed-stat",
                            div { class: "ed-stat-label", "Policy" }
                            div {
                                class: "ed-stat-val",
                                style: "display: flex; gap: 6px; align-items: baseline;",
                                span { style: "color: #34d399;", "{ev.passed_count}" }
                                span { style: "font-size: 12px; color: var(--cf-text-muted);", "/" }
                                span {
                                    style: if ev.policy_failed_count > 0 { "color: #f87171;" } else { "color: var(--cf-text-muted);" },
                                    "{ev.policy_failed_count}"
                                }
                            }
                        }
                        div { class: "ed-stat", div { class: "ed-stat-label", "Derivations" } div { class: "ed-stat-val", "{ev.system_count * 18}" } }
                    }
                    div {
                        class: "sd-tabs",
                        style: "padding: 0 16px; border-bottom: 1px solid var(--cf-card-border); flex-shrink: 0;",
                        button {
                            class: if drawer_tab() == "log" { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
                            onclick: move |_| drawer_tab.set("log".to_string()),
                            Icon { name: IconName::Terminal, size: 12 }
                            " Log"
                            if is_live { span { class: "ed-pulse" } }
                        }
                        button {
                            class: if drawer_tab() == "policy" { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
                            onclick: move |_| drawer_tab.set("policy".to_string()),
                            Icon { name: IconName::Shield, size: 12 }
                            " Policy matrix"
                        }
                        button {
                            class: if drawer_tab() == "graph" { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
                            onclick: move |_| drawer_tab.set("graph".to_string()),
                            Icon { name: IconName::Git, size: 12 }
                            " Dependency graph"
                        }
                    }
                    div {
                        class: "ed-body",
                        if drawer_tab() == "log" {
                            EvalDrawerLogTabQueue { ev: ev.clone(), live: is_live }
                        } else if drawer_tab() == "policy" {
                            EvalDrawerPolicyTab {
                                systems: ev.system_count,
                                pass: ev.passed_count,
                                fail: ev.policy_failed_count,
                            }
                        } else {
                            EvalDrawerGraphTab {
                                commit: ev.commit_hash.chars().take(12).collect::<String>(),
                                systems: ev.system_count,
                                derivations: ev.system_count * 18,
                            }
                        }
                    }
                    div {
                        class: "panel-actions",
                        button {
                            class: "btn btn-ghost focus-ring",
                            onclick: move |_| open_queue_logs.call(open_logs_ev.clone()),
                            Icon { name: IconName::Terminal, size: 14 }
                            " Logs"
                        }
                    }
                }
            }
        }
        EvalDrawerTarget::History(ev) => {
            let status_meta = eval_status_meta(&ev.evaluation_status);
            let is_live = matches!(ev.evaluation_status.as_str(), "pending" | "in_progress" | "cancelling");
            let open_logs_ev = ev.clone();
            let close_click = move |evt: MouseEvent| on_close.call(evt);

            rsx! {
                div {
                    class: "fl-tray-backdrop",
                    onclick: close_click,
                }
                aside {
                    class: "fl-tray",
                    role: "dialog",
                    "aria-label": "Evaluation detail",
                    header {
                        class: "fl-tray-head",
                        div {
                            style: "display: flex; align-items: center; gap: 12px; min-width: 0; flex: 1;",
                            Icon { name: IconName::Terminal, size: 18 }
                            div { style: "min-width: 0;",
                            h2 {
                                "{ev.flake_name}"
                                span { class: "chip chip-unknown", style: "font-size: 10px;", "{ev.branch}" }
                                span {
                                    class: "chip {status_meta.cls}",
                                    span { class: "chip-dot", style: "background: {status_meta.color};" }
                                    "{status_meta.label}"
                                    if is_live { span { class: "ed-pulse" } }
                                }
                            }
                            div {
                                class: "fqdn",
                                "{ev.commit_hash} · {ev.commit_id}"
                            }
                            }
                        }
                        button {
                            class: "btn-icon focus-ring",
                            onclick: close_click,
                            title: "Close",
                            Icon { name: IconName::X, size: 16 }
                        }
                    }
                    div {
                        class: "ed-stats",
                        div { class: "ed-stat", div { class: "ed-stat-label", "Started" } div { class: "ed-stat-val", style: "font-size: 13px; font-weight: 600;", "{format_relative_time(ev.committed_at)}" } }
                        div { class: "ed-stat", div { class: "ed-stat-label", "Duration" } div { class: "ed-stat-val mono", "{format_eval_duration(&ev)}" } }
                        div { class: "ed-stat", div { class: "ed-stat-label", "Systems" } div { class: "ed-stat-val", "{ev.system_count}" } }
                        div {
                            class: "ed-stat",
                            div { class: "ed-stat-label", "Policy" }
                            div {
                                class: "ed-stat-val",
                                style: "display: flex; gap: 6px; align-items: baseline;",
                                span { style: "color: #34d399;", "{ev.passed_count}" }
                                span { style: "font-size: 12px; color: var(--cf-text-muted);", "/" }
                                span {
                                    style: if ev.policy_failed_count > 0 { "color: #f87171;" } else { "color: var(--cf-text-muted);" },
                                    "{ev.policy_failed_count}"
                                }
                            }
                        }
                        div { class: "ed-stat", div { class: "ed-stat-label", "Derivations" } div { class: "ed-stat-val", "{ev.system_count * 18}" } }
                    }
                    div {
                        class: "sd-tabs",
                        style: "padding: 0 16px; border-bottom: 1px solid var(--cf-card-border); flex-shrink: 0;",
                        button {
                            class: if drawer_tab() == "log" { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
                            onclick: move |_| drawer_tab.set("log".to_string()),
                            Icon { name: IconName::Terminal, size: 12 }
                            " Log"
                            if is_live { span { class: "ed-pulse" } }
                        }
                        button {
                            class: if drawer_tab() == "policy" { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
                            onclick: move |_| drawer_tab.set("policy".to_string()),
                            Icon { name: IconName::Shield, size: 12 }
                            " Policy matrix"
                        }
                        button {
                            class: if drawer_tab() == "graph" { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
                            onclick: move |_| drawer_tab.set("graph".to_string()),
                            Icon { name: IconName::Git, size: 12 }
                            " Dependency graph"
                        }
                    }
                    div {
                        class: "ed-body",
                        if drawer_tab() == "log" {
                            EvalDrawerLogTabHistory { ev: ev.clone(), live: is_live }
                        } else if drawer_tab() == "policy" {
                            EvalDrawerPolicyTab {
                                systems: ev.system_count,
                                pass: ev.passed_count,
                                fail: ev.policy_failed_count,
                            }
                        } else {
                            EvalDrawerGraphTab {
                                commit: ev.commit_hash.chars().take(12).collect::<String>(),
                                systems: ev.system_count,
                                derivations: ev.system_count * 18,
                            }
                        }
                    }
                    div {
                        class: "panel-actions",
                        if ev.evaluation_status != "complete" {
                            button {
                                class: "btn btn-ghost focus-ring",
                                onclick: move |_| {
                                    let mut refresh_sig = refresh.clone();
                                    let commit_id = ev.commit_id;
                                    spawn(async move {
                                        let _ = re_evaluate_commit(commit_id).await;
                                        refresh_sig.set(refresh_sig() + 1);
                                    });
                                },
                                Icon { name: IconName::Sync, size: 14 }
                                " Re-evaluate"
                            }
                        }
                        button {
                            class: "btn btn-ghost focus-ring",
                            onclick: move |_| open_history_logs.call(open_logs_ev.clone()),
                            Icon { name: IconName::Terminal, size: 14 }
                            " Logs"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EvalDrawerLogTabQueue(ev: EvalQueueItem, live: bool) -> Element {
    let mut autoscroll = use_signal(|| true);
    // Mock log lines matching JSX EVAL_DEFAULT_LOG pattern
    let lines = generate_eval_log_lines(&ev.flake_name, &ev.commit_hash, ev.system_count, ev.passed_count, ev.policy_failed_count, &ev.evaluation_status);

    rsx! {
        div { style: "display: flex; flex-direction: column; flex: 1; min-height: 0;",
            div {
                style: "padding: 8px 16px; border-bottom: 1px solid var(--cf-divider); display: flex; gap: 10px; align-items: center; flex-shrink: 0;",
                span { style: "font-size: 11px; color: var(--cf-text-muted);", "{lines.len()} lines" }
                div { style: "flex: 1;" }
                label {
                    style: "display: flex; gap: 6px; align-items: center; font-size: 11px;",
                    input {
                        r#type: "checkbox",
                        class: "ed-checkbox",
                        checked: autoscroll(),
                        oninput: move |_| autoscroll.set(!autoscroll()),
                    }
                    "Auto-scroll"
                }
                button {
                    class: "btn-icon focus-ring",
                    title: "Download",
                    Icon { name: IconName::Download, size: 13 }
                }
            }
            pre { class: "fl-diff", style: "flex: 1; font-size: 11px; line-height: 1.55; padding: 10px 16px; margin: 0;",
                for (idx, line) in lines.iter().enumerate() {
                    {
                        let color = log_line_color(line);
                        let line_num = format!("{:>3}", idx + 1);
                        rsx! {
                            div { style: "color: {color};",
                                span { style: "color: var(--cf-text-muted); user-select: none; display: inline-block; width: 36px;", "{line_num}" }
                                " {line}"
                            }
                        }
                    }
                }
                if live {
                    {
                        let next_line_num = format!("{:>3}", lines.len() + 1);
                        rsx! {
                            div { style: "color: #60a5fa;",
                                span { style: "color: var(--cf-text-muted); display: inline-block; width: 36px;", "{next_line_num}" }
                                " "
                                span { class: "ed-pulse", style: "margin-left: 0;" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EvalDrawerLogTabHistory(ev: EvalHistoryItem, live: bool) -> Element {
    let mut autoscroll = use_signal(|| true);
    // Mock log lines matching JSX EVAL_DEFAULT_LOG pattern
    let lines = generate_eval_log_lines(&ev.flake_name, &ev.commit_hash, ev.system_count, ev.passed_count, ev.policy_failed_count, &ev.evaluation_status);

    rsx! {
        div { style: "display: flex; flex-direction: column; flex: 1; min-height: 0;",
            div {
                style: "padding: 8px 16px; border-bottom: 1px solid var(--cf-divider); display: flex; gap: 10px; align-items: center; flex-shrink: 0;",
                span { style: "font-size: 11px; color: var(--cf-text-muted);", "{lines.len()} lines" }
                div { style: "flex: 1;" }
                label {
                    style: "display: flex; gap: 6px; align-items: center; font-size: 11px;",
                    input {
                        r#type: "checkbox",
                        class: "ed-checkbox",
                        checked: autoscroll(),
                        oninput: move |_| autoscroll.set(!autoscroll()),
                    }
                    "Auto-scroll"
                }
                button {
                    class: "btn-icon focus-ring",
                    title: "Download",
                    Icon { name: IconName::Download, size: 13 }
                }
            }
            pre { class: "fl-diff", style: "flex: 1; font-size: 11px; line-height: 1.55; padding: 10px 16px; margin: 0;",
                for (idx, line) in lines.iter().enumerate() {
                    {
                        let color = log_line_color(line);
                        let line_num = format!("{:>3}", idx + 1);
                        rsx! {
                            div { style: "color: {color};",
                                span { style: "color: var(--cf-text-muted); user-select: none; display: inline-block; width: 36px;", "{line_num}" }
                                " {line}"
                            }
                        }
                    }
                }
                if live {
                    {
                        let next_line_num = format!("{:>3}", lines.len() + 1);
                        rsx! {
                            div { style: "color: #60a5fa;",
                                span { style: "color: var(--cf-text-muted); display: inline-block; width: 36px;", "{next_line_num}" }
                                " "
                                span { class: "ed-pulse", style: "margin-left: 0;" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EvalDrawerPolicyTab(systems: i64, pass: i64, fail: i64) -> Element {
    let mut filter = use_signal(|| "all".to_string());
    let mut sort = use_signal(|| "health".to_string());
    let mut policy_filter = use_signal(|| Option::<String>::None);
    let mut expanded = use_signal(|| Option::<String>::None);

    // Generate mock policy matrix data matching JSX EVAL_DEFAULT_POLICY
    let policies = vec!["stig.audit", "stig.fw", "stig.sshd", "stig.tls", "cf.hb", "cf.cve", "cf.cache"];
    let hosts: Vec<String> = (0..systems.max(1).min(9))
        .map(|i| {
            let names = ["atlas-01", "gaia-web-02", "orion-db", "helios-edge", "titan-build", "artemis-cdn", "apollo-net", "luna-mon", "perseus-vpn"];
            names.get(i as usize).unwrap_or(&"host").to_string()
        })
        .collect();

    // Build rows with results
    let rows: Vec<PolicyRow> = hosts.iter().enumerate().map(|(i, host)| {
        let results: Vec<&str> = policies.iter().enumerate().map(|(j, _)| {
            let r = ((i * 13 + j * 7) % 100) as i32;
            if fail > 0 && j == 3 && i == 0 { "fail" }
            else if r > 92 { "fail" }
            else if r > 80 { "warn" }
            else { "pass" }
        }).collect();
        let fail_count = results.iter().filter(|&&r| r == "fail").count() as i64;
        let warn_count = results.iter().filter(|&&r| r == "warn").count() as i64;
        let pass_count = results.iter().filter(|&&r| r == "pass").count() as i64;
        PolicyRow {
            host: host.clone(),
            results: results.iter().map(|s| s.to_string()).collect(),
            fail: fail_count,
            warn: warn_count,
            pass: pass_count,
        }
    }).collect();

    // Compute counts
    let fail_count = rows.iter().filter(|r| r.fail > 0).count() as i64;
    let warn_count = rows.iter().filter(|r| r.fail == 0 && r.warn > 0).count() as i64;
    let clean_count = rows.iter().filter(|r| r.fail == 0 && r.warn == 0).count() as i64;

    // Per-policy stats for headers
    let policy_stats: Vec<(i64, i64, i64)> = (0..policies.len()).map(|j| {
        let f = rows.iter().filter(|r| r.results.get(j).map(|s| s == "fail").unwrap_or(false)).count() as i64;
        let w = rows.iter().filter(|r| r.results.get(j).map(|s| s == "warn").unwrap_or(false)).count() as i64;
        let p = rows.iter().filter(|r| r.results.get(j).map(|s| s == "pass").unwrap_or(false)).count() as i64;
        (f, w, p)
    }).collect();

    // Top issues (policies with failures)
    let top_issues: Vec<(String, i64, i64)> = policies.iter().enumerate()
        .map(|(j, p)| (p.to_string(), policy_stats[j].0, rows.len() as i64))
        .filter(|(_, f, _)| *f > 0)
        .take(3)
        .collect();

    // Apply filters
    let mut filtered: Vec<&PolicyRow> = rows.iter().collect();
    let fval = filter();
    if fval == "fail" { filtered.retain(|r| r.fail > 0); }
    if fval == "warn" { filtered.retain(|r| r.warn > 0 && r.fail == 0); }
    if fval == "clean" { filtered.retain(|r| r.fail == 0 && r.warn == 0); }
    if let Some(ref pf) = *policy_filter.read() {
        if let Some(idx) = policies.iter().position(|&p| p == pf) {
            filtered.retain(|r| r.results.get(idx).map(|s| s != "pass").unwrap_or(false));
        }
    }

    // Sort
    if sort() == "health" {
        filtered.sort_by(|a, b| (b.fail * 10 + b.warn).cmp(&(a.fail * 10 + a.warn)));
    } else {
        filtered.sort_by(|a, b| a.host.cmp(&b.host));
    }

    let total = rows.len() as i64;

    rsx! {
        div { style: "flex: 1; overflow: hidden; display: flex; flex-direction: column;",
            // Top issues callout
            if !top_issues.is_empty() {
                div { class: "pm-issues",
                    div { class: "pm-issues-label", "Top issues" }
                    for (name, fail_ct, tot) in top_issues.iter() {
                        {
                            let is_active = policy_filter.read().as_ref() == Some(name);
                            let name_clone = name.clone();
                            let name_clone2 = name.clone();
                            rsx! {
                                button {
                                    key: "{name}",
                                    class: if is_active { "pm-issue-chip active" } else { "pm-issue-chip" },
                                    onclick: move |_| {
                                        let current = policy_filter.read().clone();
                                        if current.as_ref() == Some(&name_clone) {
                                            policy_filter.set(None);
                                        } else {
                                            policy_filter.set(Some(name_clone.clone()));
                                        }
                                    },
                                    span { class: "pm-issue-dot" }
                                    span { class: "mono", "{name_clone2}" }
                                    span { style: "color: #f87171; font-weight: 700;", "{fail_ct}" }
                                    span { style: "color: var(--cf-text-muted);", "/{tot} fail" }
                                }
                            }
                        }
                    }
                    if policy_filter.read().is_some() {
                        button {
                            class: "btn-icon focus-ring",
                            style: "margin-left: auto;",
                            title: "Clear policy filter",
                            onclick: move |_| policy_filter.set(None),
                            Icon { name: IconName::X, size: 12 }
                        }
                    }
                }
            }

            // Controls
            div { class: "pm-controls",
                div { class: "seg",
                    button { class: if filter() == "all" { "active" } else { "" }, onclick: move |_| filter.set("all".to_string()), "All ", span { class: "pm-count", "{total}" } }
                    button { class: if filter() == "fail" { "active" } else { "" }, onclick: move |_| filter.set("fail".to_string()), "Failing ", span { class: "pm-count pm-count-fail", "{fail_count}" } }
                    button { class: if filter() == "warn" { "active" } else { "" }, onclick: move |_| filter.set("warn".to_string()), "Warning ", span { class: "pm-count pm-count-warn", "{warn_count}" } }
                    button { class: if filter() == "clean" { "active" } else { "" }, onclick: move |_| filter.set("clean".to_string()), "Clean ", span { class: "pm-count pm-count-pass", "{clean_count}" } }
                }
                div { style: "flex: 1;" }
                span { style: "font-size: 11px; color: var(--cf-text-muted);", "Sort" }
                div { class: "seg",
                    button { class: if sort() == "health" { "active" } else { "" }, onclick: move |_| sort.set("health".to_string()), "Worst first" }
                    button { class: if sort() == "name" { "active" } else { "" }, onclick: move |_| sort.set("name".to_string()), "Name" }
                }
            }

            // Matrix table
            div { class: "pm-scroll",
                table { class: "pm-table",
                    thead {
                        tr {
                            th { class: "pm-th-host", "System" }
                            th { class: "pm-th-health", "Health" }
                            for (j, policy) in policies.iter().enumerate() {
                                {
                                    let (f, w, p) = policy_stats[j];
                                    let tot = (f + w + p).max(1);
                                    let is_filtered = policy_filter.read().as_ref().map(|pf| pf == *policy).unwrap_or(false);
                                    let policy_name = policy.to_string();
                                    let policy_name2 = policy.to_string();
                                    rsx! {
                                        th {
                                            key: "{policy}",
                                            class: if is_filtered { "pm-th-policy filtered" } else { "pm-th-policy" },
                                            title: "{policy} — {f} fail / {w} warn / {p} pass",
                                            onclick: move |_| {
                                                let current = policy_filter.read().clone();
                                                if current.as_ref() == Some(&policy_name) {
                                                    policy_filter.set(None);
                                                } else {
                                                    policy_filter.set(Some(policy_name.clone()));
                                                }
                                            },
                                            div { class: "pm-th-policy-inner",
                                                span { class: "pm-th-policy-label", "{policy_name2}" }
                                            }
                                            div { class: "pm-th-policy-bar",
                                                div { style: "width: {(f * 100 / tot)}%; background: #f87171;" }
                                                div { style: "width: {(w * 100 / tot)}%; background: #f59e0b;" }
                                                div { style: "width: {(p * 100 / tot)}%; background: #34d399;" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    tbody {
                        for row in filtered.iter() {
                            {
                                let is_exp = expanded.read().as_ref() == Some(&row.host);
                                let health_color = if row.fail > 0 { "#f87171" } else if row.warn > 0 { "#f59e0b" } else { "#34d399" };
                                let host_clone = row.host.clone();
                                let host_clone2 = row.host.clone();
                                let pol_len = policies.len() as i64;
                                rsx! {
                                    tr {
                                        key: "{row.host}",
                                        class: if is_exp { "pm-row expanded" } else { "pm-row" },
                                        onclick: move |_| {
                                            let current = expanded.read().clone();
                                            if current.as_ref() == Some(&host_clone) {
                                                expanded.set(None);
                                            } else {
                                                expanded.set(Some(host_clone.clone()));
                                            }
                                        },
                                        td { class: "pm-td-host",
                                            div { class: "pm-host-cell",
                                                Icon {
                                                    name: if is_exp { IconName::ChevronDown } else { IconName::ChevronRight },
                                                    size: 11
                                                }
                                                span { class: "mono pm-host-name", "{host_clone2}" }
                                            }
                                        }
                                        td { class: "pm-td-health",
                                            div { class: "pm-health",
                                                div { class: "pm-health-bar",
                                                    if row.fail > 0 { div { style: "width: {(row.fail * 100 / pol_len)}%; background: #f87171;" } }
                                                    if row.warn > 0 { div { style: "width: {(row.warn * 100 / pol_len)}%; background: #f59e0b;" } }
                                                    if row.pass > 0 { div { style: "width: {(row.pass * 100 / pol_len)}%; background: #34d399;" } }
                                                }
                                                span { class: "mono pm-health-num", style: "color: {health_color};", "{row.pass}/{pol_len}" }
                                            }
                                        }
                                        for (j, res) in row.results.iter().enumerate() {
                                            {
                                                let res_class = format!("pm-td-cell pm-{}", res);
                                                let is_col_filtered = policy_filter.read().as_ref().map(|pf| pf == policies[j]).unwrap_or(false);
                                                let cell_class = if is_col_filtered {
                                                    format!("{} col-filtered", res_class)
                                                } else {
                                                    res_class
                                                };
                                                let glyph = match res.as_str() {
                                                    "pass" => "✓",
                                                    "warn" => "!",
                                                    _ => "✗",
                                                };
                                                let policy_click = policies[j].to_string();
                                                rsx! {
                                                    td {
                                                        key: "{j}",
                                                        class: "{cell_class}",
                                                        title: "{policies[j]}: {res}",
                                                        onclick: move |e| {
                                                            e.stop_propagation();
                                                            let current = policy_filter.read().clone();
                                                            if current.as_ref() == Some(&policy_click) {
                                                                policy_filter.set(None);
                                                            } else {
                                                                policy_filter.set(Some(policy_click.clone()));
                                                            }
                                                        },
                                                        span { class: "pm-glyph", "{glyph}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if is_exp {
                                        tr { class: "pm-expand-row",
                                            td { colspan: "{policies.len() + 2}",
                                                div { class: "pm-expand",
                                                    div { style: "display: flex; gap: 14px; flex-wrap: wrap;",
                                                        for (j, res) in row.results.iter().enumerate() {
                                                            if res != "pass" {
                                                                {
                                                                    let glyph = if res == "fail" { "✗" } else { "!" };
                                                                    let card_class = format!("pm-failcard pm-failcard-{} focus-ring", res);
                                                                    let policy_name = policies[j];
                                                                    let desc = if res == "fail" { "Blocks deployment until resolved" } else { "Soft warning — deploy will proceed" };
                                                                    rsx! {
                                                                        button {
                                                                            key: "{j}",
                                                                            class: "{card_class}",
                                                                            title: "Open policy: {policy_name}",
                                                                            span { class: "pm-failcard-glyph pm-{res}", "{glyph}" }
                                                                            div { style: "min-width: 0; text-align: left;",
                                                                                div { class: "mono", style: "font-weight: 600; font-size: 12px;", "{policy_name}" }
                                                                                div { style: "font-size: 11px; color: var(--cf-text-muted); margin-top: 2px;", "{desc}" }
                                                                            }
                                                                            Icon { name: IconName::ArrowRight, size: 12 }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        if row.fail == 0 && row.warn == 0 {
                                                            div { style: "font-size: 12px; color: #34d399; display: flex; align-items: center; gap: 8px;",
                                                                Icon { name: IconName::Check, size: 14 }
                                                                " All policies pass for this system."
                                                            }
                                                        }
                                                    }
                                                    div { style: "display: flex; gap: 6px; margin-left: auto; flex-shrink: 0;",
                                                        button { class: "btn btn-ghost focus-ring xs",
                                                            Icon { name: IconName::ArrowRight, size: 11 }
                                                            " Open system"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if filtered.is_empty() {
                            tr {
                                td { colspan: "{policies.len() + 2}", style: "padding: 24px; text-align: center; color: var(--cf-text-muted); font-size: 13px;",
                                    "No systems match this filter."
                                }
                            }
                        }
                    }
                }
            }

            // Legend
            div { class: "pm-legend",
                span { span { class: "pm-legend-sw pm-pass", "✓" } " Pass" }
                span { span { class: "pm-legend-sw pm-warn", "!" } " Warning" }
                span { span { class: "pm-legend-sw pm-fail", "✗" } " Fail — blocks deploy" }
                span { style: "margin-left: auto; font-size: 11px; color: var(--cf-text-muted);", "Click any policy header to filter · Click a row to expand" }
            }
        }
    }
}

#[component]
fn EvalDrawerGraphTab(commit: String, systems: i64, derivations: i64) -> Element {
    // Generate mock package data matching JSX EVAL_DEFAULT_GRAPH
    let packages = vec![
        ("systemd", 48, 2),
        ("openssl", 0, 8),
        ("nginx", 6, 0),
        ("linux-kernel", 1, 0),
        ("python311", 34, 1),
        ("glibc", 12, 0),
        ("audit", 0, 4),
        ("sops-nix", 3, 0),
    ];

    let cached: i64 = packages.iter().map(|(_, c, _)| *c as i64).sum();
    let to_build: i64 = packages.iter().map(|(_, _, b)| *b as i64).sum();
    let total_derivs = cached + to_build;

    rsx! {
        div { style: "flex: 1; overflow: auto; padding: 18px;",
            // Top: source → eval → fanout
            div { class: "ed-graph-summary",
                div { class: "ed-graph-node ed-graph-source",
                    Icon { name: IconName::Git, size: 12 }
                    span { class: "mono", "{commit}" }
                }
                span { style: "color: var(--cf-text-muted);", "→" }
                div { class: "ed-graph-node ed-graph-eval",
                    Icon { name: IconName::Terminal, size: 12 }
                    "eval"
                }
                span { style: "color: var(--cf-text-muted);", "→" }
                div { class: "ed-graph-node ed-graph-fan",
                    span { style: "font-weight: 700;", "{total_derivs}" }
                    span { style: "font-size: 10px; color: var(--cf-text-muted);", "derivations" }
                }
                span { style: "color: var(--cf-text-muted);", "→" }
                div { class: "ed-graph-node ed-graph-fan",
                    span { style: "font-weight: 700; color: #34d399;", "{cached}" }
                    span { style: "font-size: 10px; color: var(--cf-text-muted);", "cached" }
                }
                div { class: "ed-graph-node ed-graph-fan",
                    span { style: "font-weight: 700; color: #60a5fa;", "{to_build}" }
                    span { style: "font-size: 10px; color: var(--cf-text-muted);", "to build" }
                }
            }

            // List of derivations
            div { style: "display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 8px;",
                h3 { style: "font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--cf-text-muted); font-weight: 700; margin: 0;", "Derivations by package" }
                span { style: "font-size: 11px; color: var(--cf-text-muted);", "{packages.len()} packages" }
            }
            div { class: "ed-graph-list",
                for (name, c, b) in packages.iter() {
                    {
                        let total = (*c + *b).max(1) as i64;
                        let cached_pct = (*c as i64 * 100) / total;
                        let build_pct = (*b as i64 * 100) / total;
                        rsx! {
                            div { key: "{name}", class: "ed-graph-row",
                                div { class: "ed-graph-pkg",
                                    span { style: "font-size: 12px; font-weight: 600;", class: "mono truncate", "{name}" }
                                    span { style: "font-size: 10px; color: var(--cf-text-muted);", "{total} derivs" }
                                }
                                div { class: "ed-graph-bar",
                                    div { class: "ed-graph-bar-cached", style: "width: {cached_pct}%;" }
                                    div { class: "ed-graph-bar-build", style: "width: {build_pct}%;" }
                                }
                                div { style: "display: flex; gap: 6px; justify-content: flex-end; font-size: 11px;",
                                    span { style: "color: #34d399; font-weight: 600;", "{c}" }
                                    span { style: "color: var(--cf-text-muted);", "·" }
                                    span { style: "color: #60a5fa; font-weight: 600;", "{b}" }
                                }
                            }
                        }
                    }
                }
            }

            div { class: "ed-graph-legend",
                span { span { class: "ed-graph-sw", style: "background: #34d399;" } "Cached (already in binary cache)" }
                span { span { class: "ed-graph-sw", style: "background: #60a5fa;" } "To build (will fan out to builders)" }
            }
        }
    }
}

// ============================================================================
// Helper types and functions
// ============================================================================

struct PolicyRow {
    host: String,
    results: Vec<String>,
    fail: i64,
    warn: i64,
    pass: i64,
}

struct StatusMeta {
    label: &'static str,
    color: &'static str,
    cls: &'static str,
}

fn eval_status_meta(status: &str) -> StatusMeta {
    let normalized = status.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "in_progress" | "in-progress" => StatusMeta {
            label: "In Progress",
            color: "#60a5fa",
            cls: "chip-info",
        },
        "cancelling" => StatusMeta {
            label: "Cancelling",
            color: "#fbbf24",
            cls: "chip-warning",
        },
        "cancelled" => StatusMeta {
            label: "Cancelled",
            color: "#6b7280",
            cls: "chip-unknown",
        },
        "pending" => StatusMeta {
            label: "Pending",
            color: "#9ca3af",
            cls: "chip-unknown",
        },
        "complete" => StatusMeta {
            label: "Complete",
            color: "#34d399",
            cls: "chip-healthy",
        },
        "failed" => StatusMeta {
            label: "Failed",
            color: "#f87171",
            cls: "chip-critical",
        },
        _ => StatusMeta {
            label: "Unknown",
            color: "#6b7280",
            cls: "chip-unknown",
        },
    }
}

fn is_active_eval_status(status: &str) -> bool {
    let normalized = status.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "pending" | "in_progress" | "in-progress" | "in progress" | "cancelling"
    )
}

fn format_relative_time(dt: chrono::DateTime<chrono::Utc>) -> String {
    let secs = (chrono::Utc::now() - dt).num_seconds();
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

fn format_eval_completed_at(item: &EvalHistoryItem) -> String {
    item.evaluation_completed_at
        .map(|dt| format_relative_time(dt))
        .unwrap_or_else(|| "—".to_string())
}

fn format_eval_duration(item: &EvalHistoryItem) -> String {
    item.evaluation_duration_ms
        .map(|ms| {
            let secs = ms / 1000;
            if secs < 60 {
                format!("{secs}s")
            } else {
                format!("{}m {}s", secs / 60, secs % 60)
            }
        })
        .unwrap_or_else(|| "—".to_string())
}

/// Generate mock log lines matching the JSX EVAL_DEFAULT_LOG pattern
fn generate_eval_log_lines(
    flake: &str,
    commit: &str,
    system_count: i64,
    passed_count: i64,
    policy_failed_count: i64,
    status: &str,
) -> Vec<String> {
    let hosts = ["atlas-01", "gaia-web-02", "orion-db", "helios-edge", "titan-build"];
    let mut lines = vec![
        format!("evaluating flake {}@{}", flake, &commit[..12.min(commit.len())]),
        "loading flake.lock".to_string(),
        "resolving inputs… nixpkgs (locked at 24.11.20260401)".to_string(),
        format!("building eval config for {} systems", system_count),
    ];

    for (i, host) in hosts.iter().take(system_count.min(5) as usize).enumerate() {
        lines.push(format!("  ► evaluating {}.nix", host));
        lines.push("    policy: stig.audit_rules ✓".to_string());
        lines.push("    policy: stig.firewall ✓".to_string());
        lines.push("    policy: cf.heartbeat_interval ✓".to_string());
        lines.push(format!("  ✓ {} evaluated ({} derivations)", host, 18 + (i % 5)));
    }

    match status.to_lowercase().as_str() {
        "complete" => {
            lines.push(format!("policy summary: {} pass / {} fail", passed_count, policy_failed_count));
            lines.push("evaluation complete".to_string());
        }
        "failed" => {
            lines.push("✗ error: attribute 'foo' missing at hosts/atlas-01/services.nix:42:14".to_string());
        }
        "in_progress" | "pending" => {
            lines.push("evaluating package overrides…".to_string());
        }
        _ => {}
    }

    lines
}

/// Determine line color based on content (matches JSX logic)
fn log_line_color(line: &str) -> &'static str {
    let lower = line.to_lowercase();
    if lower.contains("error") || lower.contains("fail") || lower.contains("✗") {
        "#f87171"
    } else if lower.contains("warn") || lower.contains("skip") {
        "#f59e0b"
    } else if lower.contains("ok") || lower.contains("pass") || lower.contains("✓") || lower.contains("complete") {
        "#34d399"
    } else {
        "inherit"
    }
}
