//! Top bar layout component.

use dioxus::prelude::*;

use crate::api::client;
use crate::components::layout::sidebar::SidebarContext;
use crate::state::app_state::AppState;
use crate::state::auth;
use crate::state::theme::UiTheme;
use crate::theme;

/// Header bar displaying the current page title and optional actions.
#[component]
pub fn TopBar(title: String) -> Element {
    let mut app_state = use_context::<Signal<AppState>>();
    let mut ui_theme = use_context::<Signal<UiTheme>>();
    let mut show_user_menu = use_signal(|| false);
    let auth_context = app_state.read().auth.clone();
    let nav = navigator();

    let sidebar_ctx = use_context::<SidebarContext>();
    let mut is_mobile_drawer_open = sidebar_ctx.is_mobile_drawer_open;
    let mut is_collapsed = sidebar_ctx.is_collapsed;

    let handle_logout = move |_| {
        spawn(async move {
            if let Ok(()) = client::logout().await {
                // Clear auth context
                app_state.write().auth = None;
                // Redirect to login
                nav.push("/login");
            }
        });
    };

    let toggle_drawer = move |_| {
        is_mobile_drawer_open.set(!is_mobile_drawer_open());
    };

    let toggle_sidebar = move |_| {
        let new_state = !is_collapsed();
        is_collapsed.set(new_state);
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
        header {
            class: "flex items-center justify-between h-16 px-6 {theme::surface::SIDEBAR_BG}",
            style: "border-bottom: 1px solid var(--cf-card-border);",
            div {
                class: "flex items-center gap-3",
                // Mobile (<480px): hamburger drawer button
                button {
                    "data-testid": "mobile-nav-toggle",
                    class: "cf-mobile-only inline-flex items-center justify-center p-2 rounded-lg border {theme::surface::CARD_BORDER} {theme::interactive::HOVER_BG} {theme::text::SECONDARY} min-h-[44px] min-w-[44px]",
                    onclick: toggle_drawer,
                    "aria-label": "Open navigation menu",
                    svg {
                        class: "w-6 h-6",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        view_box: "0 0 24 24",
                        path { d: "M4 6h16M4 12h16M4 18h16" }
                    }
                }

                // Narrow desktop/tablet and up (>=480px): sidebar collapse button
                button {
                    "data-testid": "sidebar-toggle",
                    class: "cf-desktop-only inline-flex items-center justify-center gap-2 px-3 py-2 rounded-lg border-2 {theme::surface::CARD_BORDER} {theme::interactive::HOVER_BG} {theme::text::SECONDARY} min-h-[44px]",
                    onclick: toggle_sidebar,
                    "aria-label": if is_collapsed() {
                        "Expand sidebar"
                    } else {
                        "Collapse sidebar"
                    },
                    svg {
                        class: "w-6 h-6",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        view_box: "0 0 24 24",
                        if is_collapsed() {
                            path { d: "M13 5l7 7-7 7M5 5l7 7-7 7" }
                        } else {
                            path { d: "M11 19l-7-7 7-7M19 19l-7-7 7-7" }
                        }
                    }
                    span {
                        class: "hidden md:inline text-sm font-medium",
                        if is_collapsed() {
                            "Show"
                        } else {
                            "Hide"
                        }
                    }
                }

                h1 {
                    class: "text-lg font-semibold",
                    "{title}"
                }
            }
            div {
                class: "flex items-center gap-4",

                div {
                    class: "hidden md:block",
                    input {
                        class: "rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                        r#type: "search",
                        placeholder: "Search...",
                    }
                }

                // User menu
                if let Some(user_name) = auth::user_short_name(&auth_context) {
                    div {
                        class: "relative",
                        button {
                            "data-testid": "user-menu-button",
                            class: "flex items-center gap-2 px-3 py-2 rounded-lg {theme::interactive::HOVER_BG} transition-colors",
                            onclick: move |_| show_user_menu.set(!show_user_menu()),

                            // User avatar circle
                            div {
                                class: "w-8 h-8 rounded-full bg-violet-600 flex items-center justify-center text-sm font-semibold text-white",
                                "{user_name.chars().next().unwrap_or('U').to_uppercase()}"
                            }
                            span {
                                class: "{theme::text::PRIMARY} text-sm font-medium",
                                "{user_name}"
                            }
                            // Chevron down icon
                            svg {
                                class: "w-4 h-4 {theme::text::SECONDARY}",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M19 9l-7 7-7-7"
                                }
                            }
                        }

                        // Dropdown menu
                        if show_user_menu() {
                            div {
                                "data-testid": "user-menu-dropdown",
                                class: "absolute top-full mt-2 w-25 {theme::surface::CARD_BG} border {theme::surface::CARD_BORDER} rounded-lg shadow-xl z-50",
                                style: "right: 0; min-width: 16rem;",

                                // User info section
                                div {
                                    class: "px-4 py-3 border-b {theme::surface::CARD_BORDER} text-right",
                                    if let Some(full_name) = auth::user_display_name(&auth_context) {
                                        p {
                                            class: "{theme::text::PRIMARY} text-sm font-semibold",
                                            "{full_name}"
                                        }
                                    }
                                    if let Some(ctx) = &auth_context {
                                        if let Some(user) = &ctx.user {
                                            p {
                                                class: "{theme::text::SECONDARY} text-xs break-all",
                                                "{user.email}"
                                            }
                                        }
                                        // Show roles
                                        if !ctx.roles.is_empty() {
                                            div {
                                                class: "mt-2 flex flex-wrap gap-1 justify-end",
                                                for role in &ctx.roles {
                                                    span {
                                                        class: "px-2 py-0.5 rounded text-xs bg-violet-500/20 text-violet-300",
                                                        "{role:?}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // Menu items
                                div {
                                    class: "py-2",

                                    button {
                                        class: "w-full text-right px-4 py-2 text-sm {theme::text::PRIMARY} {theme::interactive::HOVER_BG} transition-colors flex items-center justify-end gap-2",
                                        onclick: move |_| {
                                            let next = ui_theme().toggle();
                                            ui_theme.set(next);
                                        },
                                        if ui_theme() == UiTheme::Dark {
                                            // Moon icon - currently dark, click to switch to light
                                            svg {
                                                class: "w-4 h-4 shrink-0",
                                                fill: "none",
                                                stroke: "currentColor",
                                                view_box: "0 0 24 24",
                                                path {
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    stroke_width: "2",
                                                    d: "M21 12.79A9 9 0 1111.21 3a7 7 0 009.79 9.79z"
                                                }
                                            }
                                            span { "Dark Mode" }
                                        } else {
                                            // Sun icon - currently light, click to switch to dark
                                            svg {
                                                class: "w-4 h-4 shrink-0",
                                                fill: "none",
                                                stroke: "currentColor",
                                                view_box: "0 0 24 24",
                                                path {
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    stroke_width: "2",
                                                    d: "M12 3v2m0 14v2m9-9h-2M5 12H3m15.364 6.364l-1.414-1.414M7.05 7.05 5.636 5.636m12.728 0-1.414 1.414M7.05 16.95l-1.414 1.414M12 8a4 4 0 100 8 4 4 0 000-8z"
                                                }
                                            }
                                            span { "Light Mode" }
                                        }
                                    }

                                    button {
                                        class: "w-full text-right px-4 py-2 text-sm {theme::text::PRIMARY} {theme::interactive::HOVER_BG} transition-colors",
                                        onclick: handle_logout,
                                        "Sign Out"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
