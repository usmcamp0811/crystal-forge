//! Profile & Preferences view — user identity, appearance, notifications, and sessions.

use dioxus::prelude::*;

use crate::api::models::{AuthMode, Role};
use crate::components::{Icon, IconName};
use crate::state::app_state::AppState;
use crate::state::{preferences, theme};

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

#[component]
pub fn ProfileView() -> Element {
    let app_state = use_context::<Signal<AppState>>();
    let auth_context = app_state.read().auth.clone();

    // Theme state (already managed globally)
    let mut theme_pref = use_signal(|| theme::UiTheme::load());

    // Appearance preferences
    let mut density = use_signal(|| preferences::Density::load());
    let mut sidebar_mode = use_signal(|| preferences::SidebarMode::load());
    let mut default_view = use_signal(|| preferences::DefaultView::load());

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

    // Mock data for OIDC groups, org, MFA
    let mock_org = "acme-prod";
    let mock_groups: Vec<&str> = vec!["cf-admins"];
    let mock_groups_str = mock_groups.join(", ");
    let mock_mfa = true;
    let mock_joined = "Jan 2026";
    let mock_last_login = "2m ago";

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
                        span {
                            class: "chip chip-info",
                            style: "font-size: 10px;",
                            "{mock_org}"
                        }
                        for group in mock_groups {
                            span {
                                class: "chip chip-unknown mono",
                                style: "font-size: 10px;",
                                "{group}"
                            }
                        }
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
                        Icon { name: IconName::Key, size: 11 }
                        " Change password"
                    }
                    button {
                        class: "btn btn-ghost focus-ring xs",
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
                                theme_pref.set(new_theme);
                                theme::apply(new_theme);
                                theme::persist(new_theme);
                            },
                        }
                    }

                    PrefRow {
                        title: "Density",
                        desc: "Compact fits more rows per screen.",
                        SegmentedControl {
                            value: density(),
                            options: vec![preferences::Density::Comfortable, preferences::Density::Compact],
                            on_change: move |new_density| {
                                density.set(new_density);
                                new_density.persist();
                            },
                        }
                    }

                    PrefRow {
                        title: "Sidebar",
                        desc: "Rail collapses the sidebar to icons.",
                        SegmentedControl {
                            value: sidebar_mode(),
                            options: vec![preferences::SidebarMode::Full, preferences::SidebarMode::Rail],
                            on_change: move |new_mode| {
                                sidebar_mode.set(new_mode);
                                new_mode.persist();
                            },
                        }
                    }

                    PrefRow {
                        title: "Default systems view",
                        desc: "Cards or table when opening Systems.",
                        SegmentedControl {
                            value: default_view(),
                            options: vec![preferences::DefaultView::Cards, preferences::DefaultView::Table],
                            on_change: move |new_view| {
                                default_view.set(new_view);
                                new_view.persist();
                            },
                        }
                    }
                }

                // Notifications card
                div {
                    class: "card",
                    style: "padding: 8px 18px 14px;",
                    h3 {
                        style: "font-size: 13px; font-weight: 600; margin: 14px 0 4px;",
                        "Notifications"
                    }

                    PrefRow {
                        title: "Deploy failures",
                        Toggle {
                            on: notif().deploy_failed,
                            on_change: move |value| {
                                let mut n = notif();
                                n.deploy_failed = value;
                                n.persist();
                                notif.set(n);
                            },
                        }
                    }

                    PrefRow {
                        title: "Build failures",
                        Toggle {
                            on: notif().build_failed,
                            on_change: move |value| {
                                let mut n = notif();
                                n.build_failed = value;
                                n.persist();
                                notif.set(n);
                            },
                        }
                    }

                    PrefRow {
                        title: "New critical CVEs",
                        Toggle {
                            on: notif().critical_cve,
                            on_change: move |value| {
                                let mut n = notif();
                                n.critical_cve = value;
                                n.persist();
                                notif.set(n);
                            },
                        }
                    }

                    PrefRow {
                        title: "Policy violations",
                        Toggle {
                            on: notif().policy_fail,
                            on_change: move |value| {
                                let mut n = notif();
                                n.policy_fail = value;
                                n.persist();
                                notif.set(n);
                            },
                        }
                    }

                    PrefRow {
                        title: "Heartbeat lost",
                        Toggle {
                            on: notif().heartbeat_lost,
                            on_change: move |value| {
                                let mut n = notif();
                                n.heartbeat_lost = value;
                                n.persist();
                                notif.set(n);
                            },
                        }
                    }

                    PrefRow {
                        title: "Weekly digest email",
                        Toggle {
                            on: notif().weekly_digest,
                            on_change: move |value| {
                                let mut n = notif();
                                n.weekly_digest = value;
                                n.persist();
                                notif.set(n);
                            },
                        }
                    }

                    PrefRow {
                        title: "Delivery",
                        desc: "Where alerts are sent.",
                        SegmentedControl {
                            value: notif().channel,
                            options: vec![
                                preferences::NotificationChannel::InApp,
                                preferences::NotificationChannel::Email,
                                preferences::NotificationChannel::Both,
                            ],
                            on_change: move |new_channel| {
                                let mut n = notif();
                                n.channel = new_channel;
                                n.persist();
                                notif.set(n);
                            },
                        }
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
                            span {
                                class: "chip chip-info",
                                style: "font-size: 10px;",
                                "all"
                            }
                        }

                        dt { "Auth source" }
                        dd { "{auth_source} · {mock_groups_str}" }

                        dt { "Member since" }
                        dd { "{mock_joined}" }

                        dt { "Last login" }
                        dd { "{mock_last_login}" }
                    }

                    div {
                        class: "help",
                        style: "margin-top: 10px;",
                        "Role and environment scope come from your IdP groups. Contact an admin to change them."
                    }
                }

                // Active sessions card
                div {
                    class: "card",
                    style: "padding: 18px;",
                    h3 {
                        style: "font-size: 13px; font-weight: 600; margin: 0 0 12px;",
                        "Active sessions"
                    }

                    div {
                        style: "display: flex; flex-direction: column; gap: 8px;",
                        for session in MOCK_SESSIONS {
                            div {
                                style: "display: flex; align-items: center; gap: 10px; padding: 8px 10px; background: var(--cf-subtle-bg); border-radius: 8px; font-size: 12px;",
                                Icon { name: IconName::Server, size: 13 }
                                div {
                                    style: "flex: 1; min-width: 0;",
                                    div {
                                        style: "font-weight: 600;",
                                        "{session.device}"
                                    }
                                    div {
                                        class: "mono",
                                        style: "font-size: 10px; color: var(--cf-text-muted);",
                                        "{session.ip} · {session.at}"
                                    }
                                }
                                if session.current {
                                    span {
                                        class: "chip chip-healthy",
                                        style: "font-size: 9px;",
                                        "this device"
                                    }
                                } else {
                                    button {
                                        class: "btn btn-ghost focus-ring xs",
                                        "Revoke"
                                    }
                                }
                            }
                        }
                    }

                    button {
                        class: "btn btn-ghost focus-ring",
                        style: "margin-top: 12px; color: #fbbf24; border-color: rgba(251,191,36,0.3);",
                        Icon { name: IconName::Warn, size: 12 }
                        " Sign out everywhere"
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
