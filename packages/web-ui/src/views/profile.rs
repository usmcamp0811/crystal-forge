//! Profile & Preferences view — user identity, appearance, notifications, and sessions.

use dioxus::prelude::*;

use crate::api::client::{
    fetch_notification_preferences, fetch_user_sessions, logout, revoke_all_user_sessions,
    revoke_user_session, update_notification_preferences,
};
use crate::api::models::{
    AuthMode, NotificationDeliveryChannel, NotificationPreferencesDto, Role,
    UpdateNotificationPreferences, UpdateUserPreferences, UserSessionDto,
};
use crate::components::layout::sidebar::{PreferencesContext, SidebarContext};
use crate::components::{Icon, IconName};
use crate::routes::Route;
use crate::state::app_state::{AppState, clear_authenticated_context};
use crate::state::preferences;
use crate::state::theme;

fn profile_relative_time(timestamp: chrono::DateTime<chrono::Utc>) -> String {
    let delta = chrono::Utc::now().signed_duration_since(timestamp);
    if delta.num_minutes() < 1 {
        "just now".to_string()
    } else if delta.num_hours() < 1 {
        format!("{} minutes ago", delta.num_minutes())
    } else if delta.num_days() < 1 {
        format!("{} hours ago", delta.num_hours())
    } else {
        format!("{} days ago", delta.num_days())
    }
}

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
    let mut notification_prefs = use_signal(|| None::<NotificationPreferencesDto>);
    let mut notification_error = use_signal(|| None::<String>);
    let notification_saving = use_signal(|| false);
    let notification_pending_update = use_signal(|| None::<UpdateNotificationPreferences>);
    let mut sessions = use_signal(Vec::<UserSessionDto>::new);
    let mut sessions_loading = use_signal(|| true);
    let mut sessions_error = use_signal(|| None::<String>);
    let mut revoke_all_confirm = use_signal(|| false);
    let mut revoke_all_in_progress = use_signal(|| false);

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

    let auth_user_id = auth_context
        .as_ref()
        .and_then(|ctx| ctx.user.as_ref())
        .map(|user| user.id.clone());
    let auth_generation = app_state.read().auth_generation;

    let notification_load_user_id = auth_user_id.clone();
    use_effect(move || {
        let requested_user_id = notification_load_user_id.clone();
        let requested_generation = auth_generation;
        spawn(async move {
            if requested_user_id.is_none() {
                notification_prefs.set(None);
                notification_error.set(None);
                return;
            }
            match fetch_notification_preferences().await {
                Ok(preferences) => {
                    if notification_save_request_still_current(
                        current_profile_user_id(app_state),
                        current_profile_auth_generation(app_state),
                        requested_user_id.clone(),
                        requested_generation,
                    ) {
                        notification_prefs.set(Some(preferences));
                        notification_error.set(None);
                    }
                }
                Err(err) => {
                    if notification_save_request_still_current(
                        current_profile_user_id(app_state),
                        current_profile_auth_generation(app_state),
                        requested_user_id.clone(),
                        requested_generation,
                    ) {
                        notification_error.set(Some(format!(
                            "Could not load notification preferences: {err}"
                        )));
                    }
                }
            }
        });
    });

    let session_load_user_id = auth_user_id.clone();
    use_effect(move || {
        let requested_user_id = session_load_user_id.clone();
        let requested_generation = auth_generation;
        spawn(async move {
            if requested_user_id.is_none() {
                sessions.set(Vec::new());
                sessions_error.set(None);
                sessions_loading.set(false);
                return;
            }
            sessions_loading.set(true);
            match fetch_user_sessions().await {
                Ok(response) => {
                    if current_profile_user_id(app_state) == requested_user_id
                        && current_profile_auth_generation(app_state) == requested_generation
                    {
                        sessions.set(response.sessions);
                        sessions_error.set(None);
                    }
                }
                Err(err) => {
                    if current_profile_user_id(app_state) == requested_user_id
                        && current_profile_auth_generation(app_state) == requested_generation
                    {
                        sessions_error.set(Some(format!("Could not load active sessions: {err}")));
                    }
                }
            }
            if current_profile_user_id(app_state) == requested_user_id
                && current_profile_auth_generation(app_state) == requested_generation
            {
                sessions_loading.set(false);
            }
        });
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
                                        reset_profile_account_state(
                                            app_state,
                                            notification_prefs,
                                            notification_error,
                                            notification_saving,
                                            notification_pending_update,
                                            sessions,
                                            sessions_error,
                                            sessions_loading,
                                        );
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

                div {
                    class: "card",
                    style: "padding: 8px 18px 14px;",
                    h3 {
                        style: "font-size: 13px; font-weight: 600; margin: 14px 0 4px;",
                        "Notifications"
                    }
                    if let Some(prefs) = notification_prefs() {
                        NotificationToggleRow {
                            title: "Deploy failures",
                            checked: prefs.deploy_failures,
                            on_change: move |checked| save_notification_pref(
                                notification_prefs,
                                notification_error,
                                notification_saving,
                                notification_pending_update,
                                app_state,
                                UpdateNotificationPreferences { deploy_failures: Some(checked), ..UpdateNotificationPreferences::default() },
                            ),
                        }
                        NotificationToggleRow {
                            title: "Build failures",
                            checked: prefs.build_failures,
                            on_change: move |checked| save_notification_pref(
                                notification_prefs,
                                notification_error,
                                notification_saving,
                                notification_pending_update,
                                app_state,
                                UpdateNotificationPreferences { build_failures: Some(checked), ..UpdateNotificationPreferences::default() },
                            ),
                        }
                        NotificationToggleRow {
                            title: "New critical CVEs",
                            checked: prefs.critical_cves,
                            on_change: move |checked| save_notification_pref(
                                notification_prefs,
                                notification_error,
                                notification_saving,
                                notification_pending_update,
                                app_state,
                                UpdateNotificationPreferences { critical_cves: Some(checked), ..UpdateNotificationPreferences::default() },
                            ),
                        }
                        NotificationToggleRow {
                            title: "Policy violations",
                            checked: prefs.policy_violations,
                            on_change: move |checked| save_notification_pref(
                                notification_prefs,
                                notification_error,
                                notification_saving,
                                notification_pending_update,
                                app_state,
                                UpdateNotificationPreferences { policy_violations: Some(checked), ..UpdateNotificationPreferences::default() },
                            ),
                        }
                        NotificationToggleRow {
                            title: "Heartbeat lost",
                            checked: prefs.heartbeat_lost,
                            on_change: move |checked| save_notification_pref(
                                notification_prefs,
                                notification_error,
                                notification_saving,
                                notification_pending_update,
                                app_state,
                                UpdateNotificationPreferences { heartbeat_lost: Some(checked), ..UpdateNotificationPreferences::default() },
                            ),
                        }
                        NotificationToggleRow {
                            title: "Weekly digest email",
                            checked: prefs.weekly_digest,
                            disabled: !prefs.email_available,
                            on_change: move |checked| save_notification_pref(
                                notification_prefs,
                                notification_error,
                                notification_saving,
                                notification_pending_update,
                                app_state,
                                UpdateNotificationPreferences { weekly_digest: Some(checked), ..UpdateNotificationPreferences::default() },
                            ),
                        }
                        PrefRow {
                            title: "Delivery",
                            desc: prefs.email_unavailable_reason.clone(),
                            div { class: "seg", style: "width: fit-content;",
                                button { class: if prefs.delivery_channel == NotificationDeliveryChannel::InApp { "active" } else { "" }, onclick: move |_| save_notification_pref(notification_prefs, notification_error, notification_saving, notification_pending_update, app_state, UpdateNotificationPreferences { delivery_channel: Some(NotificationDeliveryChannel::InApp), ..UpdateNotificationPreferences::default() }), "In-app" }
                                button { disabled: !prefs.email_available, class: if prefs.delivery_channel == NotificationDeliveryChannel::Email { "active" } else { "" }, onclick: move |_| save_notification_pref(notification_prefs, notification_error, notification_saving, notification_pending_update, app_state, UpdateNotificationPreferences { delivery_channel: Some(NotificationDeliveryChannel::Email), ..UpdateNotificationPreferences::default() }), "Email" }
                                button { disabled: !prefs.email_available, class: if prefs.delivery_channel == NotificationDeliveryChannel::Both { "active" } else { "" }, onclick: move |_| save_notification_pref(notification_prefs, notification_error, notification_saving, notification_pending_update, app_state, UpdateNotificationPreferences { delivery_channel: Some(NotificationDeliveryChannel::Both), ..UpdateNotificationPreferences::default() }), "Both" }
                            }
                        }
                    } else {
                        div { class: "help", style: "margin: 12px 0;", "Loading notification preferences..." }
                    }
                    if let Some(error) = notification_error() {
                        div { class: "help", style: "color: var(--cf-critical); margin: 8px 0 4px;", "{error}" }
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

                div {
                    class: "card",
                    style: "padding: 18px;",
                    h3 {
                        style: "font-size: 13px; font-weight: 600; margin: 0 0 12px;",
                        "Active sessions"
                    }
                    if sessions_loading() {
                        div { class: "help", "Loading active sessions..." }
                    } else if let Some(error) = sessions_error() {
                        div { class: "help", style: "color: var(--cf-critical);", "{error}" }
                    } else if sessions().is_empty() {
                        div { class: "help", "No active sessions were returned for this account." }
                    } else {
                        div { style: "display: grid; gap: 8px;",
                            for session in sessions() {
                                SessionRow { session: session.clone(), sessions, sessions_error }
                            }
                        }
                        button {
                            class: "btn ghost",
                            style: "margin-top: 12px; color: var(--cf-critical); border-color: color-mix(in srgb, var(--cf-critical) 45%, transparent);",
                            disabled: revoke_all_in_progress(),
                            onclick: move |_| revoke_all_confirm.set(true),
                            "Sign out everywhere"
                        }
                    }
                    if revoke_all_confirm() {
                        div { class: "modal-backdrop",
                            div { class: "card", style: "padding: 18px; max-width: 420px;",
                                h3 { style: "font-size: 13px; font-weight: 600; margin: 0 0 8px;", "Sign out everywhere?" }
                                p { class: "help", "This will sign out all computers and browsers, including this one." }
                                div { style: "display: flex; justify-content: flex-end; gap: 8px; margin-top: 14px;",
                                    button { class: "btn ghost", disabled: revoke_all_in_progress(), onclick: move |_| revoke_all_confirm.set(false), "Cancel" }
                                    button {
                                        class: "btn",
                                        disabled: revoke_all_in_progress(),
                                        onclick: move |_| {
                                            if revoke_all_in_progress() {
                                                return;
                                            }
                                            revoke_all_in_progress.set(true);
                                            spawn(async move {
                                                match revoke_all_user_sessions().await {
                                                    Ok(()) => {
                                                        reset_profile_account_state(
                                                            app_state,
                                                            notification_prefs,
                                                            notification_error,
                                                            notification_saving,
                                                            notification_pending_update,
                                                            sessions,
                                                            sessions_error,
                                                            sessions_loading,
                                                        );
                                                        nav.replace(Route::LoginView {});
                                                    }
                                                    Err(err) => {
                                                        revoke_all_in_progress.set(false);
                                                        sessions_error.set(Some(format!("Could not sign out everywhere: {err}")));
                                                    }
                                                }
                                            });
                                        },
                                        if revoke_all_in_progress() { "Signing out…" } else { "Sign out everywhere" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// UI Components
// ============================================================================

fn save_notification_pref(
    mut prefs_signal: Signal<Option<NotificationPreferencesDto>>,
    mut error_signal: Signal<Option<String>>,
    mut saving_signal: Signal<bool>,
    mut pending_signal: Signal<Option<UpdateNotificationPreferences>>,
    app_state: Signal<AppState>,
    update: UpdateNotificationPreferences,
) {
    let requested_user_id = current_profile_user_id(app_state);
    let requested_generation = current_profile_auth_generation(app_state);
    let confirmed_before_update = prefs_signal();
    if let Some(mut prefs) = prefs_signal() {
        if let Some(value) = update.deploy_failures {
            prefs.deploy_failures = value;
        }
        if let Some(value) = update.build_failures {
            prefs.build_failures = value;
        }
        if let Some(value) = update.critical_cves {
            prefs.critical_cves = value;
        }
        if let Some(value) = update.policy_violations {
            prefs.policy_violations = value;
        }
        if let Some(value) = update.heartbeat_lost {
            prefs.heartbeat_lost = value;
        }
        if let Some(value) = update.weekly_digest {
            prefs.weekly_digest = value;
        }
        if let Some(value) = update.delivery_channel {
            prefs.delivery_channel = value;
        }
        prefs_signal.set(Some(prefs));
    }

    pending_signal.with_mut(|pending| merge_notification_update(pending, update.clone()));
    if saving_signal() {
        return;
    }
    saving_signal.set(true);

    spawn(async move {
        let mut last_confirmed = confirmed_before_update;
        loop {
            let next = pending_signal.with_mut(|pending| pending.take());
            let Some(next_update) = next else {
                break;
            };

            match update_notification_preferences(&next_update).await {
                Ok(mut saved) => {
                    if current_profile_user_id(app_state) == requested_user_id
                        && current_profile_auth_generation(app_state) == requested_generation
                    {
                        last_confirmed = Some(saved.clone());
                        pending_signal.with(|pending| {
                            if let Some(pending) = pending {
                                apply_notification_update(&mut saved, pending);
                            }
                        });
                        prefs_signal.set(Some(saved));
                        error_signal.set(None);
                    } else {
                        break;
                    }
                }
                Err(err) => {
                    if current_profile_user_id(app_state) == requested_user_id
                        && current_profile_auth_generation(app_state) == requested_generation
                    {
                        let newer_pending = pending_signal.with_mut(|pending| pending.take());
                        let (restored, requeued) = notification_save_failure_state(
                            &last_confirmed,
                            &next_update,
                            newer_pending,
                        );
                        pending_signal.set(requeued);
                        prefs_signal.set(restored);
                        error_signal.set(Some(format!(
                            "Could not save notification preferences: {err}"
                        )));
                    }
                    break;
                }
            }
        }
        saving_signal.set(false);
        if notification_save_request_still_current(
            current_profile_user_id(app_state),
            current_profile_auth_generation(app_state),
            requested_user_id,
            requested_generation,
        ) && pending_signal.with(|pending| pending.is_some())
        {
            save_notification_pref(
                prefs_signal,
                error_signal,
                saving_signal,
                pending_signal,
                app_state,
                UpdateNotificationPreferences::default(),
            );
        }
    });
}

fn notification_save_failure_state(
    last_confirmed: &Option<NotificationPreferencesDto>,
    failed_update: &UpdateNotificationPreferences,
    newer_pending: Option<UpdateNotificationPreferences>,
) -> (
    Option<NotificationPreferencesDto>,
    Option<UpdateNotificationPreferences>,
) {
    let mut restored = last_confirmed.clone();
    let mut requeued = None;
    if let Some(newer_pending) = newer_pending {
        if let Some(restored) = restored.as_mut() {
            apply_notification_update(restored, &newer_pending);
        }
        merge_notification_update(&mut requeued, failed_update.clone());
        merge_notification_update(&mut requeued, newer_pending);
    }
    (restored, requeued)
}

fn notification_save_request_still_current(
    current_user_id: Option<String>,
    current_generation: u64,
    requested_user_id: Option<String>,
    requested_generation: u64,
) -> bool {
    current_user_id == requested_user_id && current_generation == requested_generation
}

fn apply_notification_update(
    prefs: &mut NotificationPreferencesDto,
    update: &UpdateNotificationPreferences,
) {
    if let Some(value) = update.deploy_failures {
        prefs.deploy_failures = value;
    }
    if let Some(value) = update.build_failures {
        prefs.build_failures = value;
    }
    if let Some(value) = update.critical_cves {
        prefs.critical_cves = value;
    }
    if let Some(value) = update.policy_violations {
        prefs.policy_violations = value;
    }
    if let Some(value) = update.heartbeat_lost {
        prefs.heartbeat_lost = value;
    }
    if let Some(value) = update.weekly_digest {
        prefs.weekly_digest = value;
    }
    if let Some(value) = update.delivery_channel {
        prefs.delivery_channel = value;
    }
}

fn merge_notification_update(
    pending: &mut Option<UpdateNotificationPreferences>,
    update: UpdateNotificationPreferences,
) {
    let target = pending.get_or_insert_with(UpdateNotificationPreferences::default);
    if update.deploy_failures.is_some() {
        target.deploy_failures = update.deploy_failures;
    }
    if update.build_failures.is_some() {
        target.build_failures = update.build_failures;
    }
    if update.critical_cves.is_some() {
        target.critical_cves = update.critical_cves;
    }
    if update.policy_violations.is_some() {
        target.policy_violations = update.policy_violations;
    }
    if update.heartbeat_lost.is_some() {
        target.heartbeat_lost = update.heartbeat_lost;
    }
    if update.weekly_digest.is_some() {
        target.weekly_digest = update.weekly_digest;
    }
    if update.delivery_channel.is_some() {
        target.delivery_channel = update.delivery_channel;
    }
}

fn current_profile_user_id(app_state: Signal<AppState>) -> Option<String> {
    app_state
        .read()
        .auth
        .as_ref()
        .and_then(|ctx| ctx.user.as_ref())
        .map(|user| user.id.clone())
}

fn current_profile_auth_generation(app_state: Signal<AppState>) -> u64 {
    app_state.read().auth_generation
}

fn reset_profile_account_state(
    mut app_state: Signal<AppState>,
    mut notification_prefs: Signal<Option<NotificationPreferencesDto>>,
    mut notification_error: Signal<Option<String>>,
    mut notification_saving: Signal<bool>,
    mut notification_pending_update: Signal<Option<UpdateNotificationPreferences>>,
    mut sessions: Signal<Vec<UserSessionDto>>,
    mut sessions_error: Signal<Option<String>>,
    mut sessions_loading: Signal<bool>,
) {
    clear_authenticated_context(&mut app_state.write());
    notification_prefs.set(None);
    notification_error.set(None);
    notification_saving.set(false);
    notification_pending_update.set(None);
    sessions.set(Vec::new());
    sessions_error.set(None);
    sessions_loading.set(false);
}

#[cfg(test)]
mod tests {
    use super::{
        merge_notification_update, notification_save_failure_state,
        notification_save_request_still_current,
    };
    use crate::api::models::{
        NotificationDeliveryChannel, NotificationPreferencesDto, UpdateNotificationPreferences,
    };
    use chrono::Utc;

    fn prefs(delivery_channel: NotificationDeliveryChannel) -> NotificationPreferencesDto {
        NotificationPreferencesDto {
            deploy_failures: true,
            build_failures: true,
            critical_cves: true,
            policy_violations: true,
            heartbeat_lost: false,
            weekly_digest: false,
            delivery_channel,
            email_available: true,
            delivery_email: Some("tester@example.test".to_string()),
            email_unavailable_reason: None,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn notification_preference_merge_preserves_last_action_per_field() {
        let mut pending = None;

        merge_notification_update(
            &mut pending,
            UpdateNotificationPreferences {
                delivery_channel: Some(NotificationDeliveryChannel::Email),
                build_failures: Some(false),
                ..UpdateNotificationPreferences::default()
            },
        );
        merge_notification_update(
            &mut pending,
            UpdateNotificationPreferences {
                delivery_channel: Some(NotificationDeliveryChannel::Both),
                critical_cves: Some(false),
                ..UpdateNotificationPreferences::default()
            },
        );

        let pending = pending.expect("pending update");
        assert_eq!(
            pending.delivery_channel,
            Some(NotificationDeliveryChannel::Both)
        );
        assert_eq!(pending.build_failures, Some(false));
        assert_eq!(pending.critical_cves, Some(false));
    }

    #[test]
    fn notification_preference_merge_does_not_clear_omitted_fields() {
        let mut pending = Some(UpdateNotificationPreferences {
            weekly_digest: Some(true),
            delivery_channel: Some(NotificationDeliveryChannel::Email),
            ..UpdateNotificationPreferences::default()
        });

        merge_notification_update(
            &mut pending,
            UpdateNotificationPreferences {
                heartbeat_lost: Some(true),
                ..UpdateNotificationPreferences::default()
            },
        );

        let pending = pending.expect("pending update");
        assert_eq!(pending.weekly_digest, Some(true));
        assert_eq!(
            pending.delivery_channel,
            Some(NotificationDeliveryChannel::Email)
        );
        assert_eq!(pending.heartbeat_lost, Some(true));
    }

    #[test]
    fn notification_save_failure_restores_last_successful_snapshot() {
        let last_confirmed = Some(prefs(NotificationDeliveryChannel::Email));
        let failed_update = UpdateNotificationPreferences {
            delivery_channel: Some(NotificationDeliveryChannel::Both),
            ..UpdateNotificationPreferences::default()
        };

        let (restored, requeued) =
            notification_save_failure_state(&last_confirmed, &failed_update, None);

        assert_eq!(requeued, None);
        assert_eq!(
            restored.expect("restored prefs").delivery_channel,
            NotificationDeliveryChannel::Email
        );
    }

    #[test]
    fn notification_save_failure_layers_newer_pending_over_last_success() {
        let last_confirmed = Some(prefs(NotificationDeliveryChannel::Email));
        let failed_update = UpdateNotificationPreferences {
            build_failures: Some(false),
            delivery_channel: Some(NotificationDeliveryChannel::Both),
            ..UpdateNotificationPreferences::default()
        };
        let newer_pending = UpdateNotificationPreferences {
            delivery_channel: Some(NotificationDeliveryChannel::InApp),
            critical_cves: Some(false),
            ..UpdateNotificationPreferences::default()
        };

        let (restored, requeued) =
            notification_save_failure_state(&last_confirmed, &failed_update, Some(newer_pending));

        let restored = restored.expect("restored prefs");
        assert_eq!(
            restored.delivery_channel,
            NotificationDeliveryChannel::InApp
        );
        assert!(!restored.critical_cves);

        let requeued = requeued.expect("requeued update");
        assert_eq!(requeued.build_failures, Some(false));
        assert_eq!(requeued.critical_cves, Some(false));
        assert_eq!(
            requeued.delivery_channel,
            Some(NotificationDeliveryChannel::InApp)
        );
    }

    #[test]
    fn notification_save_generation_change_discards_queued_response() {
        assert!(notification_save_request_still_current(
            Some("user-a".to_string()),
            7,
            Some("user-a".to_string()),
            7,
        ));
        assert!(!notification_save_request_still_current(
            Some("user-a".to_string()),
            8,
            Some("user-a".to_string()),
            7,
        ));
        assert!(!notification_save_request_still_current(
            Some("user-b".to_string()),
            7,
            Some("user-a".to_string()),
            7,
        ));
    }
}

#[component]
fn NotificationToggleRow(
    title: &'static str,
    checked: bool,
    disabled: Option<bool>,
    on_change: EventHandler<bool>,
) -> Element {
    let is_disabled = disabled.unwrap_or(false);
    rsx! {
        PrefRow {
            title,
            desc: None::<String>,
            label {
                style: "display: inline-flex; align-items: center; gap: 8px; font-size: 12px; color: var(--cf-text-muted);",
                input {
                    r#type: "checkbox",
                    checked,
                    disabled: is_disabled,
                    onchange: move |evt| on_change.call(evt.checked()),
                }
            }
        }
    }
}

#[component]
fn SessionRow(
    session: UserSessionDto,
    mut sessions: Signal<Vec<UserSessionDto>>,
    mut sessions_error: Signal<Option<String>>,
) -> Element {
    let session_id = session.id;
    let label = session.device_label.clone();
    let mut revoking = use_signal(|| false);
    let session_title = if session.browser.starts_with("Unknown")
        && session.operating_system.starts_with("Unknown")
    {
        session.device_label.clone()
    } else {
        format!("{} on {}", session.browser, session.operating_system)
    };
    let ip_address = session
        .ip_address
        .clone()
        .unwrap_or_else(|| "unknown IP".to_string());
    let active_label = profile_relative_time(session.last_seen_at);
    rsx! {
        div {
            style: "display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 10px 12px; border-radius: 8px; background: var(--cf-surface-muted); flex-wrap: wrap;",
            div { style: "display: flex; gap: 10px; min-width: 180px; flex: 1 1 260px;",
                Icon { name: IconName::Server, size: 16 }
                div { style: "min-width: 0;",
                    div { style: "display: flex; align-items: center; gap: 8px; font-size: 12px; font-weight: 600; flex-wrap: wrap;",
                        span { style: "overflow-wrap: anywhere;", "{session_title}" }
                        if session.current {
                            span { class: "chip chip-healthy", style: "font-size: 10px;", "this device" }
                        }
                    }
                    div { style: "font-size: 10px; color: var(--cf-text-muted); font-family: var(--cf-font-mono); margin-top: 2px;",
                        "{ip_address} · Active {active_label}"
                    }
                }
            }
            if !session.current {
                button {
                    class: "btn ghost",
                    aria_label: "Revoke session {label}",
                    disabled: revoking(),
                    onclick: move |_| {
                        if revoking() {
                            return;
                        }
                        revoking.set(true);
                        spawn(async move {
                            match revoke_user_session(session_id).await {
                                Ok(()) => {
                                    sessions.write().retain(|item| item.id != session_id);
                                    sessions_error.set(None);
                                }
                                Err(err) => {
                                    revoking.set(false);
                                    sessions_error.set(Some(format!("Could not revoke session: {err}")));
                                }
                            }
                        });
                    },
                    if revoking() { "Revoking…" } else { "Revoke" }
                }
            }
        }
    }
}

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
