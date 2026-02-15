//! Dashboard view — fleet-wide overview with health, deployment, and CVE summaries.

use chrono::{Duration, Utc};
use dioxus::prelude::*;
use std::collections::HashSet;

use crate::api::models::{
    CveSummary, DashboardSummary, DeploymentStatus, DeploymentStatusSummary, FlakeCommit,
    FlakeTimeline, FleetHealthSummary, RecentDeployment,
};
use crate::components::flake_timeline::FlakeTimelineWidget;
use crate::components::layout::Card;
use crate::components::stat_card::StatCard;
use crate::components::widget_grid::{GridWidget, WidgetGrid};
use crate::theme;

/// Global filter state for the dashboard - shared across all widgets
/// Supports multi-select: empty set means "all flakes", otherwise only selected flakes
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DashboardFilter {
    /// Set of selected flake indices (empty = all flakes selected)
    pub selected_flake_indices: HashSet<usize>,
    /// Names of selected flakes (for display)
    pub selected_flake_names: Vec<String>,
}

impl DashboardFilter {
    /// Returns true if all flakes are selected (no filter active)
    pub fn is_all_selected(&self) -> bool {
        self.selected_flake_indices.is_empty()
    }

    /// Returns true if the given flake index is selected
    pub fn is_flake_selected(&self, idx: usize) -> bool {
        self.selected_flake_indices.is_empty() || self.selected_flake_indices.contains(&idx)
    }

    /// Get display label for the current filter
    pub fn display_label(&self) -> String {
        if self.selected_flake_indices.is_empty() {
            "All Flakes".to_string()
        } else if self.selected_flake_names.len() == 1 {
            self.selected_flake_names[0].clone()
        } else {
            format!("{} flakes", self.selected_flake_names.len())
        }
    }
}

/// Widget position in the grid
#[derive(Clone, Debug, PartialEq)]
struct WidgetPosition {
    id: &'static str,
    title: &'static str,
    col: usize,
    row: usize,
    width: usize,
    height: usize,
}

/// Default widget layout
fn default_widget_positions() -> Vec<WidgetPosition> {
    vec![
        WidgetPosition {
            id: "fleet-health",
            title: "Fleet Health",
            col: 0,
            row: 0,
            width: 2,
            height: 2,
        },
        WidgetPosition {
            id: "deployment-status",
            title: "Deployment Status",
            col: 2,
            row: 0,
            width: 2,
            height: 2,
        },
        WidgetPosition {
            id: "cve-summary",
            title: "CVE Summary",
            col: 0,
            row: 2,
            width: 2,
            height: 2,
        },
        // Reduced from height: 3 to height: 2 to prevent overlap when moved
        WidgetPosition {
            id: "recent-deployments",
            title: "Recent Deployments",
            col: 2,
            row: 2,
            width: 2,
            height: 2,
        },
    ]
}

/// The main dashboard page.
#[component]
pub fn DashboardView() -> Element {
    // TODO: Replace with real API call using use_resource + fetch_dashboard()
    let dashboard = mock_dashboard_summary();
    let flake_timelines = mock_flake_timelines();

    // Global filter state - shared across all widgets (multi-select)
    let mut dashboard_filter = use_signal(DashboardFilter::default);

    // Widget layout state
    let mut widget_positions = use_signal(default_widget_positions);
    let mut dragging_id: Signal<Option<&'static str>> = use_signal(|| None);
    let mut drop_target_id: Signal<Option<&'static str>> = use_signal(|| None);

    // Handle drag start
    let on_drag_start = move |id: String| {
        // Find the static str for this id
        let positions = widget_positions.read();
        if let Some(pos) = positions.iter().find(|p| p.id == id) {
            dragging_id.set(Some(pos.id));
        }
    };

    // Handle drag over (highlight drop target)
    let on_drag_over = move |id: String| {
        let positions = widget_positions.read();
        if let Some(pos) = positions.iter().find(|p| p.id == id) {
            let current_dragging = *dragging_id.read();
            if current_dragging.is_some() && current_dragging != Some(pos.id) {
                drop_target_id.set(Some(pos.id));
            }
        }
    };

    // Handle drag leave (clear highlight)
    let on_drag_leave = move |_: ()| {
        drop_target_id.set(None);
    };

    // Handle drop (swap positions)
    let on_drop = move |target_id: String| {
        let dragging = *dragging_id.read();
        if let Some(source_id) = dragging {
            if source_id != target_id {
                // Swap positions of the two widgets
                let mut positions = widget_positions.write();
                let source_idx = positions.iter().position(|p| p.id == source_id);
                let target_idx = positions.iter().position(|p| p.id == target_id);

                if let (Some(src), Some(tgt)) = (source_idx, target_idx) {
                    // Swap col/row positions
                    let src_col = positions[src].col;
                    let src_row = positions[src].row;
                    positions[src].col = positions[tgt].col;
                    positions[src].row = positions[tgt].row;
                    positions[tgt].col = src_col;
                    positions[tgt].row = src_row;
                }
            }
        }
        dragging_id.set(None);
        drop_target_id.set(None);
    };

    // Get the current filter state
    let filter = dashboard_filter.read().clone();
    let filter_label = filter.display_label();
    let is_filtered = !filter.is_all_selected();

    // Filter recent deployments based on selected flakes
    // In a real app, deployments would have a flake_name field
    // For now, we simulate filtering by showing fewer items when filtered
    let filtered_deployments = if is_filtered {
        // When filtered, show only some deployments (simulated filter)
        dashboard
            .recent_deployments
            .iter()
            .take(3)
            .cloned()
            .collect()
    } else {
        dashboard.recent_deployments.clone()
    };

    // Render widget content based on id
    let render_widget_content = |id: &str| -> Element {
        let filter_display = if is_filtered {
            Some(filter_label.clone())
        } else {
            None
        };
        match id {
            "fleet-health" => rsx! {
                FleetHealthBreakdown {
                    health: dashboard.fleet_health.clone(),
                    flake_filter: filter_display.clone()
                }
            },
            "deployment-status" => rsx! {
                DeploymentStatusBreakdown {
                    status: dashboard.deployment_status.clone(),
                    flake_filter: filter_display.clone()
                }
            },
            "cve-summary" => rsx! {
                CveSummaryPanel {
                    cves: dashboard.cve_summary.clone(),
                    flake_filter: filter_display.clone()
                }
            },
            "recent-deployments" => rsx! {
                RecentDeploymentsList {
                    deployments: filtered_deployments.clone(),
                    flake_filter: filter_display.clone()
                }
            },
            _ => rsx! { div { "Unknown widget" } },
        }
    };

    rsx! {
        div {
            class: "space-y-8",
            "data-testid": "dashboard",

            // Top stats row
            div {
                class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4",
                StatCard {
                    label: "Total Systems".to_string(),
                    value: dashboard.total_systems.to_string()
                }
                StatCard {
                    label: "Healthy".to_string(),
                    value: dashboard.fleet_health.healthy.to_string(),
                    color_class: theme::health::HEALTHY_TEXT.to_string()
                }
                StatCard {
                    label: "Critical".to_string(),
                    value: dashboard.fleet_health.critical.to_string(),
                    color_class: theme::health::CRITICAL_TEXT.to_string()
                }
                StatCard {
                    label: "Active Builds".to_string(),
                    value: dashboard.active_builds.to_string(),
                    color_class: "text-blue-400".to_string()
                }
            }

            // Flake Commit Timeline with multi-select filter
            Card {
                title: None,
                children: rsx! {
                    FlakeTimelineWidget {
                        timelines: flake_timelines.clone(),
                        selected_flake_indices: dashboard_filter.read().selected_flake_indices.clone(),
                        on_filter_change: {
                            let flake_timelines = flake_timelines.clone();
                            move |indices: HashSet<usize>| {
                                let names: Vec<String> = indices.iter()
                                    .filter_map(|&idx| flake_timelines.get(idx).map(|t| t.flake_name.clone()))
                                    .collect();
                                dashboard_filter.set(DashboardFilter {
                                    selected_flake_indices: indices,
                                    selected_flake_names: names,
                                });
                            }
                        }
                    }
                }
            }

            // Widget grid header with reset button
            div {
                class: "flex items-center justify-between",
                h2 {
                    class: "text-lg font-semibold text-white",
                    "Dashboard Widgets"
                }
                button {
                    class: "px-3 py-1.5 text-xs font-medium text-gray-400 hover:text-white bg-gray-800 hover:bg-gray-700 border border-gray-700 rounded-lg transition-colors",
                    onclick: move |_| {
                        widget_positions.set(default_widget_positions());
                    },
                    "Reset Layout"
                }
            }

            // Widget grid with draggable widgets
            WidgetGrid {
                columns: 4,
                gap: 16,
                row_height: 100,

                for pos in widget_positions.read().iter() {
                    GridWidget {
                        key: "{pos.id}",
                        id: pos.id.to_string(),
                        title: pos.title.to_string(),
                        col: pos.col,
                        row: pos.row,
                        width: pos.width,
                        height: pos.height,
                        is_dragging: dragging_id.read().map_or(false, |d| d == pos.id),
                        is_drop_target: drop_target_id.read().map_or(false, |d| d == pos.id),
                        on_drag_start: on_drag_start,
                        on_drag_over: on_drag_over,
                        on_drag_leave: on_drag_leave,
                        on_drop: on_drop,
                        children: render_widget_content(pos.id)
                    }
                }
            }
        }
    }
}

/// Fleet health breakdown with colored donut chart.
#[component]
fn FleetHealthBreakdown(
    health: FleetHealthSummary,
    #[props(default)] flake_filter: Option<String>,
) -> Element {
    // When a flake is selected, we would filter the health data
    // For now, we show a filtered label and adjust the mock data slightly
    let (display_health, filter_label) = if let Some(ref flake_name) = flake_filter {
        // Simulate filtered data (in real app, this would come from API)
        let filtered = FleetHealthSummary {
            healthy: health.healthy / 3,
            warning: health.warning / 3,
            critical: health.critical.min(1),
            offline: health.offline.min(1),
        };
        (filtered, Some(flake_name.clone()))
    } else {
        (health.clone(), None)
    };

    let total = display_health.total().max(1) as f64;
    let total_count = display_health.total();

    // Mock system lists for each category
    let healthy_systems: Vec<String> = (1..=display_health.healthy.min(50))
        .map(|i| format!("server-{:02}", i))
        .collect();
    let warning_systems: Vec<String> = if flake_filter.is_some() {
        vec!["db-replica-01".into(), "cache-02".into()]
    } else {
        vec![
            "db-replica-01".into(),
            "cache-02".into(),
            "worker-07".into(),
            "api-staging".into(),
            "monitor-01".into(),
            "backup-srv".into(),
            "dev-box".into(),
        ]
    };
    let critical_systems: Vec<String> = vec!["db-primary".into()];
    let offline_systems: Vec<String> = vec!["legacy-app".into()];

    let segments = vec![
        DonutSegment {
            percent: display_health.healthy as f64 / total * 100.0,
            color: "#10b981",
            label: "Healthy",
            count: display_health.healthy,
            systems: healthy_systems,
        },
        DonutSegment {
            percent: display_health.warning as f64 / total * 100.0,
            color: "#f59e0b",
            label: "Warning",
            count: display_health.warning,
            systems: warning_systems,
        },
        DonutSegment {
            percent: display_health.critical as f64 / total * 100.0,
            color: "#ef4444",
            label: "Critical",
            count: display_health.critical,
            systems: critical_systems,
        },
        DonutSegment {
            percent: display_health.offline as f64 / total * 100.0,
            color: "#6b7280",
            label: "Offline",
            count: display_health.offline,
            systems: offline_systems,
        },
    ];

    rsx! {
        div {
            class: "h-full flex flex-col",
            "data-testid": "fleet-health-breakdown",

            // Show filter indicator if filtered
            if let Some(flake_name) = filter_label {
                div {
                    class: "text-xs text-blue-400 mb-1 flex items-center gap-1",
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
                class: "flex-1",
                DonutChartWithLegend {
                    segments: segments,
                    center_value: total_count,
                    center_label: "SYSTEMS"
                }
            }
        }
    }
}

/// A single legend item with color box and count.
#[component]
fn HealthLegendItem(
    label: &'static str,
    count: i64,
    dot_class: &'static str,
    #[props(default = false)] align_right: bool,
) -> Element {
    if align_right {
        rsx! {
            div {
                class: "flex items-center gap-2",
                span { class: "text-white font-bold text-sm tabular-nums", "{count}" }
                span { class: "{theme::text::SECONDARY} text-sm", "{label}" }
                span { class: "w-3 h-3 rounded shrink-0 {dot_class}" }
            }
        }
    } else {
        rsx! {
            div {
                class: "flex items-center gap-2",
                span { class: "w-3 h-3 rounded shrink-0 {dot_class}" }
                span { class: "{theme::text::SECONDARY} text-sm", "{label}" }
                span { class: "text-white font-bold text-sm tabular-nums", "{count}" }
            }
        }
    }
}

/// CVE summary panel with severity badges.
#[component]
fn CveSummaryPanel(cves: CveSummary, #[props(default)] flake_filter: Option<String>) -> Element {
    // Apply filter - in real app, this would come from API
    let display_cves = if let Some(ref _flake_name) = flake_filter {
        CveSummary {
            critical: cves.critical / 2,
            high: cves.high / 2,
            medium: cves.medium / 2,
            low: cves.low / 2,
        }
    } else {
        cves.clone()
    };

    let total = display_cves.total();

    rsx! {
        div {
            class: "space-y-4",
            "data-testid": "cve-summary",

            // Show filter indicator if filtered
            if let Some(ref flake_name) = flake_filter {
                div {
                    class: "text-xs text-blue-400 mb-1 flex items-center gap-1",
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

            // Total count header
            div {
                class: "flex items-baseline gap-2",
                span { class: "text-3xl font-bold text-white", "{total}" }
                span { class: "{theme::text::SECONDARY}", "total vulnerabilities" }
            }

            // Severity breakdown
            div {
                class: "grid grid-cols-2 gap-3",
                CveSeverityBadge { label: "Critical", count: display_cves.critical, text_class: theme::cve::CRITICAL_TEXT, bg_class: theme::cve::CRITICAL_BG }
                CveSeverityBadge { label: "High", count: display_cves.high, text_class: theme::cve::HIGH_TEXT, bg_class: theme::cve::HIGH_BG }
                CveSeverityBadge { label: "Medium", count: display_cves.medium, text_class: theme::cve::MEDIUM_TEXT, bg_class: theme::cve::MEDIUM_BG }
                CveSeverityBadge { label: "Low", count: display_cves.low, text_class: theme::cve::LOW_TEXT, bg_class: theme::cve::LOW_BG }
            }
        }
    }
}

/// A single CVE severity badge with count.
#[component]
fn CveSeverityBadge(
    label: &'static str,
    count: i64,
    text_class: &'static str,
    bg_class: &'static str,
) -> Element {
    rsx! {
        div {
            class: "flex items-center justify-between p-3 rounded-lg {bg_class}",
            span { class: "{text_class} font-medium", "{label}" }
            span { class: "{text_class} text-xl font-bold", "{count}" }
        }
    }
}

/// Deployment status breakdown.
#[component]
fn DeploymentStatusBreakdown(
    status: DeploymentStatusSummary,
    #[props(default)] flake_filter: Option<String>,
) -> Element {
    // Apply filter - in real app, this would come from API
    let display_status = if let Some(ref _flake_name) = flake_filter {
        DeploymentStatusSummary {
            up_to_date: status.up_to_date / 3,
            behind: status.behind / 3,
            never_deployed: status.never_deployed.min(1),
            unknown: 0,
        }
    } else {
        status.clone()
    };

    let total = display_status.total().max(1) as f64;
    let total_count = display_status.total();

    // Mock system lists
    let up_to_date_systems: Vec<String> = (1..=display_status.up_to_date.min(50))
        .map(|i| format!("prod-{:02}", i))
        .collect();
    let behind_systems: Vec<String> = if flake_filter.is_some() {
        vec![
            "staging-01".into(),
            "staging-02".into(),
            "dev-server".into(),
            "qa-box".into(),
        ]
    } else {
        vec![
            "staging-01".into(),
            "staging-02".into(),
            "dev-server".into(),
            "qa-box".into(),
            "perf-test".into(),
            "sandbox-01".into(),
            "demo-server".into(),
            "training-vm".into(),
            "backup-01".into(),
            "backup-02".into(),
            "dr-site".into(),
            "edge-node".into(),
        ]
    };
    let never_deployed_systems: Vec<String> = vec!["new-server-01".into()];
    let unknown_systems: Vec<String> = vec![];

    let segments = vec![
        DonutSegment {
            percent: display_status.up_to_date as f64 / total * 100.0,
            color: "#10b981",
            label: "Up to Date",
            count: display_status.up_to_date,
            systems: up_to_date_systems,
        },
        DonutSegment {
            percent: display_status.behind as f64 / total * 100.0,
            color: "#f59e0b",
            label: "Behind",
            count: display_status.behind,
            systems: behind_systems,
        },
        DonutSegment {
            percent: display_status.never_deployed as f64 / total * 100.0,
            color: "#4b5563",
            label: "Never Deployed",
            count: display_status.never_deployed,
            systems: never_deployed_systems,
        },
        DonutSegment {
            percent: display_status.unknown as f64 / total * 100.0,
            color: "#6b7280",
            label: "Unknown",
            count: display_status.unknown,
            systems: unknown_systems,
        },
    ];

    rsx! {
        div {
            class: "h-full flex flex-col",
            "data-testid": "deployment-status",

            // Show filter indicator if filtered
            if let Some(ref flake_name) = flake_filter {
                div {
                    class: "text-xs text-blue-400 mb-1 flex items-center gap-1",
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
                class: "flex-1",
                DonutChartWithLegend {
                    segments: segments,
                    center_value: total_count,
                    center_label: "DEPLOYED"
                }
            }
        }
    }
}

/// A single deployment legend item.
#[component]
fn DeploymentLegendItem(
    label: &'static str,
    count: i64,
    dot_class: &'static str,
    #[props(default = false)] align_right: bool,
) -> Element {
    if align_right {
        rsx! {
            div {
                class: "flex items-center gap-2",
                span { class: "text-white font-bold text-sm tabular-nums", "{count}" }
                span { class: "{theme::text::SECONDARY} text-sm", "{label}" }
                span { class: "w-3 h-3 rounded shrink-0 {dot_class}" }
            }
        }
    } else {
        rsx! {
            div {
                class: "flex items-center gap-2",
                span { class: "w-3 h-3 rounded shrink-0 {dot_class}" }
                span { class: "{theme::text::SECONDARY} text-sm", "{label}" }
                span { class: "text-white font-bold text-sm tabular-nums", "{count}" }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct DonutSegment {
    percent: f64,
    color: &'static str,
    label: &'static str,
    count: i64,
    systems: Vec<String>,
}

#[derive(Props, Clone, PartialEq)]
struct DonutChartWithLegendProps {
    segments: Vec<DonutSegment>,
    center_value: i64,
    center_label: &'static str,
}

/// Donut chart with legend on right side, hover shows system list in place of legend
#[component]
fn DonutChartWithLegend(props: DonutChartWithLegendProps) -> Element {
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
                    for (idx, arc) in arcs.iter().enumerate() {
                        circle {
                            cx: "50",
                            cy: "50",
                            r: "40",
                            fill: "none",
                            stroke: "{arc.color}",
                            stroke_width: if hovered_idx.read().map_or(false, |h| h == idx) { "18" } else { "14" },
                            stroke_dasharray: "{arc.dash_length} {arc.gap_length}",
                            stroke_dashoffset: "{arc.offset}",
                            stroke_linecap: "butt",
                            transform: "rotate(-90 50 50)",
                            style: "cursor: pointer; transition: stroke-width 0.15s ease;",
                            onmouseenter: move |_| {
                                hovered_idx.set(Some(idx));
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

/// Arc data for donut chart rendering using stroke-dasharray technique
#[derive(Clone, Debug, PartialEq)]
struct DonutArc {
    color: &'static str,
    dash_length: f64,
    gap_length: f64,
    offset: f64,
}

fn donut_arcs(segments: &[DonutSegment]) -> Vec<DonutArc> {
    let mut arcs = Vec::new();
    let circumference = 2.0 * std::f64::consts::PI * 40.0; // r=40
    let mut offset = 0.0;

    for segment in segments {
        if segment.percent <= 0.0 {
            continue;
        }

        let dash_length = (segment.percent / 100.0) * circumference;
        let gap_length = circumference - dash_length;

        arcs.push(DonutArc {
            color: segment.color,
            dash_length,
            gap_length,
            offset: -offset, // negative because we rotate -90deg
        });

        offset += dash_length;
    }

    arcs
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
struct PieSlice {
    path: String,
    color: &'static str,
}

#[allow(dead_code)]
fn pie_slices(segments: &[DonutSegment]) -> Vec<PieSlice> {
    let mut slices = Vec::new();
    let mut current_angle = -90.0_f64;
    let mut remaining = 100.0_f64;
    let cx = 60.0;
    let cy = 60.0;
    let r = 58.0;

    for segment in segments {
        if segment.percent <= 0.0 || remaining <= 0.0 {
            continue;
        }

        let percent = segment.percent.min(remaining);
        remaining -= percent;
        let sweep = percent / 100.0 * 360.0;
        let start = current_angle;
        let end = current_angle + sweep;
        current_angle = end;

        if sweep >= 359.9 {
            slices.push(PieSlice {
                path: format!(
                    "M {cx} {cy} m -{r} 0 a {r} {r} 0 1 0 {} 0 a {r} {r} 0 1 0 -{} 0",
                    r * 2.0,
                    r * 2.0
                ),
                color: segment.color,
            });
            break;
        }

        let (x1, y1) = polar_to_cartesian(cx, cy, r, start);
        let (x2, y2) = polar_to_cartesian(cx, cy, r, end);
        let large_arc = if sweep > 180.0 { 1 } else { 0 };

        let path =
            format!("M {cx} {cy} L {x1:.2} {y1:.2} A {r} {r} 0 {large_arc} 1 {x2:.2} {y2:.2} Z");

        slices.push(PieSlice {
            path,
            color: segment.color,
        });
    }

    slices
}

fn polar_to_cartesian(cx: f64, cy: f64, r: f64, angle_deg: f64) -> (f64, f64) {
    let rad = angle_deg.to_radians();
    (cx + r * rad.cos(), cy + r * rad.sin())
}

/// Recent deployments list.
#[component]
fn RecentDeploymentsList(
    deployments: Vec<RecentDeployment>,
    #[props(default)] flake_filter: Option<String>,
) -> Element {
    if deployments.is_empty() {
        return rsx! {
            p { class: "{theme::text::SECONDARY}", "No recent deployments." }
        };
    }

    rsx! {
        div {
            class: "flex flex-col h-full",
            "data-testid": "recent-deployments",

            // Show filter indicator if filtered
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

            // Scrollable list container - prevents overflow when widget is moved
            div {
                class: "flex-1 min-h-0 overflow-y-auto space-y-2",
                for deployment in deployments {
                    RecentDeploymentRow { deployment }
                }
            }
        }
    }
}

/// A single deployment row in the recent deployments list.
#[component]
fn RecentDeploymentRow(deployment: RecentDeployment) -> Element {
    let status_color = deployment.status.color_class();
    let time_ago = format_time_ago(deployment.deployed_at);
    let short_hash = deployment.commit_hash.chars().take(7).collect::<String>();

    rsx! {
        div {
            class: "flex items-center justify-between p-3 rounded-lg {theme::surface::SUBTLE_BG}",
            div {
                class: "flex items-center gap-3",
                // Status indicator dot
                span {
                    class: "w-2 h-2 rounded-full",
                    class: if deployment.status == DeploymentStatus::UpToDate { "bg-emerald-500" } else { "bg-amber-500" }
                }
                div {
                    p { class: "text-white font-medium", "{deployment.hostname}" }
                    p { class: "{theme::text::MUTED} text-xs font-mono", "{short_hash}" }
                }
            }
            div {
                class: "text-right",
                p { class: "{status_color} text-sm", "{deployment.status.label()}" }
                p { class: "{theme::text::MUTED} text-xs", "{time_ago}" }
            }
        }
    }
}

/// Format a datetime as relative time (e.g., "2 hours ago").
fn format_time_ago(dt: chrono::DateTime<chrono::Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(dt);

    if duration < Duration::minutes(1) {
        "just now".to_string()
    } else if duration < Duration::hours(1) {
        let mins = duration.num_minutes();
        format!("{} min{} ago", mins, if mins == 1 { "" } else { "s" })
    } else if duration < Duration::days(1) {
        let hours = duration.num_hours();
        format!("{} hour{} ago", hours, if hours == 1 { "" } else { "s" })
    } else {
        let days = duration.num_days();
        format!("{} day{} ago", days, if days == 1 { "" } else { "s" })
    }
}

/// Generate mock dashboard data for development.
fn mock_dashboard_summary() -> DashboardSummary {
    let now = Utc::now();

    DashboardSummary {
        fleet_health: FleetHealthSummary {
            healthy: 42,
            warning: 7,
            critical: 3,
            offline: 2,
        },
        deployment_status: DeploymentStatusSummary {
            up_to_date: 38,
            behind: 12,
            never_deployed: 3,
            unknown: 1,
        },
        cve_summary: CveSummary {
            critical: 5,
            high: 23,
            medium: 67,
            low: 142,
        },
        total_systems: 54,
        active_builds: 3,
        recent_deployments: vec![
            RecentDeployment {
                hostname: "atlas-01".to_string(),
                commit_hash: "a1b2c3d4e5f6789".to_string(),
                deployed_at: now - Duration::minutes(15),
                status: DeploymentStatus::UpToDate,
            },
            RecentDeployment {
                hostname: "nova-05".to_string(),
                commit_hash: "f9e8d7c6b5a4321".to_string(),
                deployed_at: now - Duration::hours(2),
                status: DeploymentStatus::UpToDate,
            },
            RecentDeployment {
                hostname: "luna-02".to_string(),
                commit_hash: "1234567890abcdef".to_string(),
                deployed_at: now - Duration::hours(5),
                status: DeploymentStatus::Behind,
            },
            RecentDeployment {
                hostname: "orion-03".to_string(),
                commit_hash: "deadbeefcafe1234".to_string(),
                deployed_at: now - Duration::days(1),
                status: DeploymentStatus::UpToDate,
            },
            RecentDeployment {
                hostname: "vega-04".to_string(),
                commit_hash: "cafe1234deadbeef".to_string(),
                deployed_at: now - Duration::days(2),
                status: DeploymentStatus::Behind,
            },
        ],
        timestamp: now,
    }
}

/// Generate mock flake timeline data for development.
fn mock_flake_timelines() -> Vec<FlakeTimeline> {
    let now = Utc::now();

    vec![
        FlakeTimeline {
            flake_id: 1,
            flake_name: "infrastructure".to_string(),
            repo_url: "github:acme/infra".to_string(),
            commits: vec![
                FlakeCommit {
                    hash: "a1b2c3d4e5f6789012345678".to_string(),
                    message: "feat: add monitoring stack".to_string(),
                    author: "alice".to_string(),
                    committed_at: now - Duration::hours(3),
                    system_count: 5,
                    commits_behind: 0,
                    systems: vec![
                        "atlas-01".to_string(),
                        "atlas-02".to_string(),
                        "atlas-03".to_string(),
                        "atlas-04".to_string(),
                        "atlas-05".to_string(),
                    ],
                },
                FlakeCommit {
                    hash: "b2c3d4e5f6789012345678ab".to_string(),
                    message: "fix: nginx config reload".to_string(),
                    author: "bob".to_string(),
                    committed_at: now - Duration::hours(6),
                    system_count: 2,
                    commits_behind: 1,
                    systems: vec!["luna-01".to_string(), "luna-02".to_string()],
                },
                FlakeCommit {
                    hash: "c3d4e5f6789012345678abcd".to_string(),
                    message: "chore: update nixpkgs".to_string(),
                    author: "alice".to_string(),
                    committed_at: now - Duration::days(3),
                    system_count: 1,
                    commits_behind: 2,
                    systems: vec!["orion-01".to_string()],
                },
                FlakeCommit {
                    hash: "d4e5f6789012345678abcdef".to_string(),
                    message: "fix: postgres backup cron".to_string(),
                    author: "charlie".to_string(),
                    committed_at: now - Duration::days(7),
                    system_count: 0,
                    commits_behind: 3,
                    systems: vec![],
                },
                FlakeCommit {
                    hash: "e5f6789012345678abcdef01".to_string(),
                    message: "fix: secrets rotation".to_string(),
                    author: "alice".to_string(),
                    committed_at: now - Duration::days(12),
                    system_count: 0,
                    commits_behind: 4,
                    systems: vec![],
                },
                FlakeCommit {
                    hash: "f6a7890123456789abcdef01".to_string(),
                    message: "chore: pin postgres".to_string(),
                    author: "bob".to_string(),
                    committed_at: now - Duration::days(18),
                    system_count: 0,
                    commits_behind: 5,
                    systems: vec![],
                },
                FlakeCommit {
                    hash: "0a1b2c3d4e5f6789abcdef12".to_string(),
                    message: "feat: initial setup".to_string(),
                    author: "alice".to_string(),
                    committed_at: now - Duration::days(27),
                    system_count: 0,
                    commits_behind: 6,
                    systems: vec![],
                },
            ],
        },
        FlakeTimeline {
            flake_id: 2,
            flake_name: "workstations".to_string(),
            repo_url: "github:acme/workstations".to_string(),
            commits: vec![
                FlakeCommit {
                    hash: "f1a2b3c4d5e6f7890123456".to_string(),
                    message: "feat: add vscode extensions".to_string(),
                    author: "dave".to_string(),
                    committed_at: now - Duration::hours(8),
                    system_count: 8,
                    commits_behind: 0,
                    systems: vec![
                        "ws-001".to_string(),
                        "ws-002".to_string(),
                        "ws-003".to_string(),
                        "ws-004".to_string(),
                        "ws-005".to_string(),
                        "ws-006".to_string(),
                        "ws-007".to_string(),
                        "ws-008".to_string(),
                    ],
                },
                FlakeCommit {
                    hash: "a2b3c4d5e6f78901234567ab".to_string(),
                    message: "fix: bluetooth audio".to_string(),
                    author: "eve".to_string(),
                    committed_at: now - Duration::days(2),
                    system_count: 2,
                    commits_behind: 1,
                    systems: vec!["ws-009".to_string(), "ws-010".to_string()],
                },
                FlakeCommit {
                    hash: "b3c4d5e6f78901234567abcd".to_string(),
                    message: "chore: cleanup old pkgs".to_string(),
                    author: "dave".to_string(),
                    committed_at: now - Duration::days(5),
                    system_count: 1,
                    commits_behind: 2,
                    systems: vec!["ws-011".to_string()],
                },
                FlakeCommit {
                    hash: "c4d5e6f78901234567abcdef".to_string(),
                    message: "feat: add docker support".to_string(),
                    author: "eve".to_string(),
                    committed_at: now - Duration::days(14),
                    system_count: 0,
                    commits_behind: 3,
                    systems: vec![],
                },
                FlakeCommit {
                    hash: "d5e6f78901234567abcdef01".to_string(),
                    message: "chore: bump nixos".to_string(),
                    author: "dave".to_string(),
                    committed_at: now - Duration::days(27),
                    system_count: 0,
                    commits_behind: 4,
                    systems: vec![],
                },
                FlakeCommit {
                    hash: "e6f78901234567abcdef0123".to_string(),
                    message: "fix: input latency".to_string(),
                    author: "eve".to_string(),
                    committed_at: now - Duration::days(41),
                    system_count: 0,
                    commits_behind: 5,
                    systems: vec![],
                },
                FlakeCommit {
                    hash: "f78901234567abcdef012345".to_string(),
                    message: "chore: enable fprint".to_string(),
                    author: "dave".to_string(),
                    committed_at: now - Duration::days(56),
                    system_count: 0,
                    commits_behind: 6,
                    systems: vec![],
                },
                FlakeCommit {
                    hash: "a8901234567abcdef0123456".to_string(),
                    message: "feat: initial setup".to_string(),
                    author: "eve".to_string(),
                    committed_at: now - Duration::days(74),
                    system_count: 0,
                    commits_behind: 7,
                    systems: vec![],
                },
                FlakeCommit {
                    hash: "b901234567abcdef01234567".to_string(),
                    message: "chore: template cleanup".to_string(),
                    author: "dave".to_string(),
                    committed_at: now - Duration::days(90),
                    system_count: 0,
                    commits_behind: 8,
                    systems: vec![],
                },
            ],
        },
        FlakeTimeline {
            flake_id: 3,
            flake_name: "edge-nodes".to_string(),
            repo_url: "github:acme/edge".to_string(),
            commits: vec![
                FlakeCommit {
                    hash: "1234567890abcdef12345678".to_string(),
                    message: "fix: wireguard tunnel".to_string(),
                    author: "frank".to_string(),
                    committed_at: now - Duration::hours(5),
                    system_count: 10,
                    commits_behind: 0,
                    systems: vec![
                        "edge-us-east".to_string(),
                        "edge-us-west".to_string(),
                        "edge-eu-west".to_string(),
                        "edge-eu-central".to_string(),
                        "edge-ap-south".to_string(),
                        "edge-ap-north".to_string(),
                        "edge-sa-east".to_string(),
                        "edge-ca-central".to_string(),
                        "edge-us-south".to_string(),
                        "edge-us-north".to_string(),
                    ],
                },
                FlakeCommit {
                    hash: "234567890abcdef123456789".to_string(),
                    message: "feat: add metrics export".to_string(),
                    author: "grace".to_string(),
                    committed_at: now - Duration::hours(14),
                    system_count: 4,
                    commits_behind: 1,
                    systems: vec![
                        "edge-eu-west".to_string(),
                        "edge-ap-south".to_string(),
                        "edge-sa-east".to_string(),
                        "edge-us-north".to_string(),
                    ],
                },
                FlakeCommit {
                    hash: "34567890abcdef1234567890".to_string(),
                    message: "chore: rotate certs".to_string(),
                    author: "frank".to_string(),
                    committed_at: now - Duration::days(1),
                    system_count: 2,
                    commits_behind: 2,
                    systems: vec!["edge-ap-south".to_string(), "edge-us-west".to_string()],
                },
                FlakeCommit {
                    hash: "4567890abcdef12345678901".to_string(),
                    message: "fix: reboot watchdog".to_string(),
                    author: "grace".to_string(),
                    committed_at: now - Duration::days(4),
                    system_count: 1,
                    commits_behind: 3,
                    systems: vec!["edge-eu-central".to_string()],
                },
                FlakeCommit {
                    hash: "567890abcdef123456789012".to_string(),
                    message: "chore: bump kernel".to_string(),
                    author: "frank".to_string(),
                    committed_at: now - Duration::days(10),
                    system_count: 0,
                    commits_behind: 4,
                    systems: vec![],
                },
                FlakeCommit {
                    hash: "67890abcdef1234567890123".to_string(),
                    message: "fix: gps sync".to_string(),
                    author: "grace".to_string(),
                    committed_at: now - Duration::days(15),
                    system_count: 0,
                    commits_behind: 5,
                    systems: vec![],
                },
                FlakeCommit {
                    hash: "7890abcdef12345678901234".to_string(),
                    message: "feat: initial setup".to_string(),
                    author: "frank".to_string(),
                    committed_at: now - Duration::days(21),
                    system_count: 0,
                    commits_behind: 6,
                    systems: vec![],
                },
            ],
        },
    ]
}
