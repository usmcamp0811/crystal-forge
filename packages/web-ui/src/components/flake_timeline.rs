//! Flake commit timeline component for the dashboard.
//!
//! Displays a horizontal git graph-style timeline of commits for monitored flakes,
//! showing how many systems are deployed at each commit with severity coloring
//! based on how far behind the latest they are.

use dioxus::prelude::*;

use crate::api::models::{FlakeCommit, FlakeTimeline};
use crate::theme;

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

/// Combined timeline showing all flakes' commits merged chronologically.
#[component]
fn CombinedTimeline(timelines: Vec<FlakeTimeline>) -> Element {
    // Merge all commits and sort by date (oldest first for left-to-right display)
    let mut all_commits: Vec<(String, FlakeCommit)> = timelines
        .iter()
        .flat_map(|t| {
            t.commits
                .iter()
                .map(|c| (t.flake_name.clone(), c.clone()))
                .collect::<Vec<_>>()
        })
        .collect();

    all_commits.sort_by(|a, b| a.1.committed_at.cmp(&b.1.committed_at));

    rsx! {
        div {
            class: "overflow-x-auto pb-2",
            "data-testid": "combined-timeline",
            div {
                class: "flex items-center min-w-max py-4 px-2",
                for (idx, (flake_name, commit)) in all_commits.iter().enumerate() {
                    // Connector line (except for first node)
                    if idx > 0 {
                        div { class: "w-8 h-0.5 bg-gray-700" }
                    }
                    CommitNode {
                        commit: commit.clone(),
                        flake_name: Some(flake_name.clone()),
                        show_flake_label: true
                    }
                }
            }
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
    // Sort commits oldest first (left-to-right)
    let mut commits = timeline.commits.clone();
    commits.sort_by(|a, b| a.committed_at.cmp(&b.committed_at));

    rsx! {
        div {
            class: "space-y-2",
            // Flake header
            div {
                class: "flex items-center gap-2",
                span { class: "text-sm font-medium text-white", "{timeline.flake_name}" }
                span { class: "{theme::text::MUTED} text-xs font-mono", "{timeline.repo_url}" }
            }

            // Git graph timeline
            div {
                class: "overflow-x-auto pb-2",
                div {
                    class: "flex items-center min-w-max py-4 px-2",
                    for (idx, commit) in commits.iter().enumerate() {
                        // Connector line (except for first node)
                        if idx > 0 {
                            div { class: "w-8 h-0.5 bg-gray-700" }
                        }
                        CommitNode {
                            commit: commit.clone(),
                            flake_name: None,
                            show_flake_label: false
                        }
                    }
                }
            }
        }
    }
}

/// A single commit node in the git graph timeline.
#[component]
fn CommitNode(commit: FlakeCommit, flake_name: Option<String>, show_flake_label: bool) -> Element {
    let short_hash = commit.hash.chars().take(7).collect::<String>();
    let node_color = commits_behind_color(commit.commits_behind);
    let node_border = commits_behind_border(commit.commits_behind);
    
    // Build tooltip content
    let systems_list = if commit.systems.is_empty() {
        "No systems".to_string()
    } else if commit.systems.len() <= 5 {
        commit.systems.join(", ")
    } else {
        format!("{}, +{} more", commit.systems[..5].join(", "), commit.systems.len() - 5)
    };
    
    let behind_text = if commit.commits_behind == 0 {
        "Latest".to_string()
    } else {
        format!("{} commit{} behind", commit.commits_behind, if commit.commits_behind == 1 { "" } else { "s" })
    };

    let tooltip = format!(
        "{}\n{}\nBy: {}\n\n{} system{}\n{}\n\n{}",
        short_hash,
        commit.message,
        commit.author,
        commit.system_count,
        if commit.system_count == 1 { "" } else { "s" },
        systems_list,
        behind_text
    );

    // Node size based on whether it has systems
    let node_size = if commit.system_count > 0 { "w-5 h-5" } else { "w-3 h-3" };

    rsx! {
        div {
            class: "flex flex-col items-center gap-1 group relative",
            "data-testid": "commit-node",
            "data-commits-behind": "{commit.commits_behind}",

            // Flake label (for combined view)
            if show_flake_label {
                if let Some(ref name) = flake_name {
                    span {
                        class: "text-[10px] {theme::text::MUTED} truncate max-w-[60px] mb-1",
                        "{name}"
                    }
                }
            }

            // System count badge (above node if has systems)
            if commit.system_count > 0 {
                span {
                    class: "text-xs font-bold {node_color} mb-1",
                    "{commit.system_count}"
                }
            }

            // The commit node circle
            div {
                class: "rounded-full {node_size} {node_border} border-2 cursor-pointer transition-transform hover:scale-125",
                title: "{tooltip}",
                // Inner fill for nodes with systems
                if commit.system_count > 0 {
                    div {
                        class: "w-full h-full rounded-full {commits_behind_bg(commit.commits_behind)}"
                    }
                }
            }

            // Commit hash below node
            span {
                class: "text-[10px] font-mono {theme::text::MUTED} group-hover:text-white transition mt-1",
                "{short_hash}"
            }

            // Hover tooltip with more info
            div {
                class: "absolute bottom-full mb-2 left-1/2 -translate-x-1/2 opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none z-10",
                div {
                    class: "bg-gray-800 border {theme::surface::CARD_BORDER} rounded-lg p-3 shadow-xl min-w-[200px] max-w-[280px]",
                    // Commit message
                    p {
                        class: "text-sm text-white font-medium truncate",
                        "{commit.message}"
                    }
                    // Author
                    p {
                        class: "text-xs {theme::text::MUTED} mt-1",
                        "by {commit.author}"
                    }
                    // Divider
                    div { class: "h-px bg-gray-700 my-2" }
                    // Systems info
                    {
                        let system_plural = if commit.system_count == 1 { "" } else { "s" };
                        rsx! {
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
