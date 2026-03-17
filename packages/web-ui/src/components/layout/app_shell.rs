//! Application shell layout with sidebar navigation.

use dioxus::prelude::*;

use crate::api::client::fetch_config_health;
use crate::api::models::{AuthContext, AuthMode, AuthUser, Role};
use crate::components::layout::TopBar;
use crate::components::layout::sidebar::{
    MobileDrawer, SidebarContext, SidebarEdgeToggle, SidebarNav,
};
use crate::components::layout::{BannerPlacement, DevModeBanner};
use crate::components::notifications::{AlertBanner, AlertSeverity};
use crate::components::onboarding::OnboardingCoachPanel;
use crate::routes::Route;
use crate::state::app_state::{AppState, AuthFetchState, ConfigHealthFetchState};
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

    // Shared config health state — fetched once for admin users and reused by views.
    let is_admin_user = auth::is_admin(&auth_context);
    let mut dismissed_key: Signal<Option<String>> = use_signal(|| None);
    let shared_health = app_state.read().config_health.clone();

    // Derive a stable hash key for the current set of failing check IDs so we
    // can detect when the health status changes and re-show the banner.
    let health_key = shared_health.as_ref().map(|h| {
        h.checks
            .iter()
            .filter(|c| !c.passed)
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>()
            .join(",")
    });

    // Determine whether the global notification bar should be shown.
    let show_health_bar = is_admin_user
        && shared_health
            .as_ref()
            .map(|h| h.total_issues > 0)
            .unwrap_or(false)
        && dismissed_key.read().as_deref() != health_key.as_deref();

    // Fetch health data once when the admin is authenticated.
    use_effect(move || {
        if !is_admin_user {
            let mut state = app_state.write();
            if state.config_health.is_some()
                || !matches!(
                    state.config_health_fetch_state,
                    ConfigHealthFetchState::Idle
                )
            {
                state.config_health = None;
                state.config_health_fetch_state = ConfigHealthFetchState::Idle;
            }
            return;
        }

        let should_fetch = {
            let state = app_state.read();
            state.config_health.is_none()
                && matches!(
                    state.config_health_fetch_state,
                    ConfigHealthFetchState::Idle | ConfigHealthFetchState::Error
                )
        };

        if !should_fetch {
            return;
        }

        app_state.write().config_health_fetch_state = ConfigHealthFetchState::Loading;

        spawn(async move {
            let response = fetch_config_health().await;
            let mut state = app_state.write();
            match response {
                Ok(response) => {
                    state.config_health = Some(response);
                    state.config_health_fetch_state = ConfigHealthFetchState::Loaded;
                }
                Err(_) => {
                    state.config_health = None;
                    state.config_health_fetch_state = ConfigHealthFetchState::Error;
                }
            }
        });
    });

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
                    if show_health_bar {
                        if let Some(ref h) = shared_health {
                            div {
                                class: "px-6 py-4 border-b border-amber-300/35 bg-gradient-to-r from-amber-950/90 via-amber-900/45 to-yellow-950/20 shadow-[inset_0_1px_0_rgba(252,211,77,0.16)]",
                                style: "background: linear-gradient(180deg, rgba(120, 53, 15, 0.34), rgba(120, 53, 15, 0.18)); border-bottom-color: rgba(245, 158, 11, 0.28);",
                                AlertBanner {
                                    severity: AlertSeverity::Warning,
                                    message: format!(
                                        "{} configuration issue{} detected — some pipeline stages may not function.",
                                        h.total_issues,
                                        if h.total_issues == 1 { "" } else { "s" }
                                    ),
                                    action_label: Some("View details on Dashboard".to_string()),
                                    action_url: Some("/".to_string()),
                                    on_dismiss: Some(EventHandler::new(move |_| {
                                        if let Some(key) = health_key.clone() {
                                            dismissed_key.set(Some(key));
                                        }
                                    })),
                                }
                            }
                        }
                    }
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

                if auth::is_admin(&auth_context) {
                    OnboardingCoachPanel {}
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
