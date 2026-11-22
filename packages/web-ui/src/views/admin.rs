use chrono::Local;
use dioxus::prelude::*;
use std::collections::{BTreeSet, HashMap};

use crate::api::client::{
    create_admin_user, delete_admin_oidc_mapping, delete_admin_user, fetch_admin_audit_events,
    fetch_admin_oidc_mappings, fetch_admin_server_info, fetch_admin_users,
    fetch_automatic_retry_policy, fetch_environments, set_automatic_retry_policy,
    set_classification_config, set_setup_wizard_dismissed, update_admin_user,
    upsert_admin_oidc_mapping,
};
use crate::api::models::{
    AdminAuditEventsParams, AdminCreateUserRequest, AdminUpdateUserRequest,
    AdminUpsertOidcMappingRequest, AdminUserGrantSources, AdminUserSummary, AuditEvent, AuthMode,
    AutomaticRetryPolicy, ClassificationBannerConfig, EnvironmentSummary, IdentitySource,
    OidcGroupMapping, Role, ServerRuntimeInfoResponse, UpdateAutomaticRetryPolicyRequest,
    UpdateClassificationBannerRequest,
};
use crate::components::mcp_server_card::McpServerCard;
use crate::components::onboarding::CoachController;
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
        .unwrap_or(if server_info_loading {
            "Loading…"
        } else {
            "Unavailable"
        });
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
        .unwrap_or(if server_info_loading {
            "Loading…"
        } else {
            "Unavailable"
        });
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
        .unwrap_or(if server_info_loading {
            "Loading…"
        } else {
            "Unavailable"
        });
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
                            th { "Effective role" }
                            th { "Login source" }
                            th { "Assigned environments" }
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
                                    tr { "data-testid": "admin-user-{user_id}",
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
                                                        if user.role.is_some() { "{role_def.role}" } else { "None" }
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
                                                    span { class: "chip chip-unknown", style: "font-size:10px;", "None" }
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
                                                    "aria-label": "Edit user {user.identifier}",
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
    let mut grants = use_signal(|| draft.grants.clone());
    let mut draft_enabled = use_signal(|| draft.enabled);
    let mut draft_password = use_signal(String::new);
    let metadata_available = user.grant_sources.is_some();
    let effective_role = user
        .role
        .map(|role| role_to_string(&Some(role)))
        .unwrap_or_else(|| "None".to_string());
    let effective_environments = environments_summary(&user.environments);
    let role_value = grants
        .read()
        .role
        .map(|role| role_to_string(&Some(role)))
        .unwrap_or_default();
    let grant_validation = grants.read().request(None, None).err();

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_close.call(()),
            div { class: "modal", "data-testid": "edit-user-modal", style: "width:min(560px,96vw);", onclick: move |evt| evt.stop_propagation(),
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
                    if !metadata_available {
                        div { class: "sd-callout sd-callout-info", style: "font-size:12px;",
                            "Grant source metadata is unavailable. Permission edits are disabled. You can still change account status or reset the password."
                        }
                    }
                    if let Some(err) = local_error.read().as_ref() {
                        div { class: "sd-callout sd-callout-danger", style: "font-size:12px;", "{err}" }
                    }
                    div { class: "field", style: "margin:0;",
                        label { "Role" }
                        div { "data-testid": "effective-role", class: "help", "Effective role (read-only): {effective_role}" }
                        if let Some(sources) = user.grant_sources.as_ref() {
                            div { "data-testid": "role-sources", class: "help",
                                div { "Local roles: {roles_summary(&sources.local_roles)}" }
                                div { "Identity provider roles: {roles_summary(&sources.oidc_roles)}" }
                                div { "Unresolved legacy roles: {roles_summary(&sources.legacy_roles)}" }
                            }
                        }
                        label {
                            input { r#type: "checkbox", "data-testid": "edit-local-role", disabled: !metadata_available || *saving.read(), checked: grants.read().edit_role,
                                onchange: move |evt| grants.write().edit_role = evt.checked() }
                            "Edit local role override"
                        }
                        select { "aria-label": "Local role override", "data-testid": "local-role",
                            class: "input focus-ring",
                            disabled: !metadata_available || !grants.read().edit_role || *saving.read(),
                            value: "{role_value}",
                            onchange: move |evt| {
                                let mut edit = grants.write();
                                edit.role = match evt.value().as_str() { "Admin" => Some(Role::Admin), "Operator" => Some(Role::Operator), "Viewer" => Some(Role::Viewer), _ => None };
                                edit.acknowledge_legacy_role = false;
                            },
                            option { value: "", "No local role" }
                            option { value: "Admin", "Admin" }
                            option { value: "Operator", "Operator" }
                            option { value: "Viewer", "Viewer" }
                        }
                        div { class: "help", "Replaces local roles with one role. No local role removes only local roles and preserves unresolved legacy roles. Identity provider grants remain independent." }
                        if let Some(notice) = user.grant_sources.as_ref().and_then(|sources| multiple_local_roles_notice(&sources.local_roles)) {
                            div { "data-testid": "multiple-local-roles-notice", class: "help", "{notice}" }
                        }
                        if user.grant_sources.as_ref().is_some_and(|sources| !sources.legacy_roles.is_empty()) {
                            div { class: "sd-callout sd-callout-info", style: "font-size:12px;",
                                "Legacy role ownership is unresolved. Replacing this category classifies the selected role as local and removes the previous unresolved legacy roles."
                                label {
                                    input { r#type: "checkbox", "data-testid": "confirm-legacy-role", checked: grants.read().acknowledge_legacy_role,
                                        disabled: !grants.read().edit_role || grants.read().role.is_none() || *saving.read(),
                                        onchange: move |evt| grants.write().acknowledge_legacy_role = evt.checked() }
                                    "Replace unresolved legacy roles with the selected local role"
                                }
                            }
                        }
                    }
                    div { class: "field", style: "margin:0;",
                        label { "Environments" }
                        div { "data-testid": "effective-environments", class: "help", "Assigned environment memberships (read-only): {effective_environments}" }
                        if user.role == Some(Role::Admin) {
                            div { class: "help", "The Admin role allows browser access beyond assigned memberships. The source summaries and local controls below describe explicit memberships, not token wildcards." }
                        }
                        if let Some(sources) = user.grant_sources.as_ref() {
                            div { "data-testid": "environment-sources", class: "help",
                                div { "Local environments: {environments_summary(&sources.local_environments)}" }
                                div { "Identity provider environments: {environments_summary(&sources.oidc_environments)}" }
                                div { "Unresolved legacy environments: {environments_summary(&sources.legacy_environments)}" }
                            }
                        }
                        label {
                            input { r#type: "checkbox", "data-testid": "edit-local-environments", disabled: !metadata_available || *saving.read(), checked: grants.read().edit_environments,
                                onchange: move |evt| grants.write().edit_environments = evt.checked() }
                            "Edit local environment overrides"
                        }
                        LocalEnvironmentPicker { grants, available: environments.clone(), disabled: !metadata_available || !grants.read().edit_environments || *saving.read() }
                        if user.grant_sources.as_ref().is_some_and(|sources| !sources.legacy_environments.is_empty()) {
                            div { class: "sd-callout sd-callout-info", style: "font-size:12px;",
                                "Legacy environment ownership is unresolved. Saving this category replaces local and unresolved legacy memberships with your selection. An empty selection removes those permissions. Identity provider memberships remain independent."
                                label {
                                    input { r#type: "checkbox", "data-testid": "confirm-legacy-environments", checked: grants.read().acknowledge_legacy_environments,
                                        disabled: !grants.read().edit_environments || *saving.read(),
                                        onchange: move |evt| grants.write().acknowledge_legacy_environments = evt.checked() }
                                    "Replace unresolved legacy environments; remove memberships not selected"
                                }
                            }
                        }
                    }
                    div { class: "field", style: "margin:0;",
                        label { "Password reset" }
                        input {
                            "aria-label": "Password reset",
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
                            "data-testid": "user-enabled",
                            r#type: "checkbox",
                            checked: *draft_enabled.read(),
                            onchange: move |evt| draft_enabled.set(evt.checked())
                        }
                    }
                }
                div { class: "modal-foot",
                    if let Some(message) = grant_validation {
                        div { class: "help", role: "status", "{message}" }
                    }
                    button { class: "btn focus-ring", onclick: move |_| on_close.call(()), "Cancel" }
                    button {
                        "data-testid": "save-user",
                        class: "btn btn-primary focus-ring",
                        disabled: *saving.read() || grant_validation.is_some(),
                        onclick: {
                            let user_id = user.id.clone();
                            let initial_enabled = user.enabled;
                            move |_| {
                                let request = match grants.read().request(
                                    (*draft_enabled.read() != initial_enabled).then_some(*draft_enabled.read()),
                                    optional_value(draft_password.read().clone()),
                                ) {
                                    Ok(request) => request,
                                    Err(message) => { local_error.set(Some(message.to_string())); return; }
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
fn LocalEnvironmentPicker(
    mut grants: Signal<LocalGrantEdit>,
    available: Vec<EnvironmentSummary>,
    disabled: bool,
) -> Element {
    let choices: BTreeSet<String> = available
        .iter()
        .filter(|env| env.is_active)
        .map(|env| env.name.clone())
        .chain(
            grants
                .read()
                .baseline
                .iter()
                .flat_map(|sources| sources.local_environments.iter().cloned()),
        )
        .chain(grants.read().environments.iter().cloned())
        .collect();
    rsx! {
        div { "data-testid": "local-environments", style: "display:flex;flex-wrap:wrap;gap:6px;",
            button { class: "focus-ring", "data-testid": "clear-local-environments", disabled,
                style: environment_pill_style(grants.read().environments.is_empty(), "#60a5fa"),
                onclick: move |_| {
                    let mut edit = grants.write();
                    edit.environments.clear();
                    edit.acknowledge_legacy_environments = false;
                },
                "No local environments"
            }
            for name in choices {
                {
                    let selected = grants.read().environments.contains(&name);
                    rsx! {
                        button { class: "focus-ring", disabled, "aria-pressed": selected,
                            style: environment_pill_style(selected, "#60a5fa"),
                            onclick: {
                                let name = name.clone();
                                move |_| {
                                    let mut edit = grants.write();
                                    if !edit.environments.remove(&name) { edit.environments.insert(name.clone()); }
                                    edit.acknowledge_legacy_environments = false;
                                }
                            },
                            "{name}"
                        }
                    }
                }
            }
        }
        div { class: "help", "Select independent local memberships. No local environments clears this category; it does not copy identity provider access." }
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
                                let actor = audit_actor_label(&event);

                                rsx! {
                                    tr {
                                        td { style: "font-size:12px;color:var(--cf-text-muted);white-space:nowrap;", "{format_time(event.timestamp)}" }
                                        td { class: "mono", style: "font-size:12px;font-weight:600;",
                                            // The visible text uses a middle dot; screen readers get a plain sentence.
                                            span { aria_hidden: "true", "{actor.visible}" }
                                            span { class: "sr-only", "{actor.spoken}" }
                                        }
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
    let coach = use_context::<CoachController>();
    let coach_setup = (coach.setup)();
    let auth_mode_label = match auth_mode {
        AuthMode::Dev => "Dev",
        AuthMode::Local => "Local",
        AuthMode::Oidc => "OIDC",
    };
    let server_info_error_text = server_info_error.clone();
    let oidc_issuer = server_info
        .as_ref()
        .and_then(|info| info.oidc_issuer_url.as_ref())
        .map(String::as_str);
    let active_sessions = server_info
        .as_ref()
        .map(|info| info.active_sessions.to_string())
        .unwrap_or(if server_info_loading {
            "Loading…".to_string()
        } else {
            "Unavailable".to_string()
        });
    let tls_status = server_info
        .as_ref()
        .map(|info| info.tls_status.as_str())
        .unwrap_or(if server_info_loading {
            "Loading…"
        } else {
            "Unavailable"
        });
    let tls_detail = server_info
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
                    dd {
                        if let Some(issuer) = oidc_issuer {
                            span { class: "mono", style: "font-size:11px;word-break:break-word;", "{issuer}" }
                        } else if matches!(auth_mode, AuthMode::Oidc) {
                            span { class: "chip chip-unknown", "unavailable" }
                            span { style: "color:var(--cf-text-muted);", " · OIDC issuer env not set" }
                        } else {
                            span { style: "color:var(--cf-text-muted);", "not in use" }
                        }
                    }
                    dt { "Sessions" }
                    dd { "{active_sessions}" }
                    dt { "Transport" }
                    dd {
                        span { class: "chip chip-unknown", "{tls_status}" }
                        " "
                        span { style: "color:var(--cf-text-muted);", "· {tls_detail}" }
                    }
                }
            }

            // MCP server status card (full width, read-only)
            McpServerCard {}

            // Agent heartbeat config card
            HeartbeatConfigCard {
                environments,
                environments_error,
            }

            AutomaticRetriesCard {}

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
                        p { style: "margin:0;font-size:12px;color:var(--cf-text-muted);max-width:72ch;line-height:1.5;",
                            "data-testid": "admin-onboarding-copy",
                            "Setup completion is reported by the server from saved resources"
                            if coach_setup.progress().is_some() {
                                " · {coach_setup.count()} of {coach_setup.total()} steps complete"
                            }
                            ". Security walkthroughs are open to every role from "
                            strong { "Guide" }
                            " in the top bar; their progress is kept per browser and only records what each person has viewed."
                        }
                    }
                    div { style: "display:flex;gap:8px;flex-wrap:wrap;",
                        button {
                            class: "btn btn-primary focus-ring",
                            "data-testid": "admin-relaunch-setup-coach",
                            onclick: move |_| {
                                spawn(async move {
                                    // Relaunch also clears the server-side dismissal, as before.
                                    let _ = set_setup_wizard_dismissed(false).await;
                                    coach.relaunch();
                                });
                            },
                            svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:5px;vertical-align:text-bottom;",
                                path { d: "M20 12a8 8 0 0 1-14 5.3L3 14m1-4a8 8 0 0 1 14-5.3L21 8M21 3v5h-5M3 21v-5h5" }
                            }
                            "Relaunch Setup Coach"
                        }
                        button {
                            class: "btn btn-ghost focus-ring",
                            "data-testid": "admin-restart-walkthroughs",
                            title: "Clears which walkthrough stops this browser has viewed. Does not touch setup or security records.",
                            onclick: move |_| coach.restart_walkthroughs(),
                            "Restart walkthroughs"
                        }
                        button { class: "btn btn-ghost focus-ring", disabled: true, title: "Setup completion is derived from persisted resources on the server and can't be erased from here.", "Reset progress · unavailable" }
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
// AUTOMATIC RETRIES CARD
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
struct RetryPolicyFormState {
    baseline: Option<AutomaticRetryPolicy>,
    draft: AutomaticRetryPolicy,
    loading: bool,
    saving: bool,
    load_error: Option<String>,
    save_error: Option<String>,
    success: Option<String>,
}

impl Default for RetryPolicyFormState {
    fn default() -> Self {
        Self {
            baseline: None,
            draft: AutomaticRetryPolicy::default(),
            loading: true,
            saving: false,
            load_error: None,
            save_error: None,
            success: None,
        }
    }
}

impl RetryPolicyFormState {
    fn load_started(&mut self) {
        self.loading = true;
        self.load_error = None;
    }

    fn load_succeeded(&mut self, policy: AutomaticRetryPolicy) {
        self.baseline = Some(policy.clone());
        self.draft = policy;
        self.loading = false;
        self.load_error = None;
        self.save_error = None;
        self.success = None;
    }

    fn load_failed(&mut self, error: String) {
        self.loading = false;
        self.load_error = Some(error);
    }

    fn update_draft(&mut self, update: impl FnOnce(&mut AutomaticRetryPolicy)) {
        update(&mut self.draft);
        self.save_error = None;
        self.success = None;
    }

    fn reset(&mut self) {
        if let Some(baseline) = self.baseline.clone() {
            self.draft = baseline;
            self.save_error = None;
            self.success = None;
        }
    }

    fn begin_save(&mut self) -> Option<UpdateAutomaticRetryPolicyRequest> {
        if let Err(error) = validate_retry_policy(&self.draft) {
            self.save_error = Some(error);
            self.success = None;
            return None;
        }

        self.saving = true;
        self.save_error = None;
        self.success = None;
        Some(UpdateAutomaticRetryPolicyRequest {
            max_build_retries: self.draft.max_build_retries,
            max_evaluation_retries: self.draft.max_evaluation_retries,
            backoff_seconds: self.draft.backoff_seconds,
            transient_only: self.draft.transient_only,
        })
    }

    fn save_succeeded(&mut self, policy: AutomaticRetryPolicy) {
        self.baseline = Some(policy.clone());
        self.draft = policy;
        self.saving = false;
        self.save_error = None;
        self.success = Some("Automatic retry configuration saved.".to_string());
    }

    fn save_failed(&mut self, error: String) {
        self.saving = false;
        self.save_error = Some(error);
        self.success = None;
    }
}

fn validate_retry_policy(policy: &AutomaticRetryPolicy) -> Result<(), String> {
    if !(0..=5).contains(&policy.max_build_retries) {
        return Err("Max build retries must be between 0 and 5.".to_string());
    }
    if !(0..=5).contains(&policy.max_evaluation_retries) {
        return Err("Max eval retries must be between 0 and 5.".to_string());
    }
    if !matches!(policy.backoff_seconds, 0 | 10 | 30 | 60 | 120 | 300) {
        return Err("Backoff must be None, 10s, 30s, 1m, 2m, or 5m.".to_string());
    }
    Ok(())
}

#[component]
fn AutomaticRetriesCard() -> Element {
    let mut form = use_signal(RetryPolicyFormState::default);
    let mut reload = use_signal(|| 0_u32);

    use_effect(move || {
        let _ = *reload.read();
        form.with_mut(RetryPolicyFormState::load_started);
        spawn(async move {
            match fetch_automatic_retry_policy().await {
                Ok(policy) => form.with_mut(|state| state.load_succeeded(policy)),
                Err(error) => form.with_mut(|state| {
                    state.load_failed(format!("Failed to load automatic retry policy: {error}"))
                }),
            }
        });
    });

    let state = form.read().clone();
    let controls_disabled = state.loading || state.saving || state.baseline.is_none();

    rsx! {
        div {
            class: "card",
            style: "padding:16px;grid-column:1 / -1;",
            "data-testid": "automatic-retries-card",
            div { style: "display:flex;align-items:center;justify-content:space-between;margin-bottom:4px;gap:12px;flex-wrap:wrap;",
                h3 { style: "margin:0;font-size:13px;font-weight:600;", "Automatic retries" }
                span { style: "font-size:11px;color:var(--cf-text-muted);", "How many times a failed build/eval is retried before it's left failed" }
            }

            if state.loading {
                div {
                    style: "font-size:12px;color:var(--cf-text-muted);margin-top:14px;",
                    role: "status",
                    "data-testid": "automatic-retries-loading",
                    "Loading automatic retry policy…"
                }
            } else if let Some(error) = state.load_error.as_ref() {
                div {
                    class: "sd-callout sd-callout-danger",
                    style: "font-size:12px;margin-top:14px;display:flex;align-items:center;gap:10px;",
                    role: "alert",
                    "data-testid": "automatic-retries-load-error",
                    span { style: "flex:1;", "{error}" }
                    button {
                        class: "btn btn-ghost focus-ring",
                        "aria-label": "Retry loading automatic retry policy",
                        onclick: move |_| reload += 1,
                        "Retry"
                    }
                }
            }

            if let Some(error) = state.save_error.as_ref() {
                div {
                    class: "sd-callout sd-callout-danger",
                    style: "font-size:12px;margin-top:14px;",
                    role: "alert",
                    "data-testid": "automatic-retries-save-error",
                    "{error}"
                }
            }
            if let Some(message) = state.success.as_ref() {
                div {
                    class: "sd-callout sd-callout-success",
                    style: "font-size:12px;margin-top:14px;",
                    role: "status",
                    "data-testid": "automatic-retries-save-success",
                    "{message}"
                }
            }

            div { style: "display:grid;grid-template-columns:1fr 1fr;gap:14px;margin-top:14px;",
                label { style: "display:flex;flex-direction:column;gap:5px;",
                    span { style: "font-size:12px;font-weight:500;", "Max build retries" }
                    select {
                        class: "input focus-ring",
                        style: "width:120px;",
                        disabled: controls_disabled,
                        value: "{state.draft.max_build_retries}",
                        "data-testid": "automatic-retries-build-count",
                        onchange: move |event| {
                            if let Ok(value) = event.value().parse::<i16>() {
                                form.with_mut(|state| state.update_draft(|draft| draft.max_build_retries = value));
                            }
                        },
                        for count in 0_i16..=5 {
                            option { value: "{count}", if count == 0 { "Never" } else { "{count}" } }
                        }
                    }
                }
                label { style: "display:flex;flex-direction:column;gap:5px;",
                    span { style: "font-size:12px;font-weight:500;", "Max eval retries" }
                    select {
                        class: "input focus-ring",
                        style: "width:120px;",
                        disabled: controls_disabled,
                        value: "{state.draft.max_evaluation_retries}",
                        "data-testid": "automatic-retries-eval-count",
                        onchange: move |event| {
                            if let Ok(value) = event.value().parse::<i16>() {
                                form.with_mut(|state| state.update_draft(|draft| draft.max_evaluation_retries = value));
                            }
                        },
                        for count in 0_i16..=5 {
                            option { value: "{count}", if count == 0 { "Never" } else { "{count}" } }
                        }
                    }
                }
                label { style: "display:flex;flex-direction:column;gap:5px;",
                    span { style: "font-size:12px;font-weight:500;", "Backoff between attempts" }
                    select {
                        class: "input focus-ring",
                        style: "width:120px;",
                        disabled: controls_disabled,
                        value: "{state.draft.backoff_seconds}",
                        "data-testid": "automatic-retries-backoff",
                        onchange: move |event| {
                            if let Ok(value) = event.value().parse::<i32>() {
                                form.with_mut(|state| state.update_draft(|draft| draft.backoff_seconds = value));
                            }
                        },
                        option { value: "0", "None" }
                        option { value: "10", "10s" }
                        option { value: "30", "30s" }
                        option { value: "60", "1m" }
                        option { value: "120", "2m" }
                        option { value: "300", "5m" }
                    }
                }
                label { style: "display:flex;gap:9px;align-items:flex-start;cursor:pointer;margin-top:22px;",
                    input {
                        r#type: "checkbox",
                        disabled: controls_disabled,
                        checked: state.draft.transient_only,
                        style: "margin-top:2px;accent-color:var(--cf-brand-purple);",
                        "data-testid": "automatic-retries-transient-only",
                        onchange: move |event| {
                            let checked = event.checked();
                            form.with_mut(|state| state.update_draft(|draft| draft.transient_only = checked));
                        }
                    }
                    span {
                        span { style: "display:block;font-size:12px;font-weight:500;", "Only retry transient failures" }
                        span { style: "display:block;font-size:11px;color:var(--cf-text-muted);", "Skip auto-retry for eval/build errors that won't change on their own (e.g. bad derivation, assertion failure)" }
                    }
                }
            }
            div { style: "display:flex;justify-content:flex-end;gap:8px;margin-top:14px;",
                button {
                    class: "btn btn-ghost focus-ring",
                    disabled: controls_disabled,
                    "data-testid": "automatic-retries-reset",
                    onclick: move |_| form.with_mut(RetryPolicyFormState::reset),
                    "Reset"
                }
                button {
                    class: "btn btn-primary focus-ring",
                    disabled: controls_disabled,
                    "data-testid": "automatic-retries-save",
                    onclick: move |_| {
                        let request = form.with_mut(RetryPolicyFormState::begin_save);
                        if let Some(request) = request {
                            spawn(async move {
                                match set_automatic_retry_policy(&request).await {
                                    Ok(policy) => form.with_mut(|state| state.save_succeeded(policy)),
                                    Err(error) => form.with_mut(|state| {
                                        state.save_failed(format!("Failed to save automatic retry policy: {error}"))
                                    }),
                                }
                            });
                        }
                    },
                    svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:5px;vertical-align:text-bottom;",
                        polyline { points: "20 6 9 17 4 12" }
                    }
                    if state.saving { "Saving…" } else { "Save retry config" }
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

/// Visible and spoken text of an Audit Log actor cell.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AuditActorLabel {
    /// Shown text, for example `ops@example.com · via agent token OpenCode`.
    visible: String,
    /// Screen-reader text without the separator glyph.
    spoken: String,
}

/// Returns the actor cell text of one audit event.
///
/// The human owner stays the actor. An event made through an access token
/// (`via_agent`, which stays set after the token is deleted) adds
/// `via agent token <name>`; a token whose name the server no longer knows
/// reads `(deleted token)`. An event without an actor reads `system`.
fn audit_actor_label(event: &AuditEvent) -> AuditActorLabel {
    let actor = event.actor.as_deref().unwrap_or("system");
    if !event.via_agent {
        return AuditActorLabel {
            visible: actor.to_string(),
            spoken: actor.to_string(),
        };
    }
    let token = event
        .access_token_name
        .as_deref()
        .unwrap_or("(deleted token)");
    AuditActorLabel {
        visible: format!("{actor} · via agent token {token}"),
        spoken: format!("{actor}, via agent token {token}"),
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
        | AuditAction::SystemKeyRotated
        | AuditAction::SystemCreated
        | AuditAction::SystemUpdated
        | AuditAction::PolicyEdit
        | AuditAction::AccessTokenCreated
        | AuditAction::AccessTokenRotated
        | AuditAction::AccessTokenRevoked
        | AuditAction::McpWriteDenied
        | AuditAction::CveAccept => ("#f87171", "chip-critical", "security"),
        AuditAction::PoamNote
        | AuditAction::PoamStatusChanged
        | AuditAction::PoamMilestoneAdded
        | AuditAction::PoamMilestoneUpdated
        | AuditAction::PoamMilestoneRemoved
        | AuditAction::PoamVerificationAttempted => ("#60a5fa", "chip-info", "poam"),
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
        AuditAction::SystemKeyRotated => "system.rotate_key".to_string(),
        AuditAction::SystemCreated => "system.created".to_string(),
        AuditAction::SystemUpdated => "system.updated".to_string(),
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
        AuditAction::AccessTokenCreated => "access_token.create".to_string(),
        AuditAction::AccessTokenRotated => "access_token.rotate".to_string(),
        AuditAction::AccessTokenRevoked => "access_token.revoke".to_string(),
        AuditAction::McpWriteDenied => "mcp.write_denied".to_string(),
        AuditAction::PoamNote => "poam.note".to_string(),
        AuditAction::PoamStatusChanged => "poam.status_change".to_string(),
        AuditAction::PoamMilestoneAdded => "poam.milestone_add".to_string(),
        AuditAction::PoamMilestoneUpdated => "poam.milestone_update".to_string(),
        AuditAction::PoamMilestoneRemoved => "poam.milestone_remove".to_string(),
        AuditAction::PoamVerificationAttempted => "poam.verify".to_string(),
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
    grants: LocalGrantEdit,
    enabled: bool,
}

impl UserEditDraft {
    fn from_user(user: &AdminUserSummary) -> Self {
        Self {
            grants: LocalGrantEdit::new(user.grant_sources.clone()),
            enabled: user.enabled,
        }
    }
}

// The baseline contains only source metadata, never effective authority. Opt-in
// alone is not a mutation: unchanged categories must remain absent from PATCH.
#[derive(Debug, Clone)]
struct LocalGrantEdit {
    baseline: Option<AdminUserGrantSources>,
    role: Option<Role>,
    environments: BTreeSet<String>,
    edit_role: bool,
    edit_environments: bool,
    acknowledge_legacy_role: bool,
    acknowledge_legacy_environments: bool,
}

impl LocalGrantEdit {
    fn new(baseline: Option<AdminUserGrantSources>) -> Self {
        let role = baseline
            .as_ref()
            .and_then(|sources| primary_role(&sources.local_roles));
        let environments = baseline
            .as_ref()
            .map(|sources| sources.local_environments.iter().cloned().collect())
            .unwrap_or_default();
        Self {
            baseline,
            role,
            environments,
            edit_role: false,
            edit_environments: false,
            acknowledge_legacy_role: false,
            acknowledge_legacy_environments: false,
        }
    }

    fn request(
        &self,
        enabled: Option<bool>,
        password: Option<String>,
    ) -> Result<AdminUpdateUserRequest, &'static str> {
        let mut request = AdminUpdateUserRequest {
            enabled,
            password,
            ..Default::default()
        };
        let Some(baseline) = &self.baseline else {
            return if self.edit_role || self.edit_environments {
                Err("Grant source metadata is unavailable. Permission edits are disabled.")
            } else {
                Ok(request)
            };
        };
        // INVARIANT: The selection is compared with the full local role set,
        // not only the highest role. With several local roles, keeping the
        // pre-selected highest role while the override is enabled is a
        // deliberate normalization to that single role. Zero- and
        // single-role baselines keep the exact no-op behavior.
        let selected_roles: Vec<Role> = self.role.into_iter().collect();
        let role_changed = selected_roles != ordered_roles(&baseline.local_roles);
        let classify_role = !baseline.legacy_roles.is_empty()
            && self.acknowledge_legacy_role
            && self.role.is_some();
        if self.edit_role && (role_changed || classify_role) {
            if self.role.is_some()
                && !baseline.legacy_roles.is_empty()
                && !self.acknowledge_legacy_role
            {
                return Err("Confirm replacement of unresolved legacy roles before saving.");
            }
            request.role = self.role;
            request.clear_local_role = self.role.is_none();
        }
        let baseline_environments: BTreeSet<_> =
            baseline.local_environments.iter().cloned().collect();
        let classify_environments =
            !baseline.legacy_environments.is_empty() && self.acknowledge_legacy_environments;
        if self.edit_environments
            && (self.environments != baseline_environments || classify_environments)
        {
            if !baseline.legacy_environments.is_empty() && !self.acknowledge_legacy_environments {
                return Err(
                    "Confirm replacement or removal of unresolved legacy environments before saving.",
                );
            }
            request.environments = Some(self.environments.iter().cloned().collect());
        }
        request.edit_local_grants =
            request.role.is_some() || request.clear_local_role || request.environments.is_some();
        Ok(request)
    }
}

/// Returns the distinct roles in display order: Admin, Operator, Viewer.
fn ordered_roles(roles: &[Role]) -> Vec<Role> {
    [Role::Admin, Role::Operator, Role::Viewer]
        .into_iter()
        .filter(|role| roles.contains(role))
        .collect()
}

fn primary_role(roles: &[Role]) -> Option<Role> {
    ordered_roles(roles).first().copied()
}

/// Returns a notice when saving the override collapses several local roles.
fn multiple_local_roles_notice(local_roles: &[Role]) -> Option<String> {
    let roles = ordered_roles(local_roles);
    (roles.len() > 1).then(|| {
        format!(
            "This account has {} local roles ({}). Saving with the override enabled replaces them with the selected role.",
            roles.len(),
            roles_summary(&roles)
        )
    })
}

fn roles_summary(roles: &[Role]) -> String {
    if roles.is_empty() {
        "None".to_string()
    } else {
        roles
            .iter()
            .map(|role| role_to_string(&Some(*role)))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn environments_summary(environments: &[String]) -> String {
    if environments.is_empty() {
        "None".to_string()
    } else {
        environments.join(", ")
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

#[cfg(test)]
mod local_grant_edit_tests {
    use super::*;
    use serde_json::{json, to_value};

    fn sources() -> AdminUserGrantSources {
        AdminUserGrantSources {
            local_roles: vec![],
            local_environments: vec![],
            oidc_roles: vec![Role::Operator],
            oidc_environments: vec!["prod".into()],
            legacy_roles: vec![],
            legacy_environments: vec![],
        }
    }

    fn user(grant_sources: Option<AdminUserGrantSources>) -> AdminUserSummary {
        AdminUserSummary {
            id: "oidc-user".into(),
            identifier: "operator@example.invalid".into(),
            identity_source: IdentitySource::OidcDerived,
            role: Some(Role::Operator),
            enabled: true,
            environments: vec!["prod".into()],
            grant_sources,
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn oidc_only_load_and_noop_never_copy_effective_grants() {
        let draft = UserEditDraft::from_user(&user(Some(sources())));
        assert_eq!(draft.grants.role, None);
        assert!(draft.grants.environments.is_empty());
        assert!(!draft.grants.edit_role && !draft.grants.edit_environments);
        assert_eq!(
            to_value(draft.grants.request(None, None).unwrap()).unwrap(),
            json!({})
        );
        let mut edit = draft.grants;
        edit.edit_role = true;
        edit.edit_environments = true;
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({})
        );
    }

    #[test]
    fn unrelated_account_edit_omits_all_grant_fields_for_mixed_sources() {
        let mut source = sources();
        source.local_roles = vec![Role::Viewer];
        source.local_environments = vec!["staging".into()];
        source.legacy_roles = vec![Role::Admin];
        source.legacy_environments = vec!["legacy".into()];
        let edit = UserEditDraft::from_user(&user(Some(source))).grants;
        assert_eq!(edit.role, Some(Role::Viewer));
        assert_eq!(edit.environments, BTreeSet::from(["staging".into()]));
        assert_eq!(
            to_value(edit.request(Some(false), Some("reset".into())).unwrap()).unwrap(),
            json!({"enabled": false, "password": "reset"})
        );
    }

    #[test]
    fn local_role_replacement_and_deletion_are_independent_of_environments() {
        let mut source = sources();
        source.local_roles = vec![Role::Admin, Role::Viewer];
        let mut edit = LocalGrantEdit::new(Some(source));
        edit.edit_role = true;
        assert_eq!(edit.role, Some(Role::Admin));
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({"role":"Admin", "edit_local_grants":true})
        );
        edit.role = Some(Role::Viewer);
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({"role":"Viewer", "edit_local_grants":true})
        );
        edit.role = None;
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({"clear_local_role":true, "edit_local_grants":true})
        );
    }

    fn multi_role_source() -> AdminUserGrantSources {
        let mut source = sources();
        source.local_roles = vec![Role::Viewer, Role::Admin];
        source
    }

    #[test]
    fn multiple_local_roles_without_opt_in_send_nothing() {
        let mut edit = LocalGrantEdit::new(Some(multi_role_source()));
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({})
        );
        edit.role = Some(Role::Viewer);
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({})
        );
        edit.role = None;
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({})
        );
    }

    #[test]
    fn multiple_local_roles_with_opt_in_normalize_to_the_selected_role() {
        let mut edit = LocalGrantEdit::new(Some(multi_role_source()));
        edit.edit_role = true;
        assert_eq!(edit.role, Some(Role::Admin));
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({"role":"Admin", "edit_local_grants":true})
        );
        edit.role = Some(Role::Viewer);
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({"role":"Viewer", "edit_local_grants":true})
        );
        edit.role = None;
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({"clear_local_role":true, "edit_local_grants":true})
        );
    }

    #[test]
    fn single_and_duplicate_local_roles_keep_noop_when_retained() {
        let mut source = sources();
        source.local_roles = vec![Role::Viewer];
        let mut edit = LocalGrantEdit::new(Some(source));
        edit.edit_role = true;
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({})
        );
        let mut source = sources();
        source.local_roles = vec![Role::Viewer, Role::Viewer];
        let mut edit = LocalGrantEdit::new(Some(source));
        edit.edit_role = true;
        assert_eq!(edit.role, Some(Role::Viewer));
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({})
        );
        edit.role = Some(Role::Operator);
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({"role":"Operator", "edit_local_grants":true})
        );
        edit.role = None;
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({"clear_local_role":true, "edit_local_grants":true})
        );
    }

    #[test]
    fn multiple_local_roles_normalization_still_requires_legacy_acknowledgment() {
        let mut source = multi_role_source();
        source.legacy_roles = vec![Role::Operator];
        let mut edit = LocalGrantEdit::new(Some(source));
        edit.edit_role = true;
        assert_eq!(edit.role, Some(Role::Admin));
        assert!(edit.request(None, None).is_err());
        edit.acknowledge_legacy_role = true;
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({"role":"Admin", "edit_local_grants":true})
        );
    }

    #[test]
    fn unrelated_account_edit_with_multiple_local_roles_omits_role() {
        let edit = UserEditDraft::from_user(&user(Some(multi_role_source()))).grants;
        assert_eq!(
            to_value(edit.request(Some(false), Some("reset".into())).unwrap()).unwrap(),
            json!({"enabled": false, "password": "reset"})
        );
    }

    #[test]
    fn multiple_local_roles_notice_is_sorted_deduplicated_and_factual() {
        assert_eq!(multiple_local_roles_notice(&[]), None);
        assert_eq!(multiple_local_roles_notice(&[Role::Viewer]), None);
        assert_eq!(
            multiple_local_roles_notice(&[Role::Viewer, Role::Viewer]),
            None
        );
        assert_eq!(
            multiple_local_roles_notice(&[Role::Viewer, Role::Admin, Role::Viewer]).as_deref(),
            Some(
                "This account has 2 local roles (Admin, Viewer). Saving with the override enabled replaces them with the selected role."
            )
        );
    }

    #[test]
    fn environment_changes_compare_sets_and_omit_unchanged_role() {
        let mut source = sources();
        source.local_roles = vec![Role::Viewer];
        source.local_environments = vec!["staging".into(), "dev".into()];
        let mut edit = LocalGrantEdit::new(Some(source));
        edit.edit_environments = true;
        edit.environments = BTreeSet::from(["dev".into(), "staging".into()]);
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({})
        );
        edit.environments = BTreeSet::from(["dev".into()]);
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({"environments":["dev"], "edit_local_grants":true})
        );
        edit.environments.clear();
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({"environments":[], "edit_local_grants":true})
        );
    }

    #[test]
    fn changing_values_without_opt_in_does_not_mutate_grants() {
        let mut edit = LocalGrantEdit::new(Some(sources()));
        edit.role = Some(Role::Admin);
        edit.environments.insert("prod".into());
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({})
        );
    }

    #[test]
    fn missing_metadata_fails_closed_but_allows_account_edits() {
        let mut legacy_response = to_value(user(None)).unwrap();
        legacy_response
            .as_object_mut()
            .unwrap()
            .remove("grant_sources");
        let loaded: AdminUserSummary = serde_json::from_value(legacy_response).unwrap();
        assert!(loaded.grant_sources.is_none());
        let mut edit = UserEditDraft::from_user(&loaded).grants;
        assert_eq!(
            to_value(edit.request(Some(false), None).unwrap()).unwrap(),
            json!({"enabled":false})
        );
        edit.edit_role = true;
        assert!(edit.request(None, None).is_err());
        edit.edit_role = false;
        edit.edit_environments = true;
        assert!(edit.request(None, None).is_err());
    }

    #[test]
    fn legacy_role_replacement_requires_ack_but_local_clear_preserves_legacy() {
        let mut source = sources();
        source.local_roles = vec![Role::Viewer];
        source.legacy_roles = vec![Role::Admin];
        let mut edit = LocalGrantEdit::new(Some(source));
        edit.edit_role = true;
        edit.role = Some(Role::Operator);
        assert!(edit.request(None, None).is_err());
        edit.acknowledge_legacy_role = true;
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({"role":"Operator", "edit_local_grants":true})
        );
        edit.acknowledge_legacy_role = false;
        edit.role = None;
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({"clear_local_role":true, "edit_local_grants":true})
        );
    }

    #[test]
    fn unchanged_legacy_category_requires_explicit_confirmed_classification() {
        let mut source = sources();
        source.local_roles = vec![Role::Viewer];
        source.legacy_roles = vec![Role::Admin];
        source.legacy_environments = vec!["prod".into()];
        let mut edit = LocalGrantEdit::new(Some(source));
        edit.edit_role = true;
        edit.edit_environments = true;
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({})
        );
        edit.acknowledge_legacy_role = true;
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({"role":"Viewer", "edit_local_grants":true})
        );
        edit.acknowledge_legacy_role = false;
        edit.acknowledge_legacy_environments = true;
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({"environments":[], "edit_local_grants":true})
        );
        edit.acknowledge_legacy_environments = false;
        edit.environments.insert("staging".into());
        assert!(edit.request(None, None).is_err());
        edit.acknowledge_legacy_environments = true;
        assert_eq!(
            to_value(edit.request(None, None).unwrap()).unwrap(),
            json!({"environments":["staging"], "edit_local_grants":true})
        );
    }

    #[test]
    fn summaries_preserve_every_role_and_empty_sources_are_not_all() {
        assert_eq!(
            roles_summary(&[Role::Viewer, Role::Admin, Role::Operator]),
            "Viewer, Admin, Operator"
        );
        assert_eq!(roles_summary(&[]), "None");
        assert_eq!(environments_summary(&[]), "None");
    }
}

#[cfg(test)]
mod retry_policy_form_tests {
    use super::*;

    fn policy(
        max_build_retries: i16,
        max_evaluation_retries: i16,
        backoff_seconds: i32,
        transient_only: bool,
    ) -> AutomaticRetryPolicy {
        AutomaticRetryPolicy {
            max_build_retries,
            max_evaluation_retries,
            backoff_seconds,
            transient_only,
        }
    }

    #[test]
    fn load_transitions_from_defaults_to_server_values_or_error() {
        let mut state = RetryPolicyFormState::default();
        assert!(state.loading);
        assert_eq!(state.draft, AutomaticRetryPolicy::default());
        assert!(state.baseline.is_none());

        state.load_failed("HTTP 403: Forbidden".to_string());
        assert!(!state.loading);
        assert_eq!(state.load_error.as_deref(), Some("HTTP 403: Forbidden"));
        assert_eq!(state.draft, AutomaticRetryPolicy::default());

        state.load_started();
        let server_policy = policy(4, 3, 120, false);
        state.load_succeeded(server_policy.clone());
        assert!(!state.loading);
        assert_eq!(state.baseline, Some(server_policy.clone()));
        assert_eq!(state.draft, server_policy);
        assert!(state.load_error.is_none());
    }

    #[test]
    fn reset_restores_last_server_value_without_starting_save() {
        let baseline = policy(3, 2, 60, true);
        let mut state = RetryPolicyFormState::default();
        state.load_succeeded(baseline.clone());
        state.update_draft(|draft| {
            draft.max_build_retries = 5;
            draft.backoff_seconds = 300;
        });

        state.reset();

        assert_eq!(state.draft, baseline);
        assert!(!state.saving);
        assert!(state.success.is_none());
    }

    #[test]
    fn successful_save_updates_baseline_and_visible_success() {
        let mut state = RetryPolicyFormState::default();
        state.load_succeeded(AutomaticRetryPolicy::default());
        state.update_draft(|draft| draft.max_build_retries = 4);

        let request = state.begin_save().expect("valid draft should save");
        assert_eq!(request.max_build_retries, 4);
        assert!(state.saving);

        let saved = policy(4, 1, 30, true);
        state.save_succeeded(saved.clone());
        assert_eq!(state.baseline, Some(saved.clone()));
        assert_eq!(state.draft, saved);
        assert!(!state.saving);
        assert_eq!(
            state.success.as_deref(),
            Some("Automatic retry configuration saved.")
        );
    }

    #[test]
    fn failed_save_preserves_draft_and_surfaces_error() {
        let mut state = RetryPolicyFormState::default();
        state.load_succeeded(AutomaticRetryPolicy::default());
        state.update_draft(|draft| {
            draft.max_evaluation_retries = 5;
            draft.transient_only = false;
        });
        let draft = state.draft.clone();

        assert!(state.begin_save().is_some());
        state.save_failed("HTTP 403: Forbidden".to_string());

        assert_eq!(state.draft, draft);
        assert_eq!(state.baseline, Some(AutomaticRetryPolicy::default()));
        assert_eq!(state.save_error.as_deref(), Some("HTTP 403: Forbidden"));
        assert!(!state.saving);
        assert!(state.success.is_none());
    }

    #[test]
    fn local_validation_rejects_unsupported_counts_and_backoff() {
        for invalid in [-1, 6] {
            let mut state = RetryPolicyFormState::default();
            state.draft.max_build_retries = invalid;
            assert!(state.begin_save().is_none());
            assert!(!state.saving);
            assert!(state.save_error.is_some());

            state.draft = AutomaticRetryPolicy::default();
            state.draft.max_evaluation_retries = invalid;
            assert!(state.begin_save().is_none());
        }

        let mut state = RetryPolicyFormState::default();
        state.draft.backoff_seconds = 15;
        assert!(state.begin_save().is_none());
        assert_eq!(
            state.save_error.as_deref(),
            Some("Backoff must be None, 10s, 30s, 1m, 2m, or 5m.")
        );
    }
}

#[cfg(test)]
mod audit_actor_tests {
    use super::*;
    use crate::api::models::{AuditAction, Uuid};

    fn event(actor: Option<&str>, token: Option<(Uuid, Option<&str>)>) -> AuditEvent {
        AuditEvent {
            timestamp: chrono::Utc::now(),
            actor: actor.map(str::to_string),
            action: AuditAction::CveScanRequested,
            target: "web-01".to_string(),
            source: "admin_audit_events".to_string(),
            access_token_id: token.map(|(id, _)| id),
            via_agent: token.is_some(),
            access_token_name: token.and_then(|(_, name)| name.map(str::to_string)),
        }
    }

    #[test]
    fn browser_actions_show_only_the_actor() {
        let label = audit_actor_label(&event(Some("ops@example.com"), None));
        assert_eq!(label.visible, "ops@example.com");
        assert_eq!(label.spoken, "ops@example.com");
        assert_eq!(audit_actor_label(&event(None, None)).visible, "system");
    }

    #[test]
    fn agent_actions_name_the_token_or_a_deleted_token() {
        let id = Uuid::from_u128(7);
        let label = audit_actor_label(&event(
            Some("ops@example.com"),
            Some((id, Some("OpenCode"))),
        ));
        assert_eq!(label.visible, "ops@example.com · via agent token OpenCode");
        assert_eq!(label.spoken, "ops@example.com, via agent token OpenCode");
        let deleted = audit_actor_label(&event(Some("ops@example.com"), Some((id, None))));
        assert_eq!(
            deleted.visible,
            "ops@example.com · via agent token (deleted token)"
        );
    }

    #[test]
    fn agent_marker_survives_token_deletion() {
        // A hard delete sets `access_token_id` to `None`; the server still
        // reports `via_agent` and the name snapshot.
        let mut snapshot = event(Some("ops@example.com"), None);
        snapshot.via_agent = true;
        snapshot.access_token_name = Some("OpenCode".to_string());
        assert_eq!(
            audit_actor_label(&snapshot).visible,
            "ops@example.com · via agent token OpenCode"
        );
        snapshot.access_token_name = None;
        assert_eq!(
            audit_actor_label(&snapshot).visible,
            "ops@example.com · via agent token (deleted token)"
        );
    }

    #[test]
    fn agent_write_actions_have_labels_and_categories() {
        for (action, label) in [
            (AuditAction::McpWriteDenied, "mcp.write_denied"),
            (AuditAction::PoamNote, "poam.note"),
            (AuditAction::PoamStatusChanged, "poam.status_change"),
            (AuditAction::PoamMilestoneAdded, "poam.milestone_add"),
            (AuditAction::PoamMilestoneUpdated, "poam.milestone_update"),
            (AuditAction::PoamMilestoneRemoved, "poam.milestone_remove"),
            (AuditAction::PoamVerificationAttempted, "poam.verify"),
        ] {
            assert_eq!(format_action_label(&action), label);
            assert_ne!(get_audit_action_color(&action).2, "config", "{label}");
        }
        let decoded: AuditEvent = serde_json::from_value(serde_json::json!({
            "timestamp": "2026-11-20T10:00:00Z",
            "actor": "ops@example.com",
            "action": "poam_verification_attempted",
            "target": "poam:1",
            "source": "admin_audit_events",
        }))
        .expect("an event without token fields decodes");
        assert_eq!(decoded.action, AuditAction::PoamVerificationAttempted);
        assert_eq!(decoded.access_token_id, None);
        assert!(!decoded.via_agent);
    }
}
