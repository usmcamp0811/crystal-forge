//! Evaluations view - rebuilt to match JSX mockup design exactly.

use dioxus::prelude::*;
#[cfg(target_arch = "wasm32")]
use gloo_timers::future::TimeoutFuture;

use crate::api::{
    client::{
        ApiClientError, cancel_commit_evaluation, fetch_eval_history, fetch_eval_queue,
        re_evaluate_commit,
    },
    models::{EvalHistoryItem, EvalHistoryPage, EvalQueueItem, EvalQueueSummary},
};
use crate::components::{Icon, IconName};

#[derive(Clone, Copy, PartialEq, Eq)]
enum EvaluationsTab {
    ActiveQueue,
    History,
}

#[component]
pub fn EvaluationsView() -> Element {
    rsx! { EvaluationsPage {} }
}

#[component]
pub fn EvaluationsCommitView(commit_id: i32) -> Element {
    // For now, just redirect to main evaluations view
    // This can be enhanced later to focus on a specific commit
    let _ = commit_id; // Suppress unused warning
    rsx! { EvaluationsPage {} }
}

#[component]
fn EvaluationsPage() -> Element {
    let mut queue_items = use_signal(Vec::<EvalQueueItem>::new);
    let mut refresh = use_signal(|| 0_u64);
    let mut active_tab = use_signal(|| EvaluationsTab::ActiveQueue);
    let mut log_modal_target = use_signal(|| None::<EvalQueueItem>);

    // History tab state
    let mut history_page = use_signal(|| 1_i64);
    let mut history_status_filter = use_signal(|| String::from(""));
    let mut history_flake_filter = use_signal(|| String::new());

    let history_resource = use_resource(move || async move {
        let _ = refresh();
        let page = history_page();
        let status = history_status_filter();
        let flake = history_flake_filter();
        fetch_eval_history(
            page,
            50,
            if status.is_empty() {
                None
            } else {
                Some(status.as_str())
            },
            if flake.is_empty() {
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
                        Icon { name: IconName::Sync, size: 14 }
                        " Sync flakes"
                    }
                    button {
                        class: "btn btn-primary focus-ring",
                        onclick: move |_| refresh.set(refresh() + 1),
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
                div {
                    class: "stat",
                    span {
                        class: "stat-accent",
                        style: "--stat-color: #a78bfa;"
                    }
                    div { class: "stat-label", "Flakes tracked" }
                    div { class: "stat-value", "5" }
                }
            }

            // Tabs + content wrapped in card
            div {
                class: "card",
                style: "overflow: hidden;",

                // Tab bar
                div {
                    class: "sd-tabs",
                    style: "padding: 0 16px; border-bottom: 1px solid var(--cf-card-border);",
                    button {
                        class: if active_tab() == EvaluationsTab::ActiveQueue {
                            "sd-tab active focus-ring"
                        } else {
                            "sd-tab focus-ring"
                        },
                        onclick: move |_| active_tab.set(EvaluationsTab::ActiveQueue),
                        "Active Queue "
                        span {
                            class: "sd-tab-badge",
                            style: "background: rgba(96,165,250,0.15); color: #60a5fa;",
                            "{active_count}"
                        }
                    }
                    button {
                        class: if active_tab() == EvaluationsTab::History {
                            "sd-tab active focus-ring"
                        } else {
                            "sd-tab focus-ring"
                        },
                        onclick: move |_| active_tab.set(EvaluationsTab::History),
                        "History"
                    }
                }

                // Tab content
                if active_tab() == EvaluationsTab::ActiveQueue {
                    EvalActiveQueue {
                        evals: active_items.clone(),
                        on_log: move |item| log_modal_target.set(Some(item)),
                        refresh: refresh,
                    }
                }

                if active_tab() == EvaluationsTab::History {
                    EvalHistory {
                        history_resource: history_resource,
                        history_status_filter: history_status_filter,
                        history_flake_filter: history_flake_filter,
                        history_page: history_page,
                        refresh: refresh,
                    }
                }
            }

            // Log modal
            if let Some(target) = log_modal_target.read().clone() {
                EvalLogModal {
                    eval_item: target,
                    on_close: move |_| log_modal_target.set(None),
                }
            }
        }
    }
}

#[component]
fn EvalActiveQueue(
    evals: Vec<EvalQueueItem>,
    on_log: EventHandler<EvalQueueItem>,
    refresh: Signal<u64>,
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
                for (index , ev) in evals.iter().enumerate() {
                    {
                        let ev = ev.clone();
                        let commit_id = ev.commit_id;
                        let can_move_up = index > 0;
                        let can_move_down = index + 1 < evals.len();
                        let can_cancel = matches!(ev.evaluation_status.as_str(), "pending" | "in_progress");
                        let is_cancelling = ev.evaluation_status == "cancelling";
                        let status_meta = eval_status_meta(&ev.evaluation_status);

                        rsx! {
                            tr {
                                key: "{commit_id}",
                                td {
                                    style: "color: var(--cf-text-muted); font-size: 12px;",
                                    "{ev.queue_position}"
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
                                    style: "font-size: 12px; color: var(--cf-text-secondary);",
                                    "{ev.system_count} hosts"
                                }
                                td {
                                    div {
                                        style: "display: flex; gap: 6px;",
                                        span {
                                            class: "chip chip-healthy",
                                            "{ev.passed_count} ✓"
                                        }
                                        if ev.policy_failed_count > 0 {
                                            span {
                                                class: "chip chip-critical",
                                                "{ev.policy_failed_count} ✗"
                                            }
                                        }
                                    }
                                }
                                td {
                                    style: "font-size: 12px; color: var(--cf-text-muted);",
                                    "{format_relative_time(ev.committed_at)}"
                                }
                                td {
                                    div {
                                        class: "row-actions",
                                        style: "opacity: 1; gap: 4px; justify-content: flex-end;",

                                        // Move up button
                                        button {
                                            class: "btn-icon focus-ring",
                                            title: "Move up",
                                            disabled: !can_move_up,
                                            style: if !can_move_up { "opacity: 0.3;" } else { "" },
                                            "↑"
                                        }

                                        // Move down button
                                        button {
                                            class: "btn-icon focus-ring",
                                            title: "Move down",
                                            disabled: !can_move_down,
                                            style: if !can_move_down { "opacity: 0.3;" } else { "" },
                                            "↓"
                                        }

                                        // View logs button
                                        button {
                                            class: "btn-icon focus-ring",
                                            title: "View logs",
                                            onclick: move |_| on_log.call(ev.clone()),
                                            Icon { name: IconName::Terminal, size: 14 }
                                        }

                                        // Force cancel button (if cancelling)
                                        if is_cancelling {
                                            button {
                                                class: "btn btn-danger focus-ring",
                                                style: "padding: 3px 8px; font-size: 11px;",
                                                "Force cancel"
                                            }
                                        }

                                        // Cancel button (if can cancel)
                                        if can_cancel {
                                            button {
                                                class: "btn btn-ghost focus-ring",
                                                style: "padding: 3px 8px; font-size: 11px;",
                                                onclick: move |_| {
                                                    let mut refresh_clone = refresh.clone();
                                                    spawn(async move {
                                                        let _ = cancel_commit_evaluation(commit_id).await;
                                                        refresh_clone.set(refresh_clone() + 1);
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
    refresh: Signal<u64>,
) -> Element {
    rsx! {
        div {
            // Filter bar
            div {
                style: "padding: 12px 16px; border-bottom: 1px solid var(--cf-divider); display: flex; gap: 10px; flex-wrap: wrap; align-items: center;",

                // Status filter segmented control
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

                // Flake filter select
                select {
                    class: "input filter-select focus-ring",
                    style: "width: auto;",
                    value: "{history_flake_filter()}",
                    onchange: move |evt| {
                        history_flake_filter.set(evt.value().clone());
                        history_page.set(1);
                        refresh.set(refresh() + 1);
                    },
                    option { value: "", "All flakes" }
                }

                // Entry count
                if let Some(Ok(page_data)) = &*history_resource.read() {
                    span {
                        class: "filter-count",
                        "{page_data.items.len()} entries"
                    }
                }
            }

            // History table
            match &*history_resource.read() {
                Some(Ok(page_data)) => rsx! {
                    table {
                        class: "sys-table",
                        thead {
                            tr {
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

                                    rsx! {
                                        tr {
                                            key: "{commit_id}",
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
                                                    button {
                                                        class: "btn-icon focus-ring",
                                                        title: "Logs",
                                                        Icon { name: IconName::Terminal, size: 14 }
                                                    }
                                                    if ev.evaluation_status != "complete" {
                                                        button {
                                                            class: "btn-icon focus-ring",
                                                            title: "Re-evaluate",
                                                            onclick: move |_| {
                                                                let mut refresh_clone = refresh.clone();
                                                                spawn(async move {
                                                                    let _ = re_evaluate_commit(commit_id).await;
                                                                    refresh_clone.set(refresh_clone() + 1);
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
                            if page_data.items.is_empty() {
                                tr {
                                    td {
                                        colspan: "8",
                                        style: "text-align: center; padding: 24px;",
                                        "No evaluation history yet."
                                    }
                                }
                            }
                        }
                    }

                    // Pagination (if needed)
                    if page_data.total_count > page_data.limit {
                        div {
                            style: "display: flex; align-items: center; justify-content: space-between; padding: 12px 16px; border-top: 1px solid var(--cf-divider);",
                            span {
                                style: "font-size: 12px; color: var(--cf-text-muted);",
                                "Showing {((page_data.page - 1) * page_data.limit) + 1}–{(page_data.page * page_data.limit).min(page_data.total_count)} of {page_data.total_count}"
                            }
                            div {
                                style: "display: flex; gap: 8px;",
                                button {
                                    class: "btn btn-ghost focus-ring",
                                    style: "padding: 4px 8px; font-size: 11px;",
                                    disabled: page_data.page <= 1,
                                    onclick: move |_| {
                                        history_page.set((history_page() - 1).max(1));
                                        refresh.set(refresh() + 1);
                                    },
                                    "← Prev"
                                }
                                span {
                                    style: "font-size: 12px; color: var(--cf-text-muted);",
                                    "Page {page_data.page}"
                                }
                                button {
                                    class: "btn btn-ghost focus-ring",
                                    style: "padding: 4px 8px; font-size: 11px;",
                                    disabled: page_data.page * page_data.limit >= page_data.total_count,
                                    onclick: move |_| {
                                        history_page.set(history_page() + 1);
                                        refresh.set(refresh() + 1);
                                    },
                                    "Next →"
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
                        div {
                            style: "width: 16px; height: 16px; border: 2px solid currentColor; border-top-color: transparent; border-radius: 50%; animation: spin 1s linear infinite;"
                        }
                        "Loading history…"
                    }
                }
            }
        }
    }
}

#[component]
fn EvalLogModal(eval_item: EvalQueueItem, on_close: EventHandler<()>) -> Element {
    let mut log_verbosity = use_signal(|| LogVerbosity::Concise);
    let status_meta = eval_status_meta(&eval_item.evaluation_status);

    // Mock log lines for demonstration
    let mock_logs = vec![
        LogLine {
            t: "12:04:01".to_string(),
            lvl: "info".to_string(),
            m: format!(
                "eval: starting {}@{}",
                eval_item.flake_name,
                eval_item.commit_hash.chars().take(8).collect::<String>()
            ),
        },
        LogLine {
            t: "12:04:02".to_string(),
            lvl: "info".to_string(),
            m: "eval: fetching flake from git+ssh://...".to_string(),
        },
        LogLine {
            t: "12:04:05".to_string(),
            lvl: "info".to_string(),
            m: format!(
                "eval: running nix-eval-jobs --flake {}#nixosConfigurations",
                eval_item.flake_name
            ),
        },
        LogLine {
            t: "12:04:12".to_string(),
            lvl: "info".to_string(),
            m: format!(
                "eval: found {} NixOS configurations",
                eval_item.system_count
            ),
        },
        LogLine {
            t: "12:04:18".to_string(),
            lvl: "info".to_string(),
            m: format!(
                "eval: policy check — {} pass, {} fail",
                eval_item.passed_count, eval_item.policy_failed_count
            ),
        },
        LogLine {
            t: "12:04:20".to_string(),
            lvl: "info".to_string(),
            m: "eval: queuing build jobs for passing systems...".to_string(),
        },
    ];

    let shown_logs = if *log_verbosity.read() == LogVerbosity::Verbose {
        mock_logs.clone()
    } else {
        mock_logs
            .iter()
            .filter(|l| l.lvl != "debug")
            .cloned()
            .collect()
    };

    rsx! {
        div {
            class: "modal-backdrop",
            onclick: move |_| on_close.call(()),

            div {
                class: "modal",
                style: "width: min(800px, 98vw);",
                onclick: move |evt| evt.stop_propagation(),

                // Modal header
                div {
                    class: "modal-head",
                    style: "display: flex; justify-content: space-between; align-items: center;",
                    div {
                        h2 {
                            style: "margin: 0; font-size: 15px;",
                            span {
                                class: "chip {status_meta.cls}",
                                style: "margin-right: 8px;",
                                "{status_meta.label}"
                            }
                            "{eval_item.flake_name}"
                        }
                        p {
                            style: "margin: 4px 0 0; font-size: 12px; color: var(--cf-text-muted);",
                            span { class: "mono", "{eval_item.commit_hash.chars().take(12).collect::<String>()}" }
                            " · {eval_item.branch} · {eval_item.system_count} systems"
                        }
                    }
                    div {
                        style: "display: flex; gap: 8px; align-items: center;",
                        div {
                            class: "seg",
                            button {
                                class: if *log_verbosity.read() == LogVerbosity::Concise { "active" } else { "" },
                                onclick: move |_| log_verbosity.set(LogVerbosity::Concise),
                                "Concise"
                            }
                            button {
                                class: if *log_verbosity.read() == LogVerbosity::Verbose { "active" } else { "" },
                                onclick: move |_| log_verbosity.set(LogVerbosity::Verbose),
                                "Verbose"
                            }
                        }
                        button {
                            class: "btn-icon focus-ring",
                            onclick: move |_| on_close.call(()),
                            Icon { name: IconName::X, size: 16 }
                        }
                    }
                }

                // Log stream
                pre {
                    class: "sd-log-stream",
                    style: "min-height: 360px; max-height: 520px;",
                    for line in shown_logs.iter() {
                        div {
                            class: "sd-log-line sd-log-{line.lvl}",
                            span { class: "sd-log-t", "{line.t}" }
                            span { class: "sd-log-lvl", "{line.lvl.to_uppercase()}" }
                            span { class: "sd-log-m", "{line.m}" }
                        }
                    }
                    if eval_item.evaluation_status == "in_progress" {
                        div { class: "sd-log-caret", "▍" }
                    }
                }

                // Modal footer
                div {
                    class: "modal-foot",
                    span {
                        style: "font-size: 12px; color: var(--cf-text-muted); margin-right: auto;",
                        "{shown_logs.len()} lines shown"
                    }
                    button {
                        class: "btn btn-ghost focus-ring xs",
                        Icon { name: IconName::Download, size: 12 }
                        " Download"
                    }
                    button {
                        class: "btn btn-primary focus-ring",
                        onclick: move |_| on_close.call(()),
                        "Close"
                    }
                }
            }
        }
    }
}

// ============================================================================
// Helper types and functions
// ============================================================================

#[derive(Clone)]
struct LogLine {
    t: String,
    lvl: String,
    m: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LogVerbosity {
    Concise,
    Verbose,
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
            label: status,
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
