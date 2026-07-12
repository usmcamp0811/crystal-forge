//! Build detail pane component for the builds control center.

use chrono::{DateTime, Datelike, Timelike, Utc};
use dioxus::prelude::*;

use crate::theme;

use super::helpers::{
    BuildAction, BuildItem, BuildStatus, PendingAction, build_status_badge_class,
    extract_system_name, short_commit,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailTab {
    Logs,
    Details,
}

impl DetailTab {
    pub fn label(self) -> &'static str {
        match self {
            DetailTab::Logs => "Log",
            DetailTab::Details => "Details",
        }
    }
}

/// Build detail side panel matching the mockup structure.
#[component]
pub fn BuildDetailPane(
    selected: Option<BuildItem>,
    can_requeue: bool,
    on_close: EventHandler<()>,
    on_log: EventHandler<()>,
    on_build_action: EventHandler<BuildAction>,
    tab: Signal<DetailTab>,
    on_tab_change: EventHandler<DetailTab>,
    follow_logs: Signal<bool>,
    pause_logs: Signal<bool>,
    wrap_logs: Signal<bool>,
    log_query: Signal<String>,
) -> Element {
    let Some(build) = selected else {
        return rsx! { div {} };
    };

    let active_tab = *tab.read();
    let live = matches!(build.status, BuildStatus::Building | BuildStatus::Stopping);
    let active = live || build.status == BuildStatus::Queued;
    let system_name = extract_system_name(&build.hostname).to_string();
    let worker_label = if build.worker_id == "unassigned" {
        "—".to_string()
    } else {
        build.worker_id.clone()
    };
    let details_worker_label = if build.worker_id == "unassigned" {
        "unassigned".to_string()
    } else {
        build.worker_id.clone()
    };
    let duration_label = build.runtime.clone().unwrap_or_else(|| "—".to_string());
    let queued_relative = relative_label(build.queued_at);
    let queued_dtg = dtg_label(build.queued_at, Some(&queued_relative));
    let completed_dtg = build.completed_at.map(|ts| dtg_label(ts, None));
    let completed_label = if build.status == BuildStatus::Failed {
        "Failed"
    } else {
        "Completed"
    };
    let drv = build.drv();
    let drv_preview = truncate_with_ellipsis(&drv, 40);
    let has_drv_progress = build.total_derivs > 0
        && build.built_derivs < build.total_derivs
        && matches!(build.status, BuildStatus::Building | BuildStatus::Stopping);
    let derivs_label = if build.total_derivs > 0 {
        format!(
            "{}/{} built · {} cached",
            build.built_derivs, build.total_derivs, build.cached_derivs
        )
    } else {
        "—".to_string()
    };
    let raw_logs = build.logs.clone().unwrap_or_else(|| {
        format!(
            "{} build for {} queued on {}\n{} · {}\nDerivations: {}/{} built · {} cached",
            build.status_label(),
            system_name,
            build.arch,
            short_commit(&build.commit),
            drv,
            build.built_derivs,
            build.total_derivs,
            build.cached_derivs
        )
    });
    let log_lines: Vec<String> = raw_logs.lines().map(str::to_string).collect();
    let log_line_count = log_lines.len();
    let log_q = log_query.read().trim().to_lowercase();
    let log_match_count = if log_q.is_empty() {
        0
    } else {
        log_lines
            .iter()
            .filter(|line| line.to_lowercase().contains(&log_q))
            .count()
    };
    let log_count_label = if log_q.is_empty() {
        format!(
            "{} line{}",
            log_line_count,
            if log_line_count == 1 { "" } else { "s" }
        )
    } else {
        format!(
            "{} match{}",
            log_match_count,
            if log_match_count == 1 { "" } else { "es" }
        )
    };
    let log_stream_class = if *wrap_logs.read() {
        "sd-log-stream build-log-stream wrap"
    } else {
        "sd-log-stream build-log-stream"
    };

    rsx! {
        // Note: The aside.fl-tray.build-log-tray wrapper is in builds.rs.
        header { class: "fl-tray-head",
            div { style: "display: flex; align-items: center; gap: 12px; min-width: 0; flex: 1;",
                svg {
                    width: "18", height: "18",
                    view_box: "0 0 24 24",
                    fill: "none", stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round", stroke_linejoin: "round",
                    style: "color: var(--cf-brand-purple); flex-shrink: 0;",
                    path { d: "M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" }
                    polyline { points: "3.27 6.96 12 12.01 20.73 6.96" }
                    line { x1: "12", y1: "22.08", x2: "12", y2: "12" }
                }
                div {
                    style: "min-width: 0;",
                    div { style: "display: flex; align-items: center; gap: 8px; flex-wrap: wrap;",
                        span { style: "font-weight: 700; font-size: 15px;", "{system_name}" }
                        span {
                            class: "chip {build_status_badge_class(build.status)}",
                            style: "font-size: 10px;",
                            span { class: "chip-dot", style: "background: {status_color(build.status)};" }
                            "{build.status_label()}"
                            if live {
                                span { class: "live-dot", style: "margin-left: 6px;" }
                            }
                        }
                    }
                    div {
                        class: "mono",
                        style: "font-size: 11px; color: var(--cf-text-muted); margin-top: 2px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;",
                        "{short_commit(&build.commit)} · {drv_preview}"
                    }
                }
            }
            div { style: "display: flex; gap: 6px; align-items: center; flex-shrink: 0;",
                if can_requeue && active {
                    button {
                        class: "btn btn-ghost focus-ring xs",
                        style: if build.status == BuildStatus::Stopping { "color: var(--cf-red);" } else { "" },
                        onclick: move |_| {
                            if build.status == BuildStatus::Stopping {
                                on_build_action.call(BuildAction::ForceCancel);
                            } else {
                                on_build_action.call(BuildAction::Stop);
                            }
                        },
                        if build.status == BuildStatus::Stopping { "Force kill" } else { "Cancel" }
                    }
                }
                if can_requeue && build.status == BuildStatus::Failed {
                    button {
                        class: "btn btn-ghost focus-ring xs",
                        onclick: move |_| on_build_action.call(BuildAction::Restart),
                        svg {
                            width: "12", height: "12",
                            view_box: "0 0 24 24",
                            fill: "none", stroke: "currentColor",
                            stroke_width: "2",
                            style: "margin-right: 4px;",
                            path { d: "M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74" }
                            path { d: "M21 3v9h-9" }
                            path { d: "M21 12A9 9 0 0 0 3.26 9.26" }
                        }
                        "Retry"
                    }
                }
                button {
                    class: "btn-icon focus-ring",
                    title: "Close",
                    onclick: move |_| on_close.call(()),
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
        }

        // Stats grid
        div { class: "ed-stats",
            div { class: "ed-stat",
                div { class: "ed-stat-label", "Queued" }
                div { class: "ed-stat-val", style: "font-size: 12.5px; font-weight: 600;", dangerous_inner_html: "{queued_dtg}" }
            }
            div { class: "ed-stat",
                div { class: "ed-stat-label", "Duration" }
                div { class: "ed-stat-val mono", "{duration_label}" }
            }
            div { class: "ed-stat",
                div { class: "ed-stat-label", "Derivations" }
                div { class: "ed-stat-val",
                    "{build.built_derivs}"
                    span { style: "font-size: 12px; color: var(--cf-text-muted);", "/{build.total_derivs}" }
                }
            }
            div { class: "ed-stat",
                div { class: "ed-stat-label", "Worker" }
                div { class: "ed-stat-val mono", style: "font-size: 12.5px;", "{worker_label}" }
            }
            div { class: "ed-stat",
                div { class: "ed-stat-label", "Arch" }
                div { class: "ed-stat-val mono", style: "font-size: 12.5px;", "{build.arch}" }
            }
        }

        // Tabs
        div {
            class: "sd-tabs",
            style: "padding: 0 16px; border-bottom: 1px solid var(--cf-card-border); flex-shrink: 0;",
            button {
                class: if active_tab == DetailTab::Logs { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
                onclick: move |_| on_tab_change.call(DetailTab::Logs),
                svg {
                    width: "12", height: "12",
                    view_box: "0 0 24 24",
                    fill: "none", stroke: "currentColor",
                    stroke_width: "2",
                    polyline { points: "4 17 10 11 4 5" }
                    line { x1: "12", y1: "19", x2: "20", y2: "19" }
                }
                "Log"
                if live { span { class: "live-dot", style: "margin-left: 4px;" } }
            }
            button {
                class: if active_tab == DetailTab::Details { "sd-tab focus-ring active" } else { "sd-tab focus-ring" },
                onclick: move |_| on_tab_change.call(DetailTab::Details),
                svg {
                    width: "12", height: "12",
                    view_box: "0 0 24 24",
                    fill: "none", stroke: "currentColor",
                    stroke_width: "2",
                    circle { cx: "12", cy: "12", r: "10" }
                    line { x1: "12", y1: "16", x2: "12", y2: "12" }
                    line { x1: "12", y1: "8", x2: "12.01", y2: "8" }
                }
                "Details"
            }
        }

        if active_tab == DetailTab::Details {
            div { class: "ed-body", style: "padding: 14px 16px;",
                dl { class: "kv-grid",
                        dt { "System" }
                        dd { class: "mono", "{system_name}" }
                        dt { "Flake" }
                        dd { "{build.flake}" }
                        dt { "Commit" }
                        dd { class: "mono", "{build.commit}" }
                        dt { "Worker" }
                        dd { class: "mono", "{details_worker_label}" }
                        dt { "Arch" }
                        dd { class: "mono", "{build.arch}" }
                        dt { "Derivations" }
                        dd { "{derivs_label}" }
                        dt { "Queued" }
                        dd { dangerous_inner_html: "{queued_dtg}" }
                        if let Some(completed_dtg) = completed_dtg.clone() {
                            dt { "{completed_label}" }
                            dd { dangerous_inner_html: "{completed_dtg}" }
                        }
                        dt { "Duration" }
                        dd { class: "mono", "{duration_label}" }
                        dt { "Attempts" }
                        dd { "{build.attempts}" }
                    }
                if has_drv_progress {
                    section { style: "margin-top: 18px;",
                        h3 { style: "font-size: 12px; font-weight: 600; margin: 0 0 8px; color: var(--cf-text-secondary);", "Derivation progress" }
                        {
                            let total = build.total_derivs as f64;
                            let cached_pct = (build.cached_derivs as f64 / total * 100.0).min(100.0);
                            let built_pct = ((build.built_derivs.saturating_sub(build.cached_derivs)) as f64 / total * 100.0).min(100.0);
                            let col = status_color(build.status);
                            rsx! {
                                div {
                                    style: "height: 6px; background: var(--cf-subtle-bg); border-radius: 99px; overflow: hidden; display: flex;",
                                    div { style: "width: {cached_pct}%; background: #34d399;" }
                                    div { style: "width: {built_pct}%; background: {col};" }
                                }
                                div {
                                    style: "font-size: 11px; color: var(--cf-text-muted); margin-top: 4px;",
                                    "{build.built_derivs} of {build.total_derivs} derivations"
                                    if let Some(ref pkg) = build.current_pkg {
                                        " · "
                                        span { class: "mono", style: "color: #60a5fa;", "building {pkg}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            div { style: "display: flex; flex-direction: column; flex: 1; min-height: 0;",
                div { style: "padding: 8px 16px; border-bottom: 1px solid var(--cf-divider); display: flex; gap: 10px; align-items: center; flex-shrink: 0;",
                    span { style: "font-size: 11px; color: var(--cf-text-muted); white-space: nowrap;", "{log_count_label}" }
                    div { style: "flex: 1;" }
                    label { style: "display: inline-flex; align-items: center; gap: 6px; font-size: 11px; color: var(--cf-text-muted);",
                        input {
                            r#type: "checkbox",
                            checked: *follow_logs.read(),
                            onchange: move |evt| follow_logs.set(evt.checked()),
                        }
                        "Follow"
                    }
                    label { style: "display: inline-flex; align-items: center; gap: 6px; font-size: 11px; color: var(--cf-text-muted);",
                        input {
                            r#type: "checkbox",
                            checked: *wrap_logs.read(),
                            onchange: move |evt| wrap_logs.set(evt.checked()),
                        }
                        "Wrap"
                    }
                    button {
                        class: "btn-icon focus-ring",
                        title: if *pause_logs.read() { "Resume" } else { "Pause" },
                        onclick: move |_| {
                            let paused = pause_logs();
                            pause_logs.set(!paused);
                        },
                        if *pause_logs.read() { "▶" } else { "Ⅱ" }
                    }
                    div { class: "log-search",
                        svg {
                            width: "13", height: "13",
                            view_box: "0 0 24 24",
                            fill: "none", stroke: "currentColor",
                            stroke_width: "2",
                            circle { cx: "11", cy: "11", r: "8" }
                            line { x1: "21", y1: "21", x2: "16.65", y2: "16.65" }
                        }
                        input {
                            class: "log-search-input",
                            placeholder: "Search log…",
                            value: "{log_query}",
                            oninput: move |evt| log_query.set(evt.value()),
                        }
                        if !log_q.is_empty() {
                            span { class: "log-search-count", "{log_match_count}" }
                        }
                    }
                    button {
                        class: "btn-icon focus-ring",
                        title: "Open log",
                        onclick: move |_| on_log.call(()),
                        svg {
                            width: "13", height: "13",
                            view_box: "0 0 24 24",
                            fill: "none", stroke: "currentColor",
                            stroke_width: "2",
                            polyline { points: "4 17 10 11 4 5" }
                            line { x1: "12", y1: "19", x2: "20", y2: "19" }
                        }
                    }
                }
                pre { class: "{log_stream_class}",
                    for (idx, line) in log_lines.iter().enumerate() {
                        {
                            let line = line.clone();
                            let is_match = !log_q.is_empty() && line.to_lowercase().contains(&log_q);
                            rsx! {
                                div {
                                    key: "{idx}",
                                    class: if is_match { "sd-log-line sd-log-info log-line-hit" } else { "sd-log-line sd-log-info" },
                                    span { class: "sd-log-t", "{idx + 1}" }
                                    span { class: "sd-log-lvl", "LOG" }
                                    span { class: "sd-log-m", "{line}" }
                                }
                            }
                        }
                    }
                    if live && !*pause_logs.read() {
                        div { class: "sd-log-caret", "▍" }
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

fn dtg_label(ts: DateTime<Utc>, relative: Option<&str>) -> String {
    const MONTHS: [&str; 12] = [
        "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
    ];
    let dtg = format!(
        "{:02}{:02}{:02}Z {} {:02}",
        ts.day(),
        ts.hour(),
        ts.minute(),
        MONTHS[ts.month0() as usize],
        ts.year().rem_euclid(100)
    );
    let local = ts.format("%b %-d, %Y, %H:%M UTC").to_string();
    match relative {
        Some(relative) if !relative.is_empty() => format!(
            "<span class=\"mono dtg\" title=\"{} · {}\">{}<span class=\"dtg-rel\"> · {}</span></span>",
            local, relative, dtg, relative
        ),
        _ => format!(
            "<span class=\"mono dtg\" title=\"{}\">{}</span>",
            local, dtg
        ),
    }
}

fn relative_label(ts: DateTime<Utc>) -> String {
    let secs = (Utc::now() - ts).num_seconds().max(0);
    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
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

/// Queue action button component.
#[component]
pub fn QueueActionButton(label: &'static str, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            class: "inline-flex items-center gap-2 px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
            onclick: move |evt| onclick.call(evt),
            span { class: "text-xs", "+" }
            "{label}"
        }
    }
}

/// Confirmation modal for build actions.
#[component]
pub fn ConfirmActionModal(
    action: PendingAction,
    on_cancel: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    let (title, description, confirm_label) = action_prompt(&action);

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4 cf-modal-overlay",
            onclick: move |_| on_cancel.call(()),
            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6 cf-modal-panel-30",
                onclick: |evt| evt.stop_propagation(),
                h3 { class: "text-lg font-semibold text-white mb-2", "{title}" }
                p { class: "text-sm {theme::text::SECONDARY} mb-6", "{description}" }
                div {
                    class: "flex gap-3",
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-gray-700 hover:bg-gray-600 text-white",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm text-white {theme::interactive::PRIMARY_BTN}",
                        onclick: move |_| on_confirm.call(()),
                        "{confirm_label}"
                    }
                }
            }
        }
    }
}

fn action_prompt(action: &PendingAction) -> (&'static str, String, &'static str) {
    match action {
        PendingAction::Queue(super::helpers::QueueAction::StartAll) => (
            "Start all workers?",
            "This resumes queue processing on all build workers.".to_string(),
            "Start All",
        ),
        PendingAction::Queue(super::helpers::QueueAction::PauseAll) => (
            "Pause all workers?",
            "Queued builds will remain queued until workers are resumed.".to_string(),
            "Pause All",
        ),
        PendingAction::Queue(super::helpers::QueueAction::DrainAll) => (
            "Drain all workers?",
            "Workers finish active builds but stop taking new queued work.".to_string(),
            "Drain All",
        ),
        PendingAction::Worker {
            worker_id,
            action: super::helpers::WorkerAction::Start,
        } => (
            "Start worker?",
            format!("Worker {worker_id} will resume processing queued builds."),
            "Start",
        ),
        PendingAction::Worker {
            worker_id,
            action: super::helpers::WorkerAction::Pause,
        } => (
            "Pause worker?",
            format!("Worker {worker_id} will stop taking new work immediately."),
            "Pause",
        ),
        PendingAction::Worker {
            worker_id,
            action: super::helpers::WorkerAction::Drain,
        } => (
            "Drain worker?",
            format!("Worker {worker_id} will finish active builds, then idle."),
            "Drain",
        ),
        PendingAction::Build {
            build_id,
            action: BuildAction::Stop,
        } => (
            "Stop build?",
            format!(
                "Build #{build_id} will send stop to the active systemd unit and mark canceled."
            ),
            "Stop",
        ),
        PendingAction::Build {
            build_id,
            action: BuildAction::ForceCancel,
        } => (
            "Force cancel build?",
            format!(
                "Build #{build_id} will be immediately marked as cancelled without waiting for builder confirmation. Use this for stuck builds."
            ),
            "Force Cancel",
        ),
        PendingAction::Build {
            build_id,
            action: BuildAction::Restart,
        } => (
            "Restart build?",
            format!(
                "Build #{build_id} will cancel current systemd execution and rerun build commands."
            ),
            "Restart",
        ),
        PendingAction::Build {
            build_id,
            action: BuildAction::RunNext,
        } => (
            "Run this build next?",
            format!("Build #{build_id} will be promoted to the front of the queued set."),
            "Prioritize",
        ),
        PendingAction::Build {
            action: BuildAction::MoveUp | BuildAction::MoveDown,
            ..
        } => (
            "Reorder build?",
            "Adjust queued build priority order.".to_string(),
            "Move",
        ),
    }
}
