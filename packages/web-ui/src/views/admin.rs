use chrono::Local;
use dioxus::prelude::*;
use std::collections::HashMap;

use crate::api::client::{
    create_admin_user, delete_admin_oidc_mapping, delete_admin_user, fetch_admin_audit_events,
    fetch_admin_oidc_mappings, fetch_admin_server_info, fetch_admin_users, fetch_environments,
    set_classification_config, set_setup_wizard_dismissed, update_admin_user,
    upsert_admin_oidc_mapping,
};
use crate::api::models::{
    AdminAuditEventsParams, AdminCreateUserRequest, AdminUpdateUserRequest,
    AdminUpsertOidcMappingRequest, AdminUserSummary, AuditEvent, AuthMode,
    ClassificationBannerConfig, EnvironmentSummary, IdentitySource, OidcGroupMapping, Role,
    ServerRuntimeInfoResponse, UpdateClassificationBannerRequest,
};
use crate::components::{Icon, IconName};
use crate::state::app_state::AppState;
use crate::theme;

const AUDIT_PER_PAGE: i64 = 20;

// ============================================================================
// MOCK DATA STRUCTURES
// ============================================================================

#[derive(Clone, Debug)]
struct BackgroundJob {
    name: &'static str,
    desc: &'static str,
    interval: &'static str,
    enabled: bool,
    last_run: &'static str,
    last_duration: &'static str,
    next_run: &'static str,
    status: &'static str,
    impact: &'static str,
    note: Option<&'static str>,
}

const BACKGROUND_JOBS_MOCK: &[BackgroundJob] = &[];

#[derive(Clone, Debug)]
struct RoleDefinition {
    role: &'static str,
    desc: &'static str,
    color: &'static str,
    perms: &'static [&'static str],
}

const ROLE_DEFINITIONS: &[RoleDefinition] = &[
    RoleDefinition {
        role: "Admin",
        desc: "Full control — manage users, servers, all environments.",
        color: "#f87171",
        perms: &[
            "Manage users & OIDC",
            "Edit server config",
            "All operator powers",
            "View audit log",
        ],
    },
    RoleDefinition {
        role: "Operator",
        desc: "Deploy, build, evaluate, and manage assigned environments.",
        color: "#60a5fa",
        perms: &[
            "Deploy & rollback",
            "Trigger eval/build",
            "Cancel jobs",
            "Accept CVEs",
            "Edit flakes/systems",
        ],
    },
    RoleDefinition {
        role: "Viewer",
        desc: "Read-only access to dashboards and reports.",
        color: "#9ca3af",
        perms: &[
            "View all dashboards",
            "Export reports",
            "Read audit log (own actions)",
        ],
    },
];

// ============================================================================
// MAIN COMPONENT
// ============================================================================

#[component]
pub fn AdminView() -> Element {
    let nav = navigator();
    let mut app_state = use_context::<Signal<AppState>>();
    let mut users = use_signal(Vec::<AdminUserSummary>::new);
    let mut user_drafts = use_signal(HashMap::<String, UserEditDraft>::new);

    let mut audit_events = use_signal(Vec::<AuditEvent>::new);
    let mut oidc_mappings = use_signal(Vec::<OidcGroupMapping>::new);
    let mut environments = use_signal(Vec::<EnvironmentSummary>::new);
    let mut server_info = use_signal(|| None::<ServerRuntimeInfoResponse>);
    let mut audit_total = use_signal(|| 0_i64);
    let mut audit_page = use_signal(|| 1_i64);

    let mut users_loading = use_signal(|| true);
    let mut audit_loading = use_signal(|| true);
    let mut users_error = use_signal(|| None::<String>);
    let mut audit_error = use_signal(|| None::<String>);
    let mut oidc_error = use_signal(|| None::<String>);
    let mut environments_error = use_signal(|| None::<String>);
    let mut server_info_loading = use_signal(|| true);
    let mut server_info_error = use_signal(|| None::<String>);

    let mut user_search = use_signal(String::new);
    let mut user_role_filter = use_signal(|| "all".to_string());

    let mut actor_filter = use_signal(String::new);
    let mut from_filter = use_signal(String::new);
    let mut to_filter = use_signal(String::new);

    let mut active_tab = use_signal(|| "users".to_string());

    // Seed classification state from AppState (set by AppShell on load).
    // If AppState already has the config (fast path: navigated in after the
    // async fetch completed), use it; otherwise fall back to safe defaults and
    // let the sync effect below overwrite them when the fetch arrives.
    let initial_cfg = app_state
        .read()
        .classification_config
        .clone()
        .unwrap_or_else(|| ClassificationBannerConfig {
            enabled: false,
            level: "UNCLASSIFIED".to_string(),
            custom_text: String::new(),
        });
    // True when AppState already had a value on mount — no sync needed.
    let already_loaded = app_state.read().classification_config.is_some();
    let already_failed = app_state
        .read()
        .classification_fetch_state
        .as_ref()
        .is_some_and(|r| r.is_err());
    let mut classification_enabled = use_signal(|| initial_cfg.enabled);
    let mut classification_level = use_signal(|| initial_cfg.level.clone());
    let mut classification_custom_text = use_signal(|| initial_cfg.custom_text.clone());
    // Tracks whether the user has made edits so the sync effect skips overwriting them.
    let mut classification_dirty = use_signal(|| false);
    let mut classification_loaded = use_signal(|| already_loaded);
    let mut classification_fetch_error = use_signal(|| {
        if already_failed {
            Some("Failed to load classification config from server.".to_string())
        } else {
            None
        }
    });

    // Sync effect: fires whenever AppState.classification_config changes. Only
    // writes to local signals if the user has not made unsaved edits AND the
    // form has not yet been loaded from a real value.
    use_effect(move || {
        if *classification_loaded.read() {
            return;
        }
        let (config, fetch_state) = {
            let state = app_state.read();
            (
                state.classification_config.clone(),
                state.classification_fetch_state.clone(),
            )
        };
        if let Some(cfg) = config {
            if !*classification_dirty.read() {
                classification_enabled.set(cfg.enabled);
                classification_level.set(cfg.level);
                classification_custom_text.set(cfg.custom_text);
            }
            classification_loaded.set(true);
            classification_fetch_error.set(None);
        } else if let Some(Err(e)) = fetch_state {
            classification_fetch_error.set(Some(e));
        }
    });

    // Load users and OIDC mappings
    {
        let mut users = users.clone();
        let mut user_drafts = user_drafts.clone();
        let mut users_loading = users_loading.clone();
        let mut users_error = users_error.clone();
        use_effect(move || {
            spawn(async move {
                refresh_users(users, user_drafts, users_error).await;

                match fetch_admin_oidc_mappings().await {
                    Ok(next) => {
                        oidc_mappings.set(next);
                        oidc_error.set(None);
                    }
                    Err(e) => oidc_error.set(Some(format!("Failed to load OIDC mappings: {e}"))),
                }

                match fetch_environments().await {
                    Ok(next) => {
                        environments.set(next);
                        environments_error.set(None);
                    }
                    Err(e) => {
                        environments_error.set(Some(format!("Failed to load environments: {e}")))
                    }
                }

                users_loading.set(false);
            });
        });
    }

    // Load audit events
    {
        let mut audit_events = audit_events.clone();
        let mut audit_total = audit_total.clone();
        let mut audit_loading = audit_loading.clone();
        let mut audit_error = audit_error.clone();
        use_effect(move || {
            let actor = actor_filter.read().clone();
            let from = from_filter.read().clone();
            let to = to_filter.read().clone();
            let page = *audit_page.read();

            audit_loading.set(true);

            spawn(async move {
                let params = AdminAuditEventsParams {
                    actor: optional_value(actor),
                    action: None,
                    from: datetime_local_to_rfc3339(&from),
                    to: datetime_local_to_rfc3339(&to),
                    page: Some(page),
                    per_page: Some(AUDIT_PER_PAGE),
                };

                match fetch_admin_audit_events(&params).await {
                    Ok(response) => {
                        audit_events.set(response.items);
                        audit_total.set(response.total);
                        audit_error.set(None);
                    }
                    Err(e) => {
                        audit_error.set(Some(format!("Failed to load audit events: {e}")));
                    }
                }

                audit_loading.set(false);
            });
        });
    }

    // Load real server/build/database runtime information.
    {
        let mut server_info = server_info.clone();
        let mut server_info_loading = server_info_loading.clone();
        let mut server_info_error = server_info_error.clone();
        use_effect(move || {
            spawn(async move {
                match fetch_admin_server_info().await {
                    Ok(next) => {
                        server_info.set(Some(next));
                        server_info_error.set(None);
                    }
                    Err(e) => {
                        server_info_error.set(Some(format!("Failed to load server info: {e}")));
                    }
                }

                server_info_loading.set(false);
            });
        });
    }

    let total_pages = {
        let total = *audit_total.read();
        if total <= 0 {
            1
        } else {
            (total + AUDIT_PER_PAGE - 1) / AUDIT_PER_PAGE
        }
    };

    let can_go_prev = *audit_page.read() > 1;
    let can_go_next = *audit_page.read() < total_pages;
    rsx! {
        div {
            style: "display:flex;flex-direction:column;gap:16px;",

            // ── Page head ───────────────────────────────────────────────────
            div { class: "page-head",
                div {
                    h1 { class: "page-title", "Server Management" }
                    p { class: "page-subtitle", "Admin-only · users, access control, audit, and server configuration" }
                }
                span { class: "chip chip-critical", style: "align-self:center;",
                    svg { width: "11", height: "11", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:4px;vertical-align:text-bottom;",
                        path { d: "M12 3l8 3v6c0 4.5-3.3 8.5-8 9-4.7-.5-8-4.5-8-9V6l8-3z" }
                    }
                    "admin only"
                }
            }

            // ── Server info strip ──────────────────────────────────────────
            ServerInfoStrip {
                users: users.read().clone(),
                server_info: server_info.read().clone(),
                server_info_loading: *server_info_loading.read(),
                server_info_error: server_info_error.read().clone(),
                auth_mode: app_state.read().auth.as_ref().map(|a| a.auth_mode).unwrap_or(AuthMode::Local)
            }

            // ── Tab card ─────────────────────────────────────────────────────
            div { class: "card", style: "overflow:hidden;",
                // ── Tab bar ──────────────────────────────────────────────────
                div { class: "sd-tabs", style: "padding:0 16px;border-bottom:1px solid var(--cf-card-border);margin-top:0;background:color-mix(in oklab, var(--cf-page-bg) 58%, var(--cf-card-bg));box-shadow:inset 0 -1px 0 var(--cf-card-border);",
                    for (tab_id, tab_label, icon) in [
                        ("users", "Users", IconName::Server),
                        ("roles", "Roles", IconName::Key),
                        ("oidc", "OIDC Mappings", IconName::Link),
                        ("jobs", "Background Jobs", IconName::Sync),
                        ("audit", "Audit Log", IconName::History),
                        ("server", "Server", IconName::Gear),
                    ] {
                        {
                            let is_active = *active_tab.read() == tab_id;
                            let btn_class = if is_active { "sd-tab active focus-ring" } else { "sd-tab focus-ring" };
                            rsx! {
                                button {
                                    class: "{btn_class}",
                                    onclick: {
                                        let id = tab_id.to_string();
                                        move |_| active_tab.set(id.clone())
                                    },
                                    Icon { name: icon, size: 12 }
                                    "{tab_label}"
                                }
                            }
                        }
                    }
                }

                // ── Users tab ────────────────────────────────────────────────
                if active_tab.read().as_str() == "users" {
                    UsersTab {
                        users: users.clone(),
                        user_drafts: user_drafts.clone(),
                        users_loading: *users_loading.read(),
                        users_error: users_error.clone(),
                        user_search: user_search.clone(),
                        user_role_filter: user_role_filter.clone(),
                        environments: environments.read().clone(),
                    }
                }

                // ── Roles tab ────────────────────────────────────────────────
                if active_tab.read().as_str() == "roles" {
                    RolesTab { users: users.read().clone() }
                }

                // ── OIDC tab ─────────────────────────────────────────────────
                if active_tab.read().as_str() == "oidc" {
                    OidcTab {
                        oidc_mappings: oidc_mappings.clone(),
                        oidc_error: oidc_error.clone(),
                        auth_mode: app_state.read().auth.as_ref().map(|a| a.auth_mode).unwrap_or(AuthMode::Local),
                        environments: environments.read().clone(),
                    }
                }

                // ── Background Jobs tab ──────────────────────────────────────
                if active_tab.read().as_str() == "jobs" {
                    JobsTab {}
                }

                // ── Audit Log tab ───────────────────────────────────────────
                if active_tab.read().as_str() == "audit" {
                    AuditTab {
                        audit_events: audit_events.clone(),
                        audit_loading: *audit_loading.read(),
                        audit_error: audit_error.clone(),
                        actor_filter: actor_filter.clone(),
                        audit_page: audit_page.clone(),
                        can_go_prev: can_go_prev,
                        can_go_next: can_go_next,
                        total_pages: total_pages,
                        audit_total: *audit_total.read(),
                    }
                }

                // ── Server tab ──────────────────────────────────────────────
                if active_tab.read().as_str() == "server" {
                    ServerTab {
                        environments: environments.read().clone(),
                        environments_error: environments_error.read().clone(),
                        server_info: server_info.read().clone(),
                        server_info_loading: *server_info_loading.read(),
                        server_info_error: server_info_error.read().clone(),
                        auth_mode: app_state.read().auth.as_ref().map(|a| a.auth_mode).unwrap_or(AuthMode::Local),
                        classification_enabled,
                        classification_level,
                        classification_custom_text,
                        classification_loaded,
                        classification_fetch_error: classification_fetch_error.read().clone(),
                        on_classification_dirty: move |_| classification_dirty.set(true),
                        on_classification_retry: move |_| {
                            // Reset fetch state so AppShell will retry on next render.
                            app_state.write().classification_fetch_state = None;
                            classification_fetch_error.set(None);
                        },
                    }
                }
            }
        }
    }
}

// ============================================================================
// SERVER INFO STRIP
// ============================================================================

#[component]
fn ServerInfoStrip(
    users: Vec<AdminUserSummary>,
    server_info: Option<ServerRuntimeInfoResponse>,
    server_info_loading: bool,
    server_info_error: Option<String>,
    auth_mode: AuthMode,
) -> Element {
    let active_users = users.iter().filter(|u| u.enabled).count();

    let auth_mode_label = match auth_mode {
        AuthMode::Dev => "Dev",
        AuthMode::Local => "Local",
        AuthMode::Oidc => "OIDC",
    };

    let version_value = server_info
        .as_ref()
        .map(|info| info.version.as_str())
        .unwrap_or(if server_info_loading { "Loading…" } else { "Unavailable" });
    let version_meta = if server_info_error.is_some() {
        "server info unavailable".to_string()
    } else if let Some(info) = server_info.as_ref() {
        info.commit
            .as_ref()
            .map(|commit| format!("commit {}", short_commit(commit)))
            .unwrap_or_else(|| "commit unavailable".to_string())
    } else if server_info_loading {
        "loading runtime info".to_string()
    } else {
        "server info unavailable".to_string()
    };
    let database_value = server_info
        .as_ref()
        .map(|info| info.database.status.as_str())
        .unwrap_or(if server_info_loading { "Loading…" } else { "Unavailable" });
    let database_meta = if server_info_error.is_some() {
        "server info unavailable".to_string()
    } else if let Some(info) = server_info.as_ref() {
        format!("{} · {}", info.database.name, info.database.size)
    } else if server_info_loading {
        "loading database info".to_string()
    } else {
        "server info unavailable".to_string()
    };
    let active_sessions_value = server_info
        .as_ref()
        .map(|info| info.active_sessions.to_string())
        .unwrap_or(if server_info_loading {
            "Loading…".to_string()
        } else {
            "Unavailable".to_string()
        });
    let active_sessions_meta = if server_info_error.is_some() {
        "server info unavailable"
    } else if server_info_loading {
        "loading session count"
    } else {
        "currently valid sessions"
    };
    let tls_value = server_info
        .as_ref()
        .map(|info| info.tls_status.as_str())
        .unwrap_or(if server_info_loading { "Loading…" } else { "Unavailable" });
    let tls_meta = server_info
        .as_ref()
        .map(|info| info.tls_detail.as_str())
        .unwrap_or(if server_info_error.is_some() {
            "server info unavailable"
        } else if server_info_loading {
            "loading transport info"
        } else {
            "server info unavailable"
        });

    rsx! {
        div { class: "stat-strip",
            div { class: "stat",
                span { class: "stat-accent", style: "--stat-color:#a78bfa;" }
                div { class: "stat-label", "CF Version" }
                div { class: "stat-value", style: "font-size:16px;", "{version_value}" }
                div { class: "stat-meta", "{version_meta}" }
            }
            div { class: "stat",
                span { class: "stat-accent", style: "--stat-color:#34d399;" }
                div { class: "stat-label", "Auth mode" }
                div { class: "stat-value", style: "font-size:16px;", "{auth_mode_label}" }
                div { class: "stat-meta", "{active_users} active users" }
            }
            div { class: "stat",
                span { class: "stat-accent", style: "--stat-color:#60a5fa;" }
                div { class: "stat-label", "Database" }
                div { class: "stat-value", style: "font-size:16px;", "{database_value}" }
                div { class: "stat-meta", "{database_meta}" }
            }
            div { class: "stat",
                span { class: "stat-accent", style: "--stat-color:#fbbf24;" }
                div { class: "stat-label", "Active sessions" }
                div { class: "stat-value", style: "font-size:16px;", "{active_sessions_value}" }
                div { class: "stat-meta", "{active_sessions_meta}" }
            }
            div { class: "stat",
                span { class: "stat-accent", style: "--stat-color:#f87171;" }
                div { class: "stat-label", "TLS cert" }
                div { class: "stat-value", style: "font-size:16px;", "{tls_value}" }
                div { class: "stat-meta", "{tls_meta}" }
            }
        }
    }
}

// ============================================================================
// USERS TAB
// ============================================================================

#[component]
fn UsersTab(
    users: Signal<Vec<AdminUserSummary>>,
    user_drafts: Signal<HashMap<String, UserEditDraft>>,
    users_loading: bool,
    users_error: Signal<Option<String>>,
    user_search: Signal<String>,
    user_role_filter: Signal<String>,
    environments: Vec<EnvironmentSummary>,
) -> Element {
    let mut editing_user_id = use_signal(|| Option::<String>::None);
    let mut adding_user = use_signal(|| false);

    let filtered_users = {
        let search = user_search.read().to_lowercase();
        let role_filter = user_role_filter.read().clone();
        users
            .read()
            .iter()
            .filter(|u| {
                let matches_search =
                    search.is_empty() || u.identifier.to_lowercase().contains(&search);
                let matches_role = role_filter == "all" || role_to_string(&u.role) == role_filter;
                matches_search && matches_role
            })
            .cloned()
            .collect::<Vec<_>>()
    };

    rsx! {
        div { style: "padding:0;",
            // Search and filter bar
            div { style: "padding:12px 16px;border-bottom:1px solid var(--cf-divider);display:flex;gap:10px;align-items:center;flex-wrap:wrap;",
                div { class: "filter-search", style: "max-width:260px;",
                    svg { width: "16", height: "16", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                        circle { cx: "11", cy: "11", r: "8" }
                        path { d: "m21 21-4.35-4.35" }
                    }
                    input {
                        class: "input focus-ring",
                        placeholder: "Search users…",
                        value: "{user_search.read()}",
                        oninput: move |evt| user_search.set(evt.value())
                    }
                }
                div { class: "seg",
                    for role in ["all", "admin", "operator", "viewer"] {
                        button {
                            class: if *user_role_filter.read() == role { "active" } else { "" },
                            onclick: {
                                let role = role.to_string();
                                move |_| user_role_filter.set(role.clone())
                            },
                            "{role}"
                        }
                    }
                }
                span { class: "filter-count", "{filtered_users.len()} users" }
                button {
                    class: "btn btn-primary focus-ring",
                    style: "margin-left:auto;",
                    onclick: move |_| adding_user.set(true),
                    "Add user"
                }
            }

            // User table
            if users_loading {
                div { style: "padding:16px;color:var(--cf-text-muted);font-size:13px;", "Loading users..." }
            } else if let Some(err) = users_error.read().as_ref() {
                div { style: "padding:16px;",
                    div { class: "sd-callout sd-callout-danger", "{err}" }
                }
            } else {
                table { class: "sys-table",
                    thead {
                        tr {
                            th { "User" }
                            th { "Role" }
                            th { "Source" }
                            th { "Environments" }
                            th { "MFA" }
                            th { "Status" }
                            th { "Updated" }
                            th { style: "text-align:right;", " " }
                        }
                    }
                    tbody {
                        for user in filtered_users {
                            {
                                let user_id = user.id.clone();
                                let initials = get_user_initials(&user.identifier);

                                rsx! {
                                    tr {
                                        td {
                                            div { style: "display:flex;align-items:center;gap:10px;",
                                                // Avatar
                                                div {
                                                    style: "width:28px;height:28px;border-radius:50%;background:linear-gradient(135deg,#a78bc4,#654a84);display:grid;place-items:center;font-size:11px;font-weight:600;color:#fff;flex-shrink:0;",
                                                    "{initials}"
                                                }
                                                div {
                                                    div { style: "font-weight:600;font-size:13px;display:flex;align-items:center;gap:6px;",
                                                        "{user.identifier}"
                                                    }
                                                }
                                            }
                                        }
                                        td {
                                            {
                                                let role_def = get_role_definition(user.role);
                                                rsx! {
                                                    span {
                                                        class: "chip",
                                                        style: "background:color-mix(in oklab, {role_def.color} 16%, transparent);color:{role_def.color};font-size:10px;",
                                                        "{role_def.role}"
                                                    }
                                                }
                                            }
                                        }
                                        td {
                                            span { class: "chip chip-unknown", style: "font-size:10px;", "{identity_source_label(user.identity_source)}" }
                                        }
                                        td {
                                            div { style: "display:flex;gap:4px;flex-wrap:wrap;",
                                                if user.environments.is_empty() {
                                                    span { class: "chip chip-info", style: "font-size:10px;", "all" }
                                                } else {
                                                    for env in &user.environments {
                                                        span { class: "chip chip-unknown", style: "font-size:10px;", "{env}" }
                                                    }
                                                }
                                            }
                                        }
                                        td {
                                            span { class: "chip chip-unknown", style: "font-size:10px;", "unavailable" }
                                        }
                                        td {
                                            if user.enabled {
                                                span { class: "chip chip-healthy", style: "font-size:10px;", "active" }
                                            } else {
                                                span { class: "chip chip-unknown", style: "font-size:10px;", "disabled" }
                                            }
                                        }
                                        td { style: "font-size:12px;color:var(--cf-text-muted);", "{format_time(user.updated_at)}" }
                                        td {
                                            div { class: "row-actions",
                                                button {
                                                    class: "btn-icon focus-ring",
                                                    title: "Edit",
                                                    onclick: {
                                                        let user_id = user_id.clone();
                                                        move |_| editing_user_id.set(Some(user_id.clone()))
                                                    },
                                                    svg { width: "14", height: "14", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                                        path { d: "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" }
                                                        circle { cx: "12", cy: "12", r: "3" }
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
            }

            if let Some(selected_id) = editing_user_id.read().as_ref() {
                if let Some(selected_user) = users.read().iter().find(|u| &u.id == selected_id).cloned() {
                    EditUserModal {
                        user: selected_user,
                        users,
                        user_drafts,
                        users_error,
                        environments: environments.clone(),
                        on_close: move |_| editing_user_id.set(None),
                    }
                }
            }
            if *adding_user.read() {
                CreateUserModal {
                    users,
                    user_drafts,
                    users_error,
                    environments,
                    on_close: move |_| adding_user.set(false),
                }
            }
        }
    }
}

#[component]
fn CreateUserModal(
    users: Signal<Vec<AdminUserSummary>>,
    user_drafts: Signal<HashMap<String, UserEditDraft>>,
    users_error: Signal<Option<String>>,
    environments: Vec<EnvironmentSummary>,
    on_close: EventHandler<()>,
) -> Element {
    let mut email = use_signal(String::new);
    let mut display_name = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut role = use_signal(|| "Viewer".to_string());
    let mut selected_environments = use_signal(String::new);
    let mut saving = use_signal(|| false);
    let mut local_error = use_signal(|| Option::<String>::None);

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_close.call(()),
            div { class: "modal", style: "width:min(560px,96vw);", onclick: move |evt| evt.stop_propagation(),
                div { class: "modal-head",
                    div {
                        h3 { style: "margin:0;font-size:15px;", "Add user" }
                        p { style: "margin:4px 0 0;font-size:12px;color:var(--cf-text-muted);", "Create a local managed account." }
                    }
                }
                div { class: "modal-body", style: "display:grid;gap:14px;",
                    if let Some(err) = local_error.read().as_ref() {
                        div { class: "sd-callout sd-callout-danger", style: "font-size:12px;", "{err}" }
                    }
                    div { class: "field", style: "margin:0;", label { "Email" } input { class: "input focus-ring", value: "{email.read()}", oninput: move |evt| email.set(evt.value()) } }
                    div { class: "field", style: "margin:0;", label { "Display name" } input { class: "input focus-ring", value: "{display_name.read()}", oninput: move |evt| display_name.set(evt.value()) } }
                    div { class: "field", style: "margin:0;", label { "Temporary password" } input { class: "input focus-ring", r#type: "password", value: "{password.read()}", oninput: move |evt| password.set(evt.value()) } }
                    div { class: "field", style: "margin:0;", label { "Role" } select { class: "input focus-ring", value: "{role.read()}", onchange: move |evt| role.set(evt.value()), option { "Admin" } option { "Operator" } option { "Viewer" } } }
                    EnvironmentChipPicker {
                        title: "Environments".to_string(),
                        selected_csv: selected_environments,
                        available: environments.clone(),
                    }
                }
                div { class: "modal-foot",
                    button { class: "btn btn-ghost focus-ring", onclick: move |_| on_close.call(()), "Cancel" }
                    button { class: "btn btn-primary focus-ring", disabled: *saving.read(), onclick: move |_| {
                        let request = AdminCreateUserRequest {
                            email: email.read().trim().to_string(),
                            display_name: optional_value(display_name.read().clone()),
                            password: optional_value(password.read().clone()),
                            role: parse_role(&role.read()),
                            environments: parse_environments(&selected_environments.read()),
                        };
                        saving.set(true);
                        local_error.set(None);
                        spawn(async move {
                            match create_admin_user(&request).await {
                                Ok(new_user) => {
                                    users.with_mut(|items| items.push(new_user.clone()));
                                    update_user_draft(user_drafts, &new_user.id, |draft| *draft = UserEditDraft::from_user(&new_user));
                                    users_error.set(None);
                                    saving.set(false);
                                    on_close.call(());
                                }
                                Err(err) => {
                                    local_error.set(Some(err.to_string()));
                                    saving.set(false);
                                }
                            }
                        });
                    }, if *saving.read() { "Creating…" } else { "Create" } }
                }
            }
        }
    }
}

#[component]
fn EditUserModal(
    user: AdminUserSummary,
    users: Signal<Vec<AdminUserSummary>>,
    user_drafts: Signal<HashMap<String, UserEditDraft>>,
    users_error: Signal<Option<String>>,
    environments: Vec<EnvironmentSummary>,
    on_close: EventHandler<()>,
) -> Element {
    let mut saving = use_signal(|| false);
    let mut local_error = use_signal(|| Option::<String>::None);

    let draft = user_drafts
        .read()
        .get(&user.id)
        .cloned()
        .unwrap_or_else(|| UserEditDraft::from_user(&user));
    let is_oidc_user = user.identity_source == IdentitySource::OidcDerived;
    let mut draft_role = use_signal(|| draft.role.clone());
    let mut draft_enabled = use_signal(|| draft.enabled);
    let mut draft_password = use_signal(String::new);
    let selected_environments = use_signal(|| draft.environments.clone());

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_close.call(()),
            div { class: "modal", style: "width:min(560px,96vw);", onclick: move |evt| evt.stop_propagation(),
                div { class: "modal-head",
                    div {
                        h3 { style: "margin:0;font-size:15px;", "Edit user" }
                        p { style: "margin:4px 0 0;font-size:12px;color:var(--cf-text-muted);", "{user.identifier}" }
                    }
                    button { class: "btn-icon focus-ring", title: "Close", onclick: move |_| on_close.call(()),
                        svg { width: "16", height: "16", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                            line { x1: "18", y1: "6", x2: "6", y2: "18" }
                            line { x1: "6", y1: "6", x2: "18", y2: "18" }
                        }
                    }
                }
                div { class: "modal-body", style: "display:grid;gap:14px;",
                    if is_oidc_user {
                        div { class: "sd-callout sd-callout-info", style: "font-size:12px;",
                            "This user is OIDC-derived. Role and environment changes can be overwritten on the next login if group mappings still assign different access."
                        }
                    }
                    if let Some(err) = local_error.read().as_ref() {
                        div { class: "sd-callout sd-callout-danger", style: "font-size:12px;", "{err}" }
                    }
                    div { class: "field", style: "margin:0;",
                        label { "Role" }
                        select {
                            class: "input focus-ring",
                            value: "{draft_role.read()}",
                            onchange: move |evt| draft_role.set(evt.value()),
                            option { value: "Admin", "Admin" }
                            option { value: "Operator", "Operator" }
                            option { value: "Viewer", "Viewer" }
                        }
                    }
                    div { class: "field", style: "margin:0;",
                        EnvironmentChipPicker {
                            title: "Environments".to_string(),
                            selected_csv: selected_environments,
                            available: environments.clone(),
                        }
                    }
                    div { class: "field", style: "margin:0;",
                        label { "Password reset" }
                        input {
                            class: "input focus-ring",
                            r#type: "password",
                            placeholder: "Leave blank to keep current password",
                            value: "{draft_password.read()}",
                            oninput: move |evt| draft_password.set(evt.value())
                        }
                    }
                    label { style: "display:flex;align-items:center;justify-content:space-between;gap:12px;border:1px solid var(--cf-divider);border-radius:10px;padding:10px 12px;cursor:pointer;",
                        span {
                            span { style: "display:block;font-size:13px;font-weight:600;", "Enabled" }
                            span { style: "display:block;font-size:12px;color:var(--cf-text-muted);", "Disabled users cannot access Crystal Forge." }
                        }
                        input {
                            r#type: "checkbox",
                            checked: *draft_enabled.read(),
                            onchange: move |evt| draft_enabled.set(evt.checked())
                        }
                    }
                }
                div { class: "modal-foot",
                    button { class: "btn focus-ring", onclick: move |_| on_close.call(()), "Cancel" }
                    button {
                        class: "btn btn-primary focus-ring",
                        disabled: *saving.read(),
                        onclick: {
                            let user_id = user.id.clone();
                            move |_| {
                                let request = AdminUpdateUserRequest {
                                    role: Some(parse_role(&draft_role.read())),
                                    enabled: Some(*draft_enabled.read()),
                                    environments: Some(parse_environments(&selected_environments.read())),
                                    password: optional_value(draft_password.read().clone()),
                                };
                                let user_id_for_request = user_id.clone();
                                saving.set(true);
                                local_error.set(None);
                                spawn(async move {
                                    match update_admin_user(&user_id_for_request, &request).await {
                                        Ok(updated_user) => {
                                            users.with_mut(|items| {
                                                if let Some(existing) = items.iter_mut().find(|u| u.id == updated_user.id) {
                                                    *existing = updated_user.clone();
                                                }
                                            });
                                            update_user_draft(user_drafts, &updated_user.id, |draft| {
                                                *draft = UserEditDraft::from_user(&updated_user);
                                            });
                                            users_error.set(None);
                                            saving.set(false);
                                            on_close.call(());
                                        }
                                        Err(err) => {
                                            local_error.set(Some(err.to_string()));
                                            saving.set(false);
                                        }
                                    }
                                });
                            }
                        },
                        if *saving.read() { "Saving…" } else { "Save changes" }
                    }
                    button {
                        class: "btn btn-ghost focus-ring",
                        style: "margin-left:auto;color:#f87171;border-color:rgba(248,113,113,0.3);",
                        disabled: *saving.read(),
                        onclick: {
                            let user_id = user.id.clone();
                            move |_| {
                                let confirmed = web_sys::window()
                                    .and_then(|window| window.confirm_with_message("Delete this user? This cannot be undone.").ok())
                                    .unwrap_or(false);
                                if !confirmed {
                                    return;
                                }
                                let user_id_for_request = user_id.clone();
                                saving.set(true);
                                local_error.set(None);
                                spawn(async move {
                                    match delete_admin_user(&user_id_for_request).await {
                                        Ok(()) => {
                                            users.with_mut(|items| items.retain(|item| item.id != user_id_for_request));
                                            users_error.set(None);
                                            saving.set(false);
                                            on_close.call(());
                                        }
                                        Err(err) => {
                                            local_error.set(Some(err.to_string()));
                                            saving.set(false);
                                        }
                                    }
                                });
                            }
                        },
                        "Delete user"
                    }
                }
            }
        }
    }
}

#[component]
fn EnvironmentChipPicker(
    title: String,
    selected_csv: Signal<String>,
    available: Vec<EnvironmentSummary>,
) -> Element {
    let selected = parse_environments(&selected_csv.read());
    let all_selected = selected.is_empty();

    rsx! {
        div { class: "field", style: "margin:0;",
            label { "{title}" }
            div { style: "display:flex;flex-wrap:wrap;gap:6px;",
                button {
                    class: "focus-ring",
                    onclick: move |_| selected_csv.set(String::new()),
                    style: environment_pill_style(all_selected, "#60a5fa"),
                    "all"
                }
                if available.is_empty() {
                    span { class: "chip chip-unknown", style: "font-size:10px;", "No environments loaded" }
                } else {
                    for env in available.iter().filter(|env| env.is_active) {
                        {
                            let env_name = env.name.clone();
                            let env_color = env.color_hex.clone();
                            let is_selected = selected.iter().any(|value| value == &env_name);
                            rsx! {
                                button {
                                    class: "focus-ring",
                                    onclick: move |_| {
                                        let current = selected_csv.read().clone();
                                        let next = toggle_environment_selection(&current, &env_name);
                                        selected_csv.set(next);
                                    },
                                    style: environment_pill_style(is_selected, &env_color),
                                    span { style: "width:6px;height:6px;border-radius:50%;background:{env_color};" }
                                    "{env_name}"
                                }
                            }
                        }
                    }
                }
            }
            div { class: "help", "Choose all environments or select one or more scoped environments." }
        }
    }
}

// ============================================================================
// ROLES TAB
// ============================================================================

#[component]
fn RolesTab(users: Vec<AdminUserSummary>) -> Element {
    rsx! {
        div { style: "padding:16px;display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:14px;",
            for def in ROLE_DEFINITIONS {
                {
                    let user_count = users.iter().filter(|u| {
                        role_to_string(&u.role) == def.role
                    }).count();

                    rsx! {
                        div { class: "card", style: "padding:16px;border-top:3px solid {def.color};",
                            div { style: "display:flex;align-items:center;justify-content:space-between;margin-bottom:8px;",
                                span {
                                    class: "chip",
                                    style: "background:color-mix(in oklab, {def.color} 16%, transparent);color:{def.color};font-size:12px;font-weight:600;",
                                    "{def.role}"
                                }
                                span { style: "font-size:11px;color:var(--cf-text-muted);", "{user_count} users" }
                            }
                            div { style: "font-size:12px;color:var(--cf-text-secondary);margin-bottom:12px;line-height:1.5;", "{def.desc}" }
                            div { style: "display:flex;flex-direction:column;gap:6px;",
                                for perm in def.perms {
                                    div { style: "display:flex;align-items:center;gap:8px;font-size:12px;",
                                        svg { width: "12", height: "12", view_box: "0 0 24 24", fill: "none", stroke: "{def.color}", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "flex-shrink:0;",
                                            polyline { points: "20 6 9 17 4 12" }
                                        }
                                        span { "{perm}" }
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
// OIDC TAB
// ============================================================================

#[component]
fn OidcTab(
    oidc_mappings: Signal<Vec<OidcGroupMapping>>,
    oidc_error: Signal<Option<String>>,
    auth_mode: AuthMode,
    environments: Vec<EnvironmentSummary>,
) -> Element {
    let mut mapping_group = use_signal(String::new);
    let mut mapping_role = use_signal(|| "Viewer".to_string());
    let mut mapping_environments = use_signal(String::new);
    let mut mapping_submitting = use_signal(|| false);
    let mut mapping_modal_mode = use_signal(|| Option::<String>::None);
    let mut editing_mapping_id = use_signal(|| Option::<String>::None);
    let duplicate_group_counts = {
        let mut counts = HashMap::<String, usize>::new();
        for mapping in oidc_mappings.read().iter() {
            let key = mapping.group_name.trim().to_lowercase();
            *counts.entry(key).or_insert(0) += 1;
        }
        counts
    };

    rsx! {
        div { style: "padding:0;",
            // Connection status callout (only show if OIDC)
            if auth_mode == AuthMode::Oidc {
                div { style: "padding:14px 16px;border-bottom:1px solid var(--cf-divider);",
                    div { class: "sd-callout sd-callout-info", style: "font-size:12px;",
                        svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                            path { d: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" }
                            path { d: "M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" }
                        }
                        div {
                            "When a user logs in, their IdP groups are matched top-down; the first matching mapping sets their role and environment scope."
                        }
                    }
                }
            }

            // Add mapping button
            div { style: "padding:10px 16px;display:flex;justify-content:flex-end;",
                button { class: "btn btn-primary focus-ring",
                    onclick: move |_| {
                        mapping_group.set(String::new());
                        mapping_role.set("Viewer".to_string());
                        mapping_environments.set(String::new());
                        editing_mapping_id.set(None);
                        mapping_modal_mode.set(Some("add".to_string()));
                    },
                    svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:5px;vertical-align:text-bottom;",
                        line { x1: "12", y1: "5", x2: "12", y2: "19" }
                        line { x1: "5", y1: "12", x2: "19", y2: "12" }
                    }
                    "Add mapping"
                }
            }
            if let Some(err) = oidc_error.read().as_ref() {
                div { style: "padding:0 16px 10px;",
                    div { class: "sd-callout sd-callout-danger", style: "font-size:12px;", "{err}" }
                }
            }

            // OIDC mappings table
            table { class: "sys-table",
                thead {
                    tr {
                        th { style: "width:60px;", "Priority" }
                        th { "IdP Group" }
                        th { "CF Role" }
                        th { "Environments" }
                        th { "Matched users" }
                        th { style: "text-align:right;", " " }
                    }
                }
                tbody {
                    for (idx, mapping) in oidc_mappings.read().iter().enumerate() {
                        {
                            let role_def = get_role_definition(mapping.role);
                            let duplicate_count = duplicate_group_counts
                                .get(&mapping.group_name.trim().to_lowercase())
                                .copied()
                                .unwrap_or(0);
                            rsx! {
                                tr {
                                    td {
                                        span { class: "mono", style: "font-size:12px;color:var(--cf-text-muted);", "#{idx + 1}" }
                                    }
                                    td {
                                        div { style: "display:flex;align-items:center;gap:8px;flex-wrap:wrap;",
                                            span { class: "mono", style: "font-weight:600;font-size:13px;", "{mapping.group_name}" }
                                            if duplicate_count > 1 {
                                                span { class: "chip chip-warning", style: "font-size:9px;", "duplicate ×{duplicate_count}" }
                                            }
                                        }
                                    }
                                    td {
                                        span {
                                            class: "chip",
                                            style: "background:color-mix(in oklab, {role_def.color} 16%, transparent);color:{role_def.color};font-size:10px;",
                                            "{role_def.role}"
                                        }
                                    }
                                    td {
                                        div { style: "display:flex;gap:4px;flex-wrap:wrap;",
                                            if mapping.environments.is_empty() {
                                                span { style: "font-size:11px;color:var(--cf-text-muted);", "none" }
                                            } else {
                                                for env in &mapping.environments {
                                                    span { class: "chip chip-unknown", style: "font-size:10px;", "{env}" }
                                                }
                                            }
                                        }
                                    }
                                    td { span { class: "chip chip-unknown", style: "font-size:10px;", "unavailable" } }
                                    td {
                                        div { class: "row-actions",
                                            button {
                                                class: "btn-icon focus-ring",
                                                title: "Edit",
                                                onclick: {
                                                    let mapping_id = mapping.id.clone();
                                                    let group_name = mapping.group_name.clone();
                                                    let role = role_to_string(&mapping.role);
                                                    let environments = mapping.environments.join(", ");
                                                    move |_| {
                                                        editing_mapping_id.set(Some(mapping_id.clone()));
                                                        mapping_group.set(group_name.clone());
                                                        mapping_role.set(role.clone());
                                                        mapping_environments.set(environments.clone());
                                                        mapping_modal_mode.set(Some("edit".to_string()));
                                                    }
                                                },
                                                svg { width: "14", height: "14", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                                    path { d: "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" }
                                                    circle { cx: "12", cy: "12", r: "3" }
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

            if let Some(mode) = mapping_modal_mode.read().clone() {
                OidcMappingModal {
                    mode,
                    editing_mapping_id,
                    mapping_group,
                    mapping_role,
                    mapping_environments,
                    mapping_submitting,
                    oidc_mappings,
                    oidc_error,
                    environments,
                    on_close: move |_| mapping_modal_mode.set(None),
                }
            }
        }
    }
}

#[component]
fn OidcMappingModal(
    mode: String,
    editing_mapping_id: Signal<Option<String>>,
    mapping_group: Signal<String>,
    mapping_role: Signal<String>,
    mapping_environments: Signal<String>,
    mapping_submitting: Signal<bool>,
    oidc_mappings: Signal<Vec<OidcGroupMapping>>,
    oidc_error: Signal<Option<String>>,
    environments: Vec<EnvironmentSummary>,
    on_close: EventHandler<()>,
) -> Element {
    let is_edit = mode == "edit";
    let title = if is_edit {
        "Edit mapping"
    } else {
        "Add mapping"
    };

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_close.call(()),
            div { class: "modal", style: "width:min(520px,96vw);", onclick: move |evt| evt.stop_propagation(),
                div { class: "modal-head",
                    div {
                        h3 { style: "margin:0;font-size:15px;display:flex;align-items:center;gap:6px;",
                            if is_edit {
                                svg { width: "14", height: "14", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                    path { d: "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" }
                                    circle { cx: "12", cy: "12", r: "3" }
                                }
                            } else {
                                svg { width: "14", height: "14", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                    line { x1: "12", y1: "5", x2: "12", y2: "19" }
                                    line { x1: "5", y1: "12", x2: "19", y2: "12" }
                                }
                            }
                            "{title}"
                        }
                        p { style: "margin:4px 0 0;font-size:12px;color:var(--cf-text-muted);", "Map an IdP group to a Crystal Forge role and environment scope." }
                    }
                }
                div { class: "modal-body", style: "display:grid;gap:14px;",
                    div { class: "field", style: "margin:0;",
                        label { "IdP group name" }
                        input {
                            class: "input focus-ring mono",
                            value: "{mapping_group.read()}",
                            placeholder: "e.g. cf-operators",
                            style: "font-size:12px;",
                            disabled: is_edit,
                            readonly: is_edit,
                            oninput: move |evt| mapping_group.set(evt.value())
                        }
                        if is_edit {
                            div { class: "help",
                                "Group names are read-only while editing because the current backend saves mappings by group name. To rename safely, remove this mapping and add a new one."
                            }
                        }
                    }
                    div { class: "field", style: "margin:0;",
                        label { "CF role" }
                        div { class: "seg", style: "width:fit-content;",
                            for role in ["Admin", "Operator", "Viewer"] {
                                button {
                                    class: if *mapping_role.read() == role { "active" } else { "" },
                                    onclick: move |_| mapping_role.set(role.to_string()),
                                    "{role}"
                                }
                            }
                        }
                    }
                    div { class: "field", style: "margin:0;",
                        EnvironmentChipPicker {
                            title: "Environments".to_string(),
                            selected_csv: mapping_environments,
                            available: environments.clone(),
                        }
                    }
                }
                div { class: "modal-foot",
                    if is_edit {
                        button {
                            class: "btn btn-ghost focus-ring",
                            style: "margin-right:auto;color:#f87171;border-color:rgba(248,113,113,0.3);",
                            disabled: *mapping_submitting.read(),
                            onclick: move |_| {
                                if let Some(id) = editing_mapping_id.read().clone() {
                                    let confirmed = web_sys::window()
                                        .and_then(|window| window.confirm_with_message("Remove this OIDC group mapping?").ok())
                                        .unwrap_or(false);
                                    if !confirmed {
                                        return;
                                    }
                                    mapping_submitting.set(true);
                                    spawn(async move {
                                        match delete_admin_oidc_mapping(&id).await {
                                            Ok(()) => {
                                                oidc_mappings.with_mut(|items| items.retain(|mapping| mapping.id != id));
                                                oidc_error.set(None);
                                                mapping_submitting.set(false);
                                                on_close.call(());
                                            }
                                            Err(err) => {
                                                oidc_error.set(Some(err.to_string()));
                                                mapping_submitting.set(false);
                                            }
                                        }
                                    });
                                }
                            },
                            "Remove"
                        }
                    }
                    button { class: "btn btn-ghost focus-ring", onclick: move |_| on_close.call(()), "Cancel" }
                    button {
                        class: "btn btn-primary focus-ring",
                        disabled: *mapping_submitting.read(),
                        onclick: move |_| {
                            let unknown = unknown_environments(&mapping_environments.read(), &environments);
                            if !unknown.is_empty() {
                                oidc_error.set(Some(format!(
                                    "Unknown environment(s): {}. Use the environment chips or choose all.",
                                    unknown.join(", ")
                                )));
                                return;
                            }
                            let request = AdminUpsertOidcMappingRequest {
                                group_name: mapping_group.read().trim().to_string(),
                                role: Some(parse_role(&mapping_role.read())),
                                environments: parse_environments(&mapping_environments.read()),
                            };
                            mapping_submitting.set(true);
                            spawn(async move {
                                match upsert_admin_oidc_mapping(&request).await {
                                    Ok(updated_mapping) => {
                                        oidc_mappings.with_mut(|items| {
                                            if let Some(existing) = items.iter_mut().find(|mapping| mapping.id == updated_mapping.id || mapping.group_name == updated_mapping.group_name) {
                                                *existing = updated_mapping.clone();
                                            } else {
                                                items.push(updated_mapping.clone());
                                            }
                                        });
                                        oidc_error.set(None);
                                        mapping_submitting.set(false);
                                        on_close.call(());
                                    }
                                    Err(err) => {
                                        oidc_error.set(Some(err.to_string()));
                                        mapping_submitting.set(false);
                                    }
                                }
                            });
                        },
                        if *mapping_submitting.read() { "Saving…" } else if is_edit { "Save" } else { "Add" }
                    }
                }
            }
        }
    }
}

// ============================================================================
// BACKGROUND JOBS TAB
// ============================================================================

#[component]
fn JobsTab() -> Element {
    rsx! {
        div { style: "padding:0;",
            // Info callout
            div { style: "padding:14px 16px;border-bottom:1px solid var(--cf-divider);",
                div { class: "sd-callout sd-callout-info", style: "font-size:12px;",
                    svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                        path { d: "M20 12a8 8 0 0 1-14 5.3L3 14m1-4a8 8 0 0 1 14-5.3L21 8M21 3v5h-5M3 21v-5h5" }
                    }
                    div {
                        strong { "Background jobs unavailable" }
                        div { style: "margin-top:4px;", "Live scheduler data and job actions are not implemented yet. Tracked by TASK-336.5." }
                    }
                }
            }

            // Jobs table
            if false {
            table { class: "sys-table",
                thead {
                    tr {
                        th { "Job" }
                        th { "Interval" }
                        th { "Status" }
                        th { "Load" }
                        th { "Last run" }
                        th { "Next run" }
                        th { style: "text-align:right;", "Enabled" }
                    }
                }
                tbody {
                    for job in BACKGROUND_JOBS_MOCK {
                        {
                            let (status_class, status_label) = ("chip-unknown", "unavailable");
                            let (impact_class, impact_label) = ("chip-unknown", "unavailable");

                            rsx! {
                                tr {
                                    td {
                                        div { style: "font-weight:600;font-size:13px;", "{job.name}" }
                                        div { style: "font-size:11px;color:var(--cf-text-muted);max-width:380px;", "{job.desc}" }
                                    }
                                    td {
                                        span { class: "mono chip chip-unknown", style: "font-size:10px;", "{job.interval}" }
                                    }
                                    td {
                                        if job.enabled && job.note.is_some() {
                                            span { class: "chip {status_class}", style: "font-size:10px;", title: "{job.note.unwrap_or(\"\")}", "{status_label}" }
                                        } else {
                                            span { class: "chip {status_class}", style: "font-size:10px;", "{status_label}" }
                                        }
                                    }
                                    td {
                                        span { class: "chip {impact_class}", style: "font-size:10px;", "{impact_label}" }
                                    }
                                    td { style: "font-size:12px;color:var(--cf-text-muted);",
                                        "{job.last_run}"
                                        if job.enabled && job.last_duration != "—" {
                                            span { style: "opacity:0.6;", " · {job.last_duration}" }
                                        }
                                    }
                                    td { style: "font-size:12px;color:var(--cf-text-muted);", "{job.next_run}" }
                                    td {
                                        div { style: "display:flex;justify-content:flex-end;gap:6px;align-items:center;",
                                            button { class: "btn-icon focus-ring", title: "Run now",
                                                svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                                    path { d: "M20 12a8 8 0 0 1-14 5.3L3 14m1-4a8 8 0 0 1 14-5.3L21 8M21 3v5h-5M3 21v-5h5" }
                                                }
                                            }
                                            input {
                                                r#type: "checkbox",
                                                checked: job.enabled,
                                                style: "accent-color:var(--cf-brand-purple);width:16px;height:16px;cursor:pointer;"
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
        }
    }
}

// ============================================================================
// AUDIT LOG TAB
// ============================================================================

#[component]
fn AuditTab(
    audit_events: Signal<Vec<AuditEvent>>,
    audit_loading: bool,
    audit_error: Signal<Option<String>>,
    actor_filter: Signal<String>,
    audit_page: Signal<i64>,
    can_go_prev: bool,
    can_go_next: bool,
    total_pages: i64,
    audit_total: i64,
) -> Element {
    let filtered_events = audit_events.read().clone();

    rsx! {
        div { style: "padding:0;",
            // Search and filter bar
            div { style: "padding:12px 16px;border-bottom:1px solid var(--cf-divider);display:flex;gap:10px;align-items:center;flex-wrap:wrap;",
                div { class: "filter-search", style: "max-width:260px;",
                    svg { width: "16", height: "16", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                        circle { cx: "11", cy: "11", r: "8" }
                        path { d: "m21 21-4.35-4.35" }
                    }
                    input {
                        class: "input focus-ring",
                        placeholder: "Search actor…",
                        value: "{actor_filter.read()}",
                        oninput: move |evt| {
                            actor_filter.set(evt.value());
                            audit_page.set(1);
                        }
                    }
                }
                div { class: "sd-callout sd-callout-info", style: "font-size:11px;padding:7px 9px;",
                    "Category filtering is not implemented yet; showing real audit results from the API."
                }
                span { class: "filter-count", "{filtered_events.len()} events" }
                button { class: "btn btn-ghost focus-ring", style: "margin-left:auto;", disabled: true, title: "Audit export is tracked by TASK-336.8",
                    svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:5px;vertical-align:text-bottom;",
                        path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3" }
                    }
                    "Export · unavailable"
                }
            }

            // Audit log table
            if audit_loading {
                div { style: "padding:16px;color:var(--cf-text-muted);font-size:13px;", "Loading audit events..." }
            } else if let Some(err) = audit_error.read().as_ref() {
                div { style: "padding:16px;",
                    div { class: "sd-callout sd-callout-danger", "{err}" }
                }
            } else if filtered_events.is_empty() {
                div { style: "padding:16px;color:var(--cf-text-muted);font-size:13px;", "No audit events match the selected filters." }
            } else {
                table { class: "sys-table",
                    thead {
                        tr {
                            th { "When" }
                            th { "Actor" }
                            th { "Action" }
                            th { "Target" }
                            th { "Category" }
                            th { "Source IP" }
                        }
                    }
                    tbody {
                        for event in filtered_events {
                            {
                                // Mock action categorization and color
                                let (action_color, category_class, category_label) = get_audit_action_color(&event.action);

                                rsx! {
                                    tr {
                                        td { style: "font-size:12px;color:var(--cf-text-muted);white-space:nowrap;", "{format_time(event.timestamp)}" }
                                        td { class: "mono", style: "font-size:12px;font-weight:600;", "{event.actor.clone().unwrap_or_else(|| \"system\".to_string())}" }
                                        td { class: "mono", style: "font-size:12px;color:{action_color};", "{format_action_label(&event.action)}" }
                                        td { style: "font-size:12px;", "{event.target}" }
                                        td {
                                            span { class: "chip {category_class}", style: "font-size:10px;", "{category_label}" }
                                        }
                                        td { class: "mono", style: "font-size:11px;color:var(--cf-text-muted);", "unavailable" }
                                    }
                                }
                            }
                        }
                    }
                }

                // Pagination
                div { style: "display:flex;align-items:center;justify-content:space-between;padding:12px 16px;border-top:1px solid var(--cf-divider);",
                    p { style: "font-size:12px;color:var(--cf-text-muted);margin:0;",
                        "Page {audit_page.read()} of {total_pages} ({audit_total} total)"
                    }
                    div { style: "display:flex;align-items:center;gap:8px;",
                        button {
                            class: "btn btn-ghost focus-ring",
                            disabled: !can_go_prev,
                            onclick: move |_| {
                                let current = *audit_page.read();
                                if current > 1 {
                                    audit_page.set(current - 1);
                                }
                            },
                            "Previous"
                        }
                        button {
                            class: "btn btn-ghost focus-ring",
                            disabled: !can_go_next,
                            onclick: move |_| {
                                let current = *audit_page.read();
                                audit_page.set(current + 1);
                            },
                            "Next"
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// SERVER TAB
// ============================================================================

fn short_commit(commit: &str) -> String {
    commit.chars().take(12).collect()
}

fn format_uptime(total_seconds: u64) -> String {
    let days = total_seconds / 86_400;
    let hours = (total_seconds % 86_400) / 3_600;
    let minutes = (total_seconds % 3_600) / 60;

    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m")
    } else {
        format!("{total_seconds}s")
    }
}

#[component]
fn ServerTab(
    environments: Vec<EnvironmentSummary>,
    environments_error: Option<String>,
    server_info: Option<ServerRuntimeInfoResponse>,
    server_info_loading: bool,
    server_info_error: Option<String>,
    auth_mode: AuthMode,
    classification_enabled: Signal<bool>,
    classification_level: Signal<String>,
    classification_custom_text: Signal<String>,
    classification_loaded: Signal<bool>,
    classification_fetch_error: Option<String>,
    on_classification_dirty: EventHandler<()>,
    on_classification_retry: EventHandler<()>,
) -> Element {
    let nav = navigator();
    let auth_mode_label = match auth_mode {
        AuthMode::Dev => "Dev",
        AuthMode::Local => "Local",
        AuthMode::Oidc => "OIDC",
    };
    let server_info_error_text = server_info_error.clone();

    rsx! {
        div { style: "padding:16px;display:grid;grid-template-columns:1fr 1fr;gap:14px;",
            // Build info card
            div { class: "card", style: "padding:16px;",
                h3 { style: "margin:0 0 12px;font-size:13px;font-weight:600;", "Build info" }
                if server_info_loading {
                    div { style: "font-size:12px;color:var(--cf-text-muted);", "Loading server info…" }
                } else if let Some(ref err) = server_info_error_text {
                    div { class: "sd-callout sd-callout-danger", style: "font-size:12px;", "{err}" }
                } else if let Some(ref info) = server_info {
                    dl { class: "kv-grid",
                        dt { "Version" }
                        dd { class: "mono", "{info.version}" }
                        dt { "Commit" }
                        dd { class: "mono",
                            if let Some(ref commit) = info.commit {
                                "{short_commit(commit)}"
                            } else {
                                span { class: "chip chip-unknown", "unavailable" }
                            }
                        }
                        dt { "Uptime" }
                        dd { "{format_uptime(info.uptime_seconds)}" }
                        dt { "Database" }
                        dd {
                            span { class: "chip chip-healthy", "{info.database.status}" }
                            " "
                            span { style: "color:var(--cf-text-muted);", "· {info.database.size}" }
                        }
                    }
                } else {
                    div { style: "font-size:12px;color:var(--cf-text-muted);", "Server info unavailable." }
                }
            }

            // Authentication info card
            div { class: "card", style: "padding:16px;",
                h3 { style: "margin:0 0 12px;font-size:13px;font-weight:600;", "Authentication" }
                dl { class: "kv-grid",
                    dt { "Mode" }
                    dd { "{auth_mode_label}" }
                    dt { "OIDC issuer" }
                    dd { span { class: "chip chip-unknown", "unavailable" } span { style: "color:var(--cf-text-muted);", " · API not implemented yet" } }
                    dt { "Sessions" }
                    dd { span { class: "chip chip-unknown", "unavailable" } }
                    dt { "TLS expiry" }
                    dd {
                        span { class: "chip chip-unknown", "unavailable" }
                    }
                }
            }

            // Agent heartbeat config card
            HeartbeatConfigCard {
                environments,
                environments_error,
            }

            // Classification banners card
            ClassificationBannerCard {
                enabled: classification_enabled,
                level: classification_level,
                custom_text: classification_custom_text,
                loaded: classification_loaded,
                fetch_error: classification_fetch_error,
                on_dirty: move |_| on_classification_dirty.call(()),
                on_retry: move |_| on_classification_retry.call(()),
            }

            // Onboarding card
            div { class: "card", style: "padding:16px;grid-column:1 / -1;",
                div { style: "display:flex;align-items:center;justify-content:space-between;gap:12px;flex-wrap:wrap;",
                    div {
                        h3 { style: "margin:0 0 4px;font-size:13px;font-weight:600;display:flex;align-items:center;gap:7px;",
                            svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                rect { x: "3", y: "3", width: "7", height: "7" }
                                rect { x: "14", y: "3", width: "7", height: "7" }
                                rect { x: "14", y: "14", width: "7", height: "7" }
                                rect { x: "3", y: "14", width: "7", height: "7" }
                            }
                            "Onboarding"
                        }
                        p { style: "margin:0;font-size:12px;color:var(--cf-text-muted);",
                            "The Setup Coach walks admins through first-run configuration."
                        }
                    }
                    div { style: "display:flex;gap:8px;flex-wrap:wrap;",
                        button {
                            class: "btn btn-primary focus-ring",
                            onclick: move |_| {
                                spawn(async move {
                                    let _ = set_setup_wizard_dismissed(false).await;
                                    if let Some(storage) = web_sys::window()
                                        .and_then(|w| w.local_storage().ok())
                                        .flatten()
                                    {
                                        let _ = storage.set_item("cf.coach.collapsed", "false");
                                        let _ = storage.set_item("cf.coach.force_show", "true");
                                    }
                                    nav.push("/");
                                });
                            },
                            svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:5px;vertical-align:text-bottom;",
                                path { d: "M20 12a8 8 0 0 1-14 5.3L3 14m1-4a8 8 0 0 1 14-5.3L21 8M21 3v5h-5M3 21v-5h5" }
                            }
                            "Relaunch Setup Coach"
                        }
                        button { class: "btn btn-ghost focus-ring", disabled: true, title: "Reset progress is not implemented yet", "Reset progress · unavailable" }
                    }
                }
            }

            // Maintenance card
            div { class: "card", style: "padding:16px;grid-column:1 / -1;",
                h3 { style: "margin:0 0 12px;font-size:13px;font-weight:600;", "Maintenance" }
                div { style: "display:flex;gap:8px;flex-wrap:wrap;",
                    button { class: "btn btn-ghost focus-ring", disabled: true, title: "Tracked by TASK-336.8",
                        svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:5px;vertical-align:text-bottom;",
                            path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3" }
                        }
                        "Backup database · unavailable"
                    }
                    button { class: "btn btn-ghost focus-ring", disabled: true, title: "Tracked by TASK-336.8",
                        svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:5px;vertical-align:text-bottom;",
                            path { d: "M20 12a8 8 0 0 1-14 5.3L3 14m1-4a8 8 0 0 1 14-5.3L21 8M21 3v5h-5M3 21v-5h5" }
                        }
                        "Reload config · unavailable"
                    }
                    button { class: "btn btn-ghost focus-ring", disabled: true, title: "Tracked by TASK-336.8",
                        svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:5px;vertical-align:text-bottom;",
                            path { d: "M12 8v4l3 3m6-3a9 9 0 1 1-18 0 9 9 0 0 1 18 0z" }
                        }
                        "Export audit log · unavailable"
                    }
                    button { class: "btn btn-ghost focus-ring", style: "color:#fbbf24;border-color:rgba(251,191,36,0.3);", disabled: true, title: "Tracked by TASK-336.8",
                        svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:5px;vertical-align:text-bottom;",
                            path { d: "M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" }
                            line { x1: "12", y1: "9", x2: "12", y2: "13" }
                            line { x1: "12", y1: "17", x2: "12.01", y2: "17" }
                        }
                        "Invalidate all sessions · unavailable"
                    }
                }
            }
        }
    }
}

// ============================================================================
// CLASSIFICATION BANNER CARD
// ============================================================================

#[component]
fn ClassificationBannerCard(
    enabled: Signal<bool>,
    level: Signal<String>,
    custom_text: Signal<String>,
    /// True once a real persisted value has been loaded into the signals.
    loaded: Signal<bool>,
    /// Set if the initial config fetch failed; None while loading or after success.
    fetch_error: Option<String>,
    /// Called when the user makes any unsaved edit so the parent can guard syncs.
    on_dirty: EventHandler<()>,
    /// Called to request a retry of the initial fetch after a failure.
    on_retry: EventHandler<()>,
) -> Element {
    let mut saving = use_signal(|| false);
    let mut save_error = use_signal(|| Option::<String>::None);
    let mut app_state = use_context::<Signal<AppState>>();
    let controls_disabled = *saving.read() || !*loaded.read();

    // Classification levels with colors
    let levels = [
        "UNCLASSIFIED",
        "CUI",
        "CONFIDENTIAL",
        "SECRET",
        "TOP SECRET",
    ];

    rsx! {
        div { class: "card", style: "padding:16px;grid-column:1 / -1;",
            div { style: "display:flex;align-items:flex-start;justify-content:space-between;gap:12px;flex-wrap:wrap;margin-bottom:14px;",
                div {
                    h3 { style: "margin:0 0 4px;font-size:13px;font-weight:600;display:flex;align-items:center;gap:7px;",
                        svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                            path { d: "M12 3l8 3v6c0 4.5-3.3 8.5-8 9-4.7-.5-8-4.5-8-9V6l8-3z" }
                        }
                        "Classification banners"
                    }
                    p { style: "margin:0;font-size:12px;color:var(--cf-text-muted);max-width:60ch;",
                        "Display a CNSS/DoD classification marking at the top and bottom of every screen. Required on many DoD / IC information systems."
                    }
                }
                // State banner: loading / fetch error with retry / nothing when ready
                if let Some(ref err) = fetch_error {
                    div { class: "sd-callout sd-callout-danger", style: "font-size:12px;display:flex;align-items:center;gap:10px;",
                        span { style: "flex:1;", "Failed to load configuration: {err}" }
                        button {
                            class: "btn btn-ghost focus-ring",
                            style: "font-size:11px;padding:4px 10px;",
                            onclick: move |_| on_retry.call(()),
                            "Retry"
                        }
                    }
                } else if !*loaded.read() {
                    div { style: "font-size:11px;color:var(--cf-text-muted);", "Loading…" }
                }
                // Toggle switch — toggling immediately saves
                button {
                    class: "focus-ring",
                    role: "switch",
                    "aria-checked": "{enabled}",
                    disabled: controls_disabled,
                    onclick: move |_| {
                        on_dirty.call(());
                        let new_enabled = !*enabled.read();
                        enabled.set(new_enabled);
                        let req = UpdateClassificationBannerRequest {
                            enabled: new_enabled,
                            level: level.read().clone(),
                            custom_text: custom_text.read().clone(),
                        };
                        saving.set(true);
                        save_error.set(None);
                        spawn(async move {
                            match set_classification_config(&req).await {
                                Ok(cfg) => {
                                    app_state.write().classification_config = Some(ClassificationBannerConfig {
                                        enabled: cfg.enabled,
                                        level: cfg.level.clone(),
                                        custom_text: cfg.custom_text.clone(),
                                    });
                                    enabled.set(cfg.enabled);
                                    level.set(cfg.level);
                                    custom_text.set(cfg.custom_text);
                                }
                                Err(e) => {
                                    enabled.set(!new_enabled); // revert
                                    save_error.set(Some(e.to_string()));
                                }
                            }
                            saving.set(false);
                        });
                    },
                    style: if *enabled.read() {
                        "flex-shrink:0;width:44px;height:24px;border-radius:999px;background:var(--cf-brand-purple);position:relative;cursor:pointer;border:none;padding:0;transition:background 140ms;"
                    } else {
                        "flex-shrink:0;width:44px;height:24px;border-radius:999px;background:var(--cf-subtle-bg);position:relative;cursor:pointer;border:none;padding:0;transition:background 140ms;"
                    },
                    div {
                        style: if *enabled.read() {
                            "position:absolute;top:2px;left:22px;width:20px;height:20px;border-radius:50%;background:#fff;box-shadow:0 1px 3px rgba(0,0,0,0.3);transition:left 140ms;"
                        } else {
                            "position:absolute;top:2px;left:2px;width:20px;height:20px;border-radius:50%;background:#fff;box-shadow:0 1px 3px rgba(0,0,0,0.3);transition:left 140ms;"
                        }
                    }
                }
            }

            if let Some(ref err) = *save_error.read() {
                div { class: "sd-callout sd-callout-danger", style: "font-size:12px;margin-bottom:10px;", "{err}" }
            }

            // Configuration section (only show when enabled)
            if *enabled.read() {
                div { style: "border-top:1px solid var(--cf-divider);padding-top:14px;display:grid;grid-template-columns:1fr 1fr;gap:14px;",
                    div { class: "field", style: "margin:0;",
                        label { "Classification level" }
                        select {
                            class: "input focus-ring",
                            value: "{level.read()}",
                            onchange: move |evt| { on_dirty.call(()); level.set(evt.value()); },
                            for lvl in &levels {
                                option { value: "{lvl}", "{lvl}" }
                            }
                        }
                    }
                    div { class: "field", style: "margin:0;",
                        label { "Custom marking " span { style: "color:var(--cf-text-muted);font-weight:400;", "· optional" } }
                        input {
                            class: "input focus-ring",
                            value: "{custom_text.read()}",
                            placeholder: "e.g. UNCLASSIFIED//FOUO",
                            oninput: move |evt| { on_dirty.call(()); custom_text.set(evt.value()); }
                        }
                    }
                    div { style: "grid-column:1 / -1;",
                        div { style: "font-size:11px;color:var(--cf-text-muted);margin-bottom:6px;", "Preview" }
                        {
                            let display_text = classification_display_text(&level.read(), &custom_text.read());
                            let (bg, fg) = classification_colors(&level.read());

                            rsx! {
                                div {
                                    style: "height:24px;border-radius:6px;display:flex;align-items:center;justify-content:center;font-size:12px;font-weight:700;letter-spacing:0.08em;text-transform:uppercase;background:{bg};color:{fg};",
                                    "{display_text}"
                                }
                            }
                        }
                    }
                    div { style: "grid-column:1 / -1;display:flex;justify-content:flex-end;",
                        button {
                            class: "btn btn-primary focus-ring",
                            disabled: controls_disabled,
                            onclick: move |_| {
                                on_dirty.call(());
                                let req = UpdateClassificationBannerRequest {
                                    enabled: *enabled.read(),
                                    level: level.read().clone(),
                                    custom_text: custom_text.read().clone(),
                                };
                                saving.set(true);
                                save_error.set(None);
                                spawn(async move {
                                    match set_classification_config(&req).await {
                                        Ok(cfg) => {
                                            app_state.write().classification_config = Some(ClassificationBannerConfig {
                                                enabled: cfg.enabled,
                                                level: cfg.level.clone(),
                                                custom_text: cfg.custom_text.clone(),
                                            });
                                            enabled.set(cfg.enabled);
                                            level.set(cfg.level);
                                            custom_text.set(cfg.custom_text);
                                        }
                                        Err(e) => save_error.set(Some(e.to_string())),
                                    }
                                    saving.set(false);
                                });
                            },
                            if *saving.read() { "Saving…" } else { "Save banner config" }
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// HEARTBEAT CONFIG CARD
// ============================================================================

#[component]
fn HeartbeatConfigCard(
    environments: Vec<EnvironmentSummary>,
    environments_error: Option<String>,
) -> Element {
    let rows = environments
        .iter()
        .filter(|env| env.is_active)
        .cloned()
        .collect::<Vec<_>>();

    rsx! {
        div { class: "card", style: "padding:16px;grid-column:1 / -1;",
            div { style: "display:flex;align-items:center;justify-content:space-between;margin-bottom:4px;",
                h3 { style: "margin:0;font-size:13px;font-weight:600;", "Agent heartbeat" }
                span { style: "font-size:11px;color:var(--cf-text-muted);", "How often agents phone home — drives the Systems heartbeat indicator" }
            }
            div { class: "sd-callout sd-callout-info", style: "font-size:11px;margin:10px 0 14px;",
                svg { width: "12", height: "12", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                    rect { x: "3", y: "4", width: "18", height: "8", rx: "2" }
                    rect { x: "3", y: "14", width: "18", height: "6", rx: "2" }
                }
                div { "Lower intervals detect drift & outages faster but add load and chatter (costly on metered edge links). Each environment can override the global default." }
            }

            // Global settings
            div { style: "display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:16px;margin-bottom:16px;",
                div { class: "field",
                    label { "Global default" }
                    select { class: "input focus-ring", style: "width:120px;",
                        option { "1m" }
                    }
                    div { class: "help", "Applied to any environment without an override." }
                }
                div { class: "field",
                    label { "Mark stale after" }
                    select { class: "input focus-ring", style: "width:160px;",
                        option { "2 missed (2m)" }
                    }
                }
                div { class: "field",
                    label { "Mark offline after" }
                    select { class: "input focus-ring", style: "width:160px;",
                        option { "5 missed (5m)" }
                    }
                }
            }

            // Per-environment overrides table
            div {
                div { style: "font-size:11px;text-transform:uppercase;letter-spacing:0.06em;color:var(--cf-text-muted);font-weight:600;margin-bottom:8px;", "Per-environment overrides" }
                if let Some(err) = environments_error.as_ref() {
                    div { class: "sd-callout sd-callout-warning", style: "font-size:11px;margin-bottom:8px;",
                        "{err}. Showing no environment-specific overrides until environments load."
                    }
                }
                div { style: "display:flex;flex-direction:column;gap:0;border:1px solid var(--cf-divider);border-radius:8px;overflow:hidden;",
                    if rows.is_empty() {
                        div { style: "padding:12px;color:var(--cf-text-muted);font-size:12px;",
                            "No active environments found."
                        }
                    } else {
                        for (idx, env) in rows.iter().enumerate() {
                            div {
                                style: if idx > 0 { "display:flex;align-items:center;gap:12px;padding:10px 12px;border-top:1px solid var(--cf-divider);" } else { "display:flex;align-items:center;gap:12px;padding:10px 12px;" },
                                span { style: "width:8px;height:8px;border-radius:50%;background:{env.color_hex};flex-shrink:0;" }
                                div { style: "flex:1;min-width:0;",
                                    span { class: "mono", style: "font-size:13px;font-weight:600;", "{env.name}" }
                                    span { style: "font-size:11px;color:var(--cf-text-muted);margin-left:8px;", "{env.system_count} systems" }
                                }
                                span { style: "font-size:11px;color:var(--cf-text-muted);", "inherits global · 1m" }
                                select { class: "input focus-ring", style: "width:120px;",
                                    option { value: "", "(global)" }
                                    option { "30s" }
                                    option { "1m" }
                                    option { "2m" }
                                    option { "5m" }
                                }
                            }
                        }
                    }
                }
            }

            // Save button
            div { style: "display:flex;justify-content:flex-end;gap:8px;margin-top:14px;",
                button { class: "btn btn-ghost focus-ring", "Reset" }
                button { class: "btn btn-primary focus-ring",
                    svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:5px;vertical-align:text-bottom;",
                        polyline { points: "20 6 9 17 4 12" }
                    }
                    "Save heartbeat config"
                }
            }
        }
    }
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

fn get_role_definition(role: Option<Role>) -> &'static RoleDefinition {
    let role_str = role_to_string(&role);
    ROLE_DEFINITIONS
        .iter()
        .find(|d| d.role == role_str)
        .unwrap_or(&ROLE_DEFINITIONS[2]) // Default to Viewer
}

fn role_to_string(role: &Option<Role>) -> String {
    match role {
        Some(Role::Admin) => "Admin".to_string(),
        Some(Role::Operator) => "Operator".to_string(),
        Some(Role::Viewer) | None => "Viewer".to_string(),
    }
}

fn parse_role(role: &str) -> Role {
    match role {
        "Admin" => Role::Admin,
        "Operator" => Role::Operator,
        _ => Role::Viewer,
    }
}

fn parse_environments(environments: &str) -> Vec<String> {
    environments
        .split(',')
        .map(str::trim)
        .filter(|env| !env.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn unknown_environments(environments: &str, available: &[EnvironmentSummary]) -> Vec<String> {
    let known = available
        .iter()
        .map(|env| env.name.as_str())
        .collect::<Vec<_>>();

    parse_environments(environments)
        .into_iter()
        .filter(|selected| !known.iter().any(|known_name| known_name == selected))
        .collect()
}

fn toggle_environment_selection(current: &str, env_name: &str) -> String {
    let mut selected = parse_environments(current);
    if selected.iter().any(|value| value == env_name) {
        selected.retain(|value| value != env_name);
    } else {
        selected.push(env_name.to_string());
    }
    selected.join(", ")
}

fn environment_pill_style(selected: bool, color: &str) -> String {
    if selected {
        format!(
            "border:1px solid {color};background:color-mix(in oklab, {color} 18%, var(--cf-card-bg));color:{color};border-radius:999px;padding:5px 10px;font-size:12px;display:inline-flex;align-items:center;gap:6px;cursor:pointer;"
        )
    } else {
        format!(
            "border:1px solid var(--cf-divider);background:var(--cf-card-bg);color:var(--cf-text-muted);border-radius:999px;padding:5px 10px;font-size:12px;display:inline-flex;align-items:center;gap:6px;cursor:pointer;"
        )
    }
}

fn classification_display_text(level: &str, custom_text: &str) -> String {
    let custom = custom_text.trim();
    if custom.is_empty() {
        level.to_string()
    } else {
        custom.to_uppercase()
    }
}

fn classification_colors(level: &str) -> (&'static str, &'static str) {
    match level {
        "CUI" => ("#a78bfa", "#fff"),
        "CONFIDENTIAL" => ("#3b82f6", "#fff"),
        "SECRET" => ("#ef4444", "#fff"),
        "TOP SECRET" => ("#fbbf24", "#000"),
        _ => ("#10b981", "#fff"),
    }
}

fn update_user_draft(
    mut user_drafts: Signal<HashMap<String, UserEditDraft>>,
    user_id: &str,
    update: impl FnOnce(&mut UserEditDraft),
) {
    user_drafts.with_mut(|drafts| {
        if let Some(draft) = drafts.get_mut(user_id) {
            update(draft);
        }
    });
}

fn identity_source_label(source: IdentitySource) -> &'static str {
    match source {
        IdentitySource::LocalManaged => "local",
        IdentitySource::OidcDerived => "OIDC",
    }
}

fn get_user_initials(identifier: &str) -> String {
    let parts: Vec<&str> = identifier
        .split('@')
        .next()
        .unwrap_or("")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();

    if parts.len() >= 2 {
        format!(
            "{}{}",
            parts[0].chars().next().unwrap_or(' ').to_uppercase(),
            parts[1].chars().next().unwrap_or(' ').to_uppercase()
        )
    } else if let Some(first) = parts.first() {
        first.chars().take(2).collect::<String>().to_uppercase()
    } else {
        "??".to_string()
    }
}

fn get_audit_action_color(
    action: &crate::api::models::AuditAction,
) -> (&'static str, &'static str, &'static str) {
    use crate::api::models::AuditAction;
    match action {
        AuditAction::UserCreated
        | AuditAction::UserCreate
        | AuditAction::UserDeleted
        | AuditAction::UserRoleAssigned
        | AuditAction::BuilderRotateKey
        | AuditAction::PolicyEdit
        | AuditAction::CveAccept => ("#f87171", "chip-critical", "security"),
        AuditAction::SystemDeployRequested
        | AuditAction::SystemDeploy
        | AuditAction::SystemRollbackRequested
        | AuditAction::SystemRollback => ("#a78bfa", "chip-info", "deploy"),
        AuditAction::BuildComplete | AuditAction::EvalCancel | AuditAction::CveScanRequested => {
            ("#60a5fa", "chip-info", "build")
        }
        AuditAction::SessionInvalidated | AuditAction::AuthLogin | AuditAction::AuthLoginDenied => {
            ("#fbbf24", "chip-warning", "auth")
        }
        AuditAction::FlakeSync | AuditAction::CacheCreate | AuditAction::OidcMappingChanged => {
            ("var(--cf-text-secondary)", "chip-unknown", "config")
        }
        _ => ("var(--cf-text-primary)", "chip-unknown", "config"),
    }
}

fn format_action_label(action: &crate::api::models::AuditAction) -> String {
    use crate::api::models::AuditAction;
    match action {
        AuditAction::UserCreated | AuditAction::UserCreate => "user.create".to_string(),
        AuditAction::UserUpdated => "user.update".to_string(),
        AuditAction::UserDeleted => "user.delete".to_string(),
        AuditAction::UserEnabled => "user.enable".to_string(),
        AuditAction::UserDisabled => "user.disable".to_string(),
        AuditAction::UserRoleAssigned => "user.role_change".to_string(),
        AuditAction::UserEnvironmentMembershipUpdated => "user.env_change".to_string(),
        AuditAction::OidcMappingChanged => "oidc.mapping_edit".to_string(),
        AuditAction::SystemSyncRequested => "system.sync".to_string(),
        AuditAction::SystemDeployRequested | AuditAction::SystemDeploy => {
            "system.deploy".to_string()
        }
        AuditAction::SystemRollbackRequested | AuditAction::SystemRollback => {
            "system.rollback".to_string()
        }
        AuditAction::SessionInvalidated => "auth.session_kill".to_string(),
        AuditAction::CveScanRequested => "cve.scan".to_string(),
        AuditAction::CveAccept => "cve.accept".to_string(),
        AuditAction::BuilderRotateKey => "builder.rotate_key".to_string(),
        AuditAction::FlakeSync => "flake.sync".to_string(),
        AuditAction::EvalCancel => "eval.cancel".to_string(),
        AuditAction::CacheCreate => "cache.create".to_string(),
        AuditAction::PolicyEdit => "policy.edit".to_string(),
        AuditAction::BuildComplete => "build.complete".to_string(),
        AuditAction::AuthLogin => "auth.login".to_string(),
        AuditAction::AuthLoginDenied => "auth.login_denied".to_string(),
        AuditAction::Unknown => "unknown".to_string(),
    }
}

fn format_time(value: chrono::DateTime<chrono::Utc>) -> String {
    value
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S %Z")
        .to_string()
}

fn optional_value(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn datetime_local_to_rfc3339(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(format!("{trimmed}:00Z"))
}

#[derive(Debug, Clone)]
struct UserEditDraft {
    role: String,
    enabled: bool,
    environments: String,
    password: String,
}

impl UserEditDraft {
    fn from_user(user: &AdminUserSummary) -> Self {
        Self {
            role: role_to_string(&user.role),
            enabled: user.enabled,
            environments: user.environments.join(", "),
            password: String::new(),
        }
    }
}

async fn refresh_users(
    mut users: Signal<Vec<AdminUserSummary>>,
    mut user_drafts: Signal<HashMap<String, UserEditDraft>>,
    mut users_error: Signal<Option<String>>,
) {
    match fetch_admin_users().await {
        Ok(next_users) => {
            let next_drafts = next_users
                .iter()
                .map(|user| (user.id.clone(), UserEditDraft::from_user(user)))
                .collect::<HashMap<_, _>>();
            users.set(next_users);
            user_drafts.set(next_drafts);
            users_error.set(None);
        }
        Err(e) => users_error.set(Some(format!("Failed to load admin users: {e}"))),
    }
}
