//! Dashboard view — fleet-wide overview with health, deployment, and CVE summaries.

use chrono::Duration;
use dioxus::prelude::*;
use std::collections::HashSet;

use crate::api::models::{BuildStatus, FlakeCommit, FlakeTimeline};
use crate::components::dashboard::{
    BuildQueuePanel, BuildSummaryPanel, CveSummaryPanel, DeploymentStatusBreakdown,
    FleetHealthBreakdown, RecentDeploymentsList,
};
use crate::components::flake::FlakeTimelineWidget;
use crate::components::layout::Card;
use crate::components::notifications::{AlertBanner, AlertSeverity};
use crate::components::stat_card::StatCard;
use crate::components::widget_grid::{GridWidget, WidgetGrid};
use crate::dashboard::adapter::{
    deterministic_mock_timestamp, fallback_build_queue_summary, fallback_dashboard_summary,
    load_dashboard_with_fallback, load_flake_timelines_with_fallback,
};
use crate::routes::Route;
use crate::state::app_state::AppState;
use crate::state::auth;
use crate::theme;

/// Global filter state for the dashboard - shared across all widgets.
/// Supports multi-select: empty set means "all flakes", otherwise only selected flakes.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DashboardFilter {
    /// Set of selected flake indices (empty = all flakes selected)
    pub selected_flake_indices: HashSet<usize>,
    /// Names of selected flakes (for display)
    pub selected_flake_names: Vec<String>,
}

impl DashboardFilter {
    /// Returns true if all flakes are selected (no filter active).
    pub fn is_all_selected(&self) -> bool {
        self.selected_flake_indices.is_empty()
    }

    /// Returns true if the given flake index is selected.
    pub fn is_flake_selected(&self, idx: usize) -> bool {
        self.selected_flake_indices.is_empty() || self.selected_flake_indices.contains(&idx)
    }

    /// Get display label for the current filter.
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

/// Widget position in the grid.
#[derive(Clone, Debug, PartialEq)]
struct WidgetPosition {
    id: &'static str,
    title: &'static str,
    col: usize,
    row: usize,
    width: usize,
    height: usize,
}

/// Default widget layout configuration.
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
            id: "build-summary",
            title: "Build Summary",
            col: 0,
            row: 2,
            width: 2,
            height: 2,
        },
        WidgetPosition {
            id: "recent-deployments",
            title: "Recent Deployments",
            col: 2,
            row: 2,
            width: 2,
            height: 3,
        },
        WidgetPosition {
            id: "build-queue",
            title: "Build Queue",
            col: 0,
            row: 4,
            width: 2,
            height: 3,
        },
        WidgetPosition {
            id: "cve-summary",
            title: "CVE Summary",
            col: 2,
            row: 5,
            width: 2,
            height: 2,
        },
        WidgetPosition {
            id: "config-health",
            title: "Pipeline Readiness",
            col: 0,
            row: 7,
            width: 4,
            height: 2,
        },
    ]
}

/// The main dashboard page.
#[component]
pub fn DashboardView() -> Element {
    let nav = navigator();

    let dashboard = use_signal(fallback_dashboard_summary);
    let dashboard_notice = use_signal(|| None::<String>);
    let loading_dashboard = use_signal(|| true);
    let redirect_to_login = use_signal(|| false);

    // Shared config health (admin only).
    let app_state = use_context::<Signal<AppState>>();
    let state_read = app_state.read();
    let is_admin_user = auth::is_admin(&state_read.auth, &state_read.masquerade_role);
    let config_health = state_read.config_health.clone();

    // Flake timelines state
    let flake_timelines = use_signal(Vec::<FlakeTimeline>::new);
    let timelines_notice = use_signal(|| None::<String>);
    let loading_timelines = use_signal(|| true);

    {
        let mut dashboard = dashboard.clone();
        let mut dashboard_notice = dashboard_notice.clone();
        let mut loading_dashboard = loading_dashboard.clone();
        let mut redirect_to_login = redirect_to_login.clone();

        use_effect(move || {
            spawn(async move {
                let load_result = load_dashboard_with_fallback().await;

                if load_result.redirect_to_login {
                    redirect_to_login.set(true);
                    loading_dashboard.set(false);
                    return;
                }

                dashboard.set(load_result.summary);
                dashboard_notice.set(load_result.notice);
                loading_dashboard.set(false);
            });
        });
    }

    {
        let mut flake_timelines = flake_timelines.clone();
        let mut timelines_notice = timelines_notice.clone();
        let mut loading_timelines = loading_timelines.clone();
        let mut redirect_to_login = redirect_to_login.clone();

        use_effect(move || {
            spawn(async move {
                let load_result = load_flake_timelines_with_fallback().await;

                if load_result.redirect_to_login {
                    redirect_to_login.set(true);
                    loading_timelines.set(false);
                    return;
                }

                flake_timelines.set(load_result.timelines);
                timelines_notice.set(load_result.notice);
                loading_timelines.set(false);
            });
        });
    }

    if *redirect_to_login.read() {
        nav.push(Route::LoginView {});
        return rsx! {
            div {
                class: "min-h-screen flex items-center justify-center {theme::surface::PAGE_BG}",
                p {
                    class: "{theme::text::SECONDARY}",
                    "Redirecting to login..."
                }
            }
        };
    }

    let dashboard = dashboard.read().clone();
    let timelines = flake_timelines.read().clone();
    let build_queue = dashboard
        .build_queue
        .clone()
        .unwrap_or_else(|| fallback_build_queue_summary(dashboard.timestamp));

    // Global filter state - shared across all widgets (multi-select)
    let mut dashboard_filter = use_signal(DashboardFilter::default);

    // Widget layout state
    let mut widget_positions = use_signal(default_widget_positions);
    let mut dragging_id: Signal<Option<&'static str>> = use_signal(|| None);
    let mut drop_target_id: Signal<Option<&'static str>> = use_signal(|| None);
    let mut invalid_drop_target_id: Signal<Option<&'static str>> = use_signal(|| None);
    let mut drag_over_index: Signal<Option<usize>> = use_signal(|| None);

    // Handle drag start
    let on_drag_start = move |id: String| {
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
            if let Some(source_id) = current_dragging {
                if source_id != pos.id {
                    if let Some(source) = positions.iter().find(|p| p.id == source_id) {
                        let fits = pos.col + source.width <= 4;
                        if fits {
                            drop_target_id.set(Some(pos.id));
                            invalid_drop_target_id.set(None);
                        } else {
                            invalid_drop_target_id.set(Some(pos.id));
                            drop_target_id.set(None);
                        }
                    }
                }
            }
        }

        if let Some(index) = positions.iter().position(|p| p.id == id) {
            drag_over_index.set(Some(index));
        }
    };

    // Handle drag leave (clear highlight)
    let on_drag_leave = move |_: ()| {
        drop_target_id.set(None);
        invalid_drop_target_id.set(None);
        drag_over_index.set(None);
    };

    // Handle drop (reorder by target index and repack to avoid overlaps)
    let on_drop = move |target_id: String| {
        let dragging = *dragging_id.read();
        if let Some(source_id) = dragging {
            if source_id != target_id {
                let mut positions = widget_positions.write();
                let source_idx = positions.iter().position(|p| p.id == source_id);
                let target_idx = positions.iter().position(|p| p.id == target_id);

                if let (Some(src), Some(tgt)) = (source_idx, target_idx) {
                    let columns = 4usize;
                    let fits = |col: usize, width: usize| col + width <= columns;

                    if !fits(positions[tgt].col, positions[src].width) {
                        dragging_id.set(None);
                        drop_target_id.set(None);
                        invalid_drop_target_id.set(None);
                        drag_over_index.set(None);
                        return;
                    }

                    let mut ordered: Vec<WidgetPosition> = positions.iter().cloned().collect();
                    let dragged = ordered.remove(src);

                    let mut insert_at = drag_over_index.read().unwrap_or(tgt);
                    if src < insert_at {
                        insert_at = insert_at.saturating_sub(1);
                    }
                    insert_at = insert_at.min(ordered.len());
                    ordered.insert(insert_at, dragged);

                    let mut occupancy: Vec<Vec<bool>> = Vec::new();

                    for widget in &mut ordered {
                        let mut row = 0usize;
                        let width = widget.width;
                        let height = widget.height;

                        loop {
                            if occupancy.len() < row + height {
                                occupancy.resize_with(row + height, || vec![false; columns]);
                            }

                            let mut placed = false;
                            for col in 0..=columns.saturating_sub(width) {
                                let mut can_place = true;
                                for check_row in row..row + height {
                                    for check_col in col..col + width {
                                        if occupancy[check_row][check_col] {
                                            can_place = false;
                                            break;
                                        }
                                    }
                                    if !can_place {
                                        break;
                                    }
                                }

                                if can_place {
                                    widget.col = col;
                                    widget.row = row;
                                    for mark_row in row..row + height {
                                        for mark_col in col..col + width {
                                            occupancy[mark_row][mark_col] = true;
                                        }
                                    }
                                    placed = true;
                                    break;
                                }
                            }

                            if placed {
                                break;
                            }

                            row += 1;
                        }
                    }

                    *positions = ordered;
                }
            }
        }
        dragging_id.set(None);
        drop_target_id.set(None);
        invalid_drop_target_id.set(None);
        drag_over_index.set(None);
    };

    // Get the current filter state
    let filter = dashboard_filter.read().clone();
    let filter_label = filter.display_label();
    let is_filtered = !filter.is_all_selected();

    // Filter recent deployments based on selected flakes
    let filtered_deployments = if is_filtered {
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
            "build-summary" => rsx! {
                BuildSummaryPanel {
                    queue: build_queue.clone(),
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
            "build-queue" => rsx! {
                BuildQueuePanel {
                    queue: build_queue.clone(),
                    flake_filter: filter_display.clone()
                }
            },
            "config-health" => {
                if !is_admin_user {
                    // Non-admins don't see this widget at all.
                    return rsx! {};
                }
                let health_snapshot = config_health.clone();
                match health_snapshot {
                    None => rsx! {
                        p {
                            class: "text-xs {theme::text::SECONDARY}",
                            "Checking pipeline readiness..."
                        }
                    },
                    Some(ref h) if h.total_issues == 0 => rsx! {
                        div {
                            class: "flex items-center gap-2 text-emerald-400",
                            svg {
                                class: "w-5 h-5 shrink-0",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M5 13l4 4L19 7",
                                }
                            }
                            span {
                                class: "text-sm font-medium",
                                "All pipeline stages are configured and ready."
                            }
                        }
                    },
                    Some(ref h) => {
                        let suffix = if h.total_issues == 1 { "" } else { "s" };
                        let heading =
                            format!("{} configuration issue{} detected", h.total_issues, suffix);
                        rsx! {
                            div {
                                class: "space-y-3 rounded-xl border border-amber-300/35 bg-gradient-to-br from-amber-950/75 via-amber-900/30 to-yellow-950/10 p-4 shadow-[inset_0_1px_0_rgba(252,211,77,0.08)]",
                                style: "background: linear-gradient(180deg, rgba(120, 53, 15, 0.32), rgba(120, 53, 15, 0.12)); border-color: rgba(245, 158, 11, 0.3); box-shadow: inset 0 1px 0 rgba(253, 230, 138, 0.08);",
                                div {
                                    class: "flex items-center gap-2",
                                    div {
                                        class: "flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-amber-300/22 text-xs font-bold text-amber-100 border border-amber-200/20",
                                        style: "background: rgba(245, 158, 11, 0.18); color: rgb(254, 243, 199); border-color: rgba(252, 211, 77, 0.22);",
                                        "!"
                                    }
                                    p {
                                        class: "text-xs font-semibold text-amber-100 uppercase tracking-[0.18em]",
                                        style: "color: rgb(253, 230, 138);",
                                        "{heading}"
                                    }
                                }
                                for check in h.checks.iter().filter(|c| !c.passed) {
                                    AlertBanner {
                                        key: "{check.id}",
                                        severity: AlertSeverity::Warning,
                                        message: check.message.clone(),
                                        action_label: Some("Fix →".to_string()),
                                        action_url: Some(check.action_url.clone()),
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => rsx! { div { "Unknown widget" } },
        }
    };

    rsx! {
        div {
            class: "space-y-8",
            "data-testid": "dashboard",

            // Top stats row
            if *loading_dashboard.read() {
                p {
                    class: "text-xs px-3 py-2 rounded-lg border text-blue-100 cf-chip-info",
                    "Loading dashboard data..."
                }
            }

            if let Some(message) = dashboard_notice.read().clone() {
                p {
                    class: "text-xs px-3 py-2 rounded-lg border text-amber-100 cf-chip-warning",
                    "{message}"
                }
            }

            div {
                class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4",
                StatCard {
                    label: "Total Systems".to_string(),
                    value: dashboard.total_systems.to_string()
                }
                StatCard {
                    label: "Up to Date".to_string(),
                    value: dashboard.deployment_status.up_to_date.to_string(),
                    color_class: theme::deployment::UP_TO_DATE_TEXT.to_string()
                }
                StatCard {
                    label: "Behind Latest".to_string(),
                    value: dashboard.deployment_status.behind.to_string(),
                    color_class: theme::deployment::BEHIND_TEXT.to_string()
                }
                StatCard {
                    label: "No Recent Heartbeat".to_string(),
                    value: dashboard.fleet_health.offline.to_string(),
                    color_class: theme::health::OFFLINE_TEXT.to_string()
                }
            }

            // Flake Commit Timeline with multi-select filter
            if *loading_timelines.read() {
                p {
                    class: "text-xs px-3 py-2 rounded-lg border text-blue-100 cf-chip-info",
                    "Loading flake timelines..."
                }
            }

            if let Some(message) = timelines_notice.read().clone() {
                p {
                    class: "text-xs px-3 py-2 rounded-lg border text-amber-100 cf-chip-warning",
                    "{message}"
                }
            }

            Card {
                title: None,
                children: rsx! {
                    FlakeTimelineWidget {
                        timelines: timelines.clone(),
                        selected_flake_indices: dashboard_filter.read().selected_flake_indices.clone(),
                        on_filter_change: {
                            let timelines_signal = flake_timelines.clone();
                            move |indices: HashSet<usize>| {
                                let current_timelines = timelines_signal.read();
                                let names: Vec<String> = indices.iter()
                                    .filter_map(|&idx| current_timelines.get(idx).map(|t| t.flake_name.clone()))
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
                    class: "text-lg font-semibold {theme::text::PRIMARY}",
                    "Dashboard Widgets"
                }
                button {
                    class: "px-3 py-1.5 text-xs font-medium {theme::text::SECONDARY} {theme::interactive::HOVER_BG} {theme::surface::SUBTLE_BG} border {theme::surface::CARD_BORDER} rounded-lg transition-colors",
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
                        is_invalid_drop_target: invalid_drop_target_id.read().map_or(false, |d| d == pos.id),
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

// =============================================================================
// Mock Data Functions (for development)
// =============================================================================

/// Generate mock flake timeline data for development.
pub fn mock_flake_timelines() -> Vec<FlakeTimeline> {
    let now = deterministic_mock_timestamp();

    vec![
        FlakeTimeline {
            flake_id: 1,
            flake_name: "infrastructure".to_string(),
            repo_url: "github:acme/infra".to_string(),
            commits: vec![
                FlakeCommit {
                    id: 1,
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
                    build_status: Some(BuildStatus::Building),
                    evaluation_status: None,
                },
                FlakeCommit {
                    id: 2,
                    hash: "b2c3d4e5f6789012345678ab".to_string(),
                    message: "fix: nginx config reload".to_string(),
                    author: "bob".to_string(),
                    committed_at: now - Duration::hours(6),
                    system_count: 2,
                    commits_behind: 1,
                    systems: vec!["luna-01".to_string(), "luna-02".to_string()],
                    build_status: Some(BuildStatus::Queued),
                    evaluation_status: None,
                },
                FlakeCommit {
                    id: 3,
                    hash: "c3d4e5f6789012345678abcd".to_string(),
                    message: "chore: update nixpkgs".to_string(),
                    author: "alice".to_string(),
                    committed_at: now - Duration::days(3),
                    system_count: 1,
                    commits_behind: 2,
                    systems: vec!["orion-01".to_string()],
                    build_status: Some(BuildStatus::Idle),
                    evaluation_status: None,
                },
            ],
        },
        FlakeTimeline {
            flake_id: 2,
            flake_name: "workstations".to_string(),
            repo_url: "github:acme/workstations".to_string(),
            commits: vec![
                FlakeCommit {
                    id: 4,
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
                    build_status: Some(BuildStatus::Queued),
                    evaluation_status: None,
                },
                FlakeCommit {
                    id: 5,
                    hash: "a2b3c4d5e6f78901234567ab".to_string(),
                    message: "fix: bluetooth audio".to_string(),
                    author: "eve".to_string(),
                    committed_at: now - Duration::days(2),
                    system_count: 2,
                    commits_behind: 1,
                    systems: vec!["ws-009".to_string(), "ws-010".to_string()],
                    build_status: Some(BuildStatus::Queued),
                    evaluation_status: None,
                },
            ],
        },
        FlakeTimeline {
            flake_id: 3,
            flake_name: "edge-nodes".to_string(),
            repo_url: "github:acme/edge".to_string(),
            commits: vec![FlakeCommit {
                id: 6,
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
                build_status: Some(BuildStatus::Queued),
                evaluation_status: None,
            }],
        },
    ]
}
