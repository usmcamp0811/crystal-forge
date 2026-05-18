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
                    style: "display: flex; gap: 16px; flex-wrap: wrap; color: var(--cf-text-muted); font-size: 12px;",
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
    match target {
        EvalDrawerTarget::Queue(ev) => {
            let status_meta = eval_status_meta(&ev.evaluation_status);
            let can_cancel = matches!(ev.evaluation_status.as_str(), "pending" | "in_progress");
            let can_force_cancel = ev.evaluation_status == "cancelling";
            let open_logs_ev = ev.clone();
            let close_click = move |evt: MouseEvent| on_close.call(evt);

            rsx! {
                div {
                    class: "side-panel-backdrop",
                    onclick: close_click,
                }
                aside {
                    class: "side-panel",
                    role: "dialog",
                    "aria-label": "Evaluation detail",
                    header {
                        class: "panel-head",
                        div {
                            class: "panel-title",
                            h2 {
                                "{ev.flake_name}"
                                span { class: "chip chip-unknown", style: "font-size: 10px;", "{ev.branch}" }
                                span {
                                    class: "chip {status_meta.cls}",
                                    span { class: "chip-dot", style: "background: {status_meta.color};" }
                                    "{status_meta.label}"
                                }
                            }
                            div {
                                class: "fqdn",
                                "{ev.commit_hash} · {ev.commit_id}"
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
                        class: "panel-body",
                        div {
                            class: "panel-section",
                            h3 { "Evaluation details" }
                            dl {
                                class: "kv-grid",
                                dt { "Started" }
                                dd { "{format_relative_time(ev.committed_at)}" }
                                dt { "Systems" }
                                dd { "{ev.system_count}" }
                                dt { "Policy pass" }
                                dd { "{ev.passed_count}" }
                                dt { "Policy fail" }
                                dd { "{ev.policy_failed_count}" }
                                dt { "Queue position" }
                                dd { "{ev.queue_position}" }
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
            let open_logs_ev = ev.clone();
            let close_click = move |evt: MouseEvent| on_close.call(evt);

            rsx! {
                div {
                    class: "side-panel-backdrop",
                    onclick: close_click,
                }
                aside {
                    class: "side-panel",
                    role: "dialog",
                    "aria-label": "Evaluation detail",
                    header {
                        class: "panel-head",
                        div {
                            class: "panel-title",
                            h2 {
                                "{ev.flake_name}"
                                span { class: "chip chip-unknown", style: "font-size: 10px;", "{ev.branch}" }
                                span {
                                    class: "chip {status_meta.cls}",
                                    span { class: "chip-dot", style: "background: {status_meta.color};" }
                                    "{status_meta.label}"
                                }
                            }
                            div {
                                class: "fqdn",
                                "{ev.commit_hash} · {ev.commit_id}"
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
                        class: "panel-body",
                        div {
                            class: "panel-section",
                            h3 { "Evaluation details" }
                            dl {
                                class: "kv-grid",
                                dt { "Completed" }
                                dd { "{format_eval_completed_at(&ev)}" }
                                dt { "Duration" }
                                dd { "{format_eval_duration(&ev)}" }
                                dt { "Systems" }
                                dd { "{ev.system_count}" }
                                dt { "Policy pass" }
                                dd { "{ev.passed_count}" }
                                dt { "Policy fail" }
                                dd { "{ev.policy_failed_count}" }
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

// ============================================================================
// Helper types and functions
// ============================================================================

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
        format!("{secs}s ago")
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
