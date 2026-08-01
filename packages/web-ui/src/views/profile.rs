//! Profile & Preferences view — user identity, appearance, notifications, and sessions.

use dioxus::prelude::*;

use crate::api::client::logout;
use crate::api::models::{AuthMode, Role, UpdateUserPreferences};
use crate::components::layout::sidebar::{PreferencesContext, SidebarContext};
use crate::components::{Icon, IconName};
use crate::routes::Route;
use crate::state::app_state::AppState;
use crate::state::preferences;
use crate::state::theme;

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
    let save_error = prefs_ctx.save_error;
    let mut logout_error = use_signal(|| None::<String>);

    // Only display identity values supplied by the authenticated context.
    let user_name = auth_context
        .as_ref()
        .and_then(|ctx| ctx.user.as_ref())
        .map(|u| {
            u.display_name
                .clone()
                .unwrap_or_else(|| u.email.split('@').next().unwrap_or_default().to_string())
        });

    let user_email = auth_context
        .as_ref()
        .and_then(|ctx| ctx.user.as_ref())
        .map(|u| u.email.clone());

    let user_name_display = user_name.as_deref().unwrap_or("Unavailable").to_string();
    let user_email_display = user_email.as_deref().unwrap_or("unavailable").to_string();

    let user_initials = user_name
        .as_deref()
        .or(user_email.as_deref())
        .unwrap_or("?")
        .split_whitespace()
        .take(2)
        .filter_map(|word| word.chars().next())
        .collect::<String>()
        .to_uppercase();

    let user_initials = if user_initials.is_empty() {
        "?".to_string()
    } else {
        user_initials
    };

    let user_role =
        auth_context
            .as_ref()
            .and_then(|ctx| ctx.roles.first())
            .map(|role| match role {
                Role::Admin => "admin",
                Role::Operator => "operator",
                Role::Viewer => "viewer",
            });

    let auth_source = auth_context.as_ref().map(|ctx| match ctx.auth_mode {
        AuthMode::Local => "local",
        AuthMode::Oidc => "oidc",
        AuthMode::Dev => "dev",
    });

    let is_oidc = auth_context
        .as_ref()
        .map(|ctx| matches!(ctx.auth_mode, AuthMode::Oidc))
        .unwrap_or(false);

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
                        "{user_name_display}"
                        if let Some(role) = user_role {
                            span {
                                class: "chip chip-critical",
                                style: "font-size: 10px;",
                                "{role}"
                            }
                        }
                    }
                    div {
                        class: "mono",
                        style: "font-size: 12px; color: var(--cf-text-muted); margin-top: 2px;",
                        "{user_email_display}"
                    }
                    div {
                        style: "display: flex; gap: 8px; margin-top: 8px; flex-wrap: wrap;",
                        if let Some(source) = auth_source {
                            span {
                                class: "chip chip-unknown",
                                style: "font-size: 10px;",
                                "{source}"
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
                                    Err(_) => logout_error
                                        .set(Some("Unable to sign out. Please try again.".to_string())),
                                }
                            });
                        },
                        Icon { name: IconName::X, size: 11 }
                        " Sign out"
                    }
                    if let Some(error) = logout_error() {
                        div {
                            class: "help",
                            style: "color: var(--cf-critical); max-width: 180px; text-align: right;",
                            "{error}"
                        }
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
                                theme_pref.set(new_theme);
                                prefs_ctx.save_update.call(UpdateUserPreferences {
                                    theme: Some(preferences::theme_to_preference(new_theme)),
                                    ..UpdateUserPreferences::default()
                                });
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
                                preferences::write_storage(preferences::DENSITY_KEY, &value);
                                prefs_ctx.save_update.call(UpdateUserPreferences {
                                    density: Some(preferences::density_from_storage(Some(&value))),
                                    ..UpdateUserPreferences::default()
                                });
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
                                preferences::write_storage(
                                    preferences::SIDEBAR_COLLAPSED_KEY,
                                    if collapsed { "true" } else { "false" },
                                );
                                prefs_ctx.save_update.call(UpdateUserPreferences {
                                    sidebar_collapsed: Some(collapsed),
                                    ..UpdateUserPreferences::default()
                                });
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
                                preferences::write_storage(preferences::SYSTEMS_VIEW_KEY, &value);
                                prefs_ctx.save_update.call(UpdateUserPreferences {
                                    default_systems_view: Some(preferences::systems_view_from_storage(Some(&value))),
                                    ..UpdateUserPreferences::default()
                                });
                            },
                        }
                    }
                    if let Some(error) = save_error() {
                        div {
                            class: "help",
                            style: "color: var(--cf-critical); margin: 8px 18px 14px;",
                            "{error}"
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
                            if let Some(role) = user_role {
                                span {
                                    class: "chip chip-critical",
                                    style: "font-size: 10px;",
                                    "{role}"
                                }
                            } else {
                                span {
                                    class: "chip chip-unknown",
                                    style: "font-size: 10px;",
                                    "unavailable"
                                }
                            }
                        }

                        // Environments row hidden until API provides scope data
                        // dt { "Environments" }
                        // dd {
                        //     span {
                        //         class: "chip chip-unknown",
                        //         style: "font-size: 10px;",
                        //         "unavailable"
                        //     }
                        // }

                        dt { "Auth source" }
                        dd {
                            if let Some(source) = auth_source {
                                "{source}"
                            } else {
                                "unavailable"
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
