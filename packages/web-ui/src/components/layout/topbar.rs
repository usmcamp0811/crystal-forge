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
use gloo_timers::future::TimeoutFuture;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

#[derive(Clone, Copy)]
pub struct AccountNotificationsContext {
    pub items: Signal<Vec<UserNotificationDto>>,
    pub unread_count: Signal<i64>,
    pub next_cursor: Signal<Option<String>>,
    pub loading: Signal<bool>,
    pub loading_more: Signal<bool>,
    pub error: Signal<Option<String>>,
}

#[derive(Clone, Copy)]
enum NotificationKind {
    Deploy,
    Build,
    Shield,
    Warning,
    Evaluation,
}

#[derive(Clone, Debug, PartialEq)]
enum NotificationTarget {
    Route(Route),
    SystemDeploy(String),
}

fn set_root_attr(name: &str, value: &str) {
    if let Some(document) = web_sys::window().and_then(|w| w.document()) {
        if let Some(root) = document.document_element() {
            let _ = root.set_attribute(name, value);
        }
    }
}

fn focus_topbar_bell() {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            if let Some(element) = document
                .query_selector("[data-testid='topbar-notifications-button']")
                .ok()
                .flatten()
            {
                if let Some(html_element) = element.dyn_ref::<web_sys::HtmlElement>() {
                    let _ = html_element.focus();
                }
            }
        }
    }
}

fn focus_notification_menu() {
    #[cfg(target_arch = "wasm32")]
    if let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| {
            document
                .query_selector("[data-testid='topbar-notifications-panel']")
                .ok()
                .flatten()
        })
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = element.focus();
    }
}

fn focus_notification_item(current_id: Option<uuid::Uuid>, direction: isize) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let Ok(nodes) = document.query_selector_all("[data-notification-id]") else {
            return;
        };
        let items = (0..nodes.length())
            .filter_map(|index| nodes.item(index))
            .filter_map(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
            .collect::<Vec<_>>();
        if items.is_empty() {
            focus_notification_menu();
            return;
        }
        let current = current_id.and_then(|id| {
            items.iter().position(|item| {
                item.get_attribute("data-notification-id")
                    .is_some_and(|value| value == id.to_string())
            })
        });
        let target = notification_focus_index(items.len(), current, direction);
        if let Some(item) = items.get(target) {
            let _ = item.focus();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    let _ = (current_id, direction);
}

fn notification_focus_index(item_count: usize, current: Option<usize>, direction: isize) -> usize {
    if item_count == 0 {
        return 0;
    }
    match (current, direction.is_negative()) {
        (Some(0), true) => item_count - 1,
        (Some(index), true) => index - 1,
        (Some(index), false) => (index + 1) % item_count,
        (None, true) => item_count - 1,
        (None, false) => 0,
    }
}

fn close_notifications(mut notifications_open: Signal<bool>) {
    notifications_open.set(false);
    focus_topbar_bell();
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
        | NotificationCategory::CriticalCves => "var(--cf-policy-red)",
        NotificationCategory::PolicyViolations | NotificationCategory::HeartbeatLost => {
            "var(--cf-policy-amber)"
        }
    }
}

fn notification_target(route: &str) -> Option<NotificationTarget> {
    if let Some(poam_id) = route.strip_prefix("/compliance?poam=") {
        if uuid::Uuid::parse_str(poam_id).is_ok() {
            return Some(NotificationTarget::Route(Route::ComplianceView {
                bundle: String::new(),
                version: String::new(),
                system: String::new(),
                policy: String::new(),
                poam: poam_id.to_string(),
                view: String::new(),
            }));
        }
        return None;
    }

    let (path, query) = route.split_once('?').unwrap_or((route, ""));
    let segments = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if let ["systems", id] = segments.as_slice()
        && uuid::Uuid::parse_str(id).is_ok()
        && query
            .split('&')
            .filter_map(|part| part.split_once('='))
            .any(|(key, value)| key == "tab" && value == "deploy")
    {
        return Some(NotificationTarget::SystemDeploy((*id).to_string()));
    }
    match path {
        "/systems" => Some(NotificationTarget::Route(Route::SystemsView {
            query: String::new(),
        })),
        "/builds" => Some(NotificationTarget::Route(Route::BuildsView {})),
        "/cves" => Some(NotificationTarget::Route(Route::CvesView {})),
        "/evaluations" => Some(NotificationTarget::Route(Route::EvaluationsView {})),
        "/profile" => Some(NotificationTarget::Route(Route::ProfileView {})),
        _ => None,
    }
}

fn open_system_deploy(id: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window
            .location()
            .set_href(&format!("/systems/{id}?tab=deploy"));
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

fn notification_accessible_label(item: &UserNotificationDto) -> String {
    let state = if item.read_at.is_some() {
        "Read"
    } else {
        "Unread"
    };
    let title = item.title.trim_end_matches('.');
    let summary = item.summary.trim_end_matches('.');
    format!(
        "{state} notification. {title}. {summary}. Received {}.",
        item.created_at.to_rfc3339(),
    )
}

fn current_topbar_user_id(app_state: Signal<AppState>) -> Option<String> {
    app_state
        .read()
        .auth
        .as_ref()
        .and_then(|ctx| ctx.user.as_ref())
        .map(|user| user.id.clone())
}

fn current_topbar_auth_generation(app_state: Signal<AppState>) -> u64 {
    app_state.read().auth_generation
}

/// Starts one notification load unless another notification request is in progress.
///
/// Responses update account-scoped signals only while the authenticated user and
/// authentication generation still match the request owner.
pub(crate) fn load_account_notifications(
    mut ctx: AccountNotificationsContext,
    app_state: Signal<AppState>,
    auth_user_id: Option<String>,
    auth_generation: u64,
    cursor: Option<String>,
    append: bool,
) {
    if auth_user_id.is_none() {
        ctx.items.set(Vec::new());
        ctx.unread_count.set(0);
        ctx.next_cursor.set(None);
        ctx.loading.set(false);
        ctx.loading_more.set(false);
        return;
    }
    // CONCURRENCY: Polling replaces the first page while pagination appends an
    // older page. They must not overlap because their responses can otherwise
    // lose or duplicate notification history.
    if *ctx.loading.peek() || *ctx.loading_more.peek() {
        return;
    }
    if append {
        ctx.loading_more.set(true);
    } else {
        ctx.loading.set(true);
    }
    spawn(async move {
        match fetch_user_notifications(Some(10), cursor, false).await {
            Ok(response) => {
                if current_topbar_user_id(app_state) == auth_user_id
                    && current_topbar_auth_generation(app_state) == auth_generation
                {
                    if append {
                        ctx.items.write().extend(response.notifications);
                    } else {
                        ctx.items.set(response.notifications);
                    }
                    ctx.unread_count.set(response.unread_count);
                    ctx.next_cursor.set(response.next_cursor);
                    ctx.error.set(None);
                }
            }
            Err(err) => {
                if current_topbar_user_id(app_state) == auth_user_id
                    && current_topbar_auth_generation(app_state) == auth_generation
                {
                    ctx.error
                        .set(Some(format!("Could not load notifications: {err}")))
                }
            }
        }
        if current_topbar_user_id(app_state) == auth_user_id
            && current_topbar_auth_generation(app_state) == auth_generation
        {
            if append {
                ctx.loading_more.set(false);
            } else {
                ctx.loading.set(false);
            }
        }
    });
}

/// Renders the current page title, account actions, and notification menu.
///
/// Notification state is scoped to the authenticated user and authentication
/// generation. The component discards late responses after account changes and
/// delegates notification persistence and authorization to server APIs.
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
    let auth_generation = app_state.read().auth_generation;

    let sidebar_ctx = use_context::<SidebarContext>();
    let mut is_mobile_drawer_open = sidebar_ctx.is_mobile_drawer_open;
    let mut is_collapsed = sidebar_ctx.is_collapsed;

    let prefs_ctx = use_context::<PreferencesContext>();
    let mut density = prefs_ctx.density;
    let mut default_view = prefs_ctx.default_systems_view;
    let save_error = prefs_ctx.save_error;

    let mut tweaks_open = use_signal(|| false);
    let mut notifications_open = use_signal(|| false);
    let notification_ctx = use_context::<AccountNotificationsContext>();
    let mut notification_items = notification_ctx.items;
    let mut notifications_loading = notification_ctx.loading;
    let mut notifications_loading_more = notification_ctx.loading_more;
    let mut notifications_error = notification_ctx.error;
    let mut unread_count = notification_ctx.unread_count;
    let mut notification_next_cursor = notification_ctx.next_cursor;
    let (crumb_parent, crumb_current) =
        if let Some((parent, current)) = breadcrumb_override.read().clone() {
            (Some(parent), current)
        } else {
            match &current_route {
                Route::SystemDetailView { id, .. } => (Some("Systems".to_string()), id.clone()),
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

    let open_auth_user_id = auth_user_id.clone();
    let more_auth_user_id = auth_user_id.clone();

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
                    "aria-expanded": "{notifications_open()}",
                    "aria-haspopup": "menu",
                    "aria-controls": "topbar-notifications-panel",
                    title: "Notifications",
                    onclick: move |_| {
                        let next_open = !notifications_open();
                        notifications_open.set(next_open);
                        if next_open {
                            load_account_notifications(
                                notification_ctx,
                                app_state,
                                open_auth_user_id.clone(),
                                auth_generation,
                                None,
                                false,
                            );
                            spawn(async move {
                                TimeoutFuture::new(0).await;
                                focus_notification_menu();
                            });
                        }
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
                            aria_hidden: "true",
                            "{unread_count()}"
                        }
                    }
                    span { role: "status", aria_live: "polite", aria_atomic: "true", class: "sr-only", "{unread_count()} unread notifications" }
                }

                if notifications_open() {
                    div {
                        class: "cf-overlay-backdrop",
                        aria_hidden: "true",
                        onclick: move |_| close_notifications(notifications_open),
                    }
                    div {
                        "data-testid": "topbar-notifications-panel",
                        id: "topbar-notifications-panel",
                        class: "notif-panel",
                        role: "menu",
                        aria_label: "Notifications",
                        tabindex: "-1",
                        onkeydown: move |evt| {
                            match evt.key() {
                                Key::Escape => close_notifications(notifications_open),
                                Key::ArrowDown => {
                                    evt.prevent_default();
                                    focus_notification_item(None, 1);
                                }
                                Key::ArrowUp => {
                                    evt.prevent_default();
                                    focus_notification_item(None, -1);
                                }
                                _ => {}
                            }
                        },
                        div {
                            class: "notif-head",
                            strong { "Notifications" }
                            button {
                                "data-testid": "topbar-notifications-mark-read",
                                class: "btn-icon focus-ring",
                                role: "menuitem",
                                aria_label: "Mark all notifications read",
                                title: "Mark all read",
                                style: "padding: 4px;",
                                onclick: move |_| {
                                    let requested_generation = auth_generation;
                                    spawn(async move {
                                        if mark_all_user_notifications_read().await.is_ok() {
                                            if current_topbar_auth_generation(app_state) == requested_generation {
                                                notification_items.write().iter_mut().for_each(|item| {
                                                    item.read_at = Some(Utc::now());
                                                });
                                                unread_count.set(0);
                                                notifications_error.set(None);
                                            }
                                        } else {
                                            if current_topbar_auth_generation(app_state) == requested_generation {
                                                notifications_error.set(Some("Could not mark notifications read".to_string()));
                                            }
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
                            if let Some(error) = notifications_error() {
                                div { role: "alert", class: "help", style: "padding: 10px 12px; color: var(--cf-critical); border-bottom: 1px solid var(--cf-divider);", "{error}" }
                            }
                            if notifications_loading() && notification_items.read().is_empty() {
                                div { role: "status", aria_live: "polite", class: "help", style: "padding: 12px;", "Loading notifications..." }
                            } else if notification_items.read().is_empty() {
                                div { role: "status", class: "help", style: "padding: 12px;", "No notifications yet." }
                            } else {
                                for item in notification_items.read().clone() {
                                    {
                                    let accessible_label = notification_accessible_label(&item);
                                    rsx! { div {
                                        key: "notif-{item.id}",
                                        class: "notif-item-row",
                                        button {
                                        class: if item.read_at.is_none() { "notif-item unread focus-ring" } else { "notif-item focus-ring" },
                                        role: "menuitem",
                                        tabindex: "0",
                                        aria_label: "{accessible_label}",
                                        "data-notification-id": "{item.id}",
                                        "data-testid": "topbar-notification-item-{item.id}",
                                        onkeydown: {
                                            let item_id = item.id;
                                            move |evt| {
                                                if evt.key() == Key::ArrowDown || evt.key() == Key::ArrowUp {
                                                    evt.prevent_default();
                                                    evt.stop_propagation();
                                                    focus_notification_item(Some(item_id), if evt.key() == Key::ArrowDown { 1 } else { -1 });
                                                }
                                            }
                                        },
                                    onclick: {
                                        let nav = nav.clone();
                                        let target = notification_target(&item.route);
                                        let item_id = item.id;
                                        let requested_generation = auth_generation;
                                        move |_| {
                                            close_notifications(notifications_open);
                                            if let Some(target) = target.clone() {
                                                match target {
                                                    NotificationTarget::Route(route) => {
                                                        nav.push(route);
                                                    }
                                                    NotificationTarget::SystemDeploy(id) => open_system_deploy(&id),
                                                }
                                            }
                                            spawn(async move {
                                                if current_topbar_auth_generation(app_state) == requested_generation
                                                {
                                                    match mark_user_notification_read(item_id).await {
                                                        Ok(()) => {
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
                                                            notifications_error.set(None);
                                                        }
                                                        Err(err) => {
                                                            notifications_error.set(Some(format!(
                                                                "Could not mark notification read: {err}"
                                                            )));
                                                        }
                                                    }
                                                }
                                            });
                                        }
                                    },
                                    span {
                                        class: "notif-icon",
                                        aria_hidden: "true",
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
                                        if item.read_at.is_none() {
                                            span { class: "sr-only", "Unread notification." }
                                        }
                                        span {
                                            class: "notif-sub",
                                            "{item.summary}"
                                        }
                                    }
                                    time {
                                        class: "notif-at",
                                        title: "{item.created_at}",
                                        datetime: "{item.created_at.to_rfc3339()}",
                                        "{relative_time(item.created_at)}"
                                    }
                                    }
                                    button {
                                        class: "btn-icon focus-ring notif-dismiss",
                                        role: "menuitem",
                                        r#type: "button",
                                        title: "Dismiss notification",
                                        aria_label: "Dismiss {item.title}",
                                        onkeydown: move |evt| {
                                            if evt.key() == Key::ArrowDown || evt.key() == Key::ArrowUp {
                                                evt.prevent_default();
                                                focus_notification_item(Some(item.id), if evt.key() == Key::ArrowDown { 1 } else { -1 });
                                            } else if evt.key() == Key::Escape {
                                                close_notifications(notifications_open);
                                            }
                                            evt.stop_propagation();
                                        },
                                        onclick: move |evt| {
                                            evt.stop_propagation();
                                            let requested_generation = auth_generation;
                                            spawn(async move {
                                                match dismiss_user_notification(item.id).await {
                                                    Ok(()) => {
                                                        if current_topbar_auth_generation(app_state) == requested_generation {
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
                                                            TimeoutFuture::new(0).await;
                                                            focus_notification_item(None, 1);
                                                        }
                                                    }
                                                    Err(err) => {
                                                        if current_topbar_auth_generation(app_state) == requested_generation {
                                                            notifications_error.set(Some(format!("Could not dismiss notification: {err}")));
                                                        }
                                                    }
                                                }
                                            });
                                        },
                                        "×"
                                    }
                                } }
                                }
                            }
                            }
                        }
                        div {
                            class: "notif-foot",
                            if let Some(cursor) = notification_next_cursor() {
                                button {
                                    "data-testid": "topbar-notifications-load-more",
                                    class: "btn btn-ghost focus-ring xs",
                                    role: "menuitem",
                                    r#type: "button",
                                    disabled: notifications_loading() || notifications_loading_more(),
                                    onclick: move |_| {
                                        load_account_notifications(
                                            notification_ctx,
                                            app_state,
                                            more_auth_user_id.clone(),
                                            auth_generation,
                                            Some(cursor.clone()),
                                            true,
                                        )
                                    },
                                    if notifications_loading_more() { "Loading..." } else { "Load more" }
                                }
                                if notifications_loading_more() { span { role: "status", aria_live: "polite", class: "sr-only", "Loading more notifications." } }
                            }
                            button {
                                "data-testid": "topbar-notifications-settings-button",
                                class: "btn btn-ghost focus-ring xs",
                                role: "menuitem",
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

#[cfg(test)]
mod tests {
    use super::{
        NotificationTarget, notification_accessible_label, notification_focus_index,
        notification_target,
    };
    use crate::api::models::{NotificationCategory, UserNotificationDto};
    use crate::routes::Route;

    #[test]
    fn notification_route_accepts_only_exact_poam_query() {
        let poam_id = "0198f3f0-e8cc-7e5d-b53d-0f31840c8712";
        assert_eq!(
            notification_target(&format!("/compliance?poam={poam_id}")),
            Some(NotificationTarget::Route(Route::ComplianceView {
                bundle: String::new(),
                version: String::new(),
                system: String::new(),
                policy: String::new(),
                poam: poam_id.to_string(),
                view: String::new(),
            }))
        );
        assert_eq!(
            notification_target(&format!("/compliance?poam={poam_id}&view=summary")),
            None
        );
        assert_eq!(notification_target("/compliance?poam=not-a-uuid"), None);
    }

    #[test]
    fn notification_route_preserves_existing_destinations() {
        assert_eq!(
            notification_target("/systems?state=offline"),
            Some(NotificationTarget::Route(Route::SystemsView {
                query: String::new(),
            }))
        );
        assert_eq!(
            notification_target("/builds"),
            Some(NotificationTarget::Route(Route::BuildsView {}))
        );
        assert_eq!(
            notification_target("/cves"),
            Some(NotificationTarget::Route(Route::CvesView {}))
        );
        assert_eq!(
            notification_target("/evaluations"),
            Some(NotificationTarget::Route(Route::EvaluationsView {}))
        );
        assert_eq!(
            notification_target("/profile"),
            Some(NotificationTarget::Route(Route::ProfileView {}))
        );
    }

    #[test]
    fn deployment_notification_parses_exact_system_and_deploy_tab() {
        let id = "26ee295d-7f12-48ae-99b5-2ccf07716782";
        assert_eq!(
            notification_target(&format!("/systems/{id}?notice=pending&tab=deploy")),
            Some(NotificationTarget::SystemDeploy(id.to_string()))
        );
        assert_eq!(
            notification_target("/systems"),
            Some(NotificationTarget::Route(Route::SystemsView {
                query: String::new(),
            }))
        );
        assert_eq!(notification_target("/systems-not-really"), None);
        assert_eq!(
            notification_target(&format!("/systems/{id}?tab=deployment")),
            None
        );
        assert_eq!(notification_target("/systems/not-a-uuid?tab=deploy"), None);
    }

    #[test]
    fn notification_arrow_navigation_enters_and_wraps_the_item_list() {
        assert_eq!(notification_focus_index(3, None, 1), 0);
        assert_eq!(notification_focus_index(3, None, -1), 2);
        assert_eq!(notification_focus_index(3, Some(0), -1), 2);
        assert_eq!(notification_focus_index(3, Some(2), 1), 0);
        assert_eq!(notification_focus_index(3, Some(1), 1), 2);
    }

    #[test]
    fn task433_responsive_notification_preserves_server_text_and_timestamp() {
        let created_at = chrono::DateTime::parse_from_rfc3339("2026-08-31T12:34:56Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let notification = UserNotificationDto {
            id: uuid::Uuid::nil(),
            category: NotificationCategory::PolicyViolations,
            title: "POAM-0433 awaiting verification".into(),
            summary: "Platform Security must re-evaluate the finding.".into(),
            route: "/compliance?poam=00000000-0000-0000-0000-000000000433".into(),
            created_at,
            read_at: None,
        };

        assert_eq!(
            notification_accessible_label(&notification),
            "Unread notification. POAM-0433 awaiting verification. Platform Security must re-evaluate the finding. Received 2026-08-31T12:34:56+00:00."
        );

        let css = include_str!("../../../assets/app.css");
        assert!(css.contains("width: min(360px, calc(100dvw - 16px))"));
        assert!(css.contains("max-height: calc(100dvh - var(--coach-top, 64px) - 16px)"));
    }

    #[test]
    fn task433_narrow_shell_uses_overlay_navigation_and_usable_actions() {
        let css = include_str!("../../../assets/app.css");
        assert!(css.contains("@media (max-width: 767px)"));
        assert!(css.contains("grid-template-columns: minmax(0, 1fr)"));
        assert!(css.contains("@media (min-width: 768px)"));
        assert!(css.contains(".topbar-search {\n    display: none;"));
        assert!(css.contains("flex: 0 0 40px"));
        assert!(
            css.contains(":root[data-theme=\"light\"] .sidebar.cf-sidebar-shell.cf-sidebar-bg")
        );
        assert!(css.contains(":root[data-theme=\"light\"] .cf-mobile-drawer.cf-sidebar-bg"));
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
