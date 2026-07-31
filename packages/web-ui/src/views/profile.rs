//! Profile & Preferences view — user identity, appearance, notifications, and sessions.

use dioxus::prelude::*;

use crate::api::client::logout;
use crate::api::models::{AuthMode, Role};
use crate::components::layout::sidebar::{PreferencesContext, SidebarContext};
use crate::components::{Icon, IconName};
use crate::routes::Route;
use crate::state::app_state::AppState;
use crate::state::{preferences, theme};

// Storage keys matching existing application preferences
const DENSITY_KEY: &str = "cf.ui.density";
const SYSTEMS_VIEW_KEY: &str = "crystal_forge.systems.view";
const SIDEBAR_COLLAPSED_KEY: &str = "cf-sidebar-collapsed";

/// Mock session data structure.
#[derive(Clone, Debug)]
struct Session {
    device: &'static str,
    ip: &'static str,
    at: &'static str,
    current: bool,
}

const MOCK_SESSIONS: &[Session] = &[
    Session {
        device: "MacBook Pro · Chrome",
        ip: "10.2.4.18",
        at: "current session",
        current: true,
    },
    Session {
        device: "iPhone · Safari",
        ip: "10.5.2.7",
        at: "2h ago",
        current: false,
    },
];

/// Helper to store preference in localStorage.
#[cfg(target_arch = "wasm32")]
fn store_pref(key: &str, value: &str) {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            let _ = storage.set_item(key, value);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn store_pref(_key: &str, _value: &str) {}

#[component]
pub fn ProfileView() -> Element {
    let mut app_state = use_context::<Signal<AppState>>();
    let auth_context = app_state.read().auth.clone();
    let nav = navigator();

    // Use shared global theme signal from root
    let mut theme_pref = use_context::<Signal<theme::UiTheme>>();

    // Use shared sidebar context
    let sidebar_ctx = use_context::<SidebarContext>();
    let mut is_collapsed = sidebar_ctx.is_collapsed;

    // Use shared preferences context
    let prefs_ctx = use_context::<PreferencesContext>();
    let mut density = prefs_ctx.density;
    let mut default_view = prefs_ctx.default_systems_view;

    // Notification preferences
    let mut notif = use_signal(|| preferences::NotificationPreferences::load());

    // User identity data from auth context
    let user_name = auth_context
        .as_ref()
        .and_then(|ctx| ctx.user.as_ref())
        .map(|u| {
            u.display_name
                .clone()
                .unwrap_or_else(|| u.email.split('@').next().unwrap_or("User").to_string())
        })
        .unwrap_or_else(|| "User".to_string());

    let user_email = auth_context
        .as_ref()
        .and_then(|ctx| ctx.user.as_ref())
        .map(|u| u.email.clone())
        .unwrap_or_else(|| "user@example.com".to_string());

    let user_initials = user_name
        .split_whitespace()
        .take(2)
        .filter_map(|word| word.chars().next())
        .collect::<String>()
        .to_uppercase();

    let user_initials = if user_initials.is_empty() {
        "U".to_string()
    } else {
        user_initials
    };

    let user_role = auth_context
        .as_ref()
        .and_then(|ctx| ctx.roles.first())
        .cloned()
        .unwrap_or(Role::Viewer);

    let user_role_str = match user_role {
        Role::Admin => "admin",
        Role::Operator => "operator",
        Role::Viewer => "viewer",
    };

    let auth_source = auth_context
        .as_ref()
        .map(|ctx| match ctx.auth_mode {
            AuthMode::Local => "local",
            AuthMode::Oidc => "oidc",
            AuthMode::Dev => "dev",
        })
        .unwrap_or("local");

    let is_oidc = auth_context
        .as_ref()
        .map(|ctx| matches!(ctx.auth_mode, AuthMode::Oidc))
        .unwrap_or(false);

    // Mock data - TODO: Replace with actual API data when available
    let mock_org: Option<&str> = None; // Would come from OIDC claims
    let mock_groups: Vec<&str> = vec![]; // Would come from OIDC claims
    let mock_groups_str = if mock_groups.is_empty() {
        String::new()
    } else {
        mock_groups.join(", ")
    };
    let mock_mfa = false; // Would come from auth context
    let mock_environments: Vec<&str> = vec![]; // Would come from user permissions
    let mock_joined = "Jan 2026"; // Would come from user record
    let mock_last_login = "2m ago"; // Would come from auth context

    // No separate handlers needed - will use inline closures

    rsx! {
        div {
            class: "flex flex-col gap-4",

            // Page header
            div {
                class: "page-head",
                div {
                    h1 { class: "page-title", "Profile & Preferences" }
                    p { class: "page-subtitle", "Personal settings for your Crystal Forge account" }
                }
            }

            // Identity card
            div {
                class: "card",
                style: "padding: 20px; display: flex; gap: 18px; align-items: center; flex-wrap: wrap;",

                // Avatar
                div {
                    style: "width: 64px; height: 64px; border-radius: 99px; background: linear-gradient(135deg,#f472b6,#6366f1); display: grid; place-items: center; color: #fff; font-size: 22px; font-weight: 700; flex-shrink: 0;",
                    "{user_initials}"
                }

                // User info
                div {
                    style: "min-width: 0; flex: 1;",
                    div {
                        style: "font-size: 18px; font-weight: 700; display: flex; align-items: center; gap: 10px;",
                        "{user_name}"
                        span {
                            class: "chip chip-critical",
                            style: "font-size: 10px;",
                            "{user_role_str}"
                        }
                    }
                    div {
                        class: "mono",
                        style: "font-size: 12px; color: var(--cf-text-muted); margin-top: 2px;",
                        "{user_email}"
                    }
                    div {
                        style: "display: flex; gap: 8px; margin-top: 8px; flex-wrap: wrap;",
                        span {
                            class: "chip chip-unknown",
                            style: "font-size: 10px;",
                            "{auth_source}"
                        }
                        // Only show organization for OIDC when available
                        if let Some(org) = mock_org {
                            span {
                                class: "chip chip-info",
                                style: "font-size: 10px;",
                                "{org}"
                            }
                        }
                        // Only show groups when available
                        for group in &mock_groups {
                            span {
                                class: "chip chip-unknown mono",
                                style: "font-size: 10px;",
                                "{group}"
                            }
                        }
                        // Only show MFA when actually enabled
                        if mock_mfa {
                            span {
                                class: "chip chip-healthy",
                                style: "font-size: 10px;",
                                "MFA on"
                            }
                        }
                    }
                }

                // Action buttons
                div {
                    style: "display: flex; flex-direction: column; gap: 6px; align-items: flex-end;",
                    button {
                        class: "btn btn-ghost focus-ring xs",
                        disabled: true,
                        title: "Password change not yet implemented",
                        Icon { name: IconName::Key, size: 11 }
                        " Change password"
                    }
                    button {
                        class: "btn btn-ghost focus-ring xs",
                        onclick: move |_| {
                            spawn(async move {
                                match logout().await {
                                    Ok(()) => {
                                        // Clear auth state before navigating
                                        let mut state = app_state.write();
                                        state.auth = None;
                                        state.auth_fetch_state = crate::state::app_state::AuthFetchState::Loaded;
                                        drop(state);
                                        nav.replace(Route::LoginView {});
                                    }
                                    Err(_e) => {
                                        // TODO: Display error to user instead of silently failing
                                        #[cfg(target_arch = "wasm32")]
                                        {
                                            if let Some(console) = web_sys::window().and_then(|w| Some(w.console())) {
                                                console.error_1(&"Logout failed".into());
                                            }
                                        }
                                    }
                                }
                            });
                        },
                        Icon { name: IconName::X, size: 11 }
                        " Sign out"
                    }
                }
            }

            // Two-column grid for preferences
            div {
                style: "display: grid; grid-template-columns: 1fr 1fr; gap: 16px; align-items: start;",

                // Appearance card
                div {
                    class: "card",
                    style: "padding: 8px 18px 14px;",
                    h3 {
                        style: "font-size: 13px; font-weight: 600; margin: 14px 0 4px;",
                        "Appearance"
                    }

                    PrefRow {
                        title: "Theme",
                        desc: "Dark is optimized for long operational use.",
                        SegmentedControl {
                            value: theme_pref(),
                            options: vec![theme::UiTheme::Dark, theme::UiTheme::Light],
                            on_change: move |new_theme| {
                                // Only set the signal - root effect handles apply & persist
                                theme_pref.set(new_theme);
                            },
                        }
                    }

                    PrefRow {
                        title: "Density",
                        desc: "Compact fits more rows per screen.",
                        SegmentedControlString {
                            value: density(),
                            options: vec![("comfortable", "Comfort"), ("compact", "Compact")],
                            on_change: move |value: String| {
                                density.set(value.clone());
                                store_pref(DENSITY_KEY, &value);
                                // Note: set_root_attr handled by AppShell use_effect
                            },
                        }
                    }

                    PrefRow {
                        title: "Sidebar",
                        desc: "Rail collapses the sidebar to icons.",
                        SegmentedControlString {
                            value: if is_collapsed() { "rail".to_string() } else { "full".to_string() },
                            options: vec![("full", "Full"), ("rail", "Rail")],
                            on_change: move |value: String| {
                                let collapsed = value == "rail";
                                is_collapsed.set(collapsed);
                                store_pref(SIDEBAR_COLLAPSED_KEY, if collapsed { "true" } else { "false" });
                            },
                        }
                    }

                    PrefRow {
                        title: "Default systems view",
                        desc: "Cards or table when opening Systems.",
                        SegmentedControlString {
                            value: default_view(),
                            options: vec![("cards", "Cards"), ("table", "Table")],
                            on_change: move |value: String| {
                                default_view.set(value.clone());
                                store_pref(SYSTEMS_VIEW_KEY, &value);
                            },
                        }
                    }
                }

                // Notifications card - disabled until backend integration
                div {
                    class: "card",
                    style: "padding: 8px 18px 14px; opacity: 0.6; pointer-events: none;",
                    h3 {
                        style: "font-size: 13px; font-weight: 600; margin: 14px 0 4px;",
                        "Notifications"
                    }
                    div {
                        class: "help",
                        style: "margin: 12px 0;",
                        "Notification preferences will be available once backend integration is complete."
                    }
                }

                // Access summary card
                div {
                    class: "card",
                    style: "padding: 18px;",
                    h3 {
                        style: "font-size: 13px; font-weight: 600; margin: 0 0 12px;",
                        "Your access"
                    }

                    dl {
                        class: "kv-grid",
                        dt { "Role" }
                        dd {
                            span {
                                class: "chip chip-critical",
                                style: "font-size: 10px;",
                                "{user_role_str}"
                            }
                        }

                        dt { "Environments" }
                        dd {
                            if mock_environments.is_empty() {
                                span {
                                    class: "chip chip-unknown",
                                    style: "font-size: 10px;",
                                    "none assigned"
                                }
                            } else {
                                for env in &mock_environments {
                                    span {
                                        class: "chip chip-info",
                                        style: "font-size: 10px; margin-right: 4px;",
                                        "{env}"
                                    }
                                }
                            }
                        }

                        dt { "Auth source" }
                        dd {
                            if !mock_groups_str.is_empty() {
                                "{auth_source} · {mock_groups_str}"
                            } else {
                                "{auth_source}"
                            }
                        }

                        // Member since and Last login hidden until real data available
                        // dt { "Member since" }
                        // dd { span { class: "chip chip-unknown", style: "font-size: 10px;", "unavailable" } }

                        // dt { "Last login" }
                        // dd { span { class: "chip chip-unknown", style: "font-size: 10px;", "unavailable" } }
                    }

                    if is_oidc {
                        div {
                            class: "help",
                            style: "margin-top: 10px;",
                            "Role and environment scope come from your IdP groups. Contact an admin to change them."
                        }
                    }
                }

                // Active sessions card - disabled until session management implemented
                div {
                    class: "card",
                    style: "padding: 18px; opacity: 0.6;",
                    h3 {
                        style: "font-size: 13px; font-weight: 600; margin: 0 0 12px;",
                        "Active sessions"
                    }
                    div {
                        class: "help",
                        "Session management will be available in a future release."
                    }
                }
            }
        }
    }
}

// ============================================================================
// UI Components
// ============================================================================

#[component]
fn PrefRow(title: String, desc: Option<String>, children: Element) -> Element {
    rsx! {
        div {
            style: "display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 14px 0; border-bottom: 1px solid var(--cf-divider);",
            div {
                style: "min-width: 0;",
                div {
                    style: "font-size: 13px; font-weight: 600;",
                    "{title}"
                }
                if let Some(d) = desc {
                    div {
                        style: "font-size: 11px; color: var(--cf-text-muted); margin-top: 2px;",
                        "{d}"
                    }
                }
            }
            div {
                style: "flex-shrink: 0;",
                {children}
            }
        }
    }
}

trait PreferenceValue: Clone + Copy + PartialEq {
    fn label(self) -> &'static str;
}

impl PreferenceValue for theme::UiTheme {
    fn label(self) -> &'static str {
        theme::UiTheme::label(self)
    }
}

impl PreferenceValue for preferences::Density {
    fn label(self) -> &'static str {
        preferences::Density::label(self)
    }
}

impl PreferenceValue for preferences::SidebarMode {
    fn label(self) -> &'static str {
        preferences::SidebarMode::label(self)
    }
}

impl PreferenceValue for preferences::DefaultView {
    fn label(self) -> &'static str {
        preferences::DefaultView::label(self)
    }
}

impl PreferenceValue for preferences::NotificationChannel {
    fn label(self) -> &'static str {
        preferences::NotificationChannel::label(self)
    }
}

#[component]
fn SegmentedControl<T: PreferenceValue + 'static>(
    value: T,
    options: Vec<T>,
    on_change: EventHandler<T>,
) -> Element {
    rsx! {
        div {
            class: "seg",
            style: "width: fit-content;",
            for opt in options {
                button {
                    class: if value == opt { "active" } else { "" },
                    onclick: move |_| on_change.call(opt),
                    "{opt.label()}"
                }
            }
        }
    }
}

#[component]
fn SegmentedControlString(
    value: String,
    options: Vec<(&'static str, &'static str)>,
    on_change: EventHandler<String>,
) -> Element {
    rsx! {
        div {
            class: "seg",
            style: "width: fit-content;",
            for (opt_value, opt_label) in options {
                button {
                    class: if value == opt_value { "active" } else { "" },
                    onclick: move |_| on_change.call(opt_value.to_string()),
                    "{opt_label}"
                }
            }
        }
    }
}

#[component]
fn Toggle(on: bool, on_change: EventHandler<bool>) -> Element {
    rsx! {
        label {
            style: "display: inline-flex; cursor: pointer;",
            input {
                r#type: "checkbox",
                checked: on,
                oninput: move |evt| on_change.call(evt.checked()),
                style: "accent-color: var(--cf-brand-purple); width: 16px; height: 16px;",
            }
        }
    }
}
