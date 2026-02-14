//! Flake commit timeline component for the dashboard.
//!
//! Displays a horizontal git graph-style timeline of commits for monitored flakes,
//! with commits hanging below the main line and details shown on hover.

use dioxus::prelude::*;

use crate::api::models::{FlakeCommit, FlakeTimeline};
use crate::theme;

/// Minimum pixels between commit nodes.
const MIN_GAP_PX: f64 = 80.0;
/// Maximum pixels between commit nodes.
const MAX_GAP_PX: f64 = 160.0;

/// View mode for the timeline display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimelineViewMode {
    /// Single combined timeline showing all flakes.
    #[default]
    Combined,
    /// Stacked timelines, one per flake.
    Stacked,
    /// Filtered to show only one flake.
    SingleFlake,
}

/// A commit with its calculated position for rendering.
#[derive(Clone, PartialEq)]
struct PositionedCommit {
    commit: FlakeCommit,
    flake_name: Option<String>,
    /// X position in pixels from the left edge.
    x_position: f64,
}

/// The main flake timeline widget for the dashboard.
#[component]
pub fn FlakeTimelineWidget(timelines: Vec<FlakeTimeline>) -> Element {
    let mut view_mode = use_signal(|| TimelineViewMode::Combined);
    let mut selected_flake = use_signal(|| 0usize);

    let flake_names: Vec<String> = timelines.iter().map(|t| t.flake_name.clone()).collect();

    rsx! {
        div {
            class: "space-y-4",
            "data-testid": "flake-timeline-widget",

            // Header with view mode toggle
            div {
                class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3",
                div {
                    class: "flex items-center gap-3",
                    h3 { class: "{theme::typography::SECTION_TITLE} text-white", "Commit Timeline" }
                    // Legend inline with title
                    div {
                        class: "flex items-center gap-3",
                        "data-testid": "timeline-legend",
                        LegendDot { color: "bg-emerald-500", label: "Latest" }
                        LegendDot { color: "bg-yellow-500", label: "1 behind" }
                        LegendDot { color: "bg-orange-500", label: "2 behind" }
                        LegendDot { color: "bg-red-500", label: "3+ behind" }
                    }
                }

                div {
                    class: "flex items-center gap-2",

                    // View mode toggle
                    ViewModeToggle {
                        mode: *view_mode.read(),
                        on_change: move |mode| view_mode.set(mode)
                    }

                    // Flake selector (only visible in SingleFlake mode)
                    if *view_mode.read() == TimelineViewMode::SingleFlake {
                        select {
                            class: "rounded-lg px-3 py-1.5 text-sm {theme::interactive::INPUT} {theme::text::SECONDARY}",
                            value: "{selected_flake}",
                            onchange: move |evt| {
                                if let Ok(idx) = evt.value().parse::<usize>() {
                                    selected_flake.set(idx);
                                }
                            },
                            for (idx, name) in flake_names.iter().enumerate() {
                                option { value: "{idx}", "{name}" }
                            }
                        }
                    }
                }
            }

            // Timeline content based on view mode
            match *view_mode.read() {
                TimelineViewMode::Combined => rsx! {
                    CombinedTimeline { timelines: timelines.clone() }
                },
                TimelineViewMode::Stacked => rsx! {
                    StackedTimelines { timelines: timelines.clone() }
                },
                TimelineViewMode::SingleFlake => rsx! {
                    if let Some(timeline) = timelines.get(*selected_flake.read()) {
                        SingleFlakeTimeline { timeline: timeline.clone() }
                    }
                },
            }
        }
    }
}

/// Small legend dot with label.
#[component]
fn LegendDot(color: &'static str, label: &'static str) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-1",
            div { class: "w-2 h-2 rounded-full {color}" }
            span { class: "text-[10px] {theme::text::MUTED}", "{label}" }
        }
    }
}

/// Toggle buttons for switching between view modes.
#[component]
fn ViewModeToggle(mode: TimelineViewMode, on_change: EventHandler<TimelineViewMode>) -> Element {
    let combined_class = if mode == TimelineViewMode::Combined {
        "px-3 py-1.5 text-xs font-medium transition bg-gray-700 text-white"
    } else {
        "px-3 py-1.5 text-xs font-medium transition text-gray-400 hover:text-white hover:bg-gray-800"
    };
    let stacked_class = if mode == TimelineViewMode::Stacked {
        "px-3 py-1.5 text-xs font-medium transition bg-gray-700 text-white"
    } else {
        "px-3 py-1.5 text-xs font-medium transition text-gray-400 hover:text-white hover:bg-gray-800"
    };
    let filter_class = if mode == TimelineViewMode::SingleFlake {
        "px-3 py-1.5 text-xs font-medium transition bg-gray-700 text-white"
    } else {
        "px-3 py-1.5 text-xs font-medium transition text-gray-400 hover:text-white hover:bg-gray-800"
    };

    rsx! {
        div {
            class: "inline-flex rounded-lg border {theme::surface::CARD_BORDER} overflow-hidden",
            button {
                class: "{combined_class}",
                onclick: move |_| on_change.call(TimelineViewMode::Combined),
                "Combined"
            }
            button {
                class: "{stacked_class}",
                onclick: move |_| on_change.call(TimelineViewMode::Stacked),
                "Stacked"
            }
            button {
                class: "{filter_class}",
                onclick: move |_| on_change.call(TimelineViewMode::SingleFlake),
                "Filter"
            }
        }
    }
}

/// Calculate time-proportional positions for commits.
fn calculate_positions(commits: &[(Option<String>, FlakeCommit)]) -> Vec<PositionedCommit> {
    if commits.is_empty() {
        return vec![];
    }

    if commits.len() == 1 {
        return vec![PositionedCommit {
            commit: commits[0].1.clone(),
            flake_name: commits[0].0.clone(),
            x_position: MIN_GAP_PX,
        }];
    }

    // Sort by time (oldest first)
    let mut sorted: Vec<_> = commits.to_vec();
    sorted.sort_by(|a, b| a.1.committed_at.cmp(&b.1.committed_at));

    // Calculate time gaps between consecutive commits
    let gaps: Vec<i64> = sorted
        .windows(2)
        .map(|w| {
            let duration = w[1].1.committed_at.signed_duration_since(w[0].1.committed_at);
            duration.num_seconds().max(1)
        })
        .collect();

    // Use logarithmic scaling
    let max_gap_seconds = gaps.iter().copied().max().unwrap_or(1) as f64;
    let log_max = (1.0 + max_gap_seconds).ln();

    let scaled_gaps: Vec<f64> = gaps
        .iter()
        .map(|&secs| {
            let log_secs = (1.0 + secs as f64).ln();
            let normalized = log_secs / log_max;
            MIN_GAP_PX + (MAX_GAP_PX - MIN_GAP_PX) * normalized
        })
        .collect();

    // Build positioned commits
    let mut result = Vec::with_capacity(sorted.len());
    let mut x = MIN_GAP_PX;

    for (i, (flake_name, commit)) in sorted.into_iter().enumerate() {
        result.push(PositionedCommit {
            commit,
            flake_name,
            x_position: x,
        });

        if i < scaled_gaps.len() {
            x += scaled_gaps[i];
        }
    }

    result
}

/// Combined timeline showing all flakes' commits merged chronologically.
#[component]
fn CombinedTimeline(timelines: Vec<FlakeTimeline>) -> Element {
    let all_commits: Vec<(Option<String>, FlakeCommit)> = timelines
        .iter()
        .flat_map(|t| {
            t.commits
                .iter()
                .map(|c| (Some(t.flake_name.clone()), c.clone()))
                .collect::<Vec<_>>()
        })
        .collect();

    let positioned = calculate_positions(&all_commits);
    let total_width = positioned.last().map(|p| p.x_position + MIN_GAP_PX).unwrap_or(200.0);

    rsx! {
        TimelineGraph {
            positioned_commits: positioned,
            total_width: total_width,
            show_flake_labels: true,
            testid: "combined-timeline"
        }
    }
}

/// Stacked timelines showing each flake separately.
#[component]
fn StackedTimelines(timelines: Vec<FlakeTimeline>) -> Element {
    rsx! {
        div {
            class: "space-y-8",
            "data-testid": "stacked-timelines",
            for timeline in timelines {
                SingleFlakeTimeline { timeline }
            }
        }
    }
}

/// Timeline for a single flake.
#[component]
fn SingleFlakeTimeline(timeline: FlakeTimeline) -> Element {
    let commits: Vec<(Option<String>, FlakeCommit)> = timeline
        .commits
        .iter()
        .map(|c| (None, c.clone()))
        .collect();

    let positioned = calculate_positions(&commits);
    let total_width = positioned.last().map(|p| p.x_position + MIN_GAP_PX).unwrap_or(200.0);

    rsx! {
        div {
            class: "space-y-2",
            // Flake header
            div {
                class: "flex items-center gap-2",
                span { class: "text-sm font-medium text-white", "{timeline.flake_name}" }
                span { class: "{theme::text::MUTED} text-xs font-mono", "{timeline.repo_url}" }
            }

            TimelineGraph {
                positioned_commits: positioned,
                total_width: total_width,
                show_flake_labels: false,
                testid: "single-timeline"
            }
        }
    }
}

/// The timeline graph with horizontal line and commits hanging below.
#[component]
fn TimelineGraph(
    positioned_commits: Vec<PositionedCommit>,
    total_width: f64,
    show_flake_labels: bool,
    testid: &'static str,
) -> Element {
    if positioned_commits.is_empty() {
        return rsx! {
            p { class: "{theme::text::MUTED}", "No commits to display." }
        };
    }

    let width_px = total_width.max(300.0) as i32;
    let first_x = positioned_commits.first().map(|p| p.x_position).unwrap_or(0.0) as i32;
    let last_x = positioned_commits.last().map(|p| p.x_position).unwrap_or(0.0) as i32;
    let line_width = last_x - first_x;

    // Line is at y=24px (centered), content hangs below to ~90px
    let line_y = 24;
    let container_height = 100;

    rsx! {
        div {
            // Custom scrollbar styling: thin, subtle, only horizontal
            class: "overflow-x-auto overflow-y-hidden scrollbar-thin scrollbar-thumb-gray-700 scrollbar-track-transparent",
            style: "scrollbar-width: thin; scrollbar-color: #374151 transparent;",
            "data-testid": "{testid}",
            div {
                class: "relative",
                style: "width: {width_px}px; height: {container_height}px;",

                // Main horizontal line
                div {
                    class: "absolute h-0.5 bg-gray-600 rounded-full",
                    style: "left: {first_x}px; width: {line_width}px; top: {line_y}px;"
                }

                // Colored segments on top of the gray line
                for (i, pc) in positioned_commits.iter().enumerate() {
                    if i > 0 {
                        {
                            let prev = &positioned_commits[i - 1];
                            let seg_start = prev.x_position as i32;
                            let seg_width = (pc.x_position - prev.x_position) as i32;
                            let seg_color = commits_behind_bg(pc.commit.commits_behind);
                            rsx! {
                                div {
                                    class: "absolute h-0.5 rounded-full {seg_color}",
                                    style: "left: {seg_start}px; width: {seg_width}px; top: {line_y}px; z-index: 1;"
                                }
                            }
                        }
                    }
                }

                // Commit nodes ON the line
                for pc in positioned_commits.iter() {
                    CommitNode {
                        commit: pc.commit.clone(),
                        flake_name: pc.flake_name.clone(),
                        show_flake_label: show_flake_labels,
                        x_position: pc.x_position,
                        line_y: line_y
                    }
                }
            }
        }
    }
}

/// A single commit node centered ON the line with content below.
#[component]
fn CommitNode(
    commit: FlakeCommit,
    flake_name: Option<String>,
    show_flake_label: bool,
    x_position: f64,
    line_y: i32,
) -> Element {
    let short_hash = commit.hash.chars().take(7).collect::<String>();
    let node_bg = commits_behind_bg(commit.commits_behind);

    let behind_text = if commit.commits_behind == 0 {
        "Latest".to_string()
    } else {
        let plural = if commit.commits_behind == 1 { "" } else { "s" };
        format!("{} commit{} behind", commit.commits_behind, plural)
    };

    let x_px = x_position as i32;
    let system_plural = if commit.system_count == 1 { "" } else { "s" };

    // Node is 10px diameter, center it on the line
    let node_size = 10;
    let node_top = line_y - (node_size / 2);
    // Content starts below the node
    let content_top = line_y + (node_size / 2) + 4;

    rsx! {
        div {
            class: "absolute group",
            style: "left: {x_px}px; top: 0; transform: translateX(-50%);",
            "data-testid": "commit-node",
            "data-commits-behind": "{commit.commits_behind}",

            // Node circle centered ON the line
            div {
                class: "absolute left-1/2 -translate-x-1/2 z-10 rounded-full border-2 border-gray-900 cursor-pointer {node_bg}",
                style: "width: {node_size}px; height: {node_size}px; top: {node_top}px;"
            }

            // Content below the node
            div {
                class: "absolute left-1/2 -translate-x-1/2 flex flex-col items-center cursor-pointer",
                style: "top: {content_top}px; min-width: 50px;",

                // System count badge
                if commit.system_count > 0 {
                    div {
                        class: "px-1.5 py-0.5 rounded-full text-[9px] font-bold {node_bg} text-gray-900 mb-0.5",
                        "{commit.system_count}"
                    }
                }

                // Commit hash
                span {
                    class: "text-[9px] font-mono {theme::text::MUTED} group-hover:text-white transition",
                    "{short_hash}"
                }

                // Flake label
                if show_flake_label {
                    if let Some(ref name) = flake_name {
                        span {
                            class: "text-[8px] {theme::text::MUTED} truncate max-w-[50px]",
                            "{name}"
                        }
                    }
                }
            }

            // Hover popup - appears below the commit info
            div {
                class: "absolute left-1/2 -translate-x-1/2 opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none z-50",
                style: "top: 90px; min-width: 240px;",

                div {
                    class: "bg-gray-800 border {theme::surface::CARD_BORDER} rounded-lg p-4 shadow-2xl",

                    // Header with hash and status badge
                    div {
                        class: "flex items-center justify-between mb-2",
                        span {
                            class: "text-sm font-mono text-white font-medium",
                            "{short_hash}"
                        }
                        span {
                            class: "text-[10px] px-2 py-0.5 rounded-full {node_bg} text-gray-900 font-semibold",
                            "{behind_text}"
                        }
                    }

                    // Commit message
                    p {
                        class: "text-sm text-white mb-2",
                        "{commit.message}"
                    }

                    // Author
                    p {
                        class: "text-xs {theme::text::MUTED} mb-3",
                        "by {commit.author}"
                    }

                    // Divider
                    div { class: "h-px bg-gray-700 mb-3" }

                    // Systems info
                    div {
                        class: "text-xs {theme::text::SECONDARY} mb-2",
                        "{commit.system_count} system{system_plural} deployed at this commit"
                    }

                    // System hostnames
                    if !commit.systems.is_empty() && commit.systems.len() <= 10 {
                        div {
                            class: "flex flex-wrap gap-1",
                            for system in commit.systems.iter() {
                                span {
                                    class: "text-[10px] px-1.5 py-0.5 rounded bg-gray-700 text-gray-300 font-mono",
                                    "{system}"
                                }
                            }
                        }
                    }
                    if commit.systems.len() > 10 {
                        div {
                            class: "text-[10px] {theme::text::MUTED} mt-1",
                            "+{commit.systems.len() - 10} more systems"
                        }
                    }
                }
            }
        }
    }
}

/// Get the background color class based on how many commits behind.
fn commits_behind_bg(behind: i64) -> &'static str {
    match behind {
        0 => "bg-emerald-500",
        1 => "bg-yellow-500",
        2 => "bg-orange-500",
        _ => "bg-red-500",
    }
}
