//! Top bar layout component.

use dioxus::prelude::*;

use crate::api::client;
use crate::state::app_state::AppState;
use crate::state::auth;
use crate::theme;

/// Header bar displaying the current page title and optional actions.
#[component]
pub fn TopBar(title: String) -> Element {
    let mut app_state = use_context::<Signal<AppState>>();
    let mut show_user_menu = use_signal(|| false);
    let auth_context = app_state.read().auth.clone();
    let nav = navigator();

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

    rsx! {
        header {
            class: "flex items-center justify-between h-16 px-6 border-b {theme::surface::CARD_BORDER} {theme::surface::SIDEBAR_BG}",
            div {
                class: "flex items-center gap-3",
                h1 {
                    class: "text-lg font-semibold",
                    "{title}"
                }
            }
            div {
                class: "flex items-center gap-4",
                
                // Search (hidden on small screens)
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
                                class: "absolute right-0 mt-2 w-56 {theme::surface::CARD_BG} border {theme::surface::CARD_BORDER} rounded-lg shadow-xl z-50",
                                
                                // User info section
                                div {
                                    class: "px-4 py-3 border-b {theme::surface::CARD_BORDER}",
                                    if let Some(full_name) = auth::user_display_name(&auth_context) {
                                        p {
                                            class: "{theme::text::PRIMARY} text-sm font-semibold",
                                            "{full_name}"
                                        }
                                    }
                                    if let Some(ctx) = &auth_context {
                                        if let Some(user) = &ctx.user {
                                            p {
                                                class: "{theme::text::SECONDARY} text-xs",
                                                "{user.email}"
                                            }
                                        }
                                        // Show roles
                                        if !ctx.roles.is_empty() {
                                            div {
                                                class: "mt-2 flex flex-wrap gap-1",
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
                                        class: "w-full text-left px-4 py-2 text-sm {theme::text::PRIMARY} {theme::interactive::HOVER_BG} transition-colors",
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
