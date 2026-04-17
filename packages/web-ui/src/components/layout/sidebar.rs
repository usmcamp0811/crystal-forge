//! Sidebar navigation component.

use dioxus::prelude::*;

use crate::routes::Route;
use crate::state::app_state::AppState;
use crate::state::auth;
use crate::theme;

/// Context for sidebar state shared between components
#[derive(Clone, Copy)]
pub struct SidebarContext {
    pub is_mobile_drawer_open: Signal<bool>,
    pub is_collapsed: Signal<bool>,
}

/// Sidebar edge toggle button — rendered as a sibling of SidebarNav in the shell,
/// absolutely positioned to straddle the sidebar/content boundary.
#[component]
pub fn SidebarEdgeToggle() -> Element {
    let mut sidebar_ctx = use_context::<SidebarContext>();
    let is_collapsed = (sidebar_ctx.is_collapsed)();

    let toggle_sidebar = move |_| {
        let new_state = !(sidebar_ctx.is_collapsed)();
        sidebar_ctx.is_collapsed.set(new_state);
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                let _ = storage.set_item(
                    "cf-sidebar-collapsed",
                    if new_state { "true" } else { "false" },
                );
            }
        }
    };

    let nav_width = if is_collapsed { "4rem" } else { "16rem" };

    rsx! {
        button {
            "data-testid": "sidebar-edge-toggle",
            class: "cf-sidebar-shell cf-sidebar-edge-toggle",
            style: "left: {nav_width};",
            onclick: toggle_sidebar,
            "aria-label": if is_collapsed { "Expand sidebar" } else { "Collapse sidebar" },
            svg {
                class: "w-3 h-3",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2.5",
                view_box: "0 0 24 24",
                if is_collapsed {
                    path { d: "M13 5l7 7-7 7M5 5l7 7-7 7" }
                } else {
                    path { d: "M11 19l-7-7 7-7M19 19l-7-7 7-7" }
                }
            }
        }
    }
}

/// Sidebar navigation for primary routes.
#[component]
pub fn SidebarNav() -> Element {
    let app_state = use_context::<Signal<AppState>>();
    let auth_context = app_state.read().auth.clone();
    let show_admin = auth::is_admin(&auth_context);

    // Get sidebar context
    let sidebar_ctx = use_context::<SidebarContext>();
    let is_collapsed = (sidebar_ctx.is_collapsed)();

    // Responsive width logic:
    // - Mobile (<480px): hidden, use drawer
    // - Narrow desktop/tablet and up (>=480px): toggle between 4rem and 16rem
    let nav_width = if is_collapsed { "4rem" } else { "16rem" };

    let header_justify = if is_collapsed { "justify-center" } else { "" };

    #[cfg(debug_assertions)]
    let show_dev_tools = true;
    #[cfg(not(debug_assertions))]
    let show_dev_tools = false;

    rsx! {
        nav {
            "data-testid": "sidebar-nav",
            class: "cf-sidebar-shell relative z-20 {theme::surface::SIDEBAR_BG} flex-col transition-all duration-300 ease-in-out",
            style: "border-right: 1px solid var(--cf-card-border); width: {nav_width};",
            div {
                class: "flex items-center gap-3 min-h-[5rem] {header_justify}",
                style: if is_collapsed { "padding: 1rem;" } else { "padding: 1.5rem;" },
                img {
                    class: "shrink-0 object-contain",
                    style: "width: 2rem; height: 2rem;",
                    src: asset!("assets/crystal-forge-icon.png"),
                    alt: "Crystal Forge"
                }
                // Show text only when sidebar is expanded
                if !is_collapsed {
                    div {
                        h1 {
                            class: "text-xl font-bold {theme::text::PRIMARY}",
                            "Crystal Forge"
                        }
                        p {
                            class: "text-xs {theme::text::MUTED} mt-1",
                            "Fleet Management"
                        }
                    }
                }
            }
            div {
                class: "flex-1 px-3 overflow-y-auto",
                style: "padding-top: 0.25rem; padding-bottom: 0.25rem;",

                // ── Overview ──────────────────────────────────────────────
                NavLink {
                    collapsed: is_collapsed,
                    to: Route::DashboardView {},
                    label: "Dashboard",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M3 11l9-7 9 7" }
                            path { d: "M5 10v10h5v-6h4v6h5V10" }
                        }
                    )
                }

                // ── Fleet ─────────────────────────────────────────────────
                NavSection { collapsed: is_collapsed, label: "Fleet" }
                NavLink {
                    collapsed: is_collapsed,
                    to: Route::SystemsView {},
                    label: "Systems",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            rect { x: "3", y: "5", width: "18", height: "12", rx: "2" }
                            path { d: "M7 21h10" }
                            path { d: "M12 17v4" }
                        }
                    )
                }
                NavLink {
                    collapsed: is_collapsed,
                    to: Route::EnvironmentsView {},
                    label: "Environments",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M3 7h18" }
                            path { d: "M6 12h12" }
                            path { d: "M9 17h6" }
                        }
                    )
                }

                // ── Nix Pipeline ──────────────────────────────────────────
                NavSection { collapsed: is_collapsed, label: "Nix Pipeline" }
                NavLink {
                    collapsed: is_collapsed,
                    to: Route::FlakesView {},
                    label: "Flakes",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M12 2L2 7l10 5 10-5-10-5z" }
                            path { d: "M2 17l10 5 10-5" }
                            path { d: "M2 12l10 5 10-5" }
                        }
                    )
                }
                NavLink {
                    collapsed: is_collapsed,
                    to: Route::EvaluationsView {},
                    label: "Evaluations",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2" }
                            path { d: "M9 5a2 2 0 002 2h2a2 2 0 002-2" }
                            path { d: "M9 12l2 2 4-4" }
                        }
                    )
                }
                NavLink {
                    collapsed: is_collapsed,
                    to: Route::BuildsView {},
                    label: "Builds",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M14.7 6.3a1 1 0 000 1.4l1.6 1.6a1 1 0 001.4 0l3.77-3.77a6 6 0 01-7.94 7.94l-6.91 6.91a2.12 2.12 0 01-3-3l6.91-6.91a6 6 0 017.94-7.94l-3.76 3.76z" }
                        }
                    )
                }

                // ── Infrastructure ────────────────────────────────────────
                NavSection { collapsed: is_collapsed, label: "Infrastructure" }
                NavLink {
                    collapsed: is_collapsed,
                    to: Route::BuildersView {},
                    label: "Builders",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            rect { x: "2", y: "3", width: "20", height: "14", rx: "2" }
                            path { d: "M8 21h8" }
                            path { d: "M12 17v4" }
                            path { d: "M7 8h.01" }
                            path { d: "M11 8h.01" }
                        }
                    )
                }
                NavLink {
                    collapsed: is_collapsed,
                    to: Route::CachesView {},
                    label: "Caches",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M5 8h14M5 8a2 2 0 110-4h14a2 2 0 110 4M5 8v10a2 2 0 002 2h10a2 2 0 002-2V8m-9 4h4" }
                        }
                    )
                }

                // ── Compliance ────────────────────────────────────────────
                NavSection { collapsed: is_collapsed, label: "Compliance" }
                if show_admin {
                    NavLink {
                        collapsed: is_collapsed,
                        to: Route::CvesView {},
                        label: "CVEs",
                        icon: rsx!(
                            svg {
                                class: "w-4 h-4",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "1.75",
                                view_box: "0 0 24 24",
                                path { d: "M12 3l7 3v6c0 5-3 7.5-7 9-4-1.5-7-4-7-9V6l7-3z" }
                            }
                        )
                    }
                }
                NavLink {
                    collapsed: is_collapsed,
                    to: Route::PoliciesView {},
                    label: "Deployment Policies",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" }
                        }
                    )
                }

                // ── Admin (role-gated) ────────────────────────────────────
                if show_admin {
                    NavSection { collapsed: is_collapsed, label: "Admin" }
                    NavLink {
                        collapsed: is_collapsed,
                        to: Route::AdminView {},
                        label: "Server Management",
                        icon: rsx!(
                            svg {
                                class: "w-4 h-4",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "1.75",
                                view_box: "0 0 24 24",
                                path { d: "M12 3l8 4v5c0 5-3 8-8 9-5-1-8-4-8-9V7l8-4z" }
                                path { d: "M9 12l2 2 4-4" }
                            }
                        )
                    }
                }

                // ── Dev Tools (debug builds only) ─────────────────────────
                if show_dev_tools {
                    NavSection { collapsed: is_collapsed, label: "Dev Tools" }
                    NavLink {
                        collapsed: is_collapsed,
                        to: Route::StyleGuideView {},
                        label: "Component Showcase",
                        icon: rsx!(
                            svg {
                                class: "w-4 h-4",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "1.75",
                                view_box: "0 0 24 24",
                                path { d: "M4 6h16" }
                                path { d: "M4 12h16" }
                                path { d: "M4 18h16" }
                            }
                        )
                    }
                }
            }
            // Show footer only when not collapsed
            if !is_collapsed {
                div {
                    class: "p-4 border-t text-xs {theme::text::MUTED}",
                    style: "border-top-color: var(--cf-card-border);",
                    "v{env!(\"CARGO_PKG_VERSION\")}"
                }
            }
        }
    }
}

/// Mobile drawer navigation (shows on <768px)
#[component]
pub fn MobileDrawer() -> Element {
    let app_state = use_context::<Signal<AppState>>();
    let auth_context = app_state.read().auth.clone();
    let show_admin = auth::is_admin(&auth_context);

    let sidebar_ctx = use_context::<SidebarContext>();
    let mut is_mobile_drawer_open = sidebar_ctx.is_mobile_drawer_open;

    #[cfg(debug_assertions)]
    let show_dev_tools = true;
    #[cfg(not(debug_assertions))]
    let show_dev_tools = false;

    if !is_mobile_drawer_open() {
        return rsx! { div { class: "hidden" } };
    }

    rsx! {
        // Backdrop overlay
        div {
            "data-testid": "mobile-drawer-backdrop",
            class: "cf-mobile-overlay fixed inset-0 bg-black/50 z-40",
            onclick: move |_| is_mobile_drawer_open.set(false),
        }

        // Drawer
        nav {
            "data-testid": "mobile-drawer",
            class: "cf-mobile-drawer fixed left-0 top-0 bottom-0 w-64 {theme::surface::SIDEBAR_BG} z-50 flex flex-col transform transition-transform duration-300",
            style: "border-right: 1px solid var(--cf-card-border);",

            // Header with close button
            div {
                class: "p-6 flex items-center justify-between min-h-[5rem]",
                div {
                    class: "flex items-center gap-3",
                    img {
                        class: "h-8 w-8 object-contain",
                        src: asset!("assets/crystal-forge-icon.png"),
                        alt: "Crystal Forge"
                    }
                    div {
                        h1 {
                            class: "text-xl font-bold {theme::text::PRIMARY}",
                            "Crystal Forge"
                        }
                        p {
                            class: "text-xs {theme::text::MUTED} mt-1",
                            "Fleet Management"
                        }
                    }
                }
                button {
                    "data-testid": "mobile-drawer-close",
                    class: "p-2 rounded-lg {theme::interactive::HOVER_BG} {theme::text::SECONDARY}",
                    onclick: move |_| is_mobile_drawer_open.set(false),
                    "aria-label": "Close menu",
                    svg {
                        class: "w-6 h-6",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        view_box: "0 0 24 24",
                        path { d: "M6 18L18 6M6 6l12 12" }
                    }
                }
            }

            // Navigation links
            div {
                class: "flex-1 px-3 overflow-y-auto",
                style: "padding-top: 0.25rem; padding-bottom: 0.25rem;",

                NavLink {
                    collapsed: false,
                    to: Route::DashboardView {},
                    label: "Dashboard",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M3 11l9-7 9 7" }
                            path { d: "M5 10v10h5v-6h4v6h5V10" }
                        }
                    )
                }

                NavSection { collapsed: false, label: "Fleet" }
                NavLink {
                    collapsed: false,
                    to: Route::SystemsView {},
                    label: "Systems",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            rect { x: "3", y: "5", width: "18", height: "12", rx: "2" }
                            path { d: "M7 21h10" }
                            path { d: "M12 17v4" }
                        }
                    )
                }
                NavLink {
                    collapsed: false,
                    to: Route::EnvironmentsView {},
                    label: "Environments",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M3 7h18" }
                            path { d: "M6 12h12" }
                            path { d: "M9 17h6" }
                        }
                    )
                }

                NavSection { collapsed: false, label: "Nix Pipeline" }
                NavLink {
                    collapsed: false,
                    to: Route::FlakesView {},
                    label: "Flakes",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M12 2L2 7l10 5 10-5-10-5z" }
                            path { d: "M2 17l10 5 10-5" }
                            path { d: "M2 12l10 5 10-5" }
                        }
                    )
                }
                NavLink {
                    collapsed: false,
                    to: Route::EvaluationsView {},
                    label: "Evaluations",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2" }
                            path { d: "M9 5a2 2 0 002 2h2a2 2 0 002-2" }
                            path { d: "M9 12l2 2 4-4" }
                        }
                    )
                }
                NavLink {
                    collapsed: false,
                    to: Route::BuildsView {},
                    label: "Builds",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M14.7 6.3a1 1 0 000 1.4l1.6 1.6a1 1 0 001.4 0l3.77-3.77a6 6 0 01-7.94 7.94l-6.91 6.91a2.12 2.12 0 01-3-3l6.91-6.91a6 6 0 017.94-7.94l-3.76 3.76z" }
                        }
                    )
                }

                NavSection { collapsed: false, label: "Infrastructure" }
                NavLink {
                    collapsed: false,
                    to: Route::BuildersView {},
                    label: "Builders",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            rect { x: "2", y: "3", width: "20", height: "14", rx: "2" }
                            path { d: "M8 21h8" }
                            path { d: "M12 17v4" }
                            path { d: "M7 8h.01" }
                            path { d: "M11 8h.01" }
                        }
                    )
                }
                NavLink {
                    collapsed: false,
                    to: Route::CachesView {},
                    label: "Caches",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M5 8h14M5 8a2 2 0 110-4h14a2 2 0 110 4M5 8v10a2 2 0 002 2h10a2 2 0 002-2V8m-9 4h4" }
                        }
                    )
                }

                NavSection { collapsed: false, label: "Compliance" }
                if show_admin {
                    NavLink {
                        collapsed: false,
                        to: Route::CvesView {},
                        label: "CVEs",
                        icon: rsx!(
                            svg {
                                class: "w-4 h-4",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "1.75",
                                view_box: "0 0 24 24",
                                path { d: "M12 3l7 3v6c0 5-3 7.5-7 9-4-1.5-7-4-7-9V6l7-3z" }
                            }
                        )
                    }
                }
                NavLink {
                    collapsed: false,
                    to: Route::PoliciesView {},
                    label: "Deployment Policies",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" }
                        }
                    )
                }

                if show_admin {
                    NavSection { collapsed: false, label: "Admin" }
                    NavLink {
                        collapsed: false,
                        to: Route::AdminView {},
                        label: "Server Management",
                        icon: rsx!(
                            svg {
                                class: "w-4 h-4",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "1.75",
                                view_box: "0 0 24 24",
                                path { d: "M12 3l8 4v5c0 5-3 8-8 9-5-1-8-4-8-9V7l8-4z" }
                                path { d: "M9 12l2 2 4-4" }
                            }
                        )
                    }
                }

                if show_dev_tools {
                    NavSection { collapsed: false, label: "Dev Tools" }
                    NavLink {
                        collapsed: false,
                        to: Route::StyleGuideView {},
                        label: "Component Showcase",
                        icon: rsx!(
                            svg {
                                class: "w-4 h-4",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "1.75",
                                view_box: "0 0 24 24",
                                path { d: "M4 6h16" }
                                path { d: "M4 12h16" }
                                path { d: "M4 18h16" }
                            }
                        )
                    }
                }
            }

            div {
                class: "p-4 border-t text-xs {theme::text::MUTED}",
                style: "border-top-color: var(--cf-card-border);",
                "v{env!(\"CARGO_PKG_VERSION\")}"
            }
        }
    }
}

/// A sidebar navigation link.
#[component]
fn NavLink(collapsed: bool, to: Route, label: &'static str, icon: Element) -> Element {
    let sidebar_ctx = use_context::<SidebarContext>();
    let mut is_mobile_drawer_open = sidebar_ctx.is_mobile_drawer_open;
    let current_route = use_route::<Route>();
    let is_active = std::mem::discriminant(&current_route) == std::mem::discriminant(&to);

    let click_handler = move |_| {
        // Close mobile drawer when navigating
        is_mobile_drawer_open.set(false);
    };

    let base_classes =
        "flex items-center gap-3 px-3 py-2 rounded-lg transition-colors min-h-[44px]";
    let position_classes = if collapsed {
        "justify-center md:justify-center lg:justify-start"
    } else {
        ""
    };
    let state_classes = if is_active {
        "bg-violet-500/20 text-violet-300".to_string()
    } else {
        format!(
            "{} {} cf-hover-text-primary",
            theme::text::SECONDARY,
            theme::interactive::HOVER_BG
        )
    };
    let all_classes = format!("{} {} {}", base_classes, position_classes, state_classes);

    rsx! {
        Link {
            to,
            onclick: click_handler,
            class: "{all_classes}",
            title: if collapsed { label } else { "" },
            div {
                class: "shrink-0",
                {icon}
            }
            // Show label only when not collapsed
            if !collapsed {
                span {
                    "{label}"
                }
            }
        }
    }
}

/// A labeled section header in the sidebar nav.
///
/// When expanded: full-bleed shaded row with muted uppercase label.
/// When collapsed: full-bleed tinted hairline rule only (no label at 4rem).
#[component]
fn NavSection(collapsed: bool, label: &'static str) -> Element {
    rsx! {
        div {
            class: "cf-nav-section",
            if !collapsed {
                span {
                    class: "cf-nav-section-label",
                    "{label}"
                }
            }
        }
    }
}
