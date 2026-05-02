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
    on_build_action: EventHandler<(i32, BuildAction)>,
    on_log: EventHandler<i32>,
) -> Element {
    let filtered = builds;

    rsx! {
        BuildQueueTable {
            builds: filtered,
            selected_id: selected_id,
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
                            if matches!(build.status, BuildStatus::Failed | BuildStatus::Complete | BuildStatus::Cancelled) {
                                button {
                                    class: "text-xs px-3 py-1.5 rounded transition-colors cf-action-link min-h-[44px]",
                                    onclick: move |evt| {
                                        evt.stop_propagation();
                                        on_build_action.call((build.id, BuildAction::Restart));
                                    },
                                    "Restart"
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
    on_build_action: EventHandler<(i32, BuildAction)>,
    on_log: EventHandler<i32>,
) -> Element {
    rsx! {
        div {
            class: "min-h-[220px] max-h-[56vh] overflow-auto",
            "data-testid": "build-queue-table",
            table {
                class: "w-full text-xs",
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
                            let row_class = if is_selected {
                                "cf-queue-row-selected cursor-pointer"
                            } else {
                                "cf-queue-row hover:bg-white/5 cursor-pointer"
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
                                            // Line 1: Package/system name (bold)
                                            p { class: "text-[13px] font-semibold leading-[1.15] {theme::text::PRIMARY}", "{extract_system_name(&build.hostname)}" }
                                            // Line 2: Derivation/commit message (mono, muted)
                                            p { class: "text-[10px] leading-4 font-mono {theme::text::MUTED} truncate max-w-[18rem]", "{truncate_with_ellipsis(&build.summary, 40)}" }
                                            // Line 3: Flake · full commit
                                            p { class: "text-[10px] leading-4 {theme::text::MUTED}",
                                                "{build.flake} · "
                                                span { class: "font-mono", "{build.commit}" }
                                            }
                                        }
                                    }
                                    td {
                                        class: "px-3 py-2",
                                        span {
                                            class: "inline-flex items-center gap-1.5 px-2 py-0.5 text-[10px] uppercase rounded border {build_status_badge_class(build.status)}",
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
                                        class: "px-3 py-2 w-[110px]",
                                        if matches!(build.status, BuildStatus::Building | BuildStatus::Stopping) {
                                            div {
                                                class: "h-1.5 bg-slate-800 rounded-full overflow-hidden",
                                                div { class: "h-full bg-cyan-400", style: "width: 56%" }
                                            }
                                        } else {
                                            span { class: "text-xs {theme::text::MUTED}", "—" }
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
                                        div {
                                            class: "inline-flex items-center gap-1.5",
                                            button {
                                                class: "btn-icon focus-ring",
                                                title: "Logs",
                                                onclick: move |evt| {
                                                    evt.stop_propagation();
                                                    // Select the build and open log modal immediately (JSX parity)
                                                    selected_id.set(Some(build.id));
                                                    on_log.call(build.id);
                                                },
                                                "⌘"
                                            }
                                            if let Some(cancel_action) = cancel_action_for_status(build.status) {
                                                button {
                                                    class: "btn-icon focus-ring",
                                                    title: "Cancel",
                                                    onclick: move |evt| {
                                                        evt.stop_propagation();
                                                        on_build_action.call((build.id, cancel_action));
                                                    },
                                                    "✕"
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
