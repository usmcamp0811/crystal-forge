//! Application shell layout with sidebar navigation.

use dioxus::prelude::*;

use crate::api::models::{AuthContext, AuthMode, AuthUser, Role};
use crate::components::layout::DevModeBanner;
use crate::components::layout::SidebarNav;
use crate::components::layout::TopBar;
use crate::routes::Route;
use crate::state::app_state::{AppState, AuthFetchState};
use crate::state::auth;
use crate::theme;

/// Check if UI check mock auth mode is enabled via query param.
/// Only available in debug builds to prevent production auth bypass.
#[cfg(debug_assertions)]
fn ui_check_mock_auth_enabled() -> bool {
    web_sys::window()
        .and_then(|w| w.location().search().ok())
        .map(|q| q.contains("ui_check_auth=1"))
        .unwrap_or(false)
}

#[cfg(not(debug_assertions))]
fn ui_check_mock_auth_enabled() -> bool {
    false
}

#[cfg(debug_assertions)]
fn ui_check_mock_auth_context() -> AuthContext {
    AuthContext {
        is_authenticated: true,
        user: Some(AuthUser {
            id: "ui-check-user".to_string(),
            email: "ui-check@example.com".to_string(),
            display_name: Some("UI Check".to_string()),
        }),
        roles: vec![Role::Admin],
        auth_mode: AuthMode::Local,
    }
}

/// Top-level application layout wrapping all views.
///
/// Provides the sidebar navigation and main content area.
/// Redirects to login if the user is not authenticated.
#[component]
pub fn AppShell() -> Element {
    let current_route = use_route::<Route>();
    let mut app_state = use_context::<Signal<AppState>>();
    let nav = navigator();

    let state = app_state.read();
    let auth_fetch_state = state.auth_fetch_state.clone();
    let mut auth_context = state.auth.clone();
    drop(state);

    // In debug builds, allow mock auth for screenshot tests
    #[cfg(debug_assertions)]
    if auth_context.is_none() && ui_check_mock_auth_enabled() {
        let mock = ui_check_mock_auth_context();
        app_state.write().auth = Some(mock.clone());
        auth_context = Some(mock);
    }

    // Handle auth fetch states
    match auth_fetch_state {
        AuthFetchState::Loading => {
            // Show loading spinner while auth context is being fetched
            return rsx! {
                div {
                    class: "min-h-screen flex items-center justify-center {theme::surface::PAGE_BG}",
                    div {
                        class: "text-center",
                        div {
                            class: "animate-spin rounded-full h-12 w-12 border-b-2 border-violet-500 mx-auto mb-4"
                        }
                        p {
                            class: "{theme::text::SECONDARY}",
                            "Loading..."
                        }
                    }
                }
            };
        }
        AuthFetchState::Error => {
            // Auth fetch failed - redirect to login
            nav.push("/login");
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
        AuthFetchState::Loaded => {
            // Auth loaded - check if authenticated
            if !auth::is_authenticated(&auth_context) {
                nav.push("/login");
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
        }
    }

    rsx! {
        div {
            class: "min-h-screen {theme::surface::PAGE_BG} {theme::text::PRIMARY} flex",

            SidebarNav {}

            div {
                class: "flex-1 flex flex-col min-w-0",
                TopBar { title: current_route.title() }
                DevModeBanner {}
                main {
                    class: "flex-1 overflow-auto {theme::spacing::PAGE_PADDING}",
                    Outlet::<Route> {}
                }
            }
        }
    }
}
