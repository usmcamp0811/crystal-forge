//! Flake commit timeline component for the dashboard.
//!
//! Displays a horizontal git graph-style timeline of commits for monitored flakes,
//! showing how many systems are deployed at each commit with severity coloring
//! based on how far behind the latest they are.
//!
//! The timeline uses time-proportional spacing between commits, normalized so
//! that rapid commits are still visible while long gaps are compressed.

use dioxus::prelude::*;

use crate::api::models::{FlakeCommit, FlakeTimeline};
use crate::theme;

/// Minimum pixels between commit nodes.
const MIN_GAP_PX: f64 = 40.0;
/// Maximum pixels between commit nodes.
const MAX_GAP_PX: f64 = 120.0;

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
                h3 { class: "{theme::typography::SECTION_TITLE} text-white", "Commit Timeline" }

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

            // Legend
            TimelineLegend {}
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
/// Uses logarithmic scaling so rapid commits stay visible while long gaps compress.
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

    // Use logarithmic scaling: gap_px = MIN + (MAX-MIN) * log(1 + seconds) / log(1 + max_seconds)
    let max_gap_seconds = gaps.iter().copied().max().unwrap_or(1) as f64;
    let log_max = (1.0 + max_gap_seconds).ln();

    let scaled_gaps: Vec<f64> = gaps
        .iter()
        .map(|&secs| {
            let log_secs = (1.0 + secs as f64).ln();
            let normalized = log_secs / log_max; // 0.0 to 1.0
            MIN_GAP_PX + (MAX_GAP_PX - MIN_GAP_PX) * normalized
        })
        .collect();

    // Build positioned commits
    let mut result = Vec::with_capacity(sorted.len());
    let mut x = MIN_GAP_PX; // Start with some padding

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
    // Merge all commits
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
    let total_width = positioned.last().map(|p| p.x_position + MIN_GAP_PX).unwrap_or(100.0);

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
            class: "space-y-6",
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
    let total_width = positioned.last().map(|p| p.x_position + MIN_GAP_PX).unwrap_or(100.0);

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

/// A line segment between two commits with color based on "behind" status.
#[derive(Clone, PartialEq)]
struct LineSegment {
    start_x: f64,
    end_x: f64,
    /// The "commits_behind" value of the commit this segment leads TO
    /// (determines the color - how stale is this section of the graph)
    commits_behind: i64,
}

/// The actual timeline graph with colored line segments and positioned nodes.
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

    let width_px = total_width.max(200.0) as i32;

    // Build line segments between consecutive commits
    // Each segment is colored based on the commit it leads TO
    let segments: Vec<LineSegment> = positioned_commits
        .windows(2)
        .map(|w| LineSegment {
            start_x: w[0].x_position,
            end_x: w[1].x_position,
            // Color based on the destination commit's "behind" status
            commits_behind: w[1].commit.commits_behind,
        })
        .collect();

    rsx! {
        div {
            class: "overflow-x-auto pb-2",
            "data-testid": "{testid}",
            div {
                class: "relative",
                style: "width: {width_px}px; height: 120px;",

                // Colored line segments between commits
                for segment in segments.iter() {
                    TimelineSegment { segment: segment.clone() }
                }

                // Commit nodes positioned absolutely (rendered on top of lines)
                for pc in positioned_commits.iter() {
                    CommitNode {
                        commit: pc.commit.clone(),
                        flake_name: pc.flake_name.clone(),
                        show_flake_label: show_flake_labels,
                        x_position: pc.x_position
                    }
                }
            }
        }
    }
}

/// A single colored line segment in the timeline.
#[component]
fn TimelineSegment(segment: LineSegment) -> Element {
    let bg_color = commits_behind_bg(segment.commits_behind);
    let start = segment.start_x as i32;
    let width = (segment.end_x - segment.start_x) as i32;

    rsx! {
        div {
            class: "absolute h-2 rounded-full {bg_color}",
            style: "left: {start}px; width: {width}px; top: 50%; transform: translateY(-50%); z-index: 1;"
        }
    }
}

/// A single commit node in the git graph timeline.
#[component]
fn CommitNode(
    commit: FlakeCommit,
    flake_name: Option<String>,
    show_flake_label: bool,
    x_position: f64,
) -> Element {
    let short_hash = commit.hash.chars().take(7).collect::<String>();
    let node_color = commits_behind_color(commit.commits_behind);
    let node_border = commits_behind_border(commit.commits_behind);
    let node_bg = commits_behind_bg(commit.commits_behind);

    // Build tooltip content
    let systems_list = if commit.systems.is_empty() {
        "No systems".to_string()
    } else if commit.systems.len() <= 5 {
        commit.systems.join(", ")
    } else {
        format!(
            "{}, +{} more",
            commit.systems[..5].join(", "),
            commit.systems.len() - 5
        )
    };

    let behind_text = if commit.commits_behind == 0 {
        "Latest".to_string()
    } else {
        let plural = if commit.commits_behind == 1 { "" } else { "s" };
        format!("{} commit{} behind", commit.commits_behind, plural)
    };

    // Node size based on whether it has systems
    let (node_w, node_h) = if commit.system_count > 0 {
        (24, 24)
    } else {
        (16, 16)
    };

    let x_px = x_position as i32;
    let system_plural = if commit.system_count == 1 { "" } else { "s" };

    rsx! {
        div {
            class: "absolute flex flex-col items-center group",
            style: "left: {x_px}px; top: 50%; transform: translate(-50%, -50%); z-index: 10;",
            "data-testid": "commit-node",
            "data-commits-behind": "{commit.commits_behind}",

            // Top section: flake label and system count
            div {
                class: "flex flex-col items-center mb-1",
                style: "min-height: 32px;",

                // Flake label (for combined view)
                if show_flake_label {
                    if let Some(ref name) = flake_name {
                        span {
                            class: "text-[10px] {theme::text::MUTED} truncate max-w-[60px]",
                            "{name}"
                        }
                    }
                }

                // System count badge (above node if has systems)
                if commit.system_count > 0 {
                    span {
                        class: "text-xs font-bold {node_color}",
                        "{commit.system_count}"
                    }
                }
            }

            // The commit node circle - with dark ring to cleanly separate from line
            div {
                class: "rounded-full border-4 border-gray-900 cursor-pointer transition-all hover:scale-110",
                style: "width: {node_w}px; height: {node_h}px;",
                // Inner colored circle
                div {
                    class: "w-full h-full rounded-full {node_bg}"
                }
            }

            // Bottom section: hash
            div {
                class: "mt-1",
                style: "min-height: 16px;",
                span {
                    class: "text-[10px] font-mono {theme::text::MUTED} group-hover:text-white transition",
                    "{short_hash}"
                }
            }

            // Hover tooltip
            div {
                class: "absolute bottom-full mb-8 left-1/2 -translate-x-1/2 opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none z-20",
                div {
                    class: "bg-gray-800 border {theme::surface::CARD_BORDER} rounded-lg p-3 shadow-xl min-w-[200px] max-w-[280px]",
                    // Commit message
                    p {
                        class: "text-sm text-white font-medium",
                        "{commit.message}"
                    }
                    // Author and time
                    p {
                        class: "text-xs {theme::text::MUTED} mt-1",
                        "by {commit.author}"
                    }
                    // Divider
                    div { class: "h-px bg-gray-700 my-2" }
                    // Systems info
                    div {
                        class: "flex items-center justify-between",
                        span {
                            class: "text-xs {theme::text::SECONDARY}",
                            "{commit.system_count} system{system_plural}"
                        }
                        span {
                            class: "text-xs {node_color}",
                            "{behind_text}"
                        }
                    }
                    // System hostnames (if few enough)
                    if !commit.systems.is_empty() && commit.systems.len() <= 8 {
                        div {
                            class: "mt-2 flex flex-wrap gap-1",
                            for system in commit.systems.iter() {
                                span {
                                    class: "text-[10px] px-1.5 py-0.5 rounded bg-gray-700 {theme::text::SECONDARY} font-mono",
                                    "{system}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Legend explaining the severity colors.
#[component]
fn TimelineLegend() -> Element {
    rsx! {
        div {
            class: "flex flex-wrap items-center gap-4 pt-3 border-t {theme::surface::CARD_BORDER}",
            "data-testid": "timeline-legend",
            LegendItem { filled: true, color: "bg-emerald-500", border: "border-emerald-500", label: "Latest" }
            LegendItem { filled: true, color: "bg-yellow-500", border: "border-yellow-500", label: "1 behind" }
            LegendItem { filled: true, color: "bg-orange-500", border: "border-orange-500", label: "2 behind" }
            LegendItem { filled: true, color: "bg-red-500", border: "border-red-500", label: "3+ behind" }
            LegendItem { filled: false, color: "", border: "border-gray-500", label: "No systems" }
        }
    }
}

/// A single legend item.
#[component]
fn LegendItem(filled: bool, color: &'static str, border: &'static str, label: &'static str) -> Element {
    rsx! {
        div {
            class: "flex items-center gap-1.5",
            div {
                class: "w-3 h-3 rounded-full border-2 {border}",
                if filled {
                    div { class: "w-full h-full rounded-full {color}" }
                }
            }
            span { class: "text-xs {theme::text::MUTED}", "{label}" }
        }
    }
}

/// Get the text color class based on how many commits behind.
fn commits_behind_color(behind: i64) -> &'static str {
    match behind {
        0 => "text-emerald-400",
        1 => "text-yellow-400",
        2 => "text-orange-400",
        _ => "text-red-400",
    }
}

/// Get the border color class based on how many commits behind.
fn commits_behind_border(behind: i64) -> &'static str {
    match behind {
        0 => "border-emerald-500",
        1 => "border-yellow-500",
        2 => "border-orange-500",
        _ => "border-red-500",
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
