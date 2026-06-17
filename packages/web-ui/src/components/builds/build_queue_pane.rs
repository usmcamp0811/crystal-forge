//! Build queue pane component for the builds control center.
//!
//! Matches BuildsView.jsx: BuildQueueTable with q-queue-table CSS,
//! dual-segment derivations bar, and multi-select bulk bar.

use dioxus::prelude::*;

use super::helpers::{BuildAction, BuildItem, BuildStatus, extract_system_name, short_commit};

/// Public entry point — wraps the inner table and bulk-cancel bar.
#[component]
pub fn BuildQueuePane(
    builds: Vec<BuildItem>,
    selected_id: Signal<Option<i32>>,
    can_requeue: bool,
    on_build_action: EventHandler<(i32, BuildAction)>,
    on_log: EventHandler<i32>,
) -> Element {
    // Multi-select state: set of selected build IDs (only operator-cancellable ones).
    let mut selected_ids: Signal<Vec<i32>> = use_signal(Vec::new);

    let bulk_count = selected_ids.read().len();

    rsx! {
        // JSX: <table className="sys-table q-queue-table">
        div { style: "overflow-x: auto;",
            table {
                class: "sys-table q-queue-table",
                "data-testid": "build-queue-table",
                thead {
                    tr {
                        th { "System configuration" }
                        th { "Status" }
                        th { "Worker" }
                        th { "Derivations" }
                        th { "Queued" }
                        th { "Duration" }
                        th { style: "text-align: right;", "Actions" }
                    }
                }
                tbody {
                    for build in builds.iter() {
                        {
                            let build = build.clone();
                            let is_selected = *selected_id.read() == Some(build.id);
                            let is_checked = selected_ids.read().contains(&build.id);
                            let can_cancel = can_requeue && is_cancellable(build.status);
                            let mut row_class = "q-row".to_string();
                            if is_selected { row_class.push_str(" selected"); }
                            if is_checked  { row_class.push_str(" row-checked"); }
                            if can_cancel  { row_class.push_str(" selectable"); }

                            rsx! {
                                tr {
                                    key: "{build.id}",
                                    class: "{row_class}",
                                    "data-testid": "build-queue-row",
                                    onclick: move |evt| {
                                        // Shift-click: toggle multi-select on operator-cancellable rows
                                        if evt.modifiers().shift() && can_cancel {
                                            let mut ids = selected_ids.read().clone();
                                            if is_checked {
                                                ids.retain(|&id| id != build.id);
                                            } else {
                                                ids.push(build.id);
                                            }
                                            selected_ids.set(ids);
                                            return;
                                        }
                                        // Clear multi-select on normal click if nothing selected
                                        if !selected_ids.read().is_empty()
                                            && !evt.modifiers().shift()
                                        {
                                            selected_ids.set(Vec::new());
                                        }
                                        selected_id.set(Some(build.id));
                                    },

                                    // System configuration column
                                    td {
                                        div {
                                            style: "font-weight: 600; font-size: 13px; display: flex; align-items: center; gap: 6px;",
                                            // server icon
                                            svg {
                                                width: "12", height: "12",
                                                view_box: "0 0 24 24",
                                                fill: "none", stroke: "currentColor",
                                                stroke_width: "2",
                                                stroke_linecap: "round", stroke_linejoin: "round",
                                                style: "color: var(--cf-text-muted);",
                                                rect { x: "2", y: "2", width: "20", height: "8", rx: "2", ry: "2" }
                                                rect { x: "2", y: "14", width: "20", height: "8", rx: "2", ry: "2" }
                                                line { x1: "6", y1: "6", x2: "6.01", y2: "6" }
                                                line { x1: "6", y1: "18", x2: "6.01", y2: "18" }
                                            }
                                            "{extract_system_name(&build.hostname)}"
                                        }
                                        div {
                                            style: "font-size: 10px; color: var(--cf-text-muted);",
                                            "{build.flake} · "
                                            span { class: "mono", "{short_commit(&build.commit)}" }
                                            " · "
                                            span { class: "mono", "{build.arch}" }
                                        }
                                        if let Some(ref pkg) = build.current_pkg {
                                            div {
                                                class: "mono",
                                                style: "font-size: 10px; color: #60a5fa; margin-top: 2px;",
                                                "building {pkg}…"
                                            }
                                        }
                                        if let Some(ref pkg) = build.failed_pkg {
                                            div {
                                                class: "mono",
                                                style: "font-size: 10px; color: #f87171; margin-top: 2px;",
                                                "failed on {pkg}"
                                            }
                                        }
                                    }

                                    // Status chip
                                    td {
                                        span {
                                            class: "chip {status_chip_class(build.status)}",
                                            span {
                                                class: "chip-dot",
                                                style: "background: {status_color(build.status)};",
                                            }
                                            "{build.status_label()}"
                                        }
                                    }

                                    // Worker
                                    td {
                                        span {
                                            class: "mono",
                                            style: "font-size: 12px; color: var(--cf-text-secondary);",
                                            if build.worker_id == "unassigned" { "—" } else { "{build.worker_id}" }
                                        }
                                    }

                                    // Derivations dual-segment bar
                                    td { style: "width: 140px;",
                                        if build.total_derivs > 0 {
                                            {
                                                let total = build.total_derivs as f64;
                                                let cached_pct = (build.cached_derivs as f64 / total * 100.0).min(100.0);
                                                let built_pct = ((build.built_derivs.saturating_sub(build.cached_derivs)) as f64 / total * 100.0).min(100.0);
                                                let status_col = status_color(build.status);
                                                rsx! {
                                                    div { style: "display: flex; align-items: center; gap: 8px;",
                                                        div {
                                                            style: "flex: 1; height: 5px; background: var(--cf-subtle-bg); border-radius: 99px; overflow: hidden; display: flex;",
                                                            div {
                                                                style: "width: {cached_pct}%; background: #34d399;",
                                                                title: "{build.cached_derivs} from cache",
                                                            }
                                                            div {
                                                                style: "width: {built_pct}%; background: {status_col}; transition: width 1s;",
                                                                title: "{build.built_derivs.saturating_sub(build.cached_derivs)} built",
                                                            }
                                                        }
                                                        span {
                                                            class: "mono",
                                                            style: "font-size: 11px; color: var(--cf-text-muted); white-space: nowrap;",
                                                            "{build.built_derivs}/{build.total_derivs}"
                                                        }
                                                    }
                                                }
                                            }
                                        } else {
                                            span { style: "color: var(--cf-text-muted); font-size: 12px;", "—" }
                                        }
                                    }

                                    // Queued
                                    td {
                                        style: "font-size: 12px; color: var(--cf-text-muted);",
                                        "{build.queued_for}"
                                    }

                                    // Duration
                                    td {
                                        class: "mono",
                                        style: "font-size: 12px; color: var(--cf-text-secondary);",
                                        if let Some(ref rt) = build.runtime { "{rt}" } else { "—" }
                                    }

                                    // Actions column
                                    td {
                                        onclick: move |evt| evt.stop_propagation(),
                                        div {
                                            class: "row-actions",
                                            style: "opacity: 1; gap: 6px; justify-content: flex-end;",

                                            // Logs button
                                            button {
                                                class: "btn-icon focus-ring",
                                                title: "Logs",
                                                onclick: move |_| {
                                                    selected_id.set(Some(build.id));
                                                    on_log.call(build.id);
                                                },
                                                // terminal icon
                                                svg {
                                                    width: "14", height: "14",
                                                    view_box: "0 0 24 24",
                                                    fill: "none", stroke: "currentColor",
                                                    stroke_width: "2",
                                                    stroke_linecap: "round", stroke_linejoin: "round",
                                                    polyline { points: "4 17 10 11 4 5" }
                                                    line { x1: "12", y1: "19", x2: "20", y2: "19" }
                                                }
                                            }

                                            // Cancel / force-kill
                                            if can_requeue && matches!(build.status, BuildStatus::Building | BuildStatus::Queued) {
                                                button {
                                                    class: "btn-icon focus-ring",
                                                    title: "Cancel build",
                                                    onclick: move |_| on_build_action.call((build.id, BuildAction::Stop)),
                                                    svg {
                                                        width: "14", height: "14",
                                                        view_box: "0 0 24 24",
                                                        fill: "none", stroke: "currentColor",
                                                        stroke_width: "2",
                                                        stroke_linecap: "round", stroke_linejoin: "round",
                                                        line { x1: "18", y1: "6", x2: "6", y2: "18" }
                                                        line { x1: "6", y1: "6", x2: "18", y2: "18" }
                                                    }
                                                }
                                            }
                                            if can_requeue && build.status == BuildStatus::Stopping {
                                                button {
                                                    class: "btn-icon focus-ring",
                                                    title: "Force kill",
                                                    style: "color: var(--cf-red, #f87171);",
                                                    onclick: move |_| on_build_action.call((build.id, BuildAction::ForceCancel)),
                                                    svg {
                                                        width: "14", height: "14",
                                                        view_box: "0 0 24 24",
                                                        fill: "none", stroke: "currentColor",
                                                        stroke_width: "2",
                                                        stroke_linecap: "round", stroke_linejoin: "round",
                                                        line { x1: "18", y1: "6", x2: "6", y2: "18" }
                                                        line { x1: "6", y1: "6", x2: "18", y2: "18" }
                                                    }
                                                }
                                            }

                                            // Retry for failed
                                            if can_requeue && build.status == BuildStatus::Failed {
                                                button {
                                                    class: "btn-icon focus-ring",
                                                    title: "Retry build",
                                                    onclick: move |_| on_build_action.call((build.id, BuildAction::Restart)),
                                                    // rollback icon
                                                    svg {
                                                        width: "14", height: "14",
                                                        view_box: "0 0 24 24",
                                                        fill: "none", stroke: "currentColor",
                                                        stroke_width: "2",
                                                        stroke_linecap: "round", stroke_linejoin: "round",
                                                        path { d: "M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74" }
                                                        path { d: "M21 3v9h-9" }
                                                        path { d: "M21 12A9 9 0 0 0 3.26 9.26" }
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

        // Bulk cancel bar — appears when multi-select has items and caller may mutate queue
        if can_requeue && bulk_count > 0 {
            BulkBar {
                count: bulk_count,
                on_cancel: move |_| {
                    let ids = selected_ids.read().clone();
                    for id in &ids {
                        on_build_action.call((*id, BuildAction::Stop));
                    }
                    selected_ids.set(Vec::new());
                },
                on_clear: move |_| selected_ids.set(Vec::new()),
            }
        }

    }
}

/// Bulk action bar — sticky floating pill, shown when multi-select is active.
/// JSX: <BulkBar count={sel.size} onClear={sel.clear}> ... </BulkBar>
#[component]
fn BulkBar(count: usize, on_cancel: EventHandler<()>, on_clear: EventHandler<()>) -> Element {
    let s = if count == 1 { "" } else { "s" };
    rsx! {
        div {
            class: "bulk-bar",
            role: "toolbar",
            "aria-label": "Bulk actions",
            // JSX: <span className="bulk-count"><strong>{count}</strong> selected</span>
            span { class: "bulk-count",
                strong { "{count}" }
                " selected"
            }
            // JSX: <span className="bulk-sep" />
            span { class: "bulk-sep" }
            // Cancel action
            button {
                class: "btn btn-danger xs focus-ring",
                onclick: move |_| on_cancel.call(()),
                svg {
                    width: "12", height: "12",
                    view_box: "0 0 24 24",
                    fill: "none", stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round", stroke_linejoin: "round",
                    style: "margin-right: 4px;",
                    line { x1: "18", y1: "6", x2: "6", y2: "18" }
                    line { x1: "6", y1: "6", x2: "18", y2: "18" }
                }
                "Cancel {count} build{s}"
            }
            // JSX: <button className="btn btn-ghost xs focus-ring" onClick={onClear}>Clear</button>
            button {
                class: "btn btn-ghost xs focus-ring",
                onclick: move |_| on_clear.call(()),
                "Clear"
            }
        }
    }
}

fn is_cancellable(status: BuildStatus) -> bool {
    matches!(
        status,
        BuildStatus::Building | BuildStatus::Queued | BuildStatus::Stopping
    )
}

fn status_color(status: BuildStatus) -> &'static str {
    match status {
        BuildStatus::Queued => "#a78bfa",
        BuildStatus::Building => "#60a5fa",
        BuildStatus::Stopping => "#fbbf24",
        BuildStatus::Failed => "#f87171",
        BuildStatus::Complete => "#34d399",
        BuildStatus::Cancelled => "#94a3b8",
    }
}

fn status_chip_class(status: BuildStatus) -> &'static str {
    match status {
        BuildStatus::Queued => "chip-info",
        BuildStatus::Building => "chip-info",
        BuildStatus::Stopping => "chip-warning",
        BuildStatus::Failed => "chip-critical",
        BuildStatus::Complete => "chip-success",
        BuildStatus::Cancelled => "chip-unknown",
    }
}
