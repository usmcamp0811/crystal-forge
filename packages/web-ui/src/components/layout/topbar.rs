//! Top bar layout component.

use crate::components::layout::sidebar::SidebarContext;
use crate::routes::Route;
use crate::state::app_state::AppState;
use crate::state::auth;
use crate::state::theme::UiTheme;
use crate::theme;
use dioxus::prelude::*;

const DENSITY_KEY: &str = "cf.ui.density";
const SYSTEMS_VIEW_KEY: &str = "crystal_forge.systems.view";

#[derive(Clone)]
struct NotificationItem {
    id: u8,
    title: &'static str,
    subtitle: &'static str,
    age: &'static str,
    color: &'static str,
    unread: bool,
    route: Option<Route>,
    kind: NotificationKind,
}

#[derive(Clone, Copy)]
enum NotificationKind {
    Deploy,
    Build,
    Shield,
    Warning,
    Evaluation,
}

fn admin_only_route(route: &Option<Route>) -> bool {
    matches!(
        route,
        Some(Route::CvesView { .. } | Route::ScanningView { .. } | Route::AdminView { .. })
    )
}

fn visible_notifications(is_admin_user: bool) -> Vec<NotificationItem> {
    notifications()
        .into_iter()
        .filter(|item| is_admin_user || !admin_only_route(&item.route))
        .collect()
}

fn load_pref(key: &str, default: &str) -> String {
    web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
        .and_then(|storage| storage.get_item(key).ok())
        .flatten()
        .unwrap_or_else(|| default.to_string())
}

fn store_pref(key: &str, value: &str) {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    {
        let _ = storage.set_item(key, value);
    }
}

fn set_root_attr(name: &str, value: &str) {
    if let Some(document) = web_sys::window().and_then(|w| w.document()) {
        if let Some(root) = document.document_element() {
            let _ = root.set_attribute(name, value);
        }
    }
}

fn notifications() -> Vec<NotificationItem> {
    vec![
        NotificationItem {
            id: 1,
            title: "3 systems awaiting deploy approval",
            subtitle: "production · manual policy",
            age: "2m ago",
            color: "#fbbf24",
            unread: true,
            route: Some(Route::SystemsView {}),
            kind: NotificationKind::Deploy,
        },
        NotificationItem {
            id: 2,
            title: "Build failed: openssl-3.3.2",
            subtitle: "hydra-02 · attempt 3",
            age: "12m ago",
            color: "#f87171",
            unread: true,
            route: Some(Route::BuildsView {}),
            kind: NotificationKind::Build,
        },
        NotificationItem {
            id: 3,
            title: "New critical CVE: CVE-2026-31822",
            subtitle: "affects 6 systems · openssl",
            age: "38m ago",
            color: "#f87171",
            unread: true,
            route: Some(Route::CvesView {}),
            kind: NotificationKind::Shield,
        },
        NotificationItem {
            id: 4,
            title: "Heartbeat lost: edge-fra-01",
            subtitle: "no signal for 6h",
            age: "1h ago",
            color: "#fbbf24",
            unread: false,
            route: Some(Route::SystemsView {}),
            kind: NotificationKind::Warning,
        },
        NotificationItem {
            id: 5,
            title: "Eval complete: infrastructure@a3f8c12",
            subtitle: "12 systems · all policies passed",
            age: "2h ago",
            color: "#34d399",
            unread: false,
            route: Some(Route::EvaluationsView {}),
            kind: NotificationKind::Evaluation,
        },
    ]
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
    let is_admin_user = auth::is_admin(&auth_context);

    let sidebar_ctx = use_context::<SidebarContext>();
    let mut is_mobile_drawer_open = sidebar_ctx.is_mobile_drawer_open;
    let mut is_collapsed = sidebar_ctx.is_collapsed;
    let mut tweaks_open = use_signal(|| false);
    let mut notifications_open = use_signal(|| false);
    let mut density = use_signal(|| load_pref(DENSITY_KEY, "comfortable"));
    let mut default_view = use_signal(|| load_pref(SYSTEMS_VIEW_KEY, "cards"));
    let mut notification_items = use_signal(|| visible_notifications(is_admin_user));
    let unread_count = notification_items
        .read()
        .iter()
        .filter(|item| item.unread)
        .count();
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
        set_root_attr("data-density", &density());
    });

    use_effect(move || {
        notification_items.set(visible_notifications(is_admin_user));
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
                    "aria-label": "Notifications",
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
                    if unread_count > 0 {
                        span {
                            "data-testid": "topbar-notifications-badge",
                            class: "topbar-bell-badge",
                            "{unread_count}"
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
                                    notification_items.write().iter_mut().for_each(|item| {
                                        item.unread = false;
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
                            for item in notification_items.read().clone() {
                                button {
                                    key: "notif-{item.id}",
                                    class: if item.unread { "notif-item unread focus-ring" } else { "notif-item focus-ring" },
                                    onclick: {
                                        let nav = nav.clone();
                                        let route = item.route.clone();
                                        let item_id = item.id;
                                        move |_| {
                                            if let Some(clicked) = notification_items
                                                .write()
                                                .iter_mut()
                                                .find(|candidate| candidate.id == item_id)
                                            {
                                                clicked.unread = false;
                                            }
                                            notifications_open.set(false);
                                            if let Some(route) = route.clone() {
                                                nav.push(route);
                                            }
                                        }
                                    },
                                    span {
                                        class: "notif-icon",
                                        style: "color: {item.color}; background: color-mix(in oklab, {item.color} 16%, transparent);",
                                        match item.kind {
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
                                            "{item.subtitle}"
                                        }
                                    }
                                    span {
                                        class: "notif-at",
                                        "{item.age}"
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
                                    tweaks_open.set(true);
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
                            }
                        }
                        TweakRow {
                            label: "Density",
                            options: vec![("comfortable", "Comfort"), ("compact", "Compact")],
                            value: density(),
                            on_change: move |value: String| {
                                density.set(value.clone());
                                store_pref(DENSITY_KEY, &value);
                                set_root_attr("data-density", &value);
                            }
                        }
                        TweakRow {
                            label: "Default view",
                            options: vec![("cards", "Cards"), ("table", "Table")],
                            value: default_view(),
                            on_change: move |value: String| {
                                default_view.set(value.clone());
                                store_pref(SYSTEMS_VIEW_KEY, &value);
                            }
                        }
                        TweakRow {
                            label: "Sidebar",
                            options: vec![("full", "Full"), ("rail", "Rail")],
                            value: if is_collapsed() { "rail".to_string() } else { "full".to_string() },
                            on_change: move |value: String| {
                                let collapsed = value == "rail";
                                is_collapsed.set(collapsed);
                                store_pref("cf-sidebar-collapsed", if collapsed { "true" } else { "false" });
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
