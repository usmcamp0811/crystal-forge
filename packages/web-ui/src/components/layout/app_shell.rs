//! Application shell layout with sidebar navigation.

use dioxus::prelude::*;

use crate::api::client::{fetch_classification_config, fetch_config_health};
use crate::api::models::{AuthContext, AuthMode, AuthUser, ClassificationBannerConfig, Role};
use crate::components::layout::TopBar;
use crate::components::layout::sidebar::{MobileDrawer, SidebarContext, SidebarNav};
use crate::components::layout::{
    BannerPlacement, DEV_MODE_BANNER_HEIGHT_PX, DevModeBanner, use_dev_mode_enabled,
};
use crate::components::notifications::{AlertBanner, AlertSeverity};
use crate::components::onboarding::OnboardingCoachPanel;
use crate::routes::Route;
use crate::state::app_state::{AppState, AuthFetchState, ConfigHealthFetchState};
use crate::state::auth;
use crate::theme;

/// Check if UI check mock auth mode is enabled via query param.
/// Only available in debug builds to prevent production auth bypass.
#[cfg(any(debug_assertions, feature = "ui_fixture_mode"))]
fn ui_check_mock_auth_enabled() -> bool {
    web_sys::window()
        .and_then(|w| w.location().search().ok())
        .map(|q| q.contains("ui_check_auth=1"))
        .unwrap_or(false)
}

#[cfg(not(any(debug_assertions, feature = "ui_fixture_mode")))]
fn ui_check_mock_auth_enabled() -> bool {
    false
}

#[cfg(any(debug_assertions, feature = "ui_fixture_mode"))]
fn ui_check_mock_auth_role() -> Role {
    let search = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if search.contains("ui_check_role=viewer") {
        Role::Viewer
    } else if search.contains("ui_check_role=operator") {
        Role::Operator
    } else {
        Role::Admin
    }
}

#[cfg(not(any(debug_assertions, feature = "ui_fixture_mode")))]
fn ui_check_mock_auth_role() -> Role {
    Role::Admin
}

#[cfg(any(debug_assertions, feature = "ui_fixture_mode"))]
fn ui_check_mock_auth_context() -> AuthContext {
    AuthContext {
        is_authenticated: true,
        user: Some(AuthUser {
            id: "ui-check-user".to_string(),
            email: "ui-check@example.com".to_string(),
            display_name: Some("UI Check".to_string()),
        }),
        roles: vec![ui_check_mock_auth_role()],
        auth_mode: AuthMode::Local,
    }
}

fn should_show_admin_denied(route: &Route, auth_context: &Option<AuthContext>) -> bool {
    matches!(
        route,
        Route::AdminView { .. } | Route::CvesView { .. } | Route::ScanningView { .. }
    ) && !auth::is_admin(auth_context)
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

    let sidebar_width = if is_collapsed() { "64px" } else { "240px" };

    let state = app_state.read();
    let auth_fetch_state = state.auth_fetch_state.clone();
    let mut auth_context = state.auth.clone();
    drop(state);

    // In debug builds or ui_fixture_mode, allow mock auth for screenshot tests
    #[cfg(any(debug_assertions, feature = "ui_fixture_mode"))]
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

    // Load classification banner config once on mount (any authenticated user).
    // Stores result in AppState so the Admin card can show an error + retry.
    use_effect(move || {
        let fetch_state = app_state.read().classification_fetch_state.clone();
        // Fetch only when no attempt has been made. A failure remains visible
        // until the Admin card's Retry button resets this state to None.
        if fetch_state.is_some() {
            return;
        }
        spawn(async move {
            match fetch_classification_config().await {
                Ok(config) => {
                    let mut state = app_state.write();
                    state.classification_config = Some(config);
                    state.classification_fetch_state = Some(Ok(()));
                }
                Err(e) => {
                    app_state.write().classification_fetch_state = Some(Err(e.to_string()));
                }
            }
        });
    });

    let classification_config: Option<ClassificationBannerConfig> =
        app_state.read().classification_config.clone();

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

    // Classification is active when the config has been loaded and enabled.
    let classification_enabled = classification_config
        .as_ref()
        .map(|c| c.enabled)
        .unwrap_or(false);
    let dev_mode_enabled = use_dev_mode_enabled()();
    let bottom_banner_space = if classification_enabled || dev_mode_enabled {
        "24px"
    } else {
        "0px"
    };
    let classification_top = classification_config.clone();
    let classification_bottom = classification_config;

    rsx! {
        div {
            class: "min-h-screen {theme::surface::PAGE_BG} {theme::text::PRIMARY} flex flex-col overflow-x-hidden",

            // Top banner stack: the classification banner offsets only when
            // the dev banner is actually visible. Each active fixed banner has
            // a matching in-flow spacer so page content is not obscured.
            TopBannerStack {
                classification: classification_top,
                classification_enabled,
                dev_mode_enabled,
            }
            // ── end top banners ──────────────────────────────────────────────

            div {
                class: "app flex-1 min-h-0 relative",
                style: "--sidebar-w: {sidebar_width};--bottom-banner-space:{bottom_banner_space};",

                SidebarNav {}
                MobileDrawer {}

                div {
                    class: "main",
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
                        class: "content",
                        if should_show_admin_denied(&current_route, &auth_context) {
                            section {
                                class: "max-w-3xl mx-auto rounded-xl border border-amber-500/40 bg-amber-900/20 p-6 space-y-2",
                                h2 { class: "text-xl font-semibold text-amber-100", "Access Denied" }
                                p { class: "text-sm text-amber-200/90", "This page requires an administrator role." }
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

            // ── bottom banner stack ───────────────────────────────────────────
            BottomBannerStack {
                classification: classification_bottom,
                classification_enabled,
                dev_mode_enabled,
            }
        }
    }
}

fn classification_display(
    config: &ClassificationBannerConfig,
) -> (String, &'static str, &'static str) {
    let text = if config.custom_text.trim().is_empty() {
        config.level.clone()
    } else {
        config.custom_text.trim().to_uppercase()
    };
    let (bg, fg) = match config.level.as_str() {
        "CUI" => ("#a78bfa", "#fff"),
        "CONFIDENTIAL" => ("#3b82f6", "#fff"),
        "SECRET" => ("#ef4444", "#fff"),
        "TOP SECRET" => ("#fbbf24", "#000"),
        _ => ("#10b981", "#fff"),
    };
    (text, bg, fg)
}

/// Renders top banners with offsets that match the banners that are actually
/// visible. In-flow spacers matching the active banner count push content down.
#[component]
fn TopBannerStack(
    classification: Option<ClassificationBannerConfig>,
    classification_enabled: bool,
    dev_mode_enabled: bool,
) -> Element {
    let show_classification = classification_enabled;
    let classification_offset = if dev_mode_enabled {
        DEV_MODE_BANNER_HEIGHT_PX
    } else {
        0
    };

    rsx! {
        // DevModeBanner renders its own spacer when active.
        DevModeBanner { placement: BannerPlacement::Top, enabled: dev_mode_enabled }
        if show_classification {
            div { style: "height:24px;flex-shrink:0;" }
        }

        if show_classification {
            if let Some(ref cfg) = classification {
                {
                    let (text, bg, fg) = classification_display(cfg);
                    rsx! {
                        div {
                            role: "note",
                            "aria-label": "Classification banner",
                            style: "position:fixed;top:{classification_offset}px;left:0;right:0;z-index:990;height:24px;display:flex;align-items:center;justify-content:center;font-size:12px;font-weight:700;letter-spacing:0.08em;text-transform:uppercase;background:{bg};color:{fg};pointer-events:none;",
                            "{text}"
                        }
                    }
                }
            }
        }
    }
}

/// Mirrors `TopBannerStack` for the bottom edge.
#[component]
fn BottomBannerStack(
    classification: Option<ClassificationBannerConfig>,
    classification_enabled: bool,
    dev_mode_enabled: bool,
) -> Element {
    let show_classification = classification_enabled;
    let classification_offset = if dev_mode_enabled {
        DEV_MODE_BANNER_HEIGHT_PX
    } else {
        0
    };

    rsx! {
        DevModeBanner { placement: BannerPlacement::Bottom, enabled: dev_mode_enabled }
        if show_classification {
            div { style: "height:24px;flex-shrink:0;" }
        }

        if show_classification {
            if let Some(ref cfg) = classification {
                {
                    let (text, bg, fg) = classification_display(cfg);
                    rsx! {
                        div {
                            role: "note",
                            "aria-label": "Classification banner",
                            style: "position:fixed;bottom:{classification_offset}px;left:0;right:0;z-index:10;height:24px;display:flex;align-items:center;justify-content:center;font-size:12px;font-weight:700;letter-spacing:0.08em;text-transform:uppercase;background:{bg};color:{fg};pointer-events:none;",
                            "{text}"
                        }
                    }
                }
            }
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
    fn cve_route_denied_for_non_admin() {
        let route = Route::CvesView {};
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
    fn cve_route_allowed_for_admin() {
        let route = Route::CvesView {};
        assert!(!should_show_admin_denied(
            &route,
            &auth_context(true, vec![Role::Admin])
        ));
    }

    #[test]
    fn scanning_route_denied_for_non_admin() {
        let route = Route::ScanningView {};
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
    fn scanning_route_allowed_for_admin() {
        let route = Route::ScanningView {};
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
