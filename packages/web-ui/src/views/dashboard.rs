//! Dashboard view — fleet-wide overview with health, deployment, and CVE summaries.

use dioxus::prelude::*;
use std::collections::HashSet;

use crate::api::client::{ApiClientError, fetch_hardening_top_services, fetch_systems};
use crate::api::models::HardeningTopServiceResponse;
use crate::api::models::{
    BuildQueueSummary, DeploymentStatus, FlakeTimeline, SystemSummary, SystemsListParams,
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
    empty_dashboard_summary, load_dashboard_with_fallback, load_flake_timelines_with_fallback,
};
use crate::routes::Route;
use crate::state::app_state::AppState;
use crate::state::auth;
use crate::state::navigation_focus::NavigationFocus;
use crate::theme;
use crate::views::hardening::render_top_services_compact;
use crate::views::poam_api::{self, PoamApiError, PoamDashboardSummary, PoamSummary};

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
    /// Library category (used by the Widget Library filter).
    category: &'static str,
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

/// Library category display order (mirrors design reference CATEGORY_ORDER).
const CATEGORY_ORDER: &[&str] = &[
    "Fleet",
    "Pipeline",
    "Security",
    "Activity",
    "Infrastructure",
    "Actions",
];

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
            category: "Fleet",
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
            category: "Fleet",
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
            category: "Security",
            nav: Some("cves"),
            default_cols: 1,
            default_rows: 1,
            height_resizable: false,
            admin_only: false,
        },
        WidgetMeta {
            id: "poam-summary",
            title: "POA&M Summary",
            description: "Open remediation plans, overdue and awaiting verification",
            icon: IconName::History,
            category: "Security",
            nav: Some("compliance"),
            default_cols: 1,
            default_rows: 1,
            height_resizable: false,
            admin_only: false,
        },
        WidgetMeta {
            id: "poam-watchlist",
            title: "POA&M Watchlist",
            description: "Overdue and awaiting-verification remediation plans needing attention",
            icon: IconName::History,
            category: "Security",
            nav: Some("compliance"),
            default_cols: 2,
            default_rows: 1,
            height_resizable: true,
            admin_only: false,
        },
        WidgetMeta {
            id: "build-queue",
            title: "Build Queue",
            description: "Active builds and queued jobs",
            icon: IconName::Cpu,
            category: "Pipeline",
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
            category: "Pipeline",
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
            category: "Activity",
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
            category: "Activity",
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
            category: "Actions",
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
            category: "Pipeline",
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
            category: "Security",
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

fn is_widget_visible_for_user(id: &str, is_admin: bool) -> bool {
    widget_meta(id)
        .map(|meta| !meta.admin_only || is_admin)
        .unwrap_or(false)
}

fn can_view_widget_route_for_user(route: &str, is_admin: bool) -> bool {
    match route {
        // App shell guards CVEs for admins only.
        "cves" => is_admin,
        _ => true,
    }
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
        "poam-summary",
        "cve-summary",
        "build-queue",
        "build-summary",
        "deployment-status",
        "recent-deployments",
        "flake-timeline",
        "quick-actions",
        "config-health",
        "hardening-top-services",
        "poam-watchlist",
    ]
    .iter()
    .filter_map(|id| widget_meta(id).map(WidgetPosition::from_meta))
    .collect()
}

/// Migrates a legacy stored layout to the current schema.
///
/// Legacy recognized entries retain their order, width, and supported height.
/// Duplicate and unknown entries are removed. Version 3 layouts are returned
/// unchanged so a widget that the user removed is not added again.
fn migrate_stored_layout(stored: StoredLayout) -> StoredLayout {
    if stored.version >= StoredLayout::VERSION {
        return stored;
    }

    let mut seen = HashSet::new();
    let mut entries = stored
        .entries
        .into_iter()
        .filter_map(|(id, cols, rows)| {
            let meta = widget_meta(&id)?;
            if !seen.insert(id.clone()) {
                return None;
            }
            Some((
                id,
                cols.clamp(1, 3),
                if meta.height_resizable {
                    rows.clamp(1, 3)
                } else {
                    meta.default_rows
                },
            ))
        })
        .collect::<Vec<_>>();

    // A corrupt or obsolete legacy layout must retain the established
    // full-dashboard fallback instead of becoming a two-widget layout.
    if entries.is_empty() {
        entries = default_widget_positions()
            .into_iter()
            .map(|position| (position.id.to_string(), position.cols, position.rows))
            .collect();
        return StoredLayout {
            version: StoredLayout::VERSION,
            entries,
        };
    }

    if !seen.contains("poam-summary") {
        let summary = ("poam-summary".to_string(), 1, 1);
        let insert_at = entries
            .iter()
            .position(|(id, _, _)| id == "fleet-health")
            .map_or(0, |index| index + 1);
        entries.insert(insert_at, summary);
        seen.insert("poam-summary".to_string());
    }
    if !seen.contains("poam-watchlist") {
        entries.push(("poam-watchlist".to_string(), 2, 1));
    }

    StoredLayout {
        version: StoredLayout::VERSION,
        entries,
    }
}

/// Builds positions from a stored layout, falling back to fresh defaults.
fn load_widget_positions() -> Vec<WidgetPosition> {
    let Some(stored) = StoredLayout::load() else {
        return default_widget_positions();
    };
    let stored = migrate_stored_layout(stored);

    stored
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
        .collect()
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

fn move_widget_position(positions: &mut [WidgetPosition], id: &str, direction: isize) -> bool {
    let Some(current) = positions.iter().position(|position| position.id == id) else {
        return false;
    };
    let target = current
        .saturating_add_signed(direction)
        .min(positions.len().saturating_sub(1));
    if current == target {
        return false;
    }
    positions.swap(current, target);
    true
}

fn should_persist_widget_positions(positions: &[WidgetPosition]) -> bool {
    if StoredLayout::load().is_some_and(|stored| stored.version > StoredLayout::VERSION) {
        return false;
    }
    StoredLayout::exists() || positions != default_widget_positions()
}

#[derive(Clone, Debug, PartialEq)]
enum PoamDashboardState<T> {
    Loading,
    Loaded(T),
    Empty,
    Unauthorized,
    Error,
}

fn poam_summary_state(
    result: Option<&Result<PoamDashboardSummary, PoamApiError>>,
) -> PoamDashboardState<PoamDashboardSummary> {
    match result {
        None => PoamDashboardState::Loading,
        Some(Ok(summary)) if summary.total == 0 => PoamDashboardState::Empty,
        Some(Ok(summary)) => PoamDashboardState::Loaded(summary.clone()),
        Some(Err(error)) if error.is_unauthorized() => PoamDashboardState::Unauthorized,
        Some(Err(_)) => PoamDashboardState::Error,
    }
}

fn poam_watchlist_state(
    result: Option<&Result<poam_api::Page<PoamSummary>, PoamApiError>>,
) -> PoamDashboardState<Vec<PoamSummary>> {
    match result {
        None => PoamDashboardState::Loading,
        Some(Ok(page)) if page.items.is_empty() => PoamDashboardState::Empty,
        Some(Ok(page)) => PoamDashboardState::Loaded(page.items.clone()),
        Some(Err(error)) if error.is_unauthorized() => PoamDashboardState::Unauthorized,
        Some(Err(_)) => PoamDashboardState::Error,
    }
}

fn poam_watchlist_row_count(rows: usize) -> usize {
    match rows {
        1 => 4,
        2 => 8,
        _ => 13,
    }
}

fn poam_attention_label(poam: &PoamSummary) -> &'static str {
    if poam.overdue {
        "Overdue"
    } else {
        "Awaiting verification"
    }
}

fn compliance_route(poam: Option<uuid::Uuid>) -> Route {
    Route::ComplianceView {
        bundle: String::new(),
        version: String::new(),
        system: String::new(),
        policy: String::new(),
        poam: poam.map_or_else(String::new, |id| id.to_string()),
        view: String::new(),
    }
}

/// The main dashboard page.
#[component]
pub fn DashboardView() -> Element {
    let nav = navigator();
    let mut navigation_focus = use_context::<Signal<Option<NavigationFocus>>>();

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
    // These view-owned resources issue exactly one batched request per endpoint.
    // Widgets consume the shared results and never initiate their own requests.
    let mut poam_summary_resource = use_resource(move || {
        // Reading the generation makes Dioxus cancel and replace account-scoped
        // requests whenever authentication changes.
        let _ = app_state.read().auth_generation;
        async { poam_api::dashboard_summary().await }
    });
    let mut poam_watchlist_resource = use_resource(move || {
        let _ = app_state.read().auth_generation;
        async { poam_api::dashboard_watchlist().await }
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

    let poam_authentication_failed = poam_summary_resource
        .read_unchecked()
        .as_ref()
        .is_some_and(|result| result.as_ref().is_err_and(PoamApiError::is_unauthenticated))
        || poam_watchlist_resource
            .read_unchecked()
            .as_ref()
            .is_some_and(|result| result.as_ref().is_err_and(PoamApiError::is_unauthenticated));
    if poam_authentication_failed {
        nav.push(Route::LoginView {});
        return rsx! {
            div {
                class: "min-h-screen flex items-center justify-center {theme::surface::PAGE_BG}",
                p { class: "{theme::text::SECONDARY}", "Redirecting to login..." }
            }
        };
    }

    let dashboard = dashboard.read().clone();
    let timelines = flake_timelines.read().clone();
    let systems = dashboard_systems.read().clone();
    let poam_summary = poam_summary_state(poam_summary_resource.read_unchecked().as_ref());
    let poam_watchlist = poam_watchlist_state(poam_watchlist_resource.read_unchecked().as_ref());
    let build_queue = dashboard
        .build_queue
        .clone()
        .unwrap_or_else(|| BuildQueueSummary {
            building_count: 0,
            queued_count: 0,
            failed_24h_count: 0,
            active_workers: 0,
            total_workers: 0,
            used_slots: 0,
            total_slots: 0,
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

    let on_move_widget = move |(id, direction): (String, isize)| {
        move_widget_position(&mut widget_positions.write(), &id, direction);
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

    // Only render widgets the current user can actually see.
    let visible_positions: Vec<WidgetPosition> = widget_positions
        .read()
        .iter()
        .filter(|pos| is_widget_visible_for_user(pos.id, is_admin_user))
        .cloned()
        .collect();

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
    let render_widget_content = |id: &str, rows: usize| -> Element {
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
            "poam-summary" => match &poam_summary {
                PoamDashboardState::Loading => rsx! {
                    p { role: "status", class: "text-xs {theme::text::SECONDARY}", "Loading POA&M summary..." }
                },
                PoamDashboardState::Empty => rsx! {
                    p { role: "status", class: "text-xs {theme::text::SECONDARY}", "No POA&M records are visible." }
                },
                PoamDashboardState::Unauthorized => rsx! {
                    p { class: "text-xs {theme::text::SECONDARY}", "You are not authorized to view POA&M summary data." }
                },
                PoamDashboardState::Error => rsx! {
                    div { role: "alert", class: "flex items-center gap-2",
                        p { class: "text-xs text-red-300", "Unable to load POA&M summary data." }
                        button { class: "btn btn-ghost focus-ring text-xs", onclick: move |_| poam_summary_resource.restart(), "Retry" }
                    }
                },
                PoamDashboardState::Loaded(summary) => rsx! {
                    div { style: "display:flex;flex-direction:column;gap:10px;",
                        div { style: "display:flex;justify-content:space-between;align-items:baseline;",
                            span {
                                style: if summary.active > 0 { "font-size:32px;font-weight:700;color:#60a5fa;line-height:1;font-variant-numeric:tabular-nums;" } else { "font-size:32px;font-weight:700;color:#34d399;line-height:1;font-variant-numeric:tabular-nums;" },
                                "{summary.active}"
                            }
                            span { style: "font-size:12px;color:var(--cf-text-muted);", "open remediation plans" }
                        }
                        if summary.overdue > 0 {
                            div {
                                style: "padding:8px 10px;border-radius:6px;background:rgba(248,113,113,0.08);border:1px solid rgba(248,113,113,0.25);font-size:11px;color:#fca5a5;",
                                "{summary.overdue} overdue"
                            }
                        }
                        div { style: "display:grid;grid-template-columns:1fr 1fr;gap:6px;font-size:11px;",
                            div { class: "dash-w-mini", span { "Awaiting verification" } strong { style: if summary.awaiting_verification > 0 { "color:#a78bfa;" } else { "" }, "{summary.awaiting_verification}" } }
                            div { class: "dash-w-mini", span { "Completed" } strong { style: "color:#34d399;", "{summary.completed}" } }
                        }
                    }
                },
            },
            "poam-watchlist" => match &poam_watchlist {
                PoamDashboardState::Loading => rsx! {
                    p { role: "status", class: "text-xs {theme::text::SECONDARY}", "Loading POA&M watchlist..." }
                },
                PoamDashboardState::Empty => rsx! {
                    p { role: "status", class: "text-xs {theme::text::SECONDARY}", "Nothing is overdue or awaiting verification." }
                },
                PoamDashboardState::Unauthorized => rsx! {
                    p { class: "text-xs {theme::text::SECONDARY}", "You are not authorized to view the POA&M watchlist." }
                },
                PoamDashboardState::Error => rsx! {
                    div { role: "alert", class: "flex items-center gap-2",
                        p { class: "text-xs text-red-300", "Unable to load the POA&M watchlist." }
                        button { class: "btn btn-ghost focus-ring text-xs", onclick: move |_| poam_watchlist_resource.restart(), "Retry" }
                    }
                },
                PoamDashboardState::Loaded(items) => rsx! {
                    div { style: "display:flex;flex-direction:column;gap:6px;",
                        for poam in items.iter().take(poam_watchlist_row_count(rows)) {
                            {
                                let poam_id = poam.id;
                                let attention = poam_attention_label(poam);
                                let attention_style = if poam.overdue {
                                    "font-size:9.5px;flex-shrink:0;color:#f87171;background:rgba(248,113,113,0.14);"
                                } else {
                                    "font-size:9.5px;flex-shrink:0;color:#a78bfa;background:rgba(167,139,250,0.16);"
                                };
                                rsx! {
                                    button {
                                        key: "{poam.id}",
                                        class: "focus-ring",
                                        style: "width:100%;display:flex;align-items:center;gap:10px;padding:7px 10px;background:var(--cf-subtle-bg);border:0;border-radius:6px;color:inherit;font:inherit;text-align:left;cursor:pointer;",
                                        title: "Open {poam.human_id}: {poam.title}",
                                        onclick: move |_| {
                                            nav.push(compliance_route(Some(poam_id)));
                                        },
                                        span { class: "mono hidden sm:inline", style: "font-weight:700;font-size:11px;color:var(--cf-brand-purple);flex-shrink:0;", "{poam.human_id}" }
                                        span { style: "flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;", "{poam.title}" }
                                        span { class: "chip", style: attention_style, "{attention}" }
                                        span { class: "hidden md:inline", style: "font-size:10px;color:var(--cf-text-muted);flex-shrink:0;", "{poam.owner}" }
                                        if let Some(target_date) = poam.target_date {
                                            span { class: "hidden lg:inline", style: "font-size:10px;color:var(--cf-text-muted);flex-shrink:0;", "{target_date}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
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
                    },
                    on_open_build: move |focus: NavigationFocus| {
                        navigation_focus.set(Some(focus));
                    },
                    on_open_evaluation: move |focus: NavigationFocus| {
                        navigation_focus.set(Some(focus));
                    },
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

    let widget_count = visible_positions.len();
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
                for pos in visible_positions.iter() {
                    {
                        let pos = pos.clone();
                        let action_label = pos
                            .nav
                            .filter(|route| can_view_widget_route_for_user(route, is_admin_user))
                            .map(|_| if pos.id.starts_with("poam-") { "Review →" } else { "View →" }.to_string());
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
                                    let route = pos.nav.filter(|route| can_view_widget_route_for_user(route, is_admin_user));
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
                                on_move: on_move_widget,
                                on_set_cols: on_set_cols,
                                on_set_rows: on_set_rows,
                                on_remove: on_remove_widget,
                                children: render_widget_content(pos.id, pos.rows)
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
                added_ids: visible_positions.iter().map(|p| p.id.to_string()).collect::<HashSet<String>>(),
                is_admin: is_admin_user,
                on_add: on_add_widget,
                on_close: move |_| picker_open.set(false),
            }
        }
    }
}

/// Human-readable width label for a column span.
fn width_label(cols: usize) -> &'static str {
    match cols {
        1 => "⅓ width",
        2 => "⅔ width",
        _ => "Full width",
    }
}

/// Widget library modal — two-pane browse/add experience (design parity).
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

    let mut query = use_signal(String::new);
    let mut category = use_signal(|| "All".to_string());
    let mut selected_id = use_signal(|| None::<&'static str>);

    // Categories present among available widgets, in canonical order.
    let mut cats: Vec<String> = vec!["All".to_string()];
    cats.extend(
        CATEGORY_ORDER
            .iter()
            .filter(|c| all.iter().any(|w| &w.category == *c))
            .map(|c| c.to_string()),
    );

    let active_cat = category.read().clone();
    let q = query.read().trim().to_lowercase();
    let filtered: Vec<&'static WidgetMeta> = all
        .iter()
        .copied()
        .filter(|w| active_cat == "All" || w.category == active_cat)
        .filter(|w| {
            q.is_empty()
                || format!("{} {}", w.title, w.description)
                    .to_lowercase()
                    .contains(&q)
        })
        .collect();

    // Selected widget: explicit selection, else first filtered, else first.
    let sel: Option<&'static WidgetMeta> = selected_id
        .read()
        .and_then(|id| all.iter().copied().find(|w| w.id == id))
        .or_else(|| filtered.first().copied())
        .or_else(|| all.first().copied());

    rsx! {
        div {
            class: "modal-backdrop",
            onclick: move |_| on_close.call(()),
            div {
                class: "modal",
                style: "width: min(820px, 96vw); max-height: 88vh; display:flex; flex-direction:column;",
                onclick: move |evt| evt.stop_propagation(),
                div {
                    class: "modal-head",
                    h2 {
                        Icon { name: IconName::Plus, size: 14 }
                        " Widget library"
                    }
                    p { "Add widgets from the library to your dashboard. {added_count} of {total} added." }
                }

                div {
                    style: "display:flex; min-height:0; flex:1;",

                    // Catalog pane
                    div {
                        style: "flex:1 1 0; min-width:0; display:flex; flex-direction:column; border-right:1px solid var(--cf-divider);",

                        // Search + category filter
                        div {
                            style: "padding:12px 16px; display:flex; flex-direction:column; gap:10px; border-bottom:1px solid var(--cf-divider);",
                            div {
                                class: "filter-search",
                                style: "width:100%;",
                                Icon { name: IconName::Search, size: 16 }
                                input {
                                    class: "input focus-ring",
                                    placeholder: "Search widgets…",
                                    value: "{query}",
                                    oninput: move |evt| query.set(evt.value()),
                                }
                            }
                            div {
                                style: "display:flex; gap:6px; flex-wrap:wrap;",
                                for c in cats.iter() {
                                    {
                                        let c = c.clone();
                                        let is_active = active_cat == c;
                                        rsx! {
                                            button {
                                                key: "{c}",
                                                class: if is_active { "chip chip-info focus-ring" } else { "chip focus-ring" },
                                                style: if is_active {
                                                    "cursor:pointer;".to_string()
                                                } else {
                                                    "cursor:pointer; border:1px solid var(--cf-divider); background:transparent; color:var(--cf-text-secondary);".to_string()
                                                },
                                                onclick: {
                                                    let c = c.clone();
                                                    move |_| category.set(c.clone())
                                                },
                                                "{c}"
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Widget list
                        div {
                            style: "overflow-y:auto; padding:8px;",
                            if filtered.is_empty() {
                                div {
                                    class: "empty",
                                    style: "margin:16px;",
                                    h3 { "No widgets match" }
                                    div { "Try a different search or category." }
                                }
                            } else {
                                for w in filtered.iter() {
                                    {
                                        let meta = *w;
                                        let added = added_ids.contains(meta.id);
                                        let is_sel = sel.map(|s| s.id) == Some(meta.id);
                                        rsx! {
                                            button {
                                                key: "{meta.id}",
                                                class: "focus-ring widget-lib-item",
                                                style: if is_sel {
                                                    "outline:1px solid var(--cf-brand-purple); background: color-mix(in oklab, var(--cf-brand-purple) 8%, transparent);"
                                                } else {
                                                    ""
                                                },
                                                onclick: move |_| selected_id.set(Some(meta.id)),
                                                span {
                                                    class: "widget-lib-icon",
                                                    Icon { name: meta.icon, size: 15 }
                                                }
                                                span {
                                                    style: "min-width:0; flex:1;",
                                                    span { class: "widget-lib-title", "{meta.title}" }
                                                    span { class: "widget-lib-desc", "{meta.description}" }
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
                        }
                    }

                    // Detail pane
                    if let Some(meta) = sel {
                        {
                            let sel_added = added_ids.contains(meta.id);
                            let sel_id = meta.id.to_string();
                            rsx! {
                                div {
                                    style: "flex:0 0 300px; max-width:300px; padding:18px; display:flex; flex-direction:column; gap:14px; overflow-y:auto;",
                                    div {
                                        style: "display:flex; align-items:center; gap:10px;",
                                        span {
                                            class: "widget-lib-icon",
                                            style: "width:38px; height:38px;",
                                            Icon { name: meta.icon, size: 18 }
                                        }
                                        div {
                                            style: "min-width:0;",
                                            div { style: "font-size:15px; font-weight:650;", "{meta.title}" }
                                            div {
                                                style: "font-size:11px; color:var(--cf-brand-purple); font-weight:600;",
                                                "{meta.category}"
                                            }
                                        }
                                    }
                                    p {
                                        style: "margin:0; font-size:13px; color:var(--cf-text-secondary); line-height:1.55;",
                                        "{meta.description}"
                                    }
                                    div {
                                        style: "display:flex; flex-direction:column; gap:8px;",
                                        div {
                                            style: "font-size:10px; font-weight:700; letter-spacing:0.07em; text-transform:uppercase; color:var(--cf-text-muted);",
                                            "Defaults"
                                        }
                                        div {
                                            style: "display:flex; gap:6px; flex-wrap:wrap;",
                                            span {
                                                class: "chip chip-unknown",
                                                style: "font-size:11px;",
                                                Icon { name: IconName::Grid, size: 10 }
                                                " {width_label(meta.default_cols)}"
                                            }
                                            if meta.height_resizable {
                                                span {
                                                    class: "chip chip-unknown",
                                                    style: "font-size:11px;",
                                                    Icon { name: IconName::Rows, size: 10 }
                                                    " Adjustable height"
                                                }
                                            } else {
                                                span {
                                                    class: "chip chip-unknown",
                                                    style: "font-size:11px;",
                                                    "Fixed height"
                                                }
                                            }
                                        }
                                    }
                                    div {
                                        style: "margin-top:auto;",
                                        if sel_added {
                                            button {
                                                class: "btn btn-ghost focus-ring",
                                                disabled: true,
                                                style: "width:100%; justify-content:center; opacity:0.7;",
                                                Icon { name: IconName::Check, size: 13 }
                                                " Already on dashboard"
                                            }
                                        } else {
                                            button {
                                                class: "btn btn-primary focus-ring",
                                                style: "width:100%; justify-content:center;",
                                                onclick: move |_| on_add.call(sel_id.clone()),
                                                Icon { name: IconName::Plus, size: 13 }
                                                " Add to dashboard"
                                            }
                                        }
                                        div {
                                            class: "help",
                                            style: "margin-top:8px; text-align:center;",
                                            "Reorder & resize it after adding."
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
        "compliance" => compliance_route(None),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poam_widgets_are_available_to_all_roles_and_in_fresh_defaults() {
        let summary = widget_meta("poam-summary").unwrap();
        assert_eq!(summary.category, "Security");
        assert_eq!(summary.default_cols, 1);
        assert!(!summary.height_resizable);
        assert!(!summary.admin_only);

        let watchlist = widget_meta("poam-watchlist").unwrap();
        assert_eq!(watchlist.category, "Security");
        assert_eq!(watchlist.default_cols, 2);
        assert!(watchlist.height_resizable);
        assert!(!watchlist.admin_only);

        let ids = default_widget_positions()
            .into_iter()
            .map(|position| position.id)
            .collect::<Vec<_>>();
        assert_eq!(ids.iter().filter(|id| **id == "poam-summary").count(), 1);
        assert_eq!(ids.iter().filter(|id| **id == "poam-watchlist").count(), 1);
        assert_eq!(ids[0], "fleet-health");
        assert_eq!(ids[1], "poam-summary");
    }

    #[test]
    fn keyboard_widget_move_is_bounded_and_preserves_widget_identity() {
        let mut positions = default_widget_positions();
        let first = positions[0].id;
        let second = positions[1].id;

        assert!(!move_widget_position(&mut positions, first, -1));
        assert!(move_widget_position(&mut positions, first, 1));
        assert_eq!(positions[0].id, second);
        assert_eq!(positions[1].id, first);
        assert!(!move_widget_position(&mut positions, "missing", 1));
    }

    #[test]
    fn legacy_layout_migration_preserves_recognized_entries_and_adds_poam_once() {
        let migrated = migrate_stored_layout(StoredLayout {
            version: 2,
            entries: vec![
                ("build-queue".into(), 3, 3),
                ("recent-deployments".into(), 2, 3),
                ("build-queue".into(), 1, 1),
                ("removed-widget".into(), 2, 2),
                ("fleet-health".into(), 2, 3),
            ],
        });

        assert_eq!(migrated.version, StoredLayout::VERSION);
        assert_eq!(
            migrated.entries,
            vec![
                ("build-queue".into(), 3, 1),
                ("recent-deployments".into(), 2, 3),
                ("fleet-health".into(), 2, 1),
                ("poam-summary".into(), 1, 1),
                ("poam-watchlist".into(), 2, 1),
            ]
        );
        assert_eq!(migrate_stored_layout(migrated.clone()), migrated);
    }

    #[test]
    fn missing_version_is_legacy_but_current_layout_preserves_removed_widgets() {
        let legacy = migrate_stored_layout(StoredLayout {
            version: 0,
            entries: vec![("cve-summary".into(), 1, 1)],
        });
        assert!(legacy.entries.iter().any(|entry| entry.0 == "poam-summary"));
        assert!(
            legacy
                .entries
                .iter()
                .any(|entry| entry.0 == "poam-watchlist")
        );

        let current = StoredLayout {
            version: StoredLayout::VERSION,
            entries: vec![("cve-summary".into(), 1, 1)],
        };
        assert_eq!(migrate_stored_layout(current.clone()), current);
    }

    #[test]
    fn empty_or_unknown_legacy_layout_restores_full_defaults() {
        let expected = default_widget_positions()
            .into_iter()
            .map(|position| (position.id.to_string(), position.cols, position.rows))
            .collect::<Vec<_>>();

        for entries in [vec![], vec![("obsolete-widget".into(), 3, 3)]] {
            let migrated = migrate_stored_layout(StoredLayout {
                version: 2,
                entries,
            });
            assert_eq!(migrated.version, StoredLayout::VERSION);
            assert_eq!(migrated.entries, expected);
        }
    }

    #[test]
    fn poam_states_do_not_treat_errors_or_zero_counts_as_loaded_data() {
        let empty = PoamDashboardSummary {
            total: 0,
            active: 0,
            overdue: 0,
            awaiting_verification: 0,
            completed: 0,
        };
        assert_eq!(
            poam_summary_state(Some(&Ok(empty))),
            PoamDashboardState::Empty
        );
        assert_eq!(
            poam_summary_state(Some(&Err(PoamApiError::Network("down".into())))),
            PoamDashboardState::Error
        );
        let unauthorized = PoamApiError::Server(poam_api::PoamServerError {
            status: 403,
            code: "forbidden".into(),
            message: "denied".into(),
            details: None,
        });
        assert_eq!(
            poam_summary_state(Some(&Err(unauthorized))),
            PoamDashboardState::Unauthorized
        );
        assert_eq!(poam_summary_state(None), PoamDashboardState::Loading);

        let empty_watchlist = poam_api::Page {
            items: vec![],
            limit: 13,
            offset: 0,
            has_more: false,
            next_offset: None,
        };
        assert_eq!(
            poam_watchlist_state(Some(&Ok(empty_watchlist))),
            PoamDashboardState::Empty
        );
        assert_eq!(
            poam_watchlist_state(Some(&Err(PoamApiError::Network("down".into())))),
            PoamDashboardState::Error
        );
    }

    #[test]
    fn poam_watchlist_height_and_navigation_use_stable_contracts() {
        assert_eq!(poam_watchlist_row_count(1), 4);
        assert_eq!(poam_watchlist_row_count(2), 8);
        assert_eq!(poam_watchlist_row_count(3), 13);

        let id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000433").unwrap();
        match compliance_route(Some(id)) {
            Route::ComplianceView {
                bundle,
                version,
                system,
                policy,
                poam,
                view,
            } => {
                assert!(bundle.is_empty());
                assert!(version.is_empty());
                assert!(system.is_empty());
                assert!(policy.is_empty());
                assert_eq!(poam, id.to_string());
                assert!(view.is_empty());
            }
            _ => panic!("expected compliance route"),
        }
    }
}
