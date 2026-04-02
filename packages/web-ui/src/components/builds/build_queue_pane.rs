//! Build queue pane component for the builds control center.

use dioxus::prelude::*;

use crate::components::layout::Card;
use crate::theme;

use super::helpers::{
    build_status_badge_class, extract_system_name, queue_row_style, short_commit, BuildAction,
    BuildItem, BuildStatus,
};

/// View mode for the build queue display.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum QueueViewMode {
    #[default]
    Cards,
    Table,
}

/// Build queue pane showing all queued and active builds.
#[component]
pub fn BuildQueuePane(
    builds: Vec<BuildItem>,
    selected_id: Signal<Option<i32>>,
    on_build_action: EventHandler<(i32, BuildAction)>,
) -> Element {
    let mut search = use_signal(String::new);
    let mut view_mode = use_signal(QueueViewMode::default);

    let filtered: Vec<BuildItem> = builds
        .into_iter()
        .filter(|b| {
            let q = search.read().trim().to_lowercase();
            if q.is_empty() {
                true
            } else {
                b.hostname.to_lowercase().contains(&q)
                    || b.flake.to_lowercase().contains(&q)
                    || b.commit.to_lowercase().contains(&q)
            }
        })
        .collect();

    rsx! {
        Card {
            title: Some("Queue".to_string()),
            children: rsx! {
                div {
                    class: "space-y-3",
                    // Search and view mode toggle row
                    div {
                        class: "flex items-center gap-2",
                        input {
                            class: "flex-1 rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                            r#type: "search",
                            placeholder: "Search by host, flake, or commit...",
                            value: "{search.read()}",
                            oninput: move |evt| search.set(evt.value()),
                        }
                        // View mode toggle buttons
                        {
                            let cards_active = if *view_mode.read() == QueueViewMode::Cards {
                                "bg-cyan-600/20 text-cyan-300"
                            } else {
                                theme::text::MUTED
                            };
                            let table_active = if *view_mode.read() == QueueViewMode::Table {
                                "bg-cyan-600/20 text-cyan-300"
                            } else {
                                theme::text::MUTED
                            };
                            rsx! {
                                div {
                                    class: "inline-flex rounded-lg border {theme::surface::CARD_BORDER} overflow-hidden shrink-0",
                                    "data-testid": "queue-view-toggle",
                                    button {
                                        class: "px-3 py-2 text-xs transition-colors {cards_active}",
                                        title: "Card view",
                                        "data-testid": "queue-view-cards",
                                        onclick: move |_| view_mode.set(QueueViewMode::Cards),
                                        // Cards icon (grid)
                                        svg {
                                            class: "w-4 h-4",
                                            fill: "none",
                                            stroke: "currentColor",
                                            stroke_width: "2",
                                            view_box: "0 0 24 24",
                                            path {
                                                d: "M4 5a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1H5a1 1 0 01-1-1V5zM14 5a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1h-4a1 1 0 01-1-1V5zM4 15a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1H5a1 1 0 01-1-1v-4zM14 15a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1h-4a1 1 0 01-1-1v-4z"
                                            }
                                        }
                                    }
                                    button {
                                        class: "px-3 py-2 text-xs transition-colors {table_active}",
                                        title: "Table view",
                                        "data-testid": "queue-view-table",
                                        onclick: move |_| view_mode.set(QueueViewMode::Table),
                                        // Table icon (list)
                                        svg {
                                            class: "w-4 h-4",
                                            fill: "none",
                                            stroke: "currentColor",
                                            stroke_width: "2",
                                            view_box: "0 0 24 24",
                                            path {
                                                d: "M4 6h16M4 10h16M4 14h16M4 18h16"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Render based on view mode
                    if *view_mode.read() == QueueViewMode::Table {
                        BuildQueueTable {
                            builds: filtered,
                            selected_id: selected_id,
                            on_build_action: on_build_action,
                        }
                    } else {
                        BuildQueueCards {
                            builds: filtered,
                            selected_id: selected_id,
                            on_build_action: on_build_action,
                        }
                    }
                }
            }
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
                            if matches!(build.status, BuildStatus::Building | BuildStatus::Restarting) {
                                button {
                                    class: "text-xs text-red-400 hover:text-red-300 px-3 py-1.5 rounded hover:bg-red-500/10 transition-colors min-h-[44px]",
                                    onclick: move |evt| {
                                        evt.stop_propagation();
                                        on_build_action.call((build.id, BuildAction::Stop));
                                    },
                                    "Stop"
                                }
                            }
                            // Restart only valid for terminal statuses
                            if matches!(build.status, BuildStatus::Failed | BuildStatus::Complete | BuildStatus::Canceled) {
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
) -> Element {
    rsx! {
        div {
            class: "max-h-[56vh] overflow-auto",
            "data-testid": "build-queue-table",
            table {
                class: "w-full text-xs",
                thead {
                    class: "sticky top-0 {theme::surface::CARD_BG} border-b {theme::surface::CARD_BORDER}",
                    tr {
                        th { class: "text-left px-2 py-2 {theme::text::MUTED} font-medium", "Status" }
                        th { class: "text-left px-2 py-2 {theme::text::MUTED} font-medium", "System" }
                        th { class: "text-left px-2 py-2 {theme::text::MUTED} font-medium", "Flake" }
                        th { class: "text-left px-2 py-2 {theme::text::MUTED} font-medium", "Commit" }
                        th { class: "text-left px-2 py-2 {theme::text::MUTED} font-medium", "Worker" }
                        th { class: "text-left px-2 py-2 {theme::text::MUTED} font-medium", "Time" }
                        th { class: "text-right px-2 py-2 {theme::text::MUTED} font-medium", "Actions" }
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
                                    // Status
                                    td {
                                        class: "px-2 py-2",
                                        span {
                                            class: "inline-flex px-2 py-0.5 text-[10px] uppercase rounded border {build_status_badge_class(build.status)}",
                                            "{build.status_label()}"
                                        }
                                    }
                                    // System
                                    td {
                                        class: "px-2 py-2 {theme::text::PRIMARY} font-medium truncate max-w-[120px]",
                                        title: "{extract_system_name(&build.hostname)}",
                                        "{extract_system_name(&build.hostname)}"
                                    }
                                    // Flake
                                    td {
                                        class: "px-2 py-2",
                                        span {
                                            class: "inline-flex px-2 py-0.5 text-[10px] rounded border cf-chip-blue",
                                            "{build.flake}"
                                        }
                                    }
                                    // Commit
                                    td {
                                        class: "px-2 py-2 font-mono {theme::text::SECONDARY}",
                                        title: "{build.commit}",
                                        "{short_commit(&build.commit)}"
                                    }
                                    // Worker
                                    td {
                                        class: "px-2 py-2 {theme::text::MUTED}",
                                        "{build.worker_id}"
                                    }
                                    // Time (runtime or queued_for)
                                    td {
                                        class: "px-2 py-2 {theme::text::MUTED} whitespace-nowrap",
                                        if let Some(ref runtime) = build.runtime {
                                            span {
                                                class: "text-teal-400",
                                                "{runtime}"
                                            }
                                        } else {
                                            "{build.queued_for}"
                                        }
                                    }
                                    // Actions
                                    td {
                                        class: "px-2 py-2 text-right",
                                        div {
                                            class: "inline-flex items-center gap-1",
                                            if matches!(build.status, BuildStatus::Building | BuildStatus::Restarting) {
                                                button {
                                                    class: "text-[10px] text-red-400 hover:text-red-300 px-2 py-1 rounded hover:bg-red-500/10 transition-colors",
                                                    onclick: move |evt| {
                                                        evt.stop_propagation();
                                                        on_build_action.call((build.id, BuildAction::Stop));
                                                    },
                                                    "Stop"
                                                }
                                            }
                                            // Restart only valid for terminal statuses
                                            if matches!(build.status, BuildStatus::Failed | BuildStatus::Complete | BuildStatus::Canceled) {
                                                button {
                                                    class: "text-[10px] px-2 py-1 rounded transition-colors cf-action-link",
                                                    onclick: move |evt| {
                                                        evt.stop_propagation();
                                                        on_build_action.call((build.id, BuildAction::Restart));
                                                    },
                                                    "Restart"
                                                }
                                            }
                                            if build.status == BuildStatus::Queued {
                                                button {
                                                    class: "text-[10px] text-cyan-300 hover:text-cyan-200 px-2 py-1 rounded hover:bg-cyan-500/10 transition-colors",
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
            }
        }
    }
}
