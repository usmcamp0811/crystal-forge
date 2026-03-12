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

/// Sidebar navigation for primary routes.
#[component]
pub fn SidebarNav() -> Element {
    let app_state = use_context::<Signal<AppState>>();
    let auth_context = app_state.read().auth.clone();
    let show_admin = auth::is_admin(&auth_context);

    // Get sidebar context
    let mut sidebar_ctx = use_context::<SidebarContext>();
    let is_collapsed = (sidebar_ctx.is_collapsed)();

    // Responsive width logic:
    // - Mobile (<480px): hidden, use drawer
    // - Narrow desktop/tablet and up (>=480px): toggle between 4rem and 16rem
    let nav_width = if is_collapsed { "4rem" } else { "16rem" };

    let header_justify = if is_collapsed { "justify-center" } else { "" };
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

    rsx! {
        nav {
            "data-testid": "sidebar-nav",
            class: "cf-sidebar-shell relative {theme::surface::SIDEBAR_BG} flex-col transition-all duration-300 ease-in-out",
            style: "border-right: 1px solid var(--cf-card-border); width: {nav_width};",
            div {
                class: "p-6 flex items-center gap-3 min-h-[5rem] {header_justify}",
                img {
                    class: "h-8 w-8 shrink-0 object-contain",
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
            button {
                "data-testid": "sidebar-edge-toggle",
                class: "cf-desktop-only absolute top-20 -right-3 z-20 inline-flex h-8 w-6 items-center justify-center rounded-r-md border border-l-0 {theme::surface::CARD_BORDER} {theme::surface::SIDEBAR_BG} {theme::text::SECONDARY} {theme::interactive::HOVER_BG}",
                onclick: toggle_sidebar,
                "aria-label": if is_collapsed {
                    "Expand sidebar"
                } else {
                    "Collapse sidebar"
                },
                svg {
                    class: "w-3.5 h-3.5",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    view_box: "0 0 24 24",
                    if is_collapsed {
                        path { d: "M13 5l7 7-7 7M5 5l7 7-7 7" }
                    } else {
                        path { d: "M11 19l-7-7 7-7M19 19l-7-7 7-7" }
                    }
                }
            }
            div {
                class: "flex-1 px-3 space-y-1 overflow-y-auto",
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
                            path { d: "M3 7h18" }
                            path { d: "M3 12h18" }
                            path { d: "M3 17h18" }
                            path { d: "M7 7v10" }
                            path { d: "M17 7v10" }
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
                            rect { x: "4", y: "4", width: "7", height: "7", rx: "1" }
                            rect { x: "13", y: "4", width: "7", height: "7", rx: "1" }
                            rect { x: "4", y: "13", width: "7", height: "7", rx: "1" }
                            rect { x: "13", y: "13", width: "7", height: "7", rx: "1" }
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
                            path { d: "M4 6h16" }
                            path { d: "M4 12h16" }
                            path { d: "M4 18h10" }
                            path { d: "M18 16l2 2 4-4" }
                        }
                    )
                }
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
                            rect { x: "3", y: "4", width: "18", height: "16", rx: "2" }
                            path { d: "M8 8h8" }
                            path { d: "M8 12h8" }
                            path { d: "M8 16h5" }
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
                if show_admin {
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
                NavLink {
                    collapsed: is_collapsed,
                    to: Route::StyleGuideView {},
                    label: "Style Guide",
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
            // Show footer only when not collapsed
            if !is_collapsed {
                div {
                    class: "p-4 border-t text-xs {theme::text::MUTED}",
                    style: "border-top-color: var(--cf-card-border);",
                    "v0.1.0"
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
                class: "flex-1 px-3 space-y-1 overflow-y-auto",
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
                            path { d: "M3 7h18" }
                            path { d: "M3 12h18" }
                            path { d: "M3 17h18" }
                            path { d: "M7 7v10" }
                            path { d: "M17 7v10" }
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
                            rect { x: "4", y: "4", width: "7", height: "7", rx: "1" }
                            rect { x: "13", y: "4", width: "7", height: "7", rx: "1" }
                            rect { x: "4", y: "13", width: "7", height: "7", rx: "1" }
                            rect { x: "13", y: "13", width: "7", height: "7", rx: "1" }
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
                            path { d: "M4 6h16" }
                            path { d: "M4 12h16" }
                            path { d: "M4 18h10" }
                            path { d: "M18 16l2 2 4-4" }
                        }
                    )
                }
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
                            rect { x: "3", y: "4", width: "18", height: "16", rx: "2" }
                            path { d: "M8 8h8" }
                            path { d: "M8 12h8" }
                            path { d: "M8 16h5" }
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
                NavLink {
                    collapsed: false,
                    to: Route::StyleGuideView {},
                    label: "Style Guide",
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

            div {
                class: "p-4 border-t text-xs {theme::text::MUTED}",
                style: "border-top-color: var(--cf-card-border);",
                "v0.1.0"
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
