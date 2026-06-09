//! Dashboard view — fleet-wide overview with health, deployment, and CVE summaries.

use chrono::Duration;
use dioxus::prelude::*;
use std::collections::HashSet;

use crate::api::client::{ApiClientError, fetch_hardening_top_services, fetch_systems};
use crate::api::models::HardeningTopServiceResponse;
use crate::api::models::{
    BuildQueueSummary, BuildStatus, DeploymentStatus, FlakeCommit, FlakeTimeline, SystemSummary,
    SystemsListParams,
};
use crate::components::dashboard::{
    BuildQueuePanel, BuildSummaryPanel, CveSummaryPanel, DeploymentStatusBreakdown,
    FleetHealthBreakdown, RecentDeploymentsList,
};
use crate::components::flake::FlakeTimelineWidget;
use crate::components::icon::{Icon, IconName};
use crate::components::loading::DashboardLoadingSpinner;
use crate::components::notifications::{AlertBanner, AlertSeverity};
use crate::components::widget_grid::{GridWidget, StoredLayout, WidgetGrid};
use crate::dashboard::adapter::{
    deterministic_mock_timestamp, empty_dashboard_summary, load_dashboard_with_fallback,
    load_flake_timelines_with_fallback,
};
use crate::routes::Route;
use crate::state::app_state::AppState;
use crate::state::auth;
use crate::theme;
use crate::views::hardening::render_top_services_compact;

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

/// Static metadata describing an available dashboard widget.
#[derive(Clone, Copy, Debug, PartialEq)]
struct WidgetMeta {
    id: &'static str,
    title: &'static str,
    description: &'static str,
    icon: IconName,
    /// Navigation route for the "View →" header action (None hides the action).
    nav: Option<&'static str>,
    /// Default column span (1-3).
    default_cols: usize,
    /// Default row span (1-3).
    default_rows: usize,
    /// Whether the widget supports height resizing.
    height_resizable: bool,
    /// Whether the widget is admin-only.
    admin_only: bool,
}

/// Registry of every widget that can appear on the dashboard.
///
/// This is the Rust analogue of the design reference's `DASHBOARD_WIDGETS`.
/// Each entry is backed by real API data already loaded by the view.
fn widget_registry() -> &'static [WidgetMeta] {
    &[
        WidgetMeta {
            id: "fleet-health",
            title: "Fleet Health",
            description: "System health rollup across the fleet",
            icon: IconName::Cpu,
            nav: Some("systems"),
            default_cols: 2,
            default_rows: 1,
            height_resizable: false,
            admin_only: false,
        },
        WidgetMeta {
            id: "deployment-status",
            title: "Deployment Status",
            description: "Up-to-date, behind, and never-deployed hosts",
            icon: IconName::Sync,
            nav: Some("systems"),
            default_cols: 1,
            default_rows: 1,
            height_resizable: false,
            admin_only: false,
        },
        WidgetMeta {
            id: "cve-summary",
            title: "CVE Summary",
            description: "Critical / high CVE counts across the fleet",
            icon: IconName::Shield,
            nav: Some("cves"),
            default_cols: 1,
            default_rows: 1,
            height_resizable: false,
            admin_only: false,
        },
        WidgetMeta {
            id: "build-queue",
            title: "Build Queue",
            description: "Active builds and queued jobs",
            icon: IconName::Cpu,
            nav: Some("builds"),
            default_cols: 1,
            default_rows: 1,
            height_resizable: false,
            admin_only: false,
        },
        WidgetMeta {
            id: "build-summary",
            title: "Build Summary",
            description: "Building and queued counts at a glance",
            icon: IconName::Cpu,
            nav: Some("builds"),
            default_cols: 1,
            default_rows: 1,
            height_resizable: false,
            admin_only: false,
        },
        WidgetMeta {
            id: "recent-deployments",
            title: "Recent Deployments",
            description: "Chronological feed of recent deploys",
            icon: IconName::Sync,
            nav: Some("systems"),
            default_cols: 2,
            default_rows: 2,
            height_resizable: true,
            admin_only: false,
        },
        WidgetMeta {
            id: "flake-timeline",
            title: "Flake Git Graph",
            description: "Recent commits across tracked flakes",
            icon: IconName::Git,
            nav: Some("flakes"),
            default_cols: 3,
            default_rows: 3,
            height_resizable: false,
            admin_only: false,
        },
        WidgetMeta {
            id: "quick-actions",
            title: "Quick Actions",
            description: "Common operator shortcuts",
            icon: IconName::Gear,
            nav: None,
            default_cols: 1,
            default_rows: 1,
            height_resizable: false,
            admin_only: false,
        },
        WidgetMeta {
            id: "config-health",
            title: "Pipeline Readiness",
            description: "Pipeline configuration issues needing attention",
            icon: IconName::Gear,
            nav: None,
            default_cols: 2,
            default_rows: 2,
            height_resizable: true,
            admin_only: true,
        },
        WidgetMeta {
            id: "hardening-top-services",
            title: "Top Vulnerable Services",
            description: "Highest-risk hardening services",
            icon: IconName::Shield,
            nav: Some("cves"),
            default_cols: 1,
            default_rows: 2,
            height_resizable: true,
            admin_only: true,
        },
    ]
}

fn widget_meta(id: &str) -> Option<&'static WidgetMeta> {
    widget_registry().iter().find(|w| w.id == id)
}

/// A widget placed on the dashboard, with its current size.
#[derive(Clone, Debug, PartialEq)]
struct WidgetPosition {
    id: &'static str,
    title: &'static str,
    icon: IconName,
    nav: Option<&'static str>,
    cols: usize,
    rows: usize,
    height_resizable: bool,
}

impl WidgetPosition {
    fn from_meta(meta: &WidgetMeta) -> Self {
        Self {
            id: meta.id,
            title: meta.title,
            icon: meta.icon,
            nav: meta.nav,
            cols: meta.default_cols,
            rows: meta.default_rows,
            height_resizable: meta.height_resizable,
        }
    }
}

/// Default ordered dashboard layout (mirrors design reference ordering).
fn default_widget_positions() -> Vec<WidgetPosition> {
    [
        "fleet-health",
        "cve-summary",
        "build-queue",
        "build-summary",
        "deployment-status",
        "recent-deployments",
        "flake-timeline",
        "quick-actions",
        "config-health",
        "hardening-top-services",
    ]
    .iter()
    .filter_map(|id| widget_meta(id).map(WidgetPosition::from_meta))
    .collect()
}

/// Build positions from a stored layout, falling back to defaults.
fn load_widget_positions() -> Vec<WidgetPosition> {
    let Some(stored) = StoredLayout::load() else {
        return default_widget_positions();
    };

    let mut positions: Vec<WidgetPosition> = stored
        .entries
        .iter()
        .filter_map(|(id, cols, rows)| {
            widget_meta(id).map(|meta| {
                let mut pos = WidgetPosition::from_meta(meta);
                pos.cols = (*cols).clamp(1, 3);
                // Only honor a stored row span for widgets the user can actually
                // resize vertically. Fixed-height widgets always use their
                // default row span so registry changes take effect immediately.
                if meta.height_resizable {
                    pos.rows = (*rows).clamp(1, 3);
                }
                pos
            })
        })
        .collect();

    if positions.is_empty() {
        positions = default_widget_positions();
    }

    positions
}

fn persist_widget_positions(positions: &[WidgetPosition]) {
    let stored = StoredLayout {
        version: StoredLayout::VERSION,
        entries: positions
            .iter()
            .map(|pos| (pos.id.to_string(), pos.cols, pos.rows))
            .collect(),
    };
    stored.save();
}

fn should_persist_widget_positions(positions: &[WidgetPosition]) -> bool {
    StoredLayout::exists() || positions != default_widget_positions()
}

/// The main dashboard page.
#[component]
pub fn DashboardView() -> Element {
    let nav = navigator();

    let dashboard = use_signal(empty_dashboard_summary);
    let dashboard_notice = use_signal(|| None::<String>);
    let loading_dashboard = use_signal(|| true);
    let redirect_to_login = use_signal(|| false);

    // Shared config health (admin only).
    let app_state = use_context::<Signal<AppState>>();
    let is_admin_user = auth::is_admin(&app_state.read().auth);
    let config_health = app_state.read().config_health.clone();

    // Flake timelines state
    let flake_timelines = use_signal(Vec::<FlakeTimeline>::new);
    let timelines_notice = use_signal(|| None::<String>);
    let loading_timelines = use_signal(|| true);
    let dashboard_systems = use_signal(Vec::<SystemSummary>::new);
    let hardening_top_services = use_resource(move || async move {
        if is_admin_user {
            Some(fetch_hardening_top_services(Some(10)).await)
        } else {
            None
        }
    });

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
        let mut dashboard_systems = dashboard_systems.clone();
        let mut redirect_to_login = redirect_to_login.clone();

        use_effect(move || {
            spawn(async move {
                match load_dashboard_systems().await {
                    Ok(systems) => dashboard_systems.set(systems),
                    Err(error) if should_redirect_to_login(&error) => {
                        redirect_to_login.set(true);
                    }
                    Err(_) => {
                        // Keep widget-level summaries usable even if host sampling fetch fails.
                    }
                }
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
    let systems = dashboard_systems.read().clone();
    let build_queue = dashboard
        .build_queue
        .clone()
        .unwrap_or_else(|| BuildQueueSummary {
            building_count: 0,
            queued_count: 0,
            items: vec![],
            timestamp: dashboard.timestamp,
        });

    let up_to_date_hosts = hostnames_for_deployment(&systems, &[DeploymentStatus::UpToDate]);
    let behind_hosts = hostnames_for_deployment(&systems, &[DeploymentStatus::Behind]);
    let never_deployed_hosts =
        hostnames_for_deployment(&systems, &[DeploymentStatus::NeverDeployed]);
    let unknown_hosts = hostnames_for_deployment(
        &systems,
        &[
            DeploymentStatus::Unknown,
            DeploymentStatus::NoCommitsAvailable,
        ],
    );

    // Global filter state - shared across all widgets (multi-select)
    let mut dashboard_filter = use_signal(DashboardFilter::default);

    // Widget layout state
    let mut widget_positions = use_signal(load_widget_positions);
    let mut dragging_id: Signal<Option<&'static str>> = use_signal(|| None);
    let mut drop_target_id: Signal<Option<&'static str>> = use_signal(|| None);
    let mut edit_mode = use_signal(|| false);
    let mut picker_open = use_signal(|| false);

    {
        let widget_positions = widget_positions.clone();
        use_effect(move || {
            let positions = widget_positions.read().clone();
            if should_persist_widget_positions(&positions) {
                persist_widget_positions(&positions);
            }
        });
    }

    // Drag start: remember the widget being moved.
    let on_drag_start = move |id: String| {
        if let Some(pos) = widget_positions.read().iter().find(|p| p.id == id) {
            dragging_id.set(Some(pos.id));
        }
    };

    // Drag over: highlight the hovered drop target.
    let on_drag_over = move |id: String| {
        let current = *dragging_id.read();
        if let Some(source_id) = current {
            if source_id != id {
                if let Some(pos) = widget_positions.read().iter().find(|p| p.id == id) {
                    drop_target_id.set(Some(pos.id));
                }
            }
        }
    };

    // Drag leave: clear the drop highlight.
    let on_drag_leave = move |_: ()| {
        drop_target_id.set(None);
    };

    // Drop: reorder the widget list (CSS `row dense` handles packing).
    let on_drop = move |target_id: String| {
        let dragging = *dragging_id.read();
        if let Some(source_id) = dragging {
            if source_id != target_id {
                let mut positions = widget_positions.write();
                let src = positions.iter().position(|p| p.id == source_id);
                let tgt = positions.iter().position(|p| p.id == target_id);
                if let (Some(src), Some(tgt)) = (src, tgt) {
                    let moved = positions.remove(src);
                    let insert_at = tgt.min(positions.len());
                    positions.insert(insert_at, moved);
                }
            }
        }
        dragging_id.set(None);
        drop_target_id.set(None);
    };

    // Set a widget's column span.
    let on_set_cols = move |(id, cols): (String, usize)| {
        let mut positions = widget_positions.write();
        if let Some(pos) = positions.iter_mut().find(|p| p.id == id) {
            pos.cols = cols.clamp(1, 3);
        }
    };

    // Set a widget's row span (height).
    let on_set_rows = move |(id, rows): (String, usize)| {
        let mut positions = widget_positions.write();
        if let Some(pos) = positions.iter_mut().find(|p| p.id == id) {
            pos.rows = rows.clamp(1, 3);
        }
    };

    // Remove a widget from the dashboard.
    let on_remove_widget = move |id: String| {
        let mut positions = widget_positions.write();
        positions.retain(|p| p.id != id);
    };

    // Add a widget (from the library) using its default size.
    let on_add_widget = move |id: String| {
        if widget_positions.read().iter().any(|p| p.id == id) {
            return;
        }
        if let Some(meta) = widget_meta(&id) {
            widget_positions
                .write()
                .push(WidgetPosition::from_meta(meta));
        }
    };

    // Reset to the default layout.
    let on_reset_layout = move |_| {
        StoredLayout::clear();
        widget_positions.set(default_widget_positions());
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
                    flake_filter: filter_display.clone(),
                }
            },
            "deployment-status" => rsx! {
                DeploymentStatusBreakdown {
                    status: dashboard.deployment_status.clone(),
                    flake_filter: filter_display.clone(),
                    up_to_date_hosts: up_to_date_hosts.clone(),
                    behind_hosts: behind_hosts.clone(),
                    never_deployed_hosts: never_deployed_hosts.clone(),
                    unknown_hosts: unknown_hosts.clone(),
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
            "flake-timeline" => rsx! {
                FlakeTimelineWidget {
                    timelines: timelines.clone(),
                    selected_flake_indices: dashboard_filter.read().selected_flake_indices.clone(),
                    on_filter_change: {
                        let timelines_signal = flake_timelines.clone();
                        move |indices: HashSet<usize>| {
                            let current_timelines = timelines_signal.read();
                            let names: Vec<String> = indices
                                .iter()
                                .filter_map(|&idx| current_timelines.get(idx).map(|t| t.flake_name.clone()))
                                .collect();
                            dashboard_filter.set(DashboardFilter {
                                selected_flake_indices: indices,
                                selected_flake_names: names,
                            });
                        }
                    }
                }
            },
            "quick-actions" => rsx! {
                div {
                    class: "grid grid-cols-2 gap-1.5",
                    for (label, route, icon) in quick_action_items() {
                        button {
                            key: "{route}",
                            class: "btn btn-ghost focus-ring",
                            style: "justify-content:flex-start; padding:8px 10px; font-size:12px; min-width:0;",
                            onclick: move |_| {
                                if let Some(target) = route_for_nav(route) {
                                    nav.push(target);
                                }
                            },
                            Icon { name: icon, size: 13 }
                            span {
                                style: "overflow:hidden; text-overflow:ellipsis; white-space:nowrap;",
                                "{label}"
                            }
                        }
                    }
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
                                class: "rounded-xl border border-amber-300/35 bg-gradient-to-br from-amber-950/75 via-amber-900/30 to-yellow-950/10 p-4 h-full min-h-0 flex flex-col overflow-hidden shadow-[inset_0_1px_0_rgba(252,211,77,0.08)]",
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
                                div {
                                    class: "mt-3 flex-1 min-h-0 overflow-y-auto space-y-2 pr-1",
                                    style: "overflow-y: auto; overscroll-behavior: contain;",
                                    "data-testid": "pipeline-readiness-scroll",
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
            }
            "hardening-top-services" => {
                if !is_admin_user {
                    return rsx! {};
                }
                match &*hardening_top_services.read_unchecked() {
                    Some(Some(Ok(rows))) => render_top_services_compact(rows),
                    Some(Some(Err(_))) => rsx! {
                        p { class: "text-xs {theme::text::SECONDARY}", "Unable to load hardening service risk data." }
                    },
                    _ => rsx! {
                        p { class: "text-xs {theme::text::SECONDARY}", "Loading hardening service risk..." }
                    },
                }
            }
            _ => rsx! { div { "Unknown widget" } },
        }
    };

    let widget_count = widget_positions.read().len();
    let is_edit = *edit_mode.read();

    rsx! {
        div {
            style: "display:flex; flex-direction:column; gap:16px;",
            "data-testid": "dashboard",

            // Page header (matches design reference page-head)
            div {
                class: "page-head",
                div {
                    h1 { class: "page-title", "Dashboard" }
                    p {
                        class: "page-subtitle",
                        "{widget_count} widgets · drag to rearrange in edit mode"
                    }
                }
                div {
                    style: "display:flex; gap:8px;",
                    if is_edit {
                        button {
                            class: "btn btn-ghost focus-ring",
                            title: "Reset to default layout",
                            onclick: on_reset_layout,
                            Icon { name: IconName::Sync, size: 14 }
                            "Reset"
                        }
                        button {
                            class: "btn btn-ghost focus-ring",
                            onclick: move |_| picker_open.set(true),
                            Icon { name: IconName::Plus, size: 14 }
                            "Add widget"
                        }
                    }
                    button {
                        class: if is_edit { "btn btn-primary focus-ring" } else { "btn btn-ghost focus-ring" },
                        onclick: move |_| {
                            let next = !*edit_mode.read();
                            edit_mode.set(next);
                        },
                        Icon { name: if is_edit { IconName::Check } else { IconName::Gear }, size: 14 }
                        if is_edit { "Done" } else { "Customize" }
                    }
                }
            }

            if is_edit {
                div {
                    class: "sd-callout sd-callout-info",
                    Icon { name: IconName::Gear, size: 13 }
                    div {
                        style: "font-size:12px;",
                        strong { "Edit mode." }
                        " Drag widgets to reorder, set "
                        strong { "Width" }
                        " and (on list widgets) "
                        strong { "Height" }
                        ", or remove with the × button. Click \"Add widget\" to browse the widget library."
                    }
                }
            }

            // Loading + notice banners
            if *loading_dashboard.read() {
                div {
                    "data-testid": "dashboard-loading-spinner",
                    DashboardLoadingSpinner {
                        label: "Loading dashboard data...".to_string(),
                        size: 20
                    }
                }
            }
            if *loading_timelines.read() {
                DashboardLoadingSpinner {
                    label: "Loading flake timelines...".to_string(),
                    size: 20
                }
            }
            if let Some(message) = dashboard_notice.read().clone() {
                p {
                    class: "text-xs px-3 py-2 rounded-lg border text-amber-100 cf-chip-warning",
                    "{message}"
                }
            }
            if let Some(message) = timelines_notice.read().clone() {
                p {
                    class: "text-xs px-3 py-2 rounded-lg border text-amber-100 cf-chip-warning",
                    "{message}"
                }
            }

            // Widget grid (3-column dense, design-reference parity)
            WidgetGrid {
                for pos in widget_positions.read().iter() {
                    {
                        let pos = pos.clone();
                        let action_label = pos.nav.map(|_| "View →".to_string());
                        rsx! {
                            GridWidget {
                                key: "{pos.id}",
                                id: pos.id.to_string(),
                                title: pos.title.to_string(),
                                icon: pos.icon,
                                cols: pos.cols,
                                rows: pos.rows,
                                height_resizable: pos.height_resizable,
                                action_label,
                                edit_mode: is_edit,
                                is_dragging: dragging_id.read().map_or(false, |d| d == pos.id),
                                is_drop_target: drop_target_id.read().map_or(false, |d| d == pos.id),
                                on_action: {
                                    let route = pos.nav;
                                    move |_| {
                                        if let Some(target) = route.and_then(route_for_nav) {
                                            nav.push(target);
                                        }
                                    }
                                },
                                on_drag_start: on_drag_start,
                                on_drag_over: on_drag_over,
                                on_drag_leave: on_drag_leave,
                                on_drop: on_drop,
                                on_set_cols: on_set_cols,
                                on_set_rows: on_set_rows,
                                on_remove: on_remove_widget,
                                children: render_widget_content(pos.id)
                            }
                        }
                    }
                }
                if widget_count == 0 {
                    div {
                        class: "empty",
                        style: "grid-column: 1 / -1;",
                        h3 { "Empty dashboard" }
                        div {
                            "Click "
                            strong { "Customize" }
                            " then "
                            strong { "Add widget" }
                            " to get started."
                        }
                    }
                }
            }
        }

        if *picker_open.read() {
            WidgetPicker {
                added_ids: widget_positions.read().iter().map(|p| p.id.to_string()).collect::<HashSet<String>>(),
                is_admin: is_admin_user,
                on_add: on_add_widget,
                on_close: move |_| picker_open.set(false),
            }
        }
    }
}

/// Widget library modal — browse and add available widgets (design parity).
#[component]
fn WidgetPicker(
    added_ids: HashSet<String>,
    is_admin: bool,
    on_add: EventHandler<String>,
    on_close: EventHandler<()>,
) -> Element {
    let all: Vec<&'static WidgetMeta> = widget_registry()
        .iter()
        .filter(|w| !w.admin_only || is_admin)
        .collect();
    let total = all.len();
    let added_count = all.iter().filter(|w| added_ids.contains(w.id)).count();

    rsx! {
        div {
            class: "modal-backdrop",
            onclick: move |_| on_close.call(()),
            div {
                class: "modal",
                style: "width: min(640px, 96vw); max-height: 88vh; display:flex; flex-direction:column;",
                onclick: move |evt| evt.stop_propagation(),
                div {
                    class: "modal-head",
                    h2 { "Widget library" }
                    p { "Add widgets from the library to your dashboard. {added_count} of {total} added." }
                }
                div {
                    style: "overflow-y:auto; padding:8px;",
                    for w in all.iter() {
                        {
                            let id = w.id.to_string();
                            let added = added_ids.contains(w.id);
                            rsx! {
                                button {
                                    key: "{w.id}",
                                    class: "focus-ring widget-lib-item",
                                    disabled: added,
                                    onclick: {
                                        let id = id.clone();
                                        move |_| {
                                            if !added {
                                                on_add.call(id.clone());
                                            }
                                        }
                                    },
                                    span {
                                        class: "widget-lib-icon",
                                        Icon { name: w.icon, size: 15 }
                                    }
                                    span {
                                        style: "min-width:0; flex:1;",
                                        span { class: "widget-lib-title", "{w.title}" }
                                        span { class: "widget-lib-desc", "{w.description}" }
                                    }
                                    if added {
                                        span {
                                            class: "chip chip-healthy",
                                            style: "font-size:10px; flex-shrink:0;",
                                            Icon { name: IconName::Check, size: 9 }
                                            " Added"
                                        }
                                    } else {
                                        span {
                                            class: "widget-lib-add",
                                            Icon { name: IconName::Plus, size: 13 }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div {
                    class: "modal-foot",
                    button {
                        class: "btn btn-ghost focus-ring",
                        onclick: move |_| on_close.call(()),
                        "Done"
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
                    system_paths: vec![],
                    build_status: Some(BuildStatus::Building),
                    evaluation_status: None,
                    evaluation_error_message: None,
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
                    system_paths: vec![],
                    build_status: Some(BuildStatus::Queued),
                    evaluation_status: None,
                    evaluation_error_message: None,
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
                    system_paths: vec![],
                    build_status: Some(BuildStatus::Idle),
                    evaluation_status: None,
                    evaluation_error_message: None,
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
                    system_paths: vec![],
                    build_status: Some(BuildStatus::Queued),
                    evaluation_status: None,
                    evaluation_error_message: None,
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
                    system_paths: vec![],
                    build_status: Some(BuildStatus::Queued),
                    evaluation_status: None,
                    evaluation_error_message: None,
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
                system_paths: vec![],
                build_status: Some(BuildStatus::Queued),
                evaluation_status: None,
                evaluation_error_message: None,
            }],
        },
    ]
}

/// Map a widget `nav` slug to a concrete route.
fn route_for_nav(route: &str) -> Option<Route> {
    Some(match route {
        "systems" => Route::SystemsView {},
        "flakes" => Route::FlakesView {},
        "builds" => Route::BuildsView {},
        "evals" => Route::EvaluationsView {},
        "cves" => Route::CvesView {},
        "caches" => Route::CachesView {},
        "environments" => Route::EnvironmentsView {},
        _ => return None,
    })
}

/// Quick-action shortcuts shown in the Quick Actions widget.
fn quick_action_items() -> Vec<(&'static str, &'static str, IconName)> {
    vec![
        ("Systems", "systems", IconName::Cpu),
        ("Builds", "builds", IconName::Cpu),
        ("Evaluations", "evals", IconName::Gear),
        ("Flakes", "flakes", IconName::Git),
        ("CVEs", "cves", IconName::Shield),
        ("Caches", "caches", IconName::Download),
    ]
}

fn should_redirect_to_login(error: &ApiClientError) -> bool {
    matches!(
        error,
        ApiClientError::Status { code, .. } if *code == 401 || *code == 403
    )
}

async fn load_dashboard_systems() -> Result<Vec<SystemSummary>, ApiClientError> {
    let mut page = 1;
    let per_page = 200;
    let mut systems = Vec::new();

    loop {
        let response = fetch_systems(&SystemsListParams {
            page: Some(page),
            per_page: Some(per_page),
            search: None,
            health_status: None,
            deployment_status: None,
            environment: None,
            sort_by: None,
            sort_order: None,
        })
        .await?;

        let total_pages = response.total_pages();
        systems.extend(response.items);

        if page >= total_pages || total_pages == 0 {
            break;
        }
        page += 1;
    }

    Ok(systems)
}

fn hostnames_for_deployment(
    systems: &[SystemSummary],
    statuses: &[DeploymentStatus],
) -> Vec<String> {
    systems
        .iter()
        .filter(|system| statuses.contains(&system.deployment_status))
        .map(|system| system.hostname.clone())
        .take(24)
        .collect()
}
