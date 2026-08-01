//! Top bar layout component.

use crate::api::client::{
    dismiss_user_notification, fetch_user_notifications, mark_all_user_notifications_read,
    mark_user_notification_read,
};
use crate::api::models::{NotificationCategory, UpdateUserPreferences, UserNotificationDto};
use crate::components::layout::sidebar::{PreferencesContext, SidebarContext};
use crate::routes::Route;
use crate::state::app_state::AppState;
use crate::state::preferences;
use crate::state::theme::UiTheme;
use crate::theme;
use chrono::{DateTime, Utc};
use dioxus::prelude::*;

#[derive(Clone, Copy)]
enum NotificationKind {
    Deploy,
    Build,
    Shield,
    Warning,
    Evaluation,
}

fn set_root_attr(name: &str, value: &str) {
    if let Some(document) = web_sys::window().and_then(|w| w.document()) {
        if let Some(root) = document.document_element() {
            let _ = root.set_attribute(name, value);
        }
    }
}

fn notification_kind(category: NotificationCategory) -> NotificationKind {
    match category {
        NotificationCategory::DeployFailures => NotificationKind::Deploy,
        NotificationCategory::BuildFailures => NotificationKind::Build,
        NotificationCategory::CriticalCves => NotificationKind::Shield,
        NotificationCategory::PolicyViolations => NotificationKind::Evaluation,
        NotificationCategory::HeartbeatLost => NotificationKind::Warning,
    }
}

fn notification_color(category: NotificationCategory) -> &'static str {
    match category {
        NotificationCategory::DeployFailures
        | NotificationCategory::BuildFailures
        | NotificationCategory::CriticalCves => "#f87171",
        NotificationCategory::PolicyViolations | NotificationCategory::HeartbeatLost => "#fbbf24",
    }
}

fn notification_route(route: &str) -> Option<Route> {
    if route.starts_with("/systems") {
        Some(Route::SystemsView {})
    } else if route.starts_with("/builds") {
        Some(Route::BuildsView {})
    } else if route.starts_with("/cves") {
        Some(Route::CvesView {})
    } else if route.starts_with("/evaluations") {
        Some(Route::EvaluationsView {})
    } else if route.starts_with("/profile") {
        Some(Route::ProfileView {})
    } else {
        None
    }
}

fn relative_time(timestamp: DateTime<Utc>) -> String {
    let delta = Utc::now().signed_duration_since(timestamp);
    if delta.num_minutes() < 1 {
        "now".to_string()
    } else if delta.num_hours() < 1 {
        format!("{}m ago", delta.num_minutes())
    } else if delta.num_days() < 1 {
        format!("{}h ago", delta.num_hours())
    } else {
        format!("{}d ago", delta.num_days())
    }
}

fn current_topbar_user_id(app_state: Signal<AppState>) -> Option<String> {
    app_state
        .read()
        .auth
        .as_ref()
        .and_then(|ctx| ctx.user.as_ref())
        .map(|user| user.id.clone())
}

/// Header bar displaying the current page title and optional actions.
#[component]
pub fn TopBar(title: String) -> Element {
    let mut ui_theme = use_context::<Signal<UiTheme>>();
    let nav = navigator();
    let current_route = use_route::<Route>();
    let breadcrumb_override = use_context::<Signal<Option<(String, String)>>>();
    let app_state = use_context::<Signal<AppState>>();
    let auth_context = app_state.read().auth.clone();
    let auth_user_id = auth_context
        .as_ref()
        .and_then(|ctx| ctx.user.as_ref())
        .map(|user| user.id.clone());

    let sidebar_ctx = use_context::<SidebarContext>();
    let mut is_mobile_drawer_open = sidebar_ctx.is_mobile_drawer_open;
    let mut is_collapsed = sidebar_ctx.is_collapsed;

    let prefs_ctx = use_context::<PreferencesContext>();
    let mut density = prefs_ctx.density;
    let mut default_view = prefs_ctx.default_systems_view;
    let save_error = prefs_ctx.save_error;

    let mut tweaks_open = use_signal(|| false);
    let mut notifications_open = use_signal(|| false);
    let mut notification_items = use_signal(Vec::<UserNotificationDto>::new);
    let mut notifications_loading = use_signal(|| false);
    let mut notifications_error = use_signal(|| None::<String>);
    let mut unread_count = use_signal(|| 0_i64);
    let (crumb_parent, crumb_current) =
        if let Some((parent, current)) = breadcrumb_override.read().clone() {
            (Some(parent), current)
        } else {
            match &current_route {
                Route::SystemDetailView { id } => (Some("Systems".to_string()), id.clone()),
                Route::EvaluationsCommitView { commit_id } => (
                    Some("Evaluations".to_string()),
                    format!("commit {commit_id}"),
                ),
                _ => (None, title.clone()),
            }
        };

    let toggle_drawer = move |_| {
        is_mobile_drawer_open.set(!is_mobile_drawer_open());
    };

    use_effect(move || {
        let _ = js_sys::eval(
            "(() => { \
                const h = document.querySelector('header'); \
                if (h) { \
                    const b = h.getBoundingClientRect().bottom; \
                    if (b > 0) document.documentElement.style.setProperty('--coach-top', b + 'px'); \
                } \
            })()",
        );
    });

    use_effect(move || {
        let auth_user_id = auth_user_id.clone();
        spawn(async move {
            if auth_user_id.is_none() {
                notification_items.set(Vec::new());
                unread_count.set(0);
                return;
            }
            notifications_loading.set(true);
            match fetch_user_notifications(Some(10), None, false).await {
                Ok(response) => {
                    if current_topbar_user_id(app_state) == auth_user_id {
                        notification_items.set(response.notifications);
                        unread_count.set(response.unread_count);
                        notifications_error.set(None);
                    }
                }
                Err(err) => {
                    if current_topbar_user_id(app_state) == auth_user_id {
                        notifications_error.set(Some(format!("Could not load notifications: {err}")))
                    }
                }
            }
            if current_topbar_user_id(app_state) == auth_user_id {
                notifications_loading.set(false);
            }
        });
    });

    rsx! {
        header {
            class: "topbar",
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

            div {
                class: "breadcrumbs",
                span { "Fleet" }
                span { class: "sep", "/" }
                if let Some(parent) = crumb_parent.clone() {
                    span { "{parent}" }
                    span { class: "sep", "/" }
                }
                span { class: "crumb-current", "{crumb_current}" }
            }

            div {
                class: "topbar-search",
                svg {
                    class: "w-3.5 h-3.5",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    view_box: "0 0 24 24",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        d: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
                    }
                }
                input {
                    class: "input focus-ring w-full",
                    r#type: "search",
                    placeholder: "Search systems, flakes, commits…",
                }
                span {
                    class: "kbd",
                    style: "position: absolute; right: 10px; top: 50%; transform: translateY(-50%);",
                    "⌘K"
                }
            }

            div {
                class: "topbar-notifications-wrap",
                button {
                    "data-testid": "topbar-notifications-button",
                    class: "btn-icon focus-ring topbar-bell",
                    "aria-label": "Notifications ({unread_count()} unread)",
                    title: "Notifications",
                    onclick: move |_| {
                        notifications_open.set(!notifications_open());
                    },
                    svg {
                        class: "w-4 h-4",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        view_box: "0 0 24 24",
                        path {
                            d: "M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"
                        }
                    }
                    if unread_count() > 0 {
                        span {
                            "data-testid": "topbar-notifications-badge",
                            class: "topbar-bell-badge",
                            "{unread_count()}"
                        }
                    }
                }

                if notifications_open() {
                    div {
                        class: "cf-overlay-backdrop",
                        onclick: move |_| notifications_open.set(false),
                    }
                    div {
                        "data-testid": "topbar-notifications-panel",
                        class: "notif-panel",
                        div {
                            class: "notif-head",
                            strong { "Notifications" }
                            button {
                                "data-testid": "topbar-notifications-mark-read",
                                class: "btn-icon focus-ring",
                                title: "Mark all read",
                                style: "padding: 4px;",
                                onclick: move |_| {
                                    spawn(async move {
                                        if mark_all_user_notifications_read().await.is_ok() {
                                            notification_items.write().iter_mut().for_each(|item| {
                                                item.read_at = Some(Utc::now());
                                            });
                                            unread_count.set(0);
                                            notifications_error.set(None);
                                        } else {
                                            notifications_error.set(Some("Could not mark notifications read".to_string()));
                                        }
                                    });
                                },
                                svg {
                                    class: "w-3.5 h-3.5",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    view_box: "0 0 24 24",
                                    path { d: "M5 13l4 4L19 7" }
                                }
                            }
                        }
                        div {
                            class: "notif-list",
                            if notifications_loading() {
                                div { class: "help", style: "padding: 12px;", "Loading notifications..." }
                            } else if let Some(error) = notifications_error() {
                                div { class: "help", style: "padding: 12px; color: var(--cf-critical);", "{error}" }
                            } else if notification_items.read().is_empty() {
                                div { class: "help", style: "padding: 12px;", "No notifications yet." }
                            } else {
                                for item in notification_items.read().clone() {
                                    div {
                                        key: "notif-{item.id}",
                                        class: if item.read_at.is_none() { "notif-item unread focus-ring" } else { "notif-item focus-ring" },
                                        role: "button",
                                        tabindex: "0",
                                    onclick: {
                                        let nav = nav.clone();
                                        let route = notification_route(&item.route);
                                        let item_id = item.id;
                                        move |_| {
                                            let route = route.clone();
                                            spawn(async move {
                                                let _ = mark_user_notification_read(item_id).await;
                                                if let Some(clicked) = notification_items
                                                    .write()
                                                    .iter_mut()
                                                    .find(|candidate| candidate.id == item_id)
                                                {
                                                    if clicked.read_at.is_none() {
                                                        unread_count.set((unread_count() - 1).max(0));
                                                    }
                                                    clicked.read_at = Some(Utc::now());
                                                }
                                                notifications_open.set(false);
                                                if let Some(route) = route.clone() {
                                                    nav.push(route);
                                                }
                                            });
                                        }
                                    },
                                    span {
                                        class: "notif-icon",
                                        style: "color: {notification_color(item.category)}; background: color-mix(in oklab, {notification_color(item.category)} 16%, transparent);",
                                        match notification_kind(item.category) {
                                            NotificationKind::Deploy => rsx!(
                                                svg {
                                                    class: "w-3.5 h-3.5",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    view_box: "0 0 24 24",
                                                    path { d: "M14.7 6.3a1 1 0 000 1.4l1.6 1.6a1 1 0 001.4 0l3.77-3.77a6 6 0 01-7.94 7.94l-6.91 6.91a2.12 2.12 0 01-3-3l6.91-6.91a6 6 0 017.94-7.94l-3.76 3.76z" }
                                                }
                                            ),
                                            NotificationKind::Build => rsx!(
                                                svg {
                                                    class: "w-3.5 h-3.5",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    view_box: "0 0 24 24",
                                                    path { d: "M14.7 6.3a1 1 0 000 1.4l1.6 1.6a1 1 0 001.4 0l3.77-3.77a6 6 0 01-7.94 7.94l-6.91 6.91a2.12 2.12 0 01-3-3l6.91-6.91a6 6 0 017.94-7.94l-3.76 3.76z" }
                                                }
                                            ),
                                            NotificationKind::Shield => rsx!(
                                                svg {
                                                    class: "w-3.5 h-3.5",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    view_box: "0 0 24 24",
                                                    path { d: "M12 3l7 3v6c0 5-3 7.5-7 9-4-1.5-7-4-7-9V6l7-3z" }
                                                }
                                            ),
                                            NotificationKind::Warning => rsx!(
                                                svg {
                                                    class: "w-3.5 h-3.5",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    view_box: "0 0 24 24",
                                                    path { d: "M12 9v4m0 4h.01" }
                                                    path { d: "M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" }
                                                }
                                            ),
                                            NotificationKind::Evaluation => rsx!(
                                                svg {
                                                    class: "w-3.5 h-3.5",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    view_box: "0 0 24 24",
                                                    path { d: "M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2" }
                                                    path { d: "M9 5a2 2 0 002 2h2a2 2 0 002-2" }
                                                    path { d: "M9 12l2 2 4-4" }
                                                }
                                            ),
                                        }
                                    }
                                    span {
                                        style: "min-width: 0; flex: 1;",
                                        span {
                                            class: "notif-title",
                                            "{item.title}"
                                        }
                                        span {
                                            class: "notif-sub",
                                            "{item.summary}"
                                        }
                                    }
                                    span {
                                        class: "notif-at",
                                        title: "{item.created_at}",
                                        "{relative_time(item.created_at)}"
                                    }
                                    button {
                                        class: "btn-icon focus-ring",
                                        title: "Dismiss notification",
                                        onclick: move |evt| {
                                            evt.stop_propagation();
                                            spawn(async move {
                                                match dismiss_user_notification(item.id).await {
                                                    Ok(()) => {
                                                        let was_unread = notification_items
                                                            .read()
                                                            .iter()
                                                            .find(|candidate| candidate.id == item.id)
                                                            .map(|candidate| candidate.read_at.is_none())
                                                            .unwrap_or(false);
                                                        notification_items.write().retain(|candidate| candidate.id != item.id);
                                                        if was_unread {
                                                            unread_count.set((unread_count() - 1).max(0));
                                                        }
                                                    }
                                                    Err(err) => notifications_error.set(Some(format!("Could not dismiss notification: {err}"))),
                                                }
                                            });
                                        },
                                        "×"
                                    }
                                }
                            }
                            }
                        }
                        div {
                            class: "notif-foot",
                            button {
                                "data-testid": "topbar-notifications-settings-button",
                                class: "btn btn-ghost focus-ring xs",
                                r#type: "button",
                                title: "Notification settings",
                                onclick: move |_| {
                                    notifications_open.set(false);
                                    nav.push(Route::ProfileView {});
                                },
                                "Notification settings"
                            }
                        }
                    }
                }
            }

            button {
                class: "btn-icon focus-ring",
                "aria-label": "Toggle theme",
                title: "Toggle theme",
                onclick: move |_| {
                    let next = ui_theme().toggle();
                    ui_theme.set(next);
                    prefs_ctx.save_update.call(UpdateUserPreferences {
                        theme: Some(preferences::theme_to_preference(next)),
                        ..UpdateUserPreferences::default()
                    });
                },
                if ui_theme() == UiTheme::Dark {
                    svg {
                        class: "w-4 h-4",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        view_box: "0 0 24 24",
                        circle { cx: "12", cy: "12", r: "4" }
                        path {
                            d: "M12 2v2m0 16v2M4.93 4.93l1.41 1.41m11.32 11.32l1.41 1.41M2 12h2m16 0h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"
                        }
                    }
                } else {
                    svg {
                        class: "w-4 h-4",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        view_box: "0 0 24 24",
                        path {
                            d: "M21 12.79A9 9 0 1111.21 3 7 7 0 0021 12.79z"
                        }
                    }
                }
            }

            button {
                class: "btn-icon focus-ring",
                "aria-label": "Tweaks",
                title: "Tweaks",
                onclick: move |_| {
                    tweaks_open.set(!tweaks_open());
                },
                svg {
                    class: "w-4 h-4",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    view_box: "0 0 24 24",
                    path {
                        d: "M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4"
                    }
                }
            }

            if tweaks_open() {
                div {
                    style: "position: fixed; inset: 0; z-index: 49;",
                    onclick: move |_| tweaks_open.set(false),
                }
                div {
                    class: "cf-tweaks-menu",
                    div {
                        class: "cf-tweaks-head",
                        strong { "Tweaks" }
                        button {
                            class: "btn-icon focus-ring",
                            "aria-label": "Close tweaks",
                            onclick: move |_| tweaks_open.set(false),
                            svg {
                                class: "w-3.5 h-3.5",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                view_box: "0 0 24 24",
                                path { d: "M6 6l12 12M18 6L6 18" }
                            }
                        }
                    }
                    div {
                        class: "cf-tweaks-body",
                        TweakRow {
                            label: "Theme",
                            options: vec![("dark", "Dark"), ("light", "Light")],
                            value: ui_theme().as_attr().to_string(),
                            on_change: move |value: String| {
                                let next = if value == "light" { UiTheme::Light } else { UiTheme::Dark };
                                ui_theme.set(next);
                                prefs_ctx.save_update.call(UpdateUserPreferences {
                                    theme: Some(preferences::theme_to_preference(next)),
                                    ..UpdateUserPreferences::default()
                                });
                            }
                        }
                        TweakRow {
                            label: "Density",
                            options: vec![("comfortable", "Comfort"), ("compact", "Compact")],
                            value: density(),
                            on_change: move |value: String| {
                                density.set(value.clone());
                                preferences::write_storage(preferences::DENSITY_KEY, &value);
                                set_root_attr("data-density", &value);
                                prefs_ctx.save_update.call(UpdateUserPreferences {
                                    density: Some(preferences::density_from_storage(Some(&value))),
                                    ..UpdateUserPreferences::default()
                                });
                            }
                        }
                        TweakRow {
                            label: "Default view",
                            options: vec![("cards", "Cards"), ("table", "Table")],
                            value: default_view(),
                            on_change: move |value: String| {
                                default_view.set(value.clone());
                                preferences::write_storage(preferences::SYSTEMS_VIEW_KEY, &value);
                                prefs_ctx.save_update.call(UpdateUserPreferences {
                                    default_systems_view: Some(preferences::systems_view_from_storage(Some(&value))),
                                    ..UpdateUserPreferences::default()
                                });
                            }
                        }
                        TweakRow {
                            label: "Sidebar",
                            options: vec![("full", "Full"), ("rail", "Rail")],
                            value: if is_collapsed() { "rail".to_string() } else { "full".to_string() },
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
                            }
                        }
                        if let Some(error) = save_error() {
                            div {
                                class: "help",
                                style: "color: var(--cf-critical); margin-top: 8px;",
                                "{error}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TweakRow(
    label: String,
    options: Vec<(&'static str, &'static str)>,
    value: String,
    on_change: EventHandler<String>,
) -> Element {
    rsx! {
        div {
            class: "cf-tweaks-row",
            label { "{label}" }
            div {
                class: "cf-tweaks-opts",
                for (option_value, option_label) in options {
                    button {
                        class: if value == option_value { "active" } else { "" },
                        onclick: move |_| on_change.call(option_value.to_string()),
                        "{option_label}"
                    }
                }
            }
        }
    }
}
