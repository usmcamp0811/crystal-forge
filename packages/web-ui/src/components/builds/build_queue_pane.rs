//! Build queue pane component for the builds control center.

use dioxus::prelude::*;

use crate::theme;

use super::helpers::{
    BuildAction, BuildItem, BuildStatus, build_status_badge_class, extract_system_name,
    queue_row_style, short_commit,
};

/// Build queue pane showing all queued and active builds.
#[component]
pub fn BuildQueuePane(
    builds: Vec<BuildItem>,
    selected_id: Signal<Option<i32>>,
    can_requeue: bool,
    on_build_action: EventHandler<(i32, BuildAction)>,
    on_log: EventHandler<i32>,
) -> Element {
    let filtered = builds;

    rsx! {
        BuildQueueTable {
            builds: filtered,
            selected_id: selected_id,
            can_requeue,
            on_build_action: on_build_action,
            on_log: on_log,
        }
    }
}

/// Card view for build queue items.
#[component]
fn BuildQueueCards(
    builds: Vec<BuildItem>,
    selected_id: Signal<Option<i32>>,
    can_requeue: bool,
    on_build_action: EventHandler<(i32, BuildAction)>,
) -> Element {
    rsx! {
        div {
            class: "space-y-2 max-h-[56vh] overflow-y-auto pr-1",
            "data-testid": "build-queue-list",
            for build in builds {
                button {
                    key: "{build.id}",
                    class: "w-full rounded-xl border px-4 py-4 text-left transition min-h-[44px] {queue_row_style(*selected_id.read() == Some(build.id), build.status)}",
                    "data-testid": "build-queue-card",
                    onclick: move |_| selected_id.set(Some(build.id)),
                    div {
                        class: "flex items-start justify-between gap-4 mb-3",
                        div {
                            class: "flex-1 min-w-0",
                            div {
                                class: "flex items-center gap-2 flex-wrap",
                                p {
                                    class: "text-sm {theme::text::PRIMARY} font-semibold truncate",
                                    title: "{extract_system_name(&build.hostname)}",
                                    "{extract_system_name(&build.hostname)}"
                                }
                                span {
                                    class: "inline-flex px-2 py-0.5 text-[10px] rounded border cf-chip-blue shrink-0",
                                    "{build.flake}"
                                }
                            }
                            p {
                                class: "text-xs {theme::text::MUTED} mt-1 truncate",
                                title: "{build.branch} · {short_commit(&build.commit)}",
                                "{build.branch} · {short_commit(&build.commit)}"
                            }
                        }
                        div {
                            class: "text-right shrink-0",
                            span {
                                class: "inline-flex px-2 py-1 text-[10px] uppercase rounded border {build_status_badge_class(build.status)}",
                                "{build.status_label()}"
                            }
                            p { class: "text-[10px] {theme::text::DISABLED} mt-1 whitespace-nowrap", "{build.queued_for}" }
                        }
                    }

                    div {
                        class: "mt-2 rounded-md border {theme::surface::CARD_BORDER} cf-subtle-bg px-3 py-2",
                        p {
                            class: "text-[11px] {theme::text::SECONDARY} leading-5 truncate",
                            title: "Build target: {build.flake} · {extract_system_name(&build.hostname)}",
                            span { class: "{theme::text::MUTED}", "Build target: " }
                            span { class: "font-mono text-cyan-300", "{build.flake}" }
                            span { class: "{theme::text::DISABLED} mx-1", "·" }
                            span { class: "font-mono {theme::text::SECONDARY}", "{extract_system_name(&build.hostname)}" }
                        }
                        if !build.summary.is_empty() && build.summary != format!("job {}", build.job_id.map(|id| id.to_string()).unwrap_or_else(|| "unknown".to_string())) {
                            p {
                                class: "text-[11px] {theme::text::MUTED} mt-1 italic truncate",
                                title: "{build.summary}",
                                "{build.summary}"
                            }
                        }
                    }

                    div {
                        class: "mt-3 flex flex-wrap items-center justify-between gap-3",
                        div {
                            class: "inline-flex items-center gap-2 text-[10px] flex-wrap",
                            span {
                                class: "inline-flex px-2 py-1 rounded border cf-chip-slate",
                                "worker {build.worker_id}"
                            }
                            if let Some(runtime) = build.runtime {
                                span {
                                    class: "inline-flex px-2 py-1 rounded border cf-chip-teal",
                                    "runtime {runtime}"
                                }
                            }
                        }
                        div {
                            class: "inline-flex items-center gap-2 flex-wrap",
                            if matches!(build.status, BuildStatus::Building) {
                                button {
                                    class: "text-xs text-red-400 hover:text-red-300 px-3 py-1.5 rounded hover:bg-red-500/10 transition-colors min-h-[44px]",
                                    onclick: move |evt| {
                                        evt.stop_propagation();
                                        on_build_action.call((build.id, BuildAction::Stop));
                                    },
                                    "Stop"
                                }
                            }
                            // Force Cancel for stuck builds in Stopping state
                            if matches!(build.status, BuildStatus::Stopping) {
                                button {
                                    class: "text-xs text-orange-400 hover:text-orange-300 px-3 py-1.5 rounded hover:bg-orange-500/10 transition-colors min-h-[44px]",
                                    onclick: move |evt| {
                                        evt.stop_propagation();
                                        on_build_action.call((build.id, BuildAction::ForceCancel));
                                    },
                                    "Force Cancel"
                                }
                            }
                            // Restart only valid for terminal statuses
                            if can_requeue
                                && matches!(
                                    build.status,
                                    BuildStatus::Failed
                                        | BuildStatus::Complete
                                        | BuildStatus::Cancelled
                                )
                            {
                                button {
                                    class: "text-xs px-3 py-1.5 rounded transition-colors cf-action-link min-h-[44px]",
                                    onclick: move |evt| {
                                        evt.stop_propagation();
                                        on_build_action.call((build.id, BuildAction::Restart));
                                    },
                                    "Requeue"
                                }
                            }
                            if build.status == BuildStatus::Queued {
                                button {
                                    class: "text-xs text-cyan-300 hover:text-cyan-200 px-3 py-1.5 rounded hover:bg-cyan-500/10 transition-colors min-h-[44px]",
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

/// Table view for build queue items - compact at-a-glance display.
#[component]
fn BuildQueueTable(
    builds: Vec<BuildItem>,
    selected_id: Signal<Option<i32>>,
    can_requeue: bool,
    on_build_action: EventHandler<(i32, BuildAction)>,
    on_log: EventHandler<i32>,
) -> Element {
    rsx! {
        div {
            class: "min-h-[220px] max-h-[56vh] overflow-auto",
            "data-testid": "build-queue-table",
            // JSX: <table className="sys-table">
            table {
                class: "sys-table",
                thead {
                    class: "sticky top-0 {theme::surface::CARD_BG} border-b {theme::surface::CARD_BORDER}",
                    tr {
                        th { class: "text-left px-3 py-2 {theme::text::MUTED} font-medium", "Package / derivation" }
                        th { class: "text-left px-3 py-2 {theme::text::MUTED} font-medium", "Status" }
                        th { class: "text-left px-3 py-2 {theme::text::MUTED} font-medium", "Worker" }
                        th { class: "text-left px-3 py-2 {theme::text::MUTED} font-medium", "Progress" }
                        th { class: "text-left px-3 py-2 {theme::text::MUTED} font-medium", "Queued" }
                        th { class: "text-left px-3 py-2 {theme::text::MUTED} font-medium", "Duration" }
                        th { class: "text-right px-3 py-2 {theme::text::MUTED} font-medium", " " }
                    }
                }
                tbody {
                    for build in builds {
                        {
                            let is_selected = *selected_id.read() == Some(build.id);
                            // JSX: className={selected?.id===b.id?"selected":""}
                            let row_class = if is_selected {
                                "selected cursor-pointer"
                            } else {
                                "cursor-pointer hover:bg-white/5"
                            };
                            rsx! {
                                tr {
                                    key: "{build.id}",
                                    class: "{row_class} border-b {theme::surface::CARD_BORDER}",
                                    "data-testid": "build-queue-row",
                                    onclick: move |_| selected_id.set(Some(build.id)),
                                    td {
                                        class: "px-3 py-2",
                                        div {
                                            // JSX line 1: b.pkg (package name) - bold
                                            p { class: "text-[13px] font-semibold leading-[1.15] {theme::text::PRIMARY}", "{build.pkg()}" }
                                            // JSX line 2: b.drv.slice(0,40) + ellipsis (derivation path) - mono, muted
                                            p { class: "text-[10px] leading-4 font-mono {theme::text::MUTED} truncate max-w-[18rem]",
                                                "{truncate_with_ellipsis(&build.drv(), 40)}"
                                            }
                                            // JSX line 3: flake · commit - muted
                                            p { class: "text-[10px] leading-4 {theme::text::MUTED}",
                                                "{build.flake} · "
                                                span { class: "font-mono", "{build.commit}" }
                                            }
                                        }
                                    }
                                    td {
                                        class: "px-3 py-2",
                                        // JSX: chip with chip-dot - no uppercase
                                        span {
                                            class: "inline-flex items-center gap-1.5 px-2 py-0.5 text-[10px] rounded border {build_status_badge_class(build.status)}",
                                            span {
                                                class: "inline-block h-1.5 w-1.5 rounded-full",
                                                style: "background-color: {status_dot_color(build.status)};"
                                            }
                                            "{build.status_label()}"
                                        }
                                    }
                                    td {
                                        class: "px-3 py-2 font-mono text-xs {theme::text::SECONDARY} whitespace-nowrap",
                                        // JSX: {b.worker || "—"}
                                        if build.worker_id == "unassigned" {
                                            "—"
                                        } else {
                                            "{build.worker_id}"
                                        }
                                    }
                                    td {
                                        class: "px-3 py-2 w-[100px]",
                                        // JSX: only shows progress bar when b.progress > 0
                                        // We don't have real progress data, show indeterminate for building
                                        if matches!(build.status, BuildStatus::Building | BuildStatus::Stopping) {
                                            div {
                                                class: "h-[5px] bg-slate-800 rounded-full overflow-hidden",
                                                // Indeterminate progress animation
                                                div {
                                                    class: "h-full rounded-full animate-pulse",
                                                    style: "width: 60%; background-color: {status_dot_color(build.status)}; transition: width 1s;",
                                                }
                                            }
                                        }
                                    }
                                    td {
                                        class: "px-3 py-2 text-xs {theme::text::MUTED} whitespace-nowrap",
                                        "{build.queued_for}"
                                    }
                                    td {
                                        class: "px-3 py-2 font-mono text-xs {theme::text::SECONDARY} whitespace-nowrap",
                                        if let Some(ref runtime) = build.runtime {
                                            "{runtime}"
                                        } else {
                                            "—"
                                        }
                                    }
                                    td {
                                        class: "px-3 py-2 text-right",
                                        // JSX: <div className="row-actions">
                                        div {
                                            class: "row-actions",
                                            // JSX: <button title="Logs"><Icon name="terminal" size={14} /></button>
                                            button {
                                                class: "btn-icon focus-ring",
                                                title: "Logs",
                                                onclick: move |evt| {
                                                    evt.stop_propagation();
                                                    // Select the build and open log modal immediately (JSX parity)
                                                    selected_id.set(Some(build.id));
                                                    on_log.call(build.id);
                                                },
                                                svg {
                                                    width: "14",
                                                    height: "14",
                                                    view_box: "0 0 24 24",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    polyline { points: "4 17 10 11 4 5" }
                                                    line { x1: "12", y1: "19", x2: "20", y2: "19" }
                                                }
                                            }
                                            if let Some(cancel_action) = cancel_action_for_status(build.status) {
                                                // JSX: <button title="Cancel"><Icon name="x" size={14} /></button>
                                                button {
                                                    class: "btn-icon focus-ring",
                                                    title: "Cancel",
                                                    onclick: move |evt| {
                                                        evt.stop_propagation();
                                                        on_build_action.call((build.id, cancel_action));
                                                    },
                                                    svg {
                                                        width: "14",
                                                        height: "14",
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
                                            if can_requeue
                                                && matches!(
                                                    build.status,
                                                    BuildStatus::Failed
                                                        | BuildStatus::Complete
                                                        | BuildStatus::Cancelled
                                                )
                                            {
                                                button {
                                                    class: "btn-icon focus-ring",
                                                    title: "Requeue",
                                                    onclick: move |evt| {
                                                        evt.stop_propagation();
                                                        on_build_action.call((build.id, BuildAction::Restart));
                                                    },
                                                    svg {
                                                        width: "14",
                                                        height: "14",
                                                        view_box: "0 0 24 24",
                                                        fill: "none",
                                                        stroke: "currentColor",
                                                        stroke_width: "2",
                                                        stroke_linecap: "round",
                                                        stroke_linejoin: "round",
                                                        path { d: "M3 12a9 9 0 0 0 9 9 9 9 0 0 0 6.2-2.5" }
                                                        path { d: "M21 12a9 9 0 0 0-9-9 9 9 0 0 0-6.2 2.5" }
                                                        polyline { points: "3 3 3 9 9 9" }
                                                        polyline { points: "21 21 21 15 15 15" }
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
}

fn truncate_with_ellipsis(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

fn cancel_action_for_status(status: BuildStatus) -> Option<BuildAction> {
    match status {
        BuildStatus::Building => Some(BuildAction::Stop),
        BuildStatus::Stopping => Some(BuildAction::ForceCancel),
        BuildStatus::Queued => Some(BuildAction::Stop),
        _ => None,
    }
}

fn status_dot_color(status: BuildStatus) -> &'static str {
    match status {
        BuildStatus::Queued => "#a78bfa",
        BuildStatus::Building => "#34d399",
        BuildStatus::Stopping => "#fbbf24",
        BuildStatus::Failed => "#f87171",
        BuildStatus::Complete => "#34d399",
        BuildStatus::Cancelled => "#94a3b8",
    }
}
