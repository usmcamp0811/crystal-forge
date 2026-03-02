//! Build queue panel and row components.

use dioxus::prelude::*;

use crate::api::models::{BuildQueueItem, BuildQueueSummary, BuildStatus};
use crate::theme;

use super::format_elapsed;

/// Build queue panel with active build items.
#[component]
pub fn BuildQueuePanel(
    queue: BuildQueueSummary,
    #[props(default)] flake_filter: Option<String>,
) -> Element {
    let _total_active = queue.building_count + queue.queued_count;
    let mut active_items: Vec<BuildQueueItem> = queue
        .items
        .iter()
        .filter(|item| item.status.is_active())
        .cloned()
        .collect();

    active_items.sort_by_key(|item| {
        if item.status == BuildStatus::Building {
            (0i32, item.started_at.unwrap_or(item.queued_at))
        } else {
            (1i32, item.queued_at)
        }
    });

    let max_items = if flake_filter.is_some() { 4 } else { 5 };
    let total_items = active_items.len();
    let filtered_items: Vec<BuildQueueItem> = active_items.into_iter().take(max_items).collect();
    let remaining_count = total_items.saturating_sub(filtered_items.len());

    let mut queued_rank = 0;
    let ordered_rows: Vec<(BuildQueueItem, Option<String>)> = filtered_items
        .into_iter()
        .map(|item| {
            let label = if item.status == BuildStatus::Building {
                Some("Active".to_string())
            } else {
                queued_rank += 1;
                if queued_rank == 1 {
                    Some("Next".to_string())
                } else {
                    Some(format!("Queued #{queued_rank}"))
                }
            };
            (item, label)
        })
        .collect();

    rsx! {
        div {
            class: "flex flex-col h-full",
            "data-testid": "build-queue",

            if let Some(ref flake_name) = flake_filter {
                div {
                    class: "text-xs text-blue-400 mb-2 flex items-center gap-1 shrink-0",
                    svg {
                        class: "w-3 h-3",
                        fill: "none",
                        stroke: "currentColor",
                        view_box: "0 0 24 24",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            stroke_width: "2",
                            d: "M3 4a1 1 0 011-1h16a1 1 0 011 1v2.586a1 1 0 01-.293.707l-6.414 6.414a1 1 0 00-.293.707V17l-4 4v-6.586a1 1 0 00-.293-.707L3.293 7.293A1 1 0 013 6.586V4z"
                        }
                    }
                    span { "{flake_name}" }
                }
            }

            div {
                class: "flex items-center justify-between mb-3",
                div {
                    class: "text-xs {theme::text::SECONDARY} uppercase tracking-wide",
                    "Build queue"
                }
                div {
                    class: "text-[10px] {theme::text::MUTED}",
                    "Ordered by next build"
                }
            }

            if ordered_rows.is_empty() {
                p { class: "text-sm {theme::text::SECONDARY}", "No builds running or queued." }
            } else {
                div {
                    class: "flex-1 min-h-0 overflow-hidden space-y-2",
                    for (item, label) in ordered_rows {
                        BuildQueueRow { item, position_label: label }
                    }
                }
                if remaining_count > 0 {
                    p {
                        class: "text-[10px] {theme::text::MUTED} mt-2",
                        "+{remaining_count} more builds queued"
                    }
                }
            }
        }
    }
}

/// A single row in the build queue.
#[component]
pub fn BuildQueueRow(
    item: BuildQueueItem,
    #[props(default)] position_label: Option<String>,
) -> Element {
    let status_class = match item.status {
        BuildStatus::Building => "text-cyan-400",
        BuildStatus::Queued => "text-blue-400",
        BuildStatus::Complete => "text-emerald-400",
        BuildStatus::Failed => "text-red-400",
        BuildStatus::Idle => "text-gray-400",
    };
    let status_dot_color = match item.status {
        BuildStatus::Building => "#42ff65",
        BuildStatus::Queued => "#e57c00",
        BuildStatus::Complete => "#10b981",
        BuildStatus::Failed => "#ef4444",
        BuildStatus::Idle => "#6b7280",
    };
    let status_label = item.status.label();
    let short_hash = item.commit_hash.chars().take(7).collect::<String>();
    let elapsed = item.elapsed_secs.map(format_elapsed);

    rsx! {
        Link {
            class: "flex items-center justify-between p-3 rounded-lg {theme::surface::SUBTLE_BG} transition {theme::interactive::HOVER_BG} hover:border {theme::surface::CARD_BORDER}",
            to: crate::routes::Route::BuildsView {},
            div {
                class: "flex items-center gap-3 min-w-0 flex-1",
                svg {
                    class: "w-2 h-2 shrink-0",
                    view_box: "0 0 8 8",
                    circle { cx: "4", cy: "4", r: "4", fill: "{status_dot_color}" }
                }
                div {
                    class: "min-w-0 flex-1",
                    div {
                        class: "flex items-center gap-2",
                        span { class: "{theme::text::PRIMARY} text-sm font-medium truncate", "{item.hostname}" }
                        span { class: "text-[10px] font-mono {theme::text::MUTED}", "{short_hash}" }
                    }
                    if let Some(ref msg) = item.commit_message {
                        p { class: "text-xs {theme::text::SECONDARY} truncate", "{msg}" }
                    }
                }
            }
            div {
                class: "text-right shrink-0 ml-3",
                if let Some(ref elapsed) = elapsed {
                    p { class: "text-xs {theme::text::PRIMARY} font-semibold tabular-nums", "{elapsed}" }
                }
                if let Some(ref label) = position_label {
                    p { class: "text-[10px] uppercase tracking-wide {status_class}", "{label}" }
                } else {
                    p { class: "text-[10px] uppercase tracking-wide {status_class}", "{status_label}" }
                }
            }
        }
    }
}
