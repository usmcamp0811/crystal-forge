//! Sidebar navigation component.

use dioxus::prelude::*;

use crate::alerts::{NAV_BADGES, attention_count};
use crate::api::client::get_navigation_badges;
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
    let mut collapsed_signal = sidebar_ctx.is_collapsed;

    // Match design-example sizing for sidebar and rail mode.
    let nav_width = if is_collapsed { "64px" } else { "240px" };

    #[cfg(debug_assertions)]
    let show_dev_tools = true;
    #[cfg(not(debug_assertions))]
    let show_dev_tools = false;

    // Fetch navigation badge counts and re-poll every 30 seconds, writing into
    // the shared NAV_BADGES global so other views (e.g. Builds/Evaluations
    // tab badges) can read the same server-computed "new since last
    // acknowledgment" counts. alerts::acknowledge() also refreshes NAV_BADGES
    // immediately after recording an acknowledgment, so badges clear quickly
    // without waiting for the next scheduled poll.
    use_future(move || async move {
        loop {
            if let Ok(fresh) = get_navigation_badges().await {
                *NAV_BADGES.write() = fresh;
            }
            gloo_timers::future::TimeoutFuture::new(30_000).await;
        }
    });
    // NAV_BADGES is the sole source of truth for badge visibility (server-
    // computed "new since last acknowledgment" per category); reading it here
    // is what makes the sidebar re-render both on poll and on the optimistic
    // zero-out that alerts::acknowledge() performs immediately on click/visit.
    let badges = NAV_BADGES();
    let systems_attention = badges.systems_attention.max(attention_count("systems"));
    let flakes_errored = badges.flakes_errored.max(attention_count("flakes"));
    let environments_attention = badges
        .environments_attention
        .max(attention_count("environments"));
    // Builds/Evals badges are fully server-driven (delta counts persisted via
    // acknowledge()); no local raw-count reconciliation needed here.
    let builds_failed = badges.builds_failed_new;
    let evals_failed = badges.evals_failed_new;
    let cves_critical = badges.cves_critical_new.max(attention_count("cves"));

    // Get user data for profile section
    let user_initials = if let Some(name) = auth::user_short_name(&auth_context) {
        name.chars().take(2).collect::<String>().to_uppercase()
    } else {
        "U".to_string()
    };

    let user_display_name =
        auth::user_display_name(&auth_context).unwrap_or_else(|| "User".to_string());

    let user_role_and_host = if auth_context.is_some() {
        let role = if auth::is_admin(&auth_context) {
            "admin"
        } else {
            "user"
        };
        format!("{} · acme-prod", role)
    } else {
        "guest".to_string()
    };

    rsx! {
        nav {
            "data-testid": "sidebar-nav",
            class: if is_collapsed {
                "cf-sidebar-shell sidebar rail relative z-20 {theme::surface::SIDEBAR_BG} flex-col transition-all duration-300 ease-in-out"
            } else {
                "cf-sidebar-shell sidebar relative z-20 {theme::surface::SIDEBAR_BG} flex-col transition-all duration-300 ease-in-out"
            },
            style: "border-right: 1px solid var(--cf-card-border); width: {nav_width};",
            div {
                class: "sidebar-brand",
                style: if is_collapsed { "justify-content: center;" } else { "" },
                div {
                    class: "brand-mark",
                    img {
                        src: asset!("assets/cf.png"),
                        alt: "Crystal Forge logo",
                        class: "brand-mark-img",
                    }
                }
                if !is_collapsed {
                    div {
                        style: "flex: 1; min-width: 0;",
                        div {
                            class: "brand-name",
                            "Crystal Forge"
                        }
                        div {
                            class: "brand-sub",
                            "v{env!(\"CARGO_PKG_VERSION\")} · dev"
                        }
                    }
                }
                // Collapse button matching the design example (Shell.jsx).
                button {
                    class: "sidebar-collapse focus-ring",
                    onclick: move |_| {
                        let new_state = !collapsed_signal();
                        collapsed_signal.set(new_state);
                        if let Some(window) = web_sys::window() {
                            if let Ok(Some(storage)) = window.local_storage() {
                                let _ = storage.set_item(
                                    "cf-sidebar-collapsed",
                                    if new_state { "true" } else { "false" },
                                );
                            }
                        }
                    },
                    title: if is_collapsed { "Expand sidebar" } else { "Collapse sidebar" },
                    "aria-label": if is_collapsed { "Expand sidebar" } else { "Collapse sidebar" },
                    svg {
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        view_box: "0 0 24 24",
                        if is_collapsed {
                            // chevron-right
                            path { d: "M9 18l6-6-6-6" }
                        } else {
                            // chevron-left
                            path { d: "M15 18l-6-6 6-6" }
                        }
                    }
                }
            }
            div {
                class: "flex-1 px-3 overflow-y-auto",
                style: "padding-top: 0.25rem; padding-bottom: 0.25rem;",

                // ── Fleet ─────────────────────────────────────────────────
                NavSection { collapsed: is_collapsed, label: "Fleet" }
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
                    badge_count: if systems_attention > 0 { Some(systems_attention) } else { None },
                    badge_attention: systems_attention > 0,
                    badge_hidden: systems_attention == 0,
                    badge_title: Some(format!("{} of {} systems need attention (critical or offline)", systems_attention, badges.systems_total)),
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
                    to: Route::FlakesView {},
                    label: "Flakes",
                    badge_count: if flakes_errored > 0 { Some(flakes_errored) } else { None },
                    badge_attention: flakes_errored > 0,
                    badge_hidden: flakes_errored == 0,
                    badge_title: Some(if flakes_errored > 0 {
                        format!("{} of {} flakes failing to sync", flakes_errored, badges.flakes_total)
                    } else {
                        format!("{} flakes tracked", badges.flakes_total)
                    }),
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
                    to: Route::EnvironmentsView {},
                    label: "Environments",
                    badge_count: if environments_attention > 0 { Some(environments_attention) } else { None },
                    badge_attention: environments_attention > 0,
                    badge_hidden: environments_attention == 0,
                    badge_title: Some(if environments_attention > 0 {
                        format!("{} of {} environments have critical or offline systems", environments_attention, badges.environments_total)
                    } else {
                        format!("{} deployment environments", badges.environments_total)
                    }),
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

                // ── Pipeline ──────────────────────────────────────────────
                NavSection { collapsed: is_collapsed, label: "Pipeline" }
                NavLink {
                    collapsed: is_collapsed,
                    to: Route::BuildsView {},
                    label: "Builds",
                    // Builds badge is acknowledged only when the failures tab is opened (not on mount).
                    // The view itself calls acknowledge("builds") when the completed/failed tab opens.
                    badge_count: if builds_failed > 0 { Some(builds_failed) } else { None },
                    badge_attention: builds_failed > 0,
                    badge_hidden: builds_failed == 0,
                    badge_title: Some(format!("{} new failed build{} since you last checked", builds_failed, if builds_failed == 1 { "" } else { "s" })),
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
                NavLink {
                    collapsed: is_collapsed,
                    to: Route::EvaluationsView {},
                    label: "Evaluations",
                    // Evals badge is acknowledged only when the failures tab is opened (not on mount).
                    badge_count: if evals_failed > 0 { Some(evals_failed) } else { None },
                    badge_attention: evals_failed > 0,
                    badge_hidden: evals_failed == 0,
                    badge_title: Some(format!("{} new failed evaluation{} since you last checked", evals_failed, if evals_failed == 1 { "" } else { "s" })),
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
                if show_admin {
                    NavLink {
                        collapsed: is_collapsed,
                        to: Route::ScanningView {},
                        label: "Scanning",
                        icon: rsx!(
                            svg {
                                class: "w-4 h-4",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "1.75",
                                view_box: "0 0 24 24",
                                path { d: "M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3M22 12.5a10 10 0 0 1-18.8 4.2" }
                            }
                        )
                    }
                }

                // ── Compliance ────────────────────────────────────────────
                NavSection { collapsed: is_collapsed, label: "Compliance" }
                if show_admin {
                    NavLink {
                        collapsed: is_collapsed,
                        to: Route::CvesView {},
                        label: "CVEs",
                        badge_count: if cves_critical > 0 { Some(cves_critical) } else { None },
                        badge_attention: cves_critical > 0,
                        badge_hidden: cves_critical == 0,
                        badge_title: Some(format!("{} new critical CVE{} since you last checked", cves_critical, if cves_critical == 1 { "" } else { "s" })),
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
                    label: "Policies",
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
                NavLink {
                    collapsed: is_collapsed,
                    to: Route::ComplianceView {},
                    label: "Compliance",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M12 3l7 3v6c0 5-3 7.5-7 9-4-1.5-7-4-7-9V6l7-3z" }
                            path { d: "M9 12l2 2 4-4" }
                        }
                    )
                }

                // ── System ────────────────────────────────────────────────
                NavSection { collapsed: is_collapsed, label: "System" }
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
                if show_admin {
                    NavLink {
                        collapsed: is_collapsed,
                        to: Route::AdminView {},
                        label: "Server",
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

            // User profile section at bottom
            div {
                class: "sidebar-user",
                div {
                    class: "user-avatar",
                    {user_initials}
                }
                if !is_collapsed {
                    div {
                        style: "min-width: 0;",
                        div {
                            class: "user-name",
                            {user_display_name}
                        }
                        div {
                            class: "user-meta",
                            {user_role_and_host}
                        }
                    }
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

    // Get user data for profile section
    let user_initials = if let Some(name) = auth::user_short_name(&auth_context) {
        name.chars().take(2).collect::<String>().to_uppercase()
    } else {
        "U".to_string()
    };

    let user_display_name =
        auth::user_display_name(&auth_context).unwrap_or_else(|| "User".to_string());

    let user_role_and_host = if auth_context.is_some() {
        let role = if auth::is_admin(&auth_context) {
            "admin"
        } else {
            "user"
        };
        format!("{} · acme-prod", role)
    } else {
        "guest".to_string()
    };

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
                    div { class: "brand-mark", "CF" }
                    div {
                        div {
                            class: "brand-name",
                            "Crystal Forge"
                        }
                        div {
                            class: "brand-sub",
                            "v{env!(\"CARGO_PKG_VERSION\")} · dev"
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

                NavSection { collapsed: false, label: "Fleet" }
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

                NavSection { collapsed: false, label: "Pipeline" }
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
                if show_admin {
                    NavLink {
                        collapsed: false,
                        to: Route::ScanningView {},
                        label: "Scanning",
                        icon: rsx!(
                            svg {
                                class: "w-4 h-4",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "1.75",
                                view_box: "0 0 24 24",
                                path { d: "M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3M22 12.5a10 10 0 0 1-18.8 4.2" }
                            }
                        )
                    }
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
                    label: "Policies",
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
                NavLink {
                    collapsed: false,
                    to: Route::ComplianceView {},
                    label: "Compliance",
                    icon: rsx!(
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "1.75",
                            view_box: "0 0 24 24",
                            path { d: "M12 3l7 3v6c0 5-3 7.5-7 9-4-1.5-7-4-7-9V6l7-3z" }
                            path { d: "M9 12l2 2 4-4" }
                        }
                    )
                }

                NavSection { collapsed: false, label: "System" }
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

                if show_admin {
                    NavLink {
                        collapsed: false,
                        to: Route::AdminView {},
                        label: "Server",
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

            // User profile section at bottom
            div {
                class: "sidebar-user",
                div {
                    class: "user-avatar",
                    {user_initials}
                }
                div {
                    style: "min-width: 0;",
                    div {
                        class: "user-name",
                        {user_display_name}
                    }
                    div {
                        class: "user-meta",
                        {user_role_and_host}
                    }
                }
            }
        }
    }
}

/// A sidebar navigation link.
///
/// Accepts optional badge props for the alert badge system.  When `badge_count`
/// is `Some(n)` with `n > 0` (and `badge_hidden` is false), a `.nav-count`
/// badge is rendered; when `badge_attention` is `true` the badge is red
/// (`.nav-count-alert`).
#[component]
fn NavLink(
    collapsed: bool,
    to: Route,
    label: &'static str,
    icon: Element,
    /// Badge count to display. None or 0 → no badge.
    #[props(default)]
    badge_count: Option<i64>,
    /// When true, the badge is rendered in red (`.nav-count-alert`).
    #[props(default)]
    badge_attention: bool,
    /// Tooltip text for the badge element.
    #[props(default)]
    badge_title: Option<String>,
    /// When true, the badge is hidden (used for acknowledged attention badges).
    #[props(default)]
    badge_hidden: bool,
) -> Element {
    let sidebar_ctx = use_context::<SidebarContext>();
    let mut is_mobile_drawer_open = sidebar_ctx.is_mobile_drawer_open;
    let current_route = use_route::<Route>();
    let is_active = std::mem::discriminant(&current_route) == std::mem::discriminant(&to);

    let click_handler = move |_| {
        // Close mobile drawer when navigating
        is_mobile_drawer_open.set(false);
    };

    let nav_class = if is_active {
        "nav-item active focus-ring"
    } else {
        "nav-item focus-ring"
    };

    let show_badge = badge_count.is_some_and(|c| c > 0) && !badge_hidden;
    let badge_class = if badge_attention {
        "nav-count nav-count-alert"
    } else {
        "nav-count"
    };

    rsx! {
        Link {
            to,
            onclick: click_handler,
            class: "{nav_class}",
            title: if collapsed { label } else { "" },
            div {
                class: "nav-icon",
                {icon}
            }
            // Show label only when not collapsed
            if !collapsed {
                span {
                    class: "nav-label",
                    "{label}"
                }
            }
            if show_badge {
                span {
                    class: "{badge_class}",
                    title: badge_title.as_deref().unwrap_or(""),
                    "{badge_count.unwrap_or(0)}"
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
        if !collapsed {
            div {
                class: "nav-section-label",
                "{label}"
            }
        }
    }
}

#[component]
fn PlaceholderNavItem(collapsed: bool, label: &'static str, icon: Element) -> Element {
    rsx! {
        div {
            class: "nav-item nav-item-placeholder",
            title: if collapsed { label } else { "Coming soon" },
            "aria-disabled": "true",
            div {
                class: "nav-icon",
                {icon}
            }
            if !collapsed {
                span {
                    class: "nav-label",
                    "{label}"
                }
            }
        }
    }
}
