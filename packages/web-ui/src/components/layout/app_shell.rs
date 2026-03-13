//! Application shell layout with sidebar navigation.

use dioxus::prelude::*;

use crate::api::models::{AuthContext, AuthMode, AuthUser, Role};
use crate::components::layout::TopBar;
use crate::components::layout::sidebar::{
    MobileDrawer, SidebarContext, SidebarEdgeToggle, SidebarNav,
};
use crate::components::layout::{BannerPlacement, DevModeBanner};
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

fn should_show_admin_denied(route: &Route, auth_context: &Option<AuthContext>) -> bool {
    matches!(route, Route::AdminView { .. }) && !auth::is_admin(auth_context)
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

    // Initialize sidebar state
    let is_mobile_drawer_open = use_signal(|| false);
    let is_collapsed = use_signal(|| {
        // Try to read from localStorage
        let stored = web_sys::window()
            .and_then(|w| w.local_storage().ok())
            .flatten()
            .and_then(|storage| storage.get_item("cf-sidebar-collapsed").ok())
            .flatten()
            .map(|v| v == "true");

        if let Some(value) = stored {
            return value;
        }

        // Default behavior: collapse only on genuinely small screens (<768px)
        web_sys::window()
            .and_then(|w| w.inner_width().ok())
            .and_then(|v| v.as_f64())
            .map(|width| width < 768.0)
            .unwrap_or(false)
    });

    // Provide sidebar context
    use_context_provider(|| SidebarContext {
        is_mobile_drawer_open,
        is_collapsed,
    });

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
            class: "min-h-screen {theme::surface::PAGE_BG} {theme::text::PRIMARY} flex flex-col overflow-x-hidden",

            DevModeBanner { placement: BannerPlacement::Top }

            div {
                class: "flex-1 flex min-h-0 relative",

                SidebarNav {}
                SidebarEdgeToggle {}
                MobileDrawer {}

                div {
                    class: "flex-1 flex flex-col min-w-0",
                    TopBar { title: current_route.title() }
                    main {
                        class: "flex-1 overflow-auto {theme::spacing::PAGE_PADDING}",
                        if should_show_admin_denied(&current_route, &auth_context) {
                            section {
                                class: "max-w-3xl mx-auto rounded-xl border border-amber-500/40 bg-amber-900/20 p-6 space-y-2",
                                h2 { class: "text-xl font-semibold text-amber-100", "Access Denied" }
                                p { class: "text-sm text-amber-200/90", "Server Management requires an administrator role." }
                            }
                        } else {
                            Outlet::<Route> {}
                        }
                    }
                }
            }

            DevModeBanner { placement: BannerPlacement::Bottom }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth_context(authenticated: bool, roles: Vec<Role>) -> Option<AuthContext> {
        Some(AuthContext {
            is_authenticated: authenticated,
            user: Some(AuthUser {
                id: "user-1".to_string(),
                email: "user@example.com".to_string(),
                display_name: Some("Example User".to_string()),
            }),
            roles,
            auth_mode: AuthMode::Local,
        })
    }

    #[test]
    fn admin_route_denied_for_non_admin() {
        let route = Route::AdminView {};
        assert!(should_show_admin_denied(
            &route,
            &auth_context(true, vec![Role::Operator])
        ));
        assert!(should_show_admin_denied(
            &route,
            &auth_context(true, vec![Role::Viewer])
        ));
        assert!(should_show_admin_denied(&route, &None));
    }

    #[test]
    fn admin_route_allowed_for_admin() {
        let route = Route::AdminView {};
        assert!(!should_show_admin_denied(
            &route,
            &auth_context(true, vec![Role::Admin])
        ));
    }

    #[test]
    fn non_admin_route_never_shows_denial_banner() {
        let route = Route::DashboardView {};
        assert!(!should_show_admin_denied(
            &route,
            &auth_context(true, vec![Role::Viewer])
        ));
        assert!(!should_show_admin_denied(&route, &None));
    }
}
