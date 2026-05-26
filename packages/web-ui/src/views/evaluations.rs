//! Evaluations view - rebuilt to match JSX mockup design exactly.

use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use gloo_timers::future::TimeoutFuture;

use crate::api::{
    client::{
        ApiClientError, cancel_commit_evaluation, fetch_eval_dependency_graph, fetch_eval_history,
        fetch_eval_policy_matrix, fetch_eval_queue, force_cancel_commit_evaluation,
        re_evaluate_commit, reorder_eval_queue,
    },
    models::{EvalHistoryItem, EvalHistoryPage, EvalQueueItem},
};
use crate::components::{Icon, IconName};

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
    let mut drawer_target = use_signal(|| None::<EvalDrawerTarget>);
    let mut history_selected_ids = use_signal(std::collections::HashSet::<i32>::new);
    // Keyboard navigation: index into the currently visible list (queue or history)
    let mut focused_index: Signal<Option<usize>> = use_signal(|| None);

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
                style: "display: flex; flex-direction: column; gap: 16px; outline: none;",
                tabindex: 0,
                onkeydown: move |evt| {
                    // No keyboard nav while drawer is open
                    if drawer_target.read().is_some() {
                        if evt.key() == Key::Escape {
                            drawer_target.set(None);
                        }
                        return;
                    }

                    let list_len = if active_tab() == EvaluationsTab::ActiveQueue {
                        active_items.len()
                    } else {
                        history_resource
                            .read()
                            .as_ref()
                            .and_then(|r| r.as_ref().ok())
                            .map(|p| p.items.len())
                            .unwrap_or(0)
                    };

                    match evt.key() {
                        Key::Character(ref c) if c == "j" || c == "ArrowDown" => {
                            let next = match focused_index() {
                                None => 0,
                                Some(i) => (i + 1).min(list_len.saturating_sub(1)),
                            };
                            focused_index.set(Some(next));
                        }
                        Key::ArrowDown => {
                            let next = match focused_index() {
                                None => 0,
                                Some(i) => (i + 1).min(list_len.saturating_sub(1)),
                            };
                            focused_index.set(Some(next));
                        }
                        Key::Character(ref c) if c == "k" => {
                            let next = match focused_index() {
                                None => 0,
                                Some(0) => 0,
                                Some(i) => i - 1,
                            };
                            focused_index.set(Some(next));
                        }
                        Key::ArrowUp => {
                            let next = match focused_index() {
                                None => 0,
                                Some(0) => 0,
                                Some(i) => i - 1,
                            };
                            focused_index.set(Some(next));
                        }
                        Key::Enter => {
                            if let Some(idx) = focused_index() {
                                if active_tab() == EvaluationsTab::ActiveQueue {
                                    if let Some(ev) = active_items.get(idx) {
                                        drawer_target.set(Some(EvalDrawerTarget::Queue(ev.clone())));
                                    }
                                } else {
                                    let item = history_resource
                                        .read()
                                        .as_ref()
                                        .and_then(|r| r.as_ref().ok())
                                        .and_then(|p| p.items.get(idx).cloned());
                                    if let Some(ev) = item {
                                        drawer_target.set(Some(EvalDrawerTarget::History(ev)));
                                    }
                                }
                            }
                        }
                        Key::Character(ref c) if c == "c" => {
                            if active_tab() == EvaluationsTab::ActiveQueue {
                                if let Some(idx) = focused_index() {
                                    if let Some(ev) = active_items.get(idx) {
                                        let commit_id = ev.commit_id;
                                        let can_cancel = matches!(
                                            ev.evaluation_status.as_str(),
                                            "pending" | "in_progress"
                                        );
                                        if can_cancel {
                                            let mut refresh_sig = refresh;
                                            spawn(async move {
                                                let _ = cancel_commit_evaluation(commit_id).await;
                                                refresh_sig.set(refresh_sig() + 1);
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        Key::Escape => {
                            focused_index.set(None);
                        }
                        _ => {}
                    }
                },

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
                            drawer_target: drawer_target,
                            focused_index: focused_index,
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
                            history_selected_ids: history_selected_ids,
                            drawer_target: drawer_target,
                            focused_index: focused_index,
                        }
                    }
                }

                if let Some(target) = drawer_target.read().clone() {
                    EvalDrawer {
                        target: target,
                        refresh: refresh,
                        on_close: move |_| drawer_target.set(None),
                    }
                }

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

#[component]
fn EvalActiveQueue(
    evals: Vec<EvalQueueItem>,
    mut refresh: Signal<u64>,
    queue_items: Signal<Vec<EvalQueueItem>>,
    mut drawer_target: Signal<Option<EvalDrawerTarget>>,
    focused_index: Signal<Option<usize>>,
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
                        let is_focused = focused_index() == Some(i);

                        rsx! {
                            tr {
                                key: "{commit_id}",
                                class: if is_focused { "kbd-focused" } else { "" },
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
                                        // Keep row actions isolated from row-click drawer open.
                                        // Any new nested controls here must preserve stop_propagation.
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
    mut history_selected_ids: Signal<std::collections::HashSet<i32>>,
    mut drawer_target: Signal<Option<EvalDrawerTarget>>,
    focused_index: Signal<Option<usize>>,
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
                            for (row_i, ev) in page_data.items.iter().enumerate() {
                                {
                                    let ev = ev.clone();
                                    let commit_id = ev.commit_id;
                                    let status_meta = eval_status_meta(&ev.evaluation_status);
                                    let ev_for_row = ev.clone();
                                    let is_focused = focused_index() == Some(row_i);

                                    rsx! {
                                        tr {
                                            key: "{commit_id}",
                                            class: if is_focused { "kbd-focused" } else { "" },
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
                                                    // Keep row actions isolated from row-click drawer open.
                                                    // Any new nested controls here must preserve stop_propagation.
                                                    onclick: move |evt| evt.stop_propagation(),
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
) -> Element {
    let mut drawer_tab = use_signal(|| String::from("log"));

    match target {
        EvalDrawerTarget::Queue(ev) => {
            let status_meta = eval_status_meta(&ev.evaluation_status);
            let can_cancel = matches!(ev.evaluation_status.as_str(), "pending" | "in_progress");
            let can_force_cancel = ev.evaluation_status == "cancelling";
            let is_live = can_cancel || can_force_cancel;
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
                                commit_id: ev.commit_id,
                            }
                        } else {
                            EvalDrawerGraphTab {
                                commit_id: ev.commit_id,
                            }
                        }
                    }
                }
            }
        }
        EvalDrawerTarget::History(ev) => {
            let status_meta = eval_status_meta(&ev.evaluation_status);
            let is_live = matches!(
                ev.evaluation_status.as_str(),
                "pending" | "in_progress" | "cancelling"
            );
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
                                commit_id: ev.commit_id,
                            }
                        } else {
                            EvalDrawerGraphTab {
                                commit_id: ev.commit_id,
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
                    }
                }
            }
        }
    }
}

#[component]
fn EvalDrawerLogTabQueue(ev: EvalQueueItem, live: bool) -> Element {
    let mut autoscroll = use_signal(|| true);
    let commit_id = ev.commit_id;
    let mut poll_tick = use_signal(|| 0_u64);

    // When live, poll every 2 s to pick up new log lines
    {
        use_future(move || async move {
            loop {
                #[cfg(target_arch = "wasm32")]
                {
                    gloo_timers::future::TimeoutFuture::new(2000).await;
                    if live {
                        poll_tick.set(poll_tick() + 1);
                    } else {
                        break;
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                break;
            }
        });
    }

    // Re-fetch whenever poll_tick changes (driven by the loop above when live)
    let logs_resource = use_resource(move || async move {
        let _ = poll_tick();
        crate::api::client::fetch_eval_logs(commit_id).await
    });

    let logs_snapshot = logs_resource.read();
    let lines: Vec<String> = match &*logs_snapshot {
        Some(Ok(entries)) => entries
            .iter()
            .map(|e| {
                let level = e
                    .level
                    .as_ref()
                    .map(|s| s.to_uppercase())
                    .unwrap_or_else(|| "INFO".to_string());
                format!(
                    "{} [{}] {}",
                    e.timestamp.format("%Y-%m-%d %H:%M:%S%.3fZ"),
                    level,
                    e.message
                )
            })
            .collect(),
        Some(Err(_)) | None => Vec::new(),
    };
    let is_loading = logs_snapshot.is_none();

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
            pre { class: "fl-diff",
                if is_loading {
                    div { style: "color: var(--cf-text-muted); padding: 12px;", "Loading logs..." }
                } else if lines.is_empty() {
                    div { style: "color: var(--cf-text-muted); padding: 12px;",
                        if live { "Waiting for log output..." } else { "No logs available" }
                    }
                } else {
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
    let commit_id = ev.commit_id;
    let mut poll_tick = use_signal(|| 0_u64);

    // When live (cancelling state), poll every 2 s
    {
        use_future(move || async move {
            loop {
                #[cfg(target_arch = "wasm32")]
                {
                    gloo_timers::future::TimeoutFuture::new(2000).await;
                    if live {
                        poll_tick.set(poll_tick() + 1);
                    } else {
                        break;
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                break;
            }
        });
    }

    // Re-fetch whenever poll_tick changes
    let logs_resource = use_resource(move || async move {
        let _ = poll_tick();
        crate::api::client::fetch_eval_logs(commit_id).await
    });

    let logs_snapshot = logs_resource.read();
    let lines: Vec<String> = match &*logs_snapshot {
        Some(Ok(entries)) => entries
            .iter()
            .map(|e| {
                let level = e
                    .level
                    .as_ref()
                    .map(|s| s.to_uppercase())
                    .unwrap_or_else(|| "INFO".to_string());
                format!(
                    "{} [{}] {}",
                    e.timestamp.format("%Y-%m-%d %H:%M:%S%.3fZ"),
                    level,
                    e.message
                )
            })
            .collect(),
        Some(Err(_)) | None => Vec::new(),
    };
    let is_loading = logs_snapshot.is_none();

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
            pre { class: "fl-diff",
                if is_loading {
                    div { style: "color: var(--cf-text-muted); padding: 12px;", "Loading logs..." }
                } else if lines.is_empty() {
                    div { style: "color: var(--cf-text-muted); padding: 12px;",
                        if live { "Waiting for log output..." } else { "No logs available" }
                    }
                } else {
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
fn EvalDrawerPolicyTab(commit_id: i32) -> Element {
    let policy_resource =
        use_resource(move || async move { fetch_eval_policy_matrix(commit_id).await });
    let policy_snapshot = policy_resource.read();

    rsx! {
        div { style: "flex: 1; overflow: auto; padding: 14px;",
            match &*policy_snapshot {
                None => rsx! {
                    div { style: "color: var(--cf-text-muted); font-size: 12px;", "Loading policy matrix..." }
                },
                Some(Err(_)) => rsx! {
                    div { style: "color: #f87171; font-size: 12px;", "Failed to load policy matrix" }
                },
                Some(Ok(data)) => rsx! {
                    if data.systems.is_empty() {
                        div { style: "color: var(--cf-text-muted); font-size: 12px;", "No policy matrix rows for this commit" }
                    } else {
                        table { class: "pm-table",
                            thead {
                                tr {
                                    th { class: "pm-th-host", "System" }
                                    for policy in data.policies.iter() {
                                        th { class: "pm-th-health", "{policy}" }
                                    }
                                }
                            }
                            tbody {
                                for row in data.systems.iter() {
                                    tr {
                                        td { class: "pm-td-host", div { class: "pm-host-cell", span { class: "mono pm-host-name", "{row.system_name}" } } }
                                        for result in row.results.iter() {
                                            {
                                                let cls = format!("pm-td-cell pm-{}", result);
                                                let glyph = match result.as_str() {
                                                    "pass" => "✓",
                                                    "fail" => "✗",
                                                    _ => "!",
                                                };
                                                rsx! {
                                                    td { class: "{cls}", span { class: "pm-glyph", "{glyph}" } }
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

#[component]
fn EvalDrawerGraphTab(commit_id: i32) -> Element {
    let graph_resource =
        use_resource(move || async move { fetch_eval_dependency_graph(commit_id).await });
    let graph_snapshot = graph_resource.read();

    rsx! {
        div { style: "flex: 1; overflow: auto; padding: 18px;",
            match &*graph_snapshot {
                None => rsx! {
                    div { style: "color: var(--cf-text-muted); font-size: 12px;", "Loading dependency graph..." }
                },
                Some(Err(_)) => rsx! {
                    div { style: "color: #f87171; font-size: 12px;", "Failed to load dependency graph" }
                },
                Some(Ok(data)) => {
                    let systems_total = data.packages.len() as i64;
                    let systems_to_build = data
                        .packages
                        .iter()
                        .filter(|p| p.pending_count > 0)
                        .count() as i64;
                    let systems_built = data
                        .packages
                        .iter()
                        .filter(|p| p.pending_count == 0 && p.ready_count > 0)
                        .count() as i64;
                    let systems_failed = data
                        .packages
                        .iter()
                        .filter(|p| p.failed_count > 0)
                        .count() as i64;
                    let systems_unknown = data
                        .packages
                        .iter()
                        .filter(|p| p.ready_count == 0 && p.pending_count == 0 && p.failed_count == 0)
                        .count() as i64;
                    let commit_short: String = {
                        // We don't have the hash here but we can use the commit_id
                        format!("commit #{}", commit_id)
                    };
                    rsx! {
                        // Summary flow: source → eval → derivations → cached / to-build
                        div { class: "ed-graph-summary",
                            div { class: "ed-graph-node ed-graph-source",
                                Icon { name: IconName::Git, size: 12 }
                                span { class: "mono", "{commit_short}" }
                            }
                            span { style: "color: var(--cf-text-muted);", "→" }
                            div { class: "ed-graph-node ed-graph-eval",
                                Icon { name: IconName::Terminal, size: 12 }
                                "eval"
                            }
                            span { style: "color: var(--cf-text-muted);", "→" }
                            div { class: "ed-graph-node ed-graph-fan",
                                span { style: "font-weight: 700;", "{systems_total}" }
                                span { style: "font-size: 10px; color: var(--cf-text-muted);", "systems" }
                            }
                            span { style: "color: var(--cf-text-muted);", "→" }
                            div { class: "ed-graph-node ed-graph-fan",
                                span { style: "font-weight: 700; color: #34d399;", "{systems_built}" }
                                span { style: "font-size: 10px; color: var(--cf-text-muted);", "built" }
                            }
                            div { class: "ed-graph-node ed-graph-fan",
                                span { style: "font-weight: 700; color: #60a5fa;", "{systems_to_build}" }
                                span { style: "font-size: 10px; color: var(--cf-text-muted);", "to build" }
                            }
                            if systems_failed > 0 {
                                div { class: "ed-graph-node ed-graph-fan",
                                    span { style: "font-weight: 700; color: #f87171;", "{systems_failed}" }
                                    span { style: "font-size: 10px; color: var(--cf-text-muted);", "failed" }
                                }
                            }
                            if systems_unknown > 0 {
                                div { class: "ed-graph-node ed-graph-fan",
                                    span { style: "font-weight: 700; color: #9ca3af;", "{systems_unknown}" }
                                    span { style: "font-size: 10px; color: var(--cf-text-muted);", "unknown" }
                                }
                            }
                        }

                        // Per-system breakdown list
                        div { style: "display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 8px;",
                            h3 { style: "font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--cf-text-muted); font-weight: 700; margin: 0;",
                                "Systems evaluated"
                            }
                            span { style: "font-size: 11px; color: var(--cf-text-muted);",
                                "{data.packages.len()} systems"
                            }
                        }

                        if data.packages.is_empty() {
                            div { style: "color: var(--cf-text-muted); font-size: 12px;", "No derivations recorded for this commit yet." }
                        } else {
                            div { class: "ed-graph-list",
                                for pkg in data.packages.iter() {
                                    {
                                        let row_total = pkg.ready_count + pkg.pending_count + pkg.failed_count;
                                        let has_counts = row_total > 0;
                                        let cached_pct = if has_counts { pkg.ready_count * 100 / row_total } else { 0 };
                                        let build_pct = if has_counts { pkg.pending_count * 100 / row_total } else { 0 };
                                        rsx! {
                                            div { key: "{pkg.package_name}", class: "ed-graph-row",
                                                div { class: "ed-graph-pkg",
                                                    span { class: "mono truncate", style: "font-size: 12px; font-weight: 600;", "{pkg.package_name}" }
                                                    if has_counts {
                                                        span { style: "font-size: 10px; color: var(--cf-text-muted);",
                                                            "{pkg.pending_count} to build / {row_total} total"
                                                        }
                                                    } else {
                                                        span { style: "font-size: 10px; color: #9ca3af;",
                                                            "pending classification"
                                                        }
                                                    }
                                                }
                                                div { class: "ed-graph-bar",
                                                    div { class: "ed-graph-bar-cached", style: "width: {cached_pct}%;" }
                                                    div { class: "ed-graph-bar-build", style: "width: {build_pct}%;" }
                                                }
                                                if has_counts {
                                                    div { style: "display: flex; gap: 6px; justify-content: flex-end; font-size: 11px;",
                                                        span { style: "color: #34d399; font-weight: 600;", "{pkg.ready_count}" }
                                                        span { style: "color: var(--cf-text-muted);", "·" }
                                                        span { style: "color: #60a5fa; font-weight: 600;", "{pkg.pending_count}" }
                                                        if pkg.failed_count > 0 {
                                                            span { style: "color: var(--cf-text-muted);", "·" }
                                                            span { style: "color: #f87171; font-weight: 600;", "{pkg.failed_count}" }
                                                        }
                                                    }
                                                } else {
                                                    div { style: "display: flex; justify-content: flex-end; font-size: 11px; color: #9ca3af;",
                                                        "—"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "ed-graph-legend",
                                span { span { class: "ed-graph-sw", style: "background: #34d399;" } "Built (in store)" }
                                span { span { class: "ed-graph-sw", style: "background: #60a5fa;" } "To build" }
                            }
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

/// Determine line color based on content (matches JSX logic)
fn log_line_color(line: &str) -> &'static str {
    let lower = line.to_lowercase();
    if lower.contains("error") || lower.contains("fail") || lower.contains("✗") {
        "#f87171"
    } else if lower.contains("warn") || lower.contains("skip") {
        "#f59e0b"
    } else if lower.contains("ok")
        || lower.contains("pass")
        || lower.contains("✓")
        || lower.contains("complete")
    {
        "#34d399"
    } else {
        "inherit"
    }
}
