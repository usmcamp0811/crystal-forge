//! Evaluations view - rebuilt to match JSX mockup design exactly.

use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use gloo_timers::future::TimeoutFuture;

use crate::alerts::{
    NAV_BADGES, acknowledge_with_cursor_and_ids_async, attention_row_class, dismiss_attention_item,
    should_flash,
};

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
    // Keyboard navigation: index into the currently visible list (queue or history).
    // Start at 0 so the first row is auto-selected when the page loads (matching JSX).
    let mut focused_index: Signal<Option<usize>> = use_signal(|| None);

    // Active queue multi-select (bulk cancel)
    let mut active_selected_ids = use_signal(std::collections::HashSet::<i32>::new);

    // Toast state for soft-cancel undo
    let mut toast_msg = use_signal(|| None::<String>);

    // History tab state
    let mut history_page = use_signal(|| 1_i64);
    let mut history_status_filter = use_signal(|| String::from("all"));
    let mut history_flake_filter = use_signal(|| String::from("all"));
    let mut history_auto_selected = use_signal(|| false);
    let mut history_ack_cursor = use_signal(|| None::<String>);
    let mut evals_ack_sent = use_signal(|| false);

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

    // Auto-select all history items on first successful data load.
    use_effect(move || {
        let already = *history_auto_selected.read();
        if let Some(Ok(page_data)) = &*history_resource.read() {
            history_ack_cursor.set(NAV_BADGES.read_unchecked().observed_at.clone());
            if !already {
                let ids: std::collections::HashSet<i32> =
                    page_data.items.iter().map(|item| item.commit_id).collect();
                if !ids.is_empty() {
                    history_selected_ids.set(ids);
                    history_auto_selected.set(true);
                }
            }
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
    use_effect(move || {
        if active_tab() == EvaluationsTab::History && !evals_ack_sent() {
            if let Some(Ok(page_data)) = history_resource.read().as_ref() {
                let unfiltered_first_page = history_page() == 1
                    && history_status_filter() == "all"
                    && history_flake_filter() == "all";
                let complete_page = page_data.total_count <= page_data.items.len() as i64;
                if unfiltered_first_page && complete_page {
                    let history_failed_count = page_data
                        .items
                        .iter()
                        .filter(|item| item.evaluation_status == "failed")
                        .count() as i64;
                    let Some(cursor) = history_ack_cursor.read().clone() else {
                        return;
                    };
                    let alert_ids = page_data
                        .items
                        .iter()
                        .filter(|item| item.evaluation_status == "failed")
                        .map(|item| item.commit_id.to_string())
                        .collect::<Vec<_>>();
                    spawn(async move {
                        if acknowledge_with_cursor_and_ids_async(
                            "evals",
                            history_failed_count,
                            cursor,
                            None,
                            Some(alert_ids),
                        )
                        .await
                        {
                            evals_ack_sent.set(true);
                        }
                    });
                }
            }
        }
    });
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

    // Server-computed "new failed evaluations since last acknowledgment"
    // (persists across page refresh/re-login — see alerts::NAV_BADGES).
    // Drives both the History tab's badge/attention-flash-tab pulse and the
    // one-shot row flash below, replacing a raw total that would otherwise
    // reappear identically on every reload. should_flash still guards the
    // one-shot-per-page-load timing (safe: reads/writes ALERT_STATE, not a
    // signal read inside this same effect).
    let evals_failed_new = NAV_BADGES().evals_failed_new;
    let mut flash_evals_signal = use_signal(|| false);
    let flash_evals = flash_evals_signal();
    use_effect(move || {
        if should_flash("evals", evals_failed_new > 0) {
            flash_evals_signal.set(true);
            spawn(async move {
                gloo_timers::future::TimeoutFuture::new(3200).await;
                flash_evals_signal.set(false);
            });
        }
    });

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
                                            let mut toast = toast_msg;
                                            spawn(async move {
                                                if let Err(e) = cancel_commit_evaluation(commit_id).await {
                                                    toast.set(Some(format!("Cancel failed: {}", e)));
                                                } else {
                                                    refresh_sig.set(refresh_sig() + 1);
                                                }
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

                // Page head (matching JSX: title, subtitle, LiveIndicator)
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
                        style: "display: flex; gap: 12px; align-items: center;",
                        LiveIndicator {}
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
                            onclick: move |_| { active_tab.set(EvaluationsTab::ActiveQueue); focused_index.set(None); },
                            "Active Queue "
                            span { class: "sd-tab-badge", "{active_count}" }
                        }
                        button {
                            class: if active_tab() == EvaluationsTab::History {
                                "sd-tab focus-ring active"
                            } else if evals_failed_new > 0 {
                                "sd-tab focus-ring attention-flash-tab"
                            } else {
                                "sd-tab focus-ring"
                            },
                            onclick: move |_| {
                                active_tab.set(EvaluationsTab::History);
                                focused_index.set(None);
                                // Acknowledge the "evals" sidebar/tab badge when History tab
                                // is opened (persists server-side — TASK-385 follow-up).
                                evals_ack_sent.set(false);
                            },
                            "History"
                            if evals_failed_new > 0 {
                                span { class: "sd-tab-badge", "{evals_failed_new}" }
                            }
                        }
                    }

                    if active_tab() == EvaluationsTab::ActiveQueue {
                        EvalActiveQueue {
                            evals: active_items.clone(),
                            refresh: refresh,
                            queue_items: queue_items,
                            drawer_target: drawer_target,
                            focused_index: focused_index,
                            active_selected_ids: active_selected_ids,
                            toast_msg: toast_msg,
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
                                div { style: "flex: 1;" }
                                button {
                                    class: "btn btn-ghost focus-ring xs",
                                    onclick: move |_| {
                                        let selected_ids: Vec<i32> = history_selected_ids.read().iter().copied().collect();
                                        let mut refresh_sig = refresh.clone();
                                        let mut selected_sig = history_selected_ids.clone();
                                        let mut toast = toast_msg.clone();
                                        spawn(async move {
                                            let mut success = 0u32;
                                            let mut failed: Vec<i32> = Vec::new();
                                            for commit_id in selected_ids {
                                                match re_evaluate_commit(commit_id).await {
                                                    Ok(_) => success += 1,
                                                    Err(_) => failed.push(commit_id),
                                                }
                                            }
                                            if success > 0 {
                                                toast.set(Some(format!("Re-queued {} evaluation{}", success, if success == 1 { "" } else { "s" })));
                                                selected_sig.set(failed.into_iter().collect());
                                            } else {
                                                toast.set(Some("Re-evaluate failed — see server logs".to_string()));
                                            }
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
                            flash_evals: flash_evals,
                        }
                    }
                }

                if let Some(target) = drawer_target.read().clone() {
                    EvalDrawer {
                        target: target,
                        refresh: refresh,
                        on_close: move |_| drawer_target.set(None),
                        toast_msg: toast_msg,
                    }
                }

                // Bulk cancel bar for active queue multi-select (matching JSX BulkBar)
                if active_tab() == EvaluationsTab::ActiveQueue && !active_selected_ids.read().is_empty() {
                    div {
                        class: "ed-bulkbar",
                        span {
                            style: "font-size: 13px; font-weight: 600;",
                            "{active_selected_ids.read().len()} selected"
                        }
                        div { style: "flex: 1;" }
                        button {
                            class: "btn btn-danger xs focus-ring",
                            onclick: move |_| {
                                let ids: Vec<i32> = active_selected_ids.read().iter().copied().collect();
                                let mut refresh_sig = refresh.clone();
                                let mut selected_sig = active_selected_ids.clone();
                                let mut toast = toast_msg.clone();
                                spawn(async move {
                                    let mut success = 0u32;
                                    let mut failed = Vec::new();
                                    for commit_id in &ids {
                                        match cancel_commit_evaluation(*commit_id).await {
                                            Ok(_) => success += 1,
                                            Err(e) => failed.push((*commit_id, e.to_string())),
                                        }
                                    }
                                    if success > 0 {
                                        toast.set(Some(format!("Cancelled {} evaluation{}", success, if success == 1 { "" } else { "s" })));
                                        selected_sig.set(failed.iter().map(|(id, _)| *id).collect());
                                    } else {
                                        toast.set(Some("Failed to cancel evaluations — see server logs".to_string()));
                                    }
                                    refresh_sig.set(refresh_sig() + 1);
                                });
                            },
                            Icon { name: IconName::X, size: 12 }
                            {
                                let n = active_selected_ids.read().len();
                                format!(" Cancel {} eval{}", n, if n == 1 { "" } else { "s" })
                            }
                        }
                    }
                }

                // Toast notification (matching JSX ed-toast)
                if let Some(msg) = toast_msg.read().as_ref() {
                    div {
                        class: "ed-toast",
                        span { style: "color: #34d399;", Icon { name: IconName::Check, size: 14 } }
                        span { "{msg}" }
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
    mut active_selected_ids: Signal<std::collections::HashSet<i32>>,
    mut toast_msg: Signal<Option<String>>,
) -> Element {
    // Drag-and-drop reorder state (matching JSX dragId/overIdx)
    let mut drag_id = use_signal(|| None::<i32>);
    let mut over_idx = use_signal(|| None::<usize>);

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
            class: "sys-table q-queue-table",
            thead {
                tr {
                    th { style: "width: 64px;", "#" }
                    th { "Flake · commit" }
                    th { "Branch" }
                    th { "Status" }
                    th { "Systems" }
                    th { "Policy" }
                    th { "Started" }
                    th { style: "text-align: right;", "Reorder · actions" }
                }
            }
            tbody {
                for (i , ev) in evals.iter().enumerate() {
                    {
                        let ev_clone = ev.clone();
                        let commit_id = ev.commit_id;
                        let status_meta = eval_status_meta(&ev.evaluation_status);
                        let can_cancel = matches!(ev.evaluation_status.as_str(), "pending" | "in_progress");
                        let is_first = i == 0;
                        let is_last = i == evals.len() - 1;
                        let ev_for_row = ev_clone.clone();
                        let is_focused = focused_index() == Some(i);
                        let checked = active_selected_ids.read().contains(&commit_id);
                        let is_dragging = drag_id() == Some(commit_id);
                        let drag_idx = drag_id().and_then(|id| evals.iter().position(|e| e.commit_id == id));
                        let show_drop_before = drag_id().is_some() && over_idx() == Some(i) && drag_idx.map(|d| d > i).unwrap_or(false);
                        let show_drop_after = drag_id().is_some() && over_idx() == Some(i) && drag_idx.map(|d| d < i).unwrap_or(false);

                        let mut row_draw = drawer_target.clone();
                        let row_ev = ev_for_row.clone();
                        let row_onclick = move |_| {
                            row_draw.set(Some(EvalDrawerTarget::Queue(row_ev.clone())));
                        };

                        let row_classes = format!(
                            "selectable q-row{}{}{}{}{}",
                            if is_focused { " selected" } else { "" },
                            if checked { " row-checked" } else { "" },
                            if is_dragging { " q-dragging" } else { "" },
                            if show_drop_before { " q-drop-before" } else { "" },
                            if show_drop_after { " q-drop-after" } else { "" },
                        );

                        rsx! {
                            tr {
                                key: "{commit_id}",
                                class: "{row_classes}",
                                draggable: "true",
                                ondragstart: move |_| { drag_id.set(Some(commit_id)); },
                                ondragover: move |evt| {
                                    evt.prevent_default();
                                    if over_idx() != Some(i) { over_idx.set(Some(i)); }
                                },
                                ondrop: move |evt| {
                                    evt.prevent_default();
                                    let src_id = drag_id();
                                    if let Some(src_id) = src_id {
                                        let target_cid = commit_id;
                                        if src_id == target_cid { return; }
                                        let items = queue_items.clone();
                                        let mut refresh_sig = refresh.clone();
                                        let mut drag_sig = drag_id.clone();
                                        let mut over_sig = over_idx.clone();
                                        let mut toast = toast_msg.clone();
                                        spawn(async move {
                                            let active: Vec<_> = items.read().iter()
                                                .filter(|item| is_active_eval_status(&item.evaluation_status))
                                                .cloned().collect();
                                            let src_pos = active.iter().position(|e| e.commit_id == src_id);
                                            let target_pos = active.iter().position(|e| e.commit_id == target_cid);
                                            if let (Some(sp), Some(tp)) = (src_pos, target_pos) {
                                                let mut reordered = active.clone();
                                                let removed = reordered.remove(sp);
                                                let adjusted_tp = if sp < tp { tp - 1 } else { tp };
                                                reordered.insert(adjusted_tp, removed);
                                                let ordered_ids: Vec<i32> = reordered.iter().map(|e| e.commit_id).collect();
                                                if let Err(e) = reorder_eval_queue(&ordered_ids).await {
                                                    toast.set(Some(format!("Reorder failed: {}", e)));
                                                } else {
                                                    refresh_sig.set(refresh_sig() + 1);
                                                }
                                            }
                                        });
                                        drag_sig.set(None);
                                        over_sig.set(None);
                                    }
                                },
                                onclick: row_onclick,
                                td {
                                    onclick: move |evt| evt.stop_propagation(),
                                    div { style: "display: flex; align-items: center; gap: 6px;",
                                        span {
                                            class: "q-drag-handle",
                                            title: "Drag to reorder",
                                            Icon { name: IconName::Grip, size: 15 }
                                        }
                                        span {
                                            style: "color: var(--cf-text-muted); font-size: 12px; font-variant-numeric: tabular-nums;",
                                            "{ev.queue_position}"
                                        }
                                    }
                                }
                                td {
                                    div { style: "font-weight: 600; font-size: 13px;", "{ev_clone.flake_name}" }
                                    div { class: "mono", style: "font-size: 11px; color: var(--cf-text-muted);",
                                        "{ev_clone.commit_hash.chars().take(12).collect::<String>()}"
                                    }
                                }
                                td { span { class: "chip chip-unknown", "{ev_clone.branch}" } }
                                td {
                                    span {
                                        class: "chip {status_meta.cls}",
                                        span { class: "chip-dot", style: "background: {status_meta.color};" }
                                        "{status_meta.label}"
                                    }
                                }
                                td { style: "font-size: 12px; color: var(--cf-text-secondary);", "{ev_clone.system_count} hosts" }
                                td {
                                    div { style: "display: flex; gap: 6px;",
                                        span { class: "chip chip-healthy", "{ev_clone.passed_count} ✓" }
                                        if ev_clone.policy_failed_count > 0 {
                                            span { class: "chip chip-critical", "{ev_clone.policy_failed_count} ✗" }
                                        }
                                    }
                                }
                                td { style: "font-size: 12px; color: var(--cf-text-muted);", "{format_relative_time(ev_clone.committed_at)}" }
                                td {
                                    onclick: move |evt| evt.stop_propagation(),
                                    div {
                                        class: "row-actions",
                                        style: "opacity: 1; gap: 6px; justify-content: flex-end;",
                                        div { class: "q-move-group",
                                            button {
                                                class: "q-move-btn focus-ring",
                                                title: "Move up",
                                                disabled: is_first,
                                                onclick: move |_| {
                                                    if is_first { return; }
                                                    let items = queue_items.clone();
                                                    let mut refresh_sig = refresh.clone();
                                                    let mut toast = toast_msg.clone();
                                                    spawn(async move {
                                                        let active: Vec<_> = items.read().iter()
                                                            .filter(|item| is_active_eval_status(&item.evaluation_status))
                                                            .cloned().collect();
                                                        if let Some(idx) = active.iter().position(|e| e.commit_id == commit_id) {
                                                            if idx > 0 {
                                                                let mut reordered = active;
                                                                let removed = reordered.remove(idx);
                                                                reordered.insert(idx - 1, removed);
                                                                let ordered_ids: Vec<i32> = reordered.iter().map(|e| e.commit_id).collect();
                                                                if let Err(e) = reorder_eval_queue(&ordered_ids).await {
                                                                    toast.set(Some(format!("Reorder failed: {}", e)));
                                                                } else {
                                                                    refresh_sig.set(refresh_sig() + 1);
                                                                }
                                                            }
                                                        }
                                                    });
                                                },
                                                Icon { name: IconName::ChevronUp, size: 15 }
                                            }
                                            button {
                                                class: "q-move-btn focus-ring",
                                                title: "Move down",
                                                disabled: is_last,
                                                onclick: move |_| {
                                                    if is_last { return; }
                                                    let items = queue_items.clone();
                                                    let mut refresh_sig = refresh.clone();
                                                    let mut toast = toast_msg.clone();
                                                    spawn(async move {
                                                        let active: Vec<_> = items.read().iter()
                                                            .filter(|item| is_active_eval_status(&item.evaluation_status))
                                                            .cloned().collect();
                                                        if let Some(idx) = active.iter().position(|e| e.commit_id == commit_id) {
                                                            if idx + 1 < active.len() {
                                                                let mut reordered = active;
                                                                let removed = reordered.remove(idx);
                                                                reordered.insert(idx, removed);
                                                                let ordered_ids: Vec<i32> = reordered.iter().map(|e| e.commit_id).collect();
                                                                if let Err(e) = reorder_eval_queue(&ordered_ids).await {
                                                                    toast.set(Some(format!("Reorder failed: {}", e)));
                                                                } else {
                                                                    refresh_sig.set(refresh_sig() + 1);
                                                                }
                                                            }
                                                        }
                                                    });
                                                },
                                                Icon { name: IconName::ChevronDown, size: 15 }
                                            }
                                        }
                                        if can_cancel {
                                            button {
                                                class: "btn btn-ghost focus-ring",
                                                style: "padding: 3px 8px; font-size: 11px;",
                                                onclick: move |_| {
                                                    let mut refresh_sig = refresh.clone();
                                                    let mut toast = toast_msg.clone();
                                                    spawn(async move {
                                                        if let Err(e) = cancel_commit_evaluation(commit_id).await {
                                                            toast.set(Some(format!("Cancel failed: {}", e)));
                                                        } else {
                                                            refresh_sig.set(refresh_sig() + 1);
                                                        }
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
    flash_evals: bool,
) -> Element {
    let history_snapshot = history_resource.read();

    rsx! {
        div {
            // Filter bar
            div {
                style: "padding: 12px 16px; border-bottom: 1px solid var(--cf-divider); display: flex; gap: 10px; flex-wrap: wrap; align-items: center;",

                div {
                    class: "seg",
                    for (label, value) in [("all", "all"), ("complete", "complete"), ("failed", "failed"), ("cancelled", "cancelled")] {
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

                                    let is_failed = ev.evaluation_status == "failed";
                                    // Include evaluation_completed_at epoch so
                                    // a commit that is re-evaluated and fails
                                    // again generates a new dismissal key.
                                    let eval_key = format!(
                                        "{}:{}",
                                        commit_id,
                                        ev.evaluation_completed_at
                                            .map(|t| t.timestamp().to_string())
                                            .unwrap_or_default()
                                    );
                                    let row_class = attention_row_class(
                                        if is_focused { "kbd-focused" } else { "" },
                                        "evals",
                                        &eval_key,
                                        is_failed,
                                        is_failed && flash_evals,
                                    );

                                    rsx! {
                                        tr {
                                            key: "{commit_id}",
                                            class: "{row_class}",
                                            style: "cursor: pointer;",
                                            onclick: move |_| {
                                                if is_failed {
                                                    dismiss_attention_item("evals", &eval_key);
                                                }
                                                drawer_target.set(Some(EvalDrawerTarget::History(ev_for_row.clone())));
                                            },
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
    mut toast_msg: Signal<Option<String>>,
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
                                        let mut toast = toast_msg.clone();
                                        let commit_id = ev.commit_id;
                                        spawn(async move {
                                            if let Err(e) = cancel_commit_evaluation(commit_id).await {
                                                toast.set(Some(format!("Cancel failed: {}", e)));
                                            } else {
                                                refresh_sig.set(refresh_sig() + 1);
                                            }
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
                                        let mut toast = toast_msg.clone();
                                        let commit_id = ev.commit_id;
                                        spawn(async move {
                                            if let Err(e) = force_cancel_commit_evaluation(commit_id).await {
                                                toast.set(Some(format!("Force cancel failed: {}", e)));
                                            } else {
                                                refresh_sig.set(refresh_sig() + 1);
                                            }
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
                                    let mut toast = toast_msg.clone();
                                    let commit_id = ev.commit_id;
                                    spawn(async move {
                                        if let Err(e) = re_evaluate_commit(commit_id).await {
                                            toast.set(Some(format!("Re-evaluate failed: {}", e)));
                                        } else {
                                            refresh_sig.set(refresh_sig() + 1);
                                        }
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

    // Interactive state: filter, sort, expanded row, policy filter (matches JSX)
    let mut filter_state = use_signal(|| "all".to_string());
    let mut sort_state = use_signal(|| "health".to_string());
    let mut expanded = use_signal(|| None::<String>);
    let mut policy_filter = use_signal(|| None::<String>);

    rsx! {
        div {
            style: "flex: 1; overflow: hidden; display: flex; flex-direction: column;",
            match &*policy_snapshot {
                None => rsx! {
                    div { style: "color: var(--cf-text-muted); font-size: 12px; padding: 14px;", "Loading policy matrix..." }
                },
                Some(Err(_)) => rsx! {
                    div { style: "color: #f87171; font-size: 12px; padding: 14px;", "Failed to load policy matrix" }
                },
                Some(Ok(data)) => {
                    let policies = &data.policies;
                    let base_rows = &data.systems;

                    // Annotate rows with counts (matching JSX annotated)
                    struct AnnotatedRow {
                        system_name: String,
                        results: Vec<String>,
                        fail: usize,
                        warn: usize,
                        pass: usize,
                    }

                    let annotated: Vec<AnnotatedRow> = base_rows.iter().map(|r| {
                        let fail = r.results.iter().filter(|x| *x == "fail").count();
                        let warn = r.results.iter().filter(|x| *x == "warn").count();
                        let pass = r.results.iter().filter(|x| *x == "pass").count();
                        AnnotatedRow {
                            system_name: r.system_name.clone(),
                            results: r.results.clone(),
                            fail,
                            warn,
                            pass,
                        }
                    }).collect();

                    // Apply filter (matching JSX)
                    let filtered = {
                        let f = filter_state.read().clone();
                        let pf = policy_filter.read().clone();
                        let mut result: Vec<&AnnotatedRow> = annotated.iter().collect();
                        match f.as_str() {
                            "fail" => result.retain(|r| r.fail > 0),
                            "warn" => result.retain(|r| r.warn > 0 && r.fail == 0),
                            "clean" => result.retain(|r| r.fail == 0 && r.warn == 0),
                            _ => {}
                        }
                        if let Some(ref policy_name) = pf {
                            if let Some(idx) = policies.iter().position(|p| p == policy_name) {
                                result.retain(|r| r.results.get(idx).map_or(false, |res| res != "pass"));
                            }
                        }
                        // Sort
                        let sort = sort_state.read().clone();
                        if sort == "health" {
                            result.sort_by(|a, b| (b.fail * 10 + b.warn).cmp(&(a.fail * 10 + a.warn)));
                        } else {
                            result.sort_by(|a, b| a.system_name.cmp(&b.system_name));
                        }
                        result
                    };

                    // Per-policy summary (matching JSX policyStats)
                    struct PolicyStat {
                        name: String,
                        fail: usize,
                        warn: usize,
                        pass: usize,
                        total: usize,
                    }
                    let policy_stats: Vec<PolicyStat> = policies.iter().enumerate().map(|(i, name)| {
                        let fail = annotated.iter().filter(|r| r.results.get(i).map_or(false, |x| x == "fail")).count();
                        let warn = annotated.iter().filter(|r| r.results.get(i).map_or(false, |x| x == "warn")).count();
                        let pass = annotated.iter().filter(|r| r.results.get(i).map_or(false, |x| x == "pass")).count();
                        PolicyStat { name: name.clone(), fail, warn, pass, total: annotated.len() }
                    }).collect();

                    // Top issues — top 3 most-failed policies (matching JSX)
                    let top_issues: Vec<&PolicyStat> = policy_stats.iter()
                        .filter(|s| s.fail > 0)
                        .collect::<Vec<_>>();

                    let top_issues_sorted = {
                        let mut v = top_issues.clone();
                        v.sort_by(|a, b| b.fail.cmp(&a.fail));
                        v.into_iter().take(3).collect::<Vec<_>>()
                    };

                    // Counts for filter badges
                    let count_fail = annotated.iter().filter(|r| r.fail > 0).count();
                    let count_warn = annotated.iter().filter(|r| r.fail == 0 && r.warn > 0).count();
                    let count_clean = annotated.iter().filter(|r| r.fail == 0 && r.warn == 0).count();

                    let cell_glyph = |res: &str| -> &'static str {
                        match res { "pass" => "✓", "warn" => "!", _ => "✗" }
                    };

                    rsx! {
                        // Top issues callout (matching JSX)
                        if !top_issues_sorted.is_empty() {
                            div {
                                class: "pm-issues",
                                div { class: "pm-issues-label", "Top issues" }
                                {top_issues_sorted.iter().map(|iss| {
                                    let is_active = policy_filter.read().as_ref().map(|f| f == &iss.name).unwrap_or(false);
                                    let name = iss.name.clone();
                                    let mut pf = policy_filter.clone();
                                    let click_iss = move |_| {
                                        if pf.read().as_ref().map(|f| f == &name).unwrap_or(false) {
                                            pf.set(None);
                                        } else {
                                            pf.set(Some(name.clone()));
                                        }
                                    };
                                    let counts = format!("{}+{}+{}", iss.fail, iss.warn, iss.pass);
                                    rsx! {
                                        button {
                                            key: "{iss.name}",
                                            class: if is_active { "pm-issue-chip active" } else { "pm-issue-chip" },
                                            onclick: click_iss,
                                            "{iss.name} ({counts})"
                                        }
                                    }
                                })}
                                {if policy_filter.read().is_some() {
                                    Some(rsx! {
                                        button {
                                            class: "btn-icon focus-ring",
                                            style: "margin-left: auto;",
                                            title: "Clear policy filter",
                                            onclick: move |_| policy_filter.set(None),
                                            Icon { name: IconName::X, size: 12 }
                                        }
                                    })
                                } else {
                                    None
                                }}
                            }
                        }

                        // Controls: filter seg + sort seg (matching JSX)
                        div {
                            class: "pm-controls",
                            div {
                                class: "seg",
                                {
                                    let f_all = move |_| { filter_state.set("all".to_string()); };
                                    let f_fail = move |_| { filter_state.set("fail".to_string()); };
                                    let f_warn = move |_| { filter_state.set("warn".to_string()); };
                                    let f_clean = move |_| { filter_state.set("clean".to_string()); };
                                    let f_cur = filter_state.read().clone();
                                    rsx! {
                                        button { class: if f_cur == "all" { "active" } else { "" }, onclick: f_all, "All ", span { class: "pm-count", "{annotated.len()}" } }
                                        button { class: if f_cur == "fail" { "active" } else { "" }, onclick: f_fail, "Failing ", span { class: "pm-count pm-count-fail", "{count_fail}" } }
                                        button { class: if f_cur == "warn" { "active" } else { "" }, onclick: f_warn, "Warning ", span { class: "pm-count pm-count-warn", "{count_warn}" } }
                                        button { class: if f_cur == "clean" { "active" } else { "" }, onclick: f_clean, "Clean ", span { class: "pm-count pm-count-pass", "{count_clean}" } }
                                    }
                                }
                            }
                            div { style: "flex: 1;" }
                            span { style: "font-size: 11px; color: var(--cf-text-muted);", "Sort" }
                            {
                                let s_health = move |_| { sort_state.set("health".to_string()); };
                                let s_name = move |_| { sort_state.set("name".to_string()); };
                                let s_cur = sort_state.read().clone();
                                rsx! {
                                    div { class: "seg",
                                        button { class: if s_cur == "health" { "active" } else { "" }, onclick: s_health, "Worst first" }
                                        button { class: if s_cur == "name" { "active" } else { "" }, onclick: s_name, "Name" }
                                    }
                                }
                            }
                        }

                        // Scrollable matrix table (matching JSX pm-scroll > pm-table)
                        div {
                            class: "pm-scroll",
                            table { class: "pm-table",
                                thead {
                                    tr {
                                        th { class: "pm-th-host", "System" }
                                        th { class: "pm-th-health", "Health" }
                                        for (p_idx, policy) in policies.iter().enumerate() {
                                            {
                                                let ps = &policy_stats[p_idx];
                                                let is_filtered = policy_filter.read().as_ref() == Some(policy);
                                                let click_header = {
                                                    let mut pf = policy_filter.clone();
                                                    let name = policy.clone();
                                                    move |_| {
                                                        if pf.read().as_ref() == Some(&name) {
                                                            pf.set(None);
                                                        } else {
                                                            pf.set(Some(name.clone()));
                                                        }
                                                    }
                                                };
                                                let fail_pct = if ps.total > 0 { (ps.fail as f64 / ps.total as f64) * 100.0 } else { 0.0 };
                                                let warn_pct = if ps.total > 0 { (ps.warn as f64 / ps.total as f64) * 100.0 } else { 0.0 };
                                                let pass_pct = if ps.total > 0 { (ps.pass as f64 / ps.total as f64) * 100.0 } else { 0.0 };
                                                rsx! {
                                                    th {
                                                        key: "{policy}",
                                                        class: if is_filtered { "pm-th-policy filtered" } else { "pm-th-policy" },
                                                        title: "{policy} — {ps.fail} fail / {ps.warn} warn / {ps.pass} pass",
                                                        onclick: click_header,
                                                        div { class: "pm-th-policy-inner",
                                                            span { class: "pm-th-policy-label", "{policy}" }
                                                        }
                                                        div { class: "pm-th-policy-bar",
                                                            if ps.fail > 0 { div { style: "width: {fail_pct}%; background: #f87171;" } }
                                                            if ps.warn > 0 { div { style: "width: {warn_pct}%; background: #f59e0b;" } }
                                                            if ps.pass > 0 { div { style: "width: {pass_pct}%; background: #34d399;" } }
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
                                            let is_exp = expanded.read().as_ref() == Some(&row.system_name);
                                            let health_color = if row.fail > 0 { "#f87171" } else if row.warn > 0 { "#f59e0b" } else { "#34d399" };
                                            let host = row.system_name.clone();
                                            let click_row = {
                                                let mut exp = expanded.clone();
                                                let h = host.clone();
                                                move |_| {
                                                    if exp.read().as_ref() == Some(&h) {
                                                        exp.set(None);
                                                    } else {
                                                        exp.set(Some(h.clone()));
                                                    }
                                                }
                                            };
                                            let row_class = format!("pm-row{}", if is_exp { " expanded" } else { "" });
                                            rsx! {
                                                tr {
                                                    key: "{host}",
                                                    class: "{row_class}",
                                                    onclick: click_row,
                                                    style: "cursor: pointer;",
                                                    td { class: "pm-td-host",
                                                        div { class: "pm-host-cell",
                                                            span { style: "color: var(--cf-text-muted); flex-shrink: 0; display: flex;",
                                                                Icon {
                                                                    name: if is_exp { IconName::ChevronDown } else { IconName::ChevronRight },
                                                                    size: 11,
                                                                }
                                                            }
                                                            span { class: "mono pm-host-name", "{row.system_name}" }
                                                        }
                                                    }
                                                    td { class: "pm-td-health",
                                                        div { class: "pm-health",
                                                            div { class: "pm-health-bar",
                                                                if row.fail > 0 {
                                                                    div { style: "width: {(row.fail as f64 / policies.len() as f64) * 100.0}%; background: #f87171;" }
                                                                }
                                                                if row.warn > 0 {
                                                                    div { style: "width: {(row.warn as f64 / policies.len() as f64) * 100.0}%; background: #f59e0b;" }
                                                                }
                                                                if row.pass > 0 {
                                                                    div { style: "width: {(row.pass as f64 / policies.len() as f64) * 100.0}%; background: #34d399;" }
                                                                }
                                                            }
                                                            span { class: "mono pm-health-num", style: "color: {health_color};",
                                                                "{row.pass}/{policies.len()}"
                                                            }
                                                        }
                                                    }
                                                    {row.results.iter().enumerate().map(|(res_idx, result)| {
                                                        let policy_name = &policies[res_idx];
                                                        let col_filtered = policy_filter.read().as_ref() == Some(policy_name);
                                                        let cls = format!("pm-td-cell pm-{}{}", result, if col_filtered { " col-filtered" } else { "" });
                                                        let mut pf = policy_filter.clone();
                                                        let name = policy_name.clone();
                                                        let cell_click = move |e: MouseEvent| {
                                                            e.stop_propagation();
                                                            if pf.read().as_ref() == Some(&name) {
                                                                pf.set(None);
                                                            } else {
                                                                pf.set(Some(name.clone()));
                                                            }
                                                        };
                                                        rsx! {
                                                            td {
                                                                key: "{res_idx}",
                                                                class: "{cls}",
                                                                title: "{policy_name}: {result}",
                                                                onclick: cell_click,
                                                                span { class: "pm-glyph", "{cell_glyph(result)}" }
                                                            }
                                                        }
                                                    })}
                                                }
                                                if is_exp {
                                                    tr { class: "pm-expand-row",
                                                        td {
                                                            colspan: policies.len() + 2,
                                                            div { class: "pm-expand",
                                                                 div { style: "display: flex; gap: 14px; flex-wrap: wrap;",
                                                                     {row.results.iter().enumerate()
                                                                         .filter(|(_, result)| *result != "pass")
                                                                         .map(|(res_idx, result)| {
                                                                             let policy_name = &policies[res_idx];
                                                                             let failcard_class = format!("pm-failcard pm-failcard-{}", result);
                                                                             let glyph = cell_glyph(result);
                                                                             let desc = if *result == "fail" {
                                                                                 "Blocks deployment until resolved"
                                                                             } else {
                                                                                 "Soft warning — deploy will proceed"
                                                                             };
                                                                             rsx! {
                                                                                 button {
                                                                                     key: "{res_idx}",
                                                                                     class: "{failcard_class} focus-ring",
                                                                                     title: "Open policy: {policy_name}",
                                                                                     span { class: "pm-failcard-glyph pm-{result}", "{glyph}" }
                                                                                     div { style: "min-width: 0; text-align: left;",
                                                                                         div { class: "mono", style: "font-weight: 600; font-size: 12px;", "{policy_name}" }
                                                                                         div { style: "font-size: 11px; color: var(--cf-text-muted); margin-top: 2px;", "{desc}" }
                                                                                     }
                                                                                     Icon { name: IconName::ArrowRight, size: 12 }
                                                                                 }
                                                                             }
                                                                         })}
                                                                        if row.fail == 0 && row.warn == 0 {
                                                                            div { style: "font-size: 12px; color: #34d399; display: flex; align-items: center; gap: 8px;",
                                                                                Icon { name: IconName::Check, size: 14 }
                                                                                " All policies pass for this system."
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
                                    if filtered.is_empty() {
                                        tr {
                                            td {
                                                colspan: policies.len() + 2,
                                                style: "padding: 24px; text-align: center; color: var(--cf-text-muted); font-size: 13px;",
                                                "No systems match this filter."
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Legend (matching JSX)
                        div {
                            class: "pm-legend",
                            span { span { class: "pm-legend-sw pm-pass", "✓" } " Pass" }
                            span { span { class: "pm-legend-sw pm-warn", "!" } " Warning" }
                            span { span { class: "pm-legend-sw pm-fail", "✗" } " Fail — blocks deploy" }
                            span { style: "margin-left: auto; font-size: 11px; color: var(--cf-text-muted);", "Click any policy header to filter · Click a row to expand" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EvalDrawerGraphTab(commit_id: i32) -> Element {
    const GRAPH_PENDING_POLL_MAX: u64 = 60;

    let mut graph_refresh = use_signal(|| 0_u64);
    let mut has_pending_counts = use_signal(|| true);
    let mut graph_pending_polls = use_signal(|| 0_u64);
    let graph_resource = use_resource(move || async move {
        let _ = graph_refresh();
        fetch_eval_dependency_graph(commit_id).await
    });

    use_effect(move || {
        if let Some(Ok(data)) = &*graph_resource.read() {
            let pending = data.packages.iter().any(|p| !p.closure_counted);
            has_pending_counts.set(pending);
            if !pending {
                graph_pending_polls.set(GRAPH_PENDING_POLL_MAX);
            }
        }
    });

    {
        let mut graph_refresh = graph_refresh.clone();
        use_future(move || async move {
            loop {
                #[cfg(target_arch = "wasm32")]
                {
                    gloo_timers::future::TimeoutFuture::new(2000).await;
                    if has_pending_counts() && graph_pending_polls() < GRAPH_PENDING_POLL_MAX {
                        graph_pending_polls.set(graph_pending_polls() + 1);
                        graph_refresh.set(graph_refresh() + 1);
                    }
                }

                #[cfg(not(target_arch = "wasm32"))]
                break;
            }
        });
    }

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
                    let total_packages: i64 = data.packages.iter().filter(|p| p.closure_counted).map(|p| p.ready_count + p.pending_count + p.failed_count).sum();
                    let packages_cached: i64 = data.packages.iter().filter(|p| p.closure_counted).map(|p| p.ready_count).sum();
                    let packages_to_build: i64 = data.packages.iter().filter(|p| p.closure_counted).map(|p| p.pending_count).sum();
                    let packages_failed: i64 = data.packages.iter().filter(|p| p.closure_counted).map(|p| p.failed_count).sum();
                    let commit_short: String = format!("commit #{}", commit_id);
                    rsx! {
                        // Summary flow: source → eval → systems → package closure counts
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
                            if total_packages > 0 {
                                span { style: "color: var(--cf-text-muted);", "→" }
                                div { class: "ed-graph-node ed-graph-fan",
                                    span { style: "font-weight: 700; color: #34d399;", "{packages_cached}" }
                                    span { style: "font-size: 10px; color: var(--cf-text-muted);", "cached/local" }
                                }
                                div { class: "ed-graph-node ed-graph-fan",
                                    span { style: "font-weight: 700; color: #60a5fa;", "{packages_to_build}" }
                                    span { style: "font-size: 10px; color: var(--cf-text-muted);", "to build" }
                                }
                                if packages_failed > 0 {
                                    div { class: "ed-graph-node ed-graph-fan",
                                        span { style: "font-weight: 700; color: #f87171;", "{packages_failed}" }
                                        span { style: "font-size: 10px; color: var(--cf-text-muted);", "failed" }
                                    }
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
                            div { style: "color: var(--cf-text-muted); font-size: 12px;", "No systems recorded for this commit yet." }
                        } else {
                            div { class: "ed-graph-list",
                                for pkg in data.packages.iter() {
                                    {
                                        let row_total = pkg.ready_count + pkg.pending_count + pkg.failed_count;
                                        let has_counts = pkg.closure_counted && row_total > 0;
                                        let cached_pct = if has_counts { pkg.ready_count * 100 / row_total } else { 0 };
                                        let build_pct = if has_counts { pkg.pending_count * 100 / row_total } else { 0 };
                                        let failed_pct = if has_counts { pkg.failed_count * 100 / row_total } else { 0 };
                                        rsx! {
                                            div { key: "{pkg.package_name}", class: "ed-graph-row",
                                                div { class: "ed-graph-pkg",
                                                    span { class: "mono truncate", style: "font-size: 12px; font-weight: 600;", "{pkg.package_name}" }
                                                    if has_counts {
                                                        span { style: "font-size: 10px; color: var(--cf-text-muted);",
                                                            "{pkg.ready_count}/{row_total} cached/local · {pkg.pending_count} to build"
                                                            if pkg.failed_count > 0 { " · {pkg.failed_count} failed" }
                                                        }
                                                    } else if pkg.failed_count > 0 {
                                                        span { style: "font-size: 10px; color: #f87171;", "failed before closure count" }
                                                    } else {
                                                        span { style: "font-size: 10px; color: #9ca3af;", "pending closure count" }
                                                    }
                                                }
                                                div { class: "ed-graph-bar",
                                                    div { class: "ed-graph-bar-cached", style: "width: {cached_pct}%;" }
                                                    div { class: "ed-graph-bar-build", style: "width: {build_pct}%;" }
                                                    if pkg.failed_count > 0 {
                                                        div { style: "width: {failed_pct}%; background: #f87171;" }
                                                    }
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
                                                } else if pkg.failed_count > 0 {
                                                    div { style: "display: flex; justify-content: flex-end; font-size: 11px; color: #f87171; font-weight: 600;", "failed" }
                                                } else {
                                                    div { style: "display: flex; justify-content: flex-end; font-size: 11px; color: #9ca3af;", "—" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "ed-graph-legend",
                                span { span { class: "ed-graph-sw", style: "background: #34d399;" } "Cached/local" }
                                span { span { class: "ed-graph-sw", style: "background: #60a5fa;" } "To build" }
                                if packages_failed > 0 {
                                    span { span { class: "ed-graph-sw", style: "background: #f87171;" } "Failed" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// LiveIndicator — pulsing dot + "updated Ns ago" (matching BuildsView.jsx)
// ============================================================================

#[component]
fn LiveIndicator() -> Element {
    let mut secs = use_signal(|| 0_u64);

    {
        use_future(move || async move {
            loop {
                #[cfg(target_arch = "wasm32")]
                {
                    gloo_timers::future::TimeoutFuture::new(1000).await;
                    secs.set((secs() + 1) % 6);
                }
                #[cfg(not(target_arch = "wasm32"))]
                break;
            }
        });
    }

    let label = if secs() == 0 {
        "just now".to_string()
    } else {
        format!("{}s ago", secs())
    };

    rsx! {
        div {
            style: "display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--cf-text-muted);",
            span {
                style: "display: inline-flex; align-items: center; gap: 6px;",
                span { class: "ed-pulse", style: "position: static; margin: 0;" }
                span { style: "color: #34d399; font-weight: 600;", "Live" }
            }
            span { "· updated {label}" }
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
