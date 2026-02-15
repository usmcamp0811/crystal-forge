//! Flake commit timeline component for the dashboard.
//!
//! Displays a horizontal git graph-style timeline of commits for monitored flakes,
//! with commits hanging below the main line and details shown on hover.

use dioxus::prelude::*;
use std::collections::HashSet;

use crate::api::models::{BuildStatus, FlakeCommit, FlakeTimeline};
use crate::theme;
use chrono::TimeZone;

/// Minimum pixels between commit nodes (prevents overlap during bursts).
const MIN_GAP_PX: f64 = 32.0;
/// Base time scale (seconds) for log spacing.
const TIME_SCALE_SECONDS: f64 = 60.0 * 60.0;
/// Pixels per log-scaled time unit.
const TIME_SCALE_PX: f64 = 80.0;

/// View mode for the timeline display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimelineViewMode {
    /// Single combined timeline showing all flakes (or filtered subset).
    #[default]
    Combined,
    /// Stacked timelines, one per flake.
    Stacked,
    /// Filtered to show only one flake (legacy, kept for compatibility).
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

#[derive(Clone, PartialEq)]
struct TimelineScale {
    ticks: Vec<TimelineTick>,
}

#[derive(Clone, PartialEq)]
struct TimelineTick {
    x_position: f64,
    label: String,
}

/// The main flake timeline widget for the dashboard.
///
/// Props:
/// - `timelines`: The list of flake timelines to display
/// - `selected_flake_indices`: Set of selected flake indices (empty = all selected)
/// - `on_filter_change`: Callback when filter selection changes
#[component]
pub fn FlakeTimelineWidget(
    timelines: Vec<FlakeTimeline>,
    #[props(default)] selected_flake_indices: HashSet<usize>,
    #[props(default)] on_filter_change: Option<EventHandler<HashSet<usize>>>,
) -> Element {
    // Local state for dropdown visibility
    let mut dropdown_open = use_signal(|| false);
    // Local state for view mode (Stacked vs Combined)
    let mut view_mode = use_signal(|| TimelineViewMode::Combined);

    let flake_names: Vec<String> = timelines.iter().map(|t| t.flake_name.clone()).collect();
    let flake_count = flake_names.len();
    let is_all_selected = selected_flake_indices.is_empty();

    // Get display label for the filter button
    let filter_label = if is_all_selected {
        "All Flakes".to_string()
    } else if selected_flake_indices.len() == 1 {
        let idx = *selected_flake_indices.iter().next().unwrap();
        flake_names
            .get(idx)
            .cloned()
            .unwrap_or_else(|| "1 flake".to_string())
    } else {
        format!("{} flakes", selected_flake_indices.len())
    };

    // Filter timelines based on selection
    let filtered_timelines: Vec<FlakeTimeline> = if is_all_selected {
        timelines.clone()
    } else {
        timelines
            .iter()
            .enumerate()
            .filter(|(idx, _)| selected_flake_indices.contains(idx))
            .map(|(_, t)| t.clone())
            .collect()
    };

    rsx! {
        div {
            class: "space-y-4",
            style: "overflow: visible;",
            "data-testid": "flake-timeline-widget",

            // Header with filter dropdown and view mode toggle
            // z-20 ensures dropdown appears above timeline content
            div {
                class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3 relative z-20",
                div {
                    class: "flex items-center gap-3",
                    h3 { class: "{theme::typography::SECTION_TITLE} text-white", "Commit Timeline" }
                    // Legend inline with title
                    div {
                        class: "flex flex-wrap items-center gap-3",
                        "data-testid": "timeline-legend",
                        LegendDot { color: "bg-emerald-500", label: "Latest" }
                        LegendDot { color: "bg-yellow-500", label: "1 behind" }
                        LegendDot { color: "bg-orange-500", label: "2 behind" }
                        LegendDot { color: "bg-red-500", label: "3+ behind" }
                        RingLegendSwatch { style: "box-shadow: 0 0 0 3px #42ff65", label: "Building" }
                        RingLegendSwatch { style: "box-shadow: 0 0 0 2px #e57c00", label: "Queued" }
                    }
                }

                div {
                    class: "flex items-center gap-2",

                    // View mode toggle (Stacked only - Combined is default, no button needed)
                    ViewModeToggle {
                        mode: *view_mode.read(),
                        on_change: move |mode| view_mode.set(mode)
                    }

                    // Multi-select flake filter dropdown (Grafana-style)
                    div {
                        class: "relative z-30",

                        // Dropdown trigger button
                        button {
                            class: "flex items-center gap-2 rounded-lg px-3 py-1.5 text-sm border transition-colors",
                            class: if !is_all_selected {
                                "bg-blue-900/30 border-blue-500 text-blue-300"
                            } else {
                                "{theme::interactive::INPUT} {theme::surface::CARD_BORDER} {theme::text::SECONDARY}"
                            },
                            onclick: move |_| {
                                let current = *dropdown_open.read();
                                dropdown_open.set(!current);
                            },

                            // Filter icon
                            svg {
                                class: "w-4 h-4",
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
                            span { "{filter_label}" }
                            // Chevron
                            svg {
                                class: "w-4 h-4 transition-transform",
                                class: if *dropdown_open.read() { "rotate-180" } else { "" },
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M19 9l-7 7-7-7"
                                }
                            }
                        }

                        // Dropdown menu - very high z-index to appear above timeline
                        if *dropdown_open.read() {
                            div {
                                class: "absolute right-0 top-full mt-1 min-w-[200px] rounded-lg border {theme::surface::CARD_BG} {theme::surface::CARD_BORDER} shadow-xl",
                                style: "z-index: 9999;",

                                // "All Flakes" option
                                button {
                                    class: "w-full flex items-center gap-2 px-3 py-2 text-sm text-left rounded-md hover:bg-gray-700 transition-colors",
                                    onclick: {
                                        let on_filter_change = on_filter_change.clone();
                                        move |_| {
                                            if let Some(handler) = &on_filter_change {
                                                handler.call(HashSet::new());
                                            }
                                            dropdown_open.set(false);
                                        }
                                    },

                                    // Checkbox
                                    div {
                                        class: "w-4 h-4 rounded border flex items-center justify-center",
                                        class: if is_all_selected { "bg-blue-500 border-blue-500" } else { "border-gray-500" },
                                        if is_all_selected {
                                            svg {
                                                class: "w-3 h-3 text-white",
                                                fill: "none",
                                                stroke: "currentColor",
                                                view_box: "0 0 24 24",
                                                path {
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    stroke_width: "3",
                                                    d: "M5 13l4 4L19 7"
                                                }
                                            }
                                        }
                                    }
                                    span { class: "text-white font-medium", "All Flakes" }
                                }

                                // Divider
                                div { class: "border-t border-gray-700 my-1" }

                                // Individual flake options
                                for (idx, name) in flake_names.iter().enumerate() {
                                    {
                                        let is_selected = selected_flake_indices.contains(&idx);
                                        let name = name.clone();
                                        rsx! {
                                            button {
                                                key: "{idx}",
                                                class: "group w-full flex items-center gap-2 px-3 py-2 text-sm text-left rounded-md hover:bg-gray-700 transition-colors",
                                                onclick: {
                                                    let on_filter_change = on_filter_change.clone();
                                                    let selected = selected_flake_indices.clone();
                                                    move |_| {
                                                        let mut new_selection = selected.clone();
                                                        if new_selection.contains(&idx) {
                                                            new_selection.remove(&idx);
                                                        } else {
                                                            new_selection.insert(idx);
                                                        }
                                                        // If all are selected individually, treat as "All"
                                                        if new_selection.len() == flake_count {
                                                            new_selection.clear();
                                                        }
                                                        if let Some(handler) = &on_filter_change {
                                                            handler.call(new_selection);
                                                        }
                                                    }
                                                },

                                                // Checkbox
                                                div {
                                                    class: "w-4 h-4 rounded border flex items-center justify-center",
                                                    class: if is_selected || is_all_selected { "bg-blue-500 border-blue-500" } else { "border-gray-500" },
                                                    if is_selected || is_all_selected {
                                                        svg {
                                                            class: "w-3 h-3 text-white",
                                                            fill: "none",
                                                            stroke: "currentColor",
                                                            view_box: "0 0 24 24",
                                                            path {
                                                                stroke_linecap: "round",
                                                                stroke_linejoin: "round",
                                                                stroke_width: "3",
                                                                d: "M5 13l4 4L19 7"
                                                            }
                                                        }
                                                    }
                                                }
                                                span { class: "{theme::text::SECONDARY} group-hover:text-white transition-colors", "{name}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Timeline content based on view mode
            // z-10 is lower than header's z-20, so dropdown appears above
            div {
                class: "relative z-10",
                match *view_mode.read() {
                    TimelineViewMode::Combined | TimelineViewMode::SingleFlake => rsx! {
                        CombinedTimeline { timelines: filtered_timelines.clone() }
                    },
                    TimelineViewMode::Stacked => rsx! {
                        StackedTimelines { timelines: filtered_timelines.clone() }
                    },
                }
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

/// Ring legend marker for build status.
#[component]
fn RingLegendSwatch(style: &'static str, label: &'static str) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-1",
            div { class: "w-2 h-2 rounded-full", style: "{style}" }
            span { class: "text-[10px] {theme::text::MUTED}", "{label}" }
        }
    }
}

/// Toggle buttons for switching between Combined and Stacked view modes.
#[component]
fn ViewModeToggle(mode: TimelineViewMode, on_change: EventHandler<TimelineViewMode>) -> Element {
    let is_stacked = mode == TimelineViewMode::Stacked;

    let combined_class = if !is_stacked {
        "px-3 py-1.5 text-xs font-medium transition bg-gray-700 text-white"
    } else {
        "px-3 py-1.5 text-xs font-medium transition text-gray-400 hover:text-white hover:bg-gray-800"
    };
    let stacked_class = if is_stacked {
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
                "Timeline"
            }
            button {
                class: "{stacked_class}",
                onclick: move |_| on_change.call(TimelineViewMode::Stacked),
                "Stacked"
            }
        }
    }
}

/// Calculate time-proportional positions for commits.
fn calculate_positions(commits: &[(Option<String>, FlakeCommit)]) -> Vec<PositionedCommit> {
    calculate_positions_with_scale(commits).0
}

fn calculate_positions_with_scale(
    commits: &[(Option<String>, FlakeCommit)],
) -> (Vec<PositionedCommit>, Option<TimelineScale>) {
    if commits.is_empty() {
        return (vec![], None);
    }

    if commits.len() == 1 {
        let only = PositionedCommit {
            commit: commits[0].1.clone(),
            flake_name: commits[0].0.clone(),
            x_position: MIN_GAP_PX,
        };
        return (
            vec![only.clone()],
            Some(TimelineScale {
                ticks: vec![TimelineTick {
                    x_position: only.x_position,
                    label: format_tick_label(only.commit.committed_at, 0),
                }],
            }),
        );
    }

    // Sort by time (oldest first)
    let mut sorted: Vec<_> = commits.to_vec();
    sorted.sort_by(|a, b| a.1.committed_at.cmp(&b.1.committed_at));

    // Calculate time gaps between consecutive commits
    let gaps: Vec<i64> = sorted
        .windows(2)
        .map(|w| {
            let duration = w[1]
                .1
                .committed_at
                .signed_duration_since(w[0].1.committed_at);
            duration.num_seconds().max(1)
        })
        .collect();

    // Log-scaled spacing with a minimum floor for dense bursts.
    let scaled_gaps: Vec<f64> = gaps
        .iter()
        .map(|&secs| {
            let log_units = (1.0 + secs as f64 / TIME_SCALE_SECONDS).ln();
            MIN_GAP_PX + TIME_SCALE_PX * log_units
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

    let scale = build_scale(&result);
    (result, Some(scale))
}

fn build_scale(positions: &[PositionedCommit]) -> TimelineScale {
    let start = positions
        .first()
        .map(|p| p.commit.committed_at)
        .unwrap_or_else(chrono::Utc::now);
    let end = positions
        .last()
        .map(|p| p.commit.committed_at)
        .unwrap_or_else(chrono::Utc::now);
    let span_seconds = end.signed_duration_since(start).num_seconds().max(1);
    let tick_seconds = pick_tick_interval(span_seconds);
    let ticks = build_ticks(positions, tick_seconds);
    TimelineScale { ticks }
}

fn pick_tick_interval(span_seconds: i64) -> i64 {
    let candidates = [
        60,
        5 * 60,
        15 * 60,
        30 * 60,
        60 * 60,
        2 * 60 * 60,
        6 * 60 * 60,
        12 * 60 * 60,
        24 * 60 * 60,
        7 * 24 * 60 * 60,
        14 * 24 * 60 * 60,
        30 * 24 * 60 * 60,
    ];

    for candidate in candidates {
        if span_seconds / candidate <= 6 {
            return candidate;
        }
    }

    30 * 24 * 60 * 60
}

fn build_ticks(positions: &[PositionedCommit], tick_seconds: i64) -> Vec<TimelineTick> {
    let start = positions
        .first()
        .map(|p| p.commit.committed_at)
        .unwrap_or_else(chrono::Utc::now);
    let end = positions
        .last()
        .map(|p| p.commit.committed_at)
        .unwrap_or_else(chrono::Utc::now);
    let start_ts = start.timestamp();
    let end_ts = end.timestamp();
    let mut ticks = Vec::new();
    let mut current = start_ts;
    let mut idx = 0usize;

    while current <= end_ts {
        while idx + 1 < positions.len()
            && positions[idx + 1].commit.committed_at.timestamp() < current
        {
            idx += 1;
        }

        let left = &positions[idx];
        let left_ts = left.commit.committed_at.timestamp();
        let right = positions.get(idx + 1).unwrap_or(left);
        let right_ts = right.commit.committed_at.timestamp();
        let span = (right_ts - left_ts).max(1) as f64;
        let ratio = ((current - left_ts) as f64 / span).clamp(0.0, 1.0);
        let x_position = left.x_position + (right.x_position - left.x_position) * ratio;

        ticks.push(TimelineTick {
            x_position,
            label: format_tick_label(chrono::Utc.timestamp_opt(current, 0).unwrap(), tick_seconds),
        });

        current += tick_seconds;
    }

    ticks
}

fn format_tick_label(timestamp: chrono::DateTime<chrono::Utc>, tick_seconds: i64) -> String {
    match tick_seconds {
        s if s < 60 * 60 => timestamp.format("%H:%M").to_string(),
        s if s < 24 * 60 * 60 => timestamp.format("%b %-d %H:%M").to_string(),
        s if s < 7 * 24 * 60 * 60 => timestamp.format("%b %-d").to_string(),
        s if s < 30 * 24 * 60 * 60 => timestamp.format("%b %-d").to_string(),
        _ => timestamp.format("%Y-%m-%d").to_string(),
    }
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

    let (positioned, scale) = calculate_positions_with_scale(&all_commits);
    let total_width = positioned
        .last()
        .map(|p| p.x_position + MIN_GAP_PX)
        .unwrap_or(200.0);

    rsx! {
        TimelineGraph {
            positioned_commits: positioned,
            total_width: total_width,
            scale: scale,
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
            class: "space-y-4",
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
    let commits: Vec<(Option<String>, FlakeCommit)> =
        timeline.commits.iter().map(|c| (None, c.clone())).collect();

    let (positioned, scale) = calculate_positions_with_scale(&commits);
    let total_width = positioned
        .last()
        .map(|p| p.x_position + MIN_GAP_PX)
        .unwrap_or(200.0);

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
                scale: scale,
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
    scale: Option<TimelineScale>,
    show_flake_labels: bool,
    testid: &'static str,
) -> Element {
    if positioned_commits.is_empty() {
        return rsx! {
            p { class: "{theme::text::MUTED}", "No commits to display." }
        };
    }

    let width_px = total_width.max(300.0) as i32;
    let first_x = positioned_commits
        .first()
        .map(|p| p.x_position)
        .unwrap_or(0.0) as i32;
    let last_x = positioned_commits
        .last()
        .map(|p| p.x_position)
        .unwrap_or(0.0) as i32;
    let line_width = last_x - first_x;
    let scale = scale.unwrap_or_else(|| TimelineScale { ticks: vec![] });

    // Layout: nodes are 20px circles with count inside, line passes through center
    let node_size = 20;
    let node_top = 4; // Node starts at y=4
    let node_center = node_top + (node_size / 2); // Center at y=14
    let line_thickness = 5;
    let line_top = node_center - 2; // Line at y=12, 5px thick, centers at y=14
    // Height for node + text labels only
    let container_height = 65;

    rsx! {
        // Outer wrapper
        div {
            class: "relative",
            "data-testid": "{testid}",

            // Scrollable timeline area - uses CSS to start scrolled right
            // direction: rtl on container + ltr on content makes it scroll to "end" (right) by default
            div {
                class: "overflow-x-auto scrollbar-hide",
                style: "scrollbar-width: none; direction: rtl; -ms-overflow-style: none;",

                div {
                    class: "relative",
                    style: "width: {width_px}px; height: {container_height}px; direction: ltr;",

                    // Layer 1: Lines (behind everything)
                    div {
                        class: "absolute inset-0",
                        style: "z-index: 1;",

                        // Main horizontal line
                        div {
                            class: "absolute bg-gray-600",
                            style: "left: {first_x}px; width: {line_width}px; top: {line_top}px; height: {line_thickness}px;"
                        }

                        // Colored segments
                        for (i, pc) in positioned_commits.iter().enumerate() {
                            if i > 0 {
                                {
                                    let prev = &positioned_commits[i - 1];
                                    let seg_start = prev.x_position as i32;
                                    let seg_width = (pc.x_position - prev.x_position) as i32;
                                    let seg_color = segment_color(&prev.commit, &pc.commit);
                                    rsx! {
                                        if let Some(color) = seg_color {
                                            div {
                                                class: "absolute {color}",
                                                style: "left: {seg_start}px; width: {seg_width}px; top: {line_top}px; height: {line_thickness}px;"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Layer 2: Nodes (on top of lines)
                    div {
                        class: "absolute inset-0",
                        style: "z-index: 2;",

                        for pc in positioned_commits.iter() {
                            CommitNode {
                                commit: pc.commit.clone(),
                                flake_name: pc.flake_name.clone(),
                                show_flake_label: show_flake_labels,
                                x_position: pc.x_position,
                                node_top: node_top,
                                node_size: node_size
                            }
                        }
                    }

                    // Layer 3: Time scale
                    div {
                        class: "absolute inset-0",
                        style: "z-index: 0;",

                        for tick in scale.ticks.iter() {
                            div {
                                class: "absolute text-[9px] {theme::text::MUTED}",
                                style: "left: {tick.x_position as i32}px; top: {container_height - 14}px; transform: translateX(-50%);",
                                "{tick.label}"
                            }
                            div {
                                class: "absolute bg-gray-700",
                                style: "left: {tick.x_position as i32}px; top: {line_top + 8}px; width: 1px; height: 6px;"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A single commit node with the line passing through its center.
#[component]
fn CommitNode(
    commit: FlakeCommit,
    flake_name: Option<String>,
    show_flake_label: bool,
    x_position: f64,
    node_top: i32,
    node_size: i32,
) -> Element {
    let short_hash = commit.hash.chars().take(7).collect::<String>();
    let node_bg = commit_node_bg(commit.system_count, commit.commits_behind);
    let build_status = commit.build_status.unwrap_or(BuildStatus::Idle);
    let build_ring = build_ring_style(build_status);

    let behind_text = if commit.commits_behind == 0 {
        "Latest".to_string()
    } else {
        let plural = if commit.commits_behind == 1 { "" } else { "s" };
        format!("{} commit{} behind", commit.commits_behind, plural)
    };

    let x_px = x_position as i32;
    let system_plural = if commit.system_count == 1 { "" } else { "s" };

    let badge_top = node_top;
    let text_top = node_top + node_size + 4;

    // Build tooltip text
    let tooltip = format!(
        "{}\n\nby {}\n{}\n{} system{}",
        commit.message, commit.author, behind_text, commit.system_count, system_plural
    );

    rsx! {
        div {
            class: "absolute group",
            style: "left: {x_px}px; top: 0; transform: translateX(-50%);",
            "data-testid": "commit-node",
            "data-commits-behind": "{commit.commits_behind}",
            title: "{tooltip}",

            // Main node - colored circle with system count, centered ON the line
            div {
                class: "absolute left-1/2 -translate-x-1/2 rounded-full flex items-center justify-center cursor-pointer {node_bg} border-2 border-gray-900",
                style: "width: {node_size}px; height: {node_size}px; top: {badge_top}px; box-shadow: {build_ring};",
                title: "{build_status.label()}",
                span {
                    class: "text-[9px] font-bold text-gray-900",
                    "{commit.system_count}"
                }

                if build_status == BuildStatus::Building {
                    span {
                        class: "absolute inset-[-4px] rounded-full animate-spin",
                        style: "background: conic-gradient(#42ff65 0deg, rgba(66, 255, 101, 0.2) 120deg, transparent 360deg);"
                    }
                }

                if build_status == BuildStatus::Queued {
                    span { class: "absolute text-[7px] text-orange-300 font-semibold", "Q" }
                    span { class: "absolute inset-[-4px] rounded-full animate-pulse", style: "box-shadow: 0 0 0 2px rgba(228, 124, 0, 0.6);" }
                }
            }

            // Text content below the node
            div {
                class: "absolute left-1/2 -translate-x-1/2 flex flex-col items-center cursor-pointer",
                style: "top: {text_top}px; min-width: 50px;",

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

/// Get the status badge color classes based on how many commits behind.
fn commits_behind_color(behind: i64) -> &'static str {
    match behind {
        0 => "bg-emerald-500/20 text-emerald-400",
        1 => "bg-yellow-500/20 text-yellow-400",
        2 => "bg-orange-500/20 text-orange-400",
        _ => "bg-red-500/20 text-red-400",
    }
}

fn commit_node_bg(system_count: i64, behind: i64) -> &'static str {
    if system_count == 0 {
        "bg-gray-700"
    } else {
        commits_behind_bg(behind)
    }
}

fn build_ring_style(status: BuildStatus) -> &'static str {
    match status {
        BuildStatus::Queued => "0 0 0 2px #e57c00",
        BuildStatus::Building => "0 0 0 3px #42ff65",
        _ => "0 0 0 2px #9ca3af",
    }
}

fn segment_color(_prev: &FlakeCommit, next: &FlakeCommit) -> Option<&'static str> {
    if next.system_count > 0 {
        Some(commits_behind_bg(next.commits_behind))
    } else {
        None
    }
}
