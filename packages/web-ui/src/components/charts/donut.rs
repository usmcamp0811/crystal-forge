//! Donut chart component with legend and hover interactions.
//!
//! Renders an SVG donut chart with segments, center text, and a legend
//! that shows system lists on hover.

use dioxus::prelude::*;

/// A single segment in the donut chart.
#[derive(Clone, Debug, PartialEq)]
pub struct DonutSegment {
    /// Percentage of the chart this segment occupies (0-100)
    pub percent: f64,
    /// CSS color for the segment
    pub color: &'static str,
    /// Display label for the legend
    pub label: &'static str,
    /// Count value to display
    pub count: i64,
    /// List of systems/items in this segment (shown on hover)
    pub systems: Vec<String>,
}

/// Arc data for donut chart rendering using stroke-dasharray technique.
#[derive(Clone, Debug, PartialEq)]
pub struct DonutArc {
    /// Index of the segment this arc represents
    pub segment_idx: usize,
    /// CSS color for the arc
    pub color: &'static str,
    /// Length of the visible dash (segment length)
    pub dash_length: f64,
    /// Length of the gap (remaining circumference)
    pub gap_length: f64,
    /// Stroke dash offset for positioning
    pub offset: f64,
}

#[derive(Props, Clone, PartialEq)]
pub struct DonutChartWithLegendProps {
    /// Segments to render in the chart
    segments: Vec<DonutSegment>,
    /// Value to display in the center
    center_value: i64,
    /// Label to display below the center value
    center_label: &'static str,
}

/// Donut chart with legend on right side, hover shows system list in place of legend.
///
/// # Example
/// ```ignore
/// let segments = vec![
///     DonutSegment {
///         percent: 60.0,
///         color: "#10b981",
///         label: "Healthy",
///         count: 12,
///         systems: vec!["server-01".into(), "server-02".into()],
///     },
///     DonutSegment {
///         percent: 40.0,
///         color: "#f59e0b",
///         label: "Warning",
///         count: 8,
///         systems: vec!["server-03".into()],
///     },
/// ];
///
/// rsx! {
///     DonutChartWithLegend {
///         segments: segments,
///         center_value: 20,
///         center_label: "SYSTEMS"
///     }
/// }
/// ```
#[component]
pub fn DonutChartWithLegend(props: DonutChartWithLegendProps) -> Element {
    let DonutChartWithLegendProps {
        segments,
        center_value,
        center_label,
    } = props;
    let arcs = donut_arcs(&segments);

    // Track which segment is hovered
    let mut hovered_idx: Signal<Option<usize>> = use_signal(|| None);

    rsx! {
        div {
            class: "flex items-center justify-center h-full gap-4",

            // Donut chart on the left
            div {
                class: "shrink-0",
                style: "width: 120px; height: 120px;",

                svg {
                    width: "120",
                    height: "120",
                    view_box: "0 0 100 100",
                    role: "img",

                    // Background circle
                    circle {
                        cx: "50",
                        cy: "50",
                        r: "40",
                        fill: "none",
                        stroke: "#374151",
                        stroke_width: "14"
                    }

                    // Donut segments with hover
                    for arc in arcs.clone() {
                        circle {
                            cx: "50",
                            cy: "50",
                            r: "40",
                            fill: "none",
                            stroke: "{arc.color}",
                            stroke_width: if hovered_idx.read().map_or(false, |h| h == arc.segment_idx) { "18" } else { "14" },
                            stroke_dasharray: "{arc.dash_length} {arc.gap_length}",
                            stroke_dashoffset: "{arc.offset}",
                            stroke_linecap: "butt",
                            transform: "rotate(-90 50 50)",
                            style: "cursor: pointer; transition: stroke-width 0.15s ease;",
                            onmouseenter: move |_| {
                                hovered_idx.set(Some(arc.segment_idx));
                            },
                            onmouseleave: move |_| {
                                hovered_idx.set(None);
                            }
                        }
                    }

                    // Center text - value
                    text {
                        x: "50",
                        y: "46",
                        text_anchor: "middle",
                        dominant_baseline: "middle",
                        fill: "white",
                        font_size: "18",
                        font_weight: "bold",
                        "{center_value}"
                    }

                    // Center text - label
                    text {
                        x: "50",
                        y: "60",
                        text_anchor: "middle",
                        dominant_baseline: "middle",
                        fill: "#9ca3af",
                        font_size: "8",
                        "{center_label}"
                    }
                }
            }

            // Right side: either legend or system list on hover
            div {
                class: "flex-1 min-w-0",

                if let Some(idx) = *hovered_idx.read() {
                    // Show system list for hovered segment
                    if let Some(segment) = segments.get(idx) {
                        {
                            // Calculate how many to show (max 12 in 2 columns of 6)
                            let max_display = 12;
                            let show_count = segment.systems.len().min(max_display);
                            let remaining = segment.systems.len().saturating_sub(max_display);

                            rsx! {
                                div {
                                    class: "bg-gray-800/50 rounded-lg p-2 h-full",

                                    // Header
                                    div {
                                        class: "flex items-center gap-2 mb-1.5 pb-1 border-b border-gray-700",
                                        span {
                                            class: "w-3 h-3 rounded shrink-0",
                                            style: "background-color: {segment.color};"
                                        }
                                        span { class: "text-white font-semibold text-xs", "{segment.label}" }
                                        span { class: "text-gray-400 text-xs ml-auto", "{segment.count}" }
                                    }

                                    // System list - 2 column grid
                                    div {
                                        class: "grid grid-cols-2 gap-x-2 gap-y-0.5",
                                        for system in segment.systems.iter().take(show_count) {
                                            div {
                                                class: "text-gray-300 text-xs font-mono truncate",
                                                "{system}"
                                            }
                                        }
                                    }

                                    if remaining > 0 {
                                        div {
                                            class: "text-gray-500 text-xs italic mt-1",
                                            "+{remaining} more..."
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    // Show legend when not hovering
                    div {
                        class: "flex flex-col gap-2",
                        for segment in segments.iter() {
                            div {
                                class: "flex items-center gap-2",
                                span {
                                    class: "w-3 h-3 rounded shrink-0",
                                    style: "background-color: {segment.color};"
                                }
                                span { class: "text-gray-400 text-sm", "{segment.label}" }
                                span { class: "text-white font-bold text-sm tabular-nums ml-auto", "{segment.count}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Compute arc data for rendering donut chart segments.
///
/// Uses the stroke-dasharray technique where each segment is a circle
/// with a dash that covers only that segment's portion of the circumference.
pub fn donut_arcs(segments: &[DonutSegment]) -> Vec<DonutArc> {
    let mut arcs = Vec::new();
    let circumference = 2.0 * std::f64::consts::PI * 40.0; // r=40
    let mut offset = 0.0;

    for (segment_idx, segment) in segments.iter().enumerate() {
        if segment.percent <= 0.0 {
            continue;
        }

        let dash_length = (segment.percent / 100.0) * circumference;
        let gap_length = circumference - dash_length;

        arcs.push(DonutArc {
            segment_idx,
            color: segment.color,
            dash_length,
            gap_length,
            offset: -offset, // negative because we rotate -90deg
        });

        offset += dash_length;
    }

    arcs
}
