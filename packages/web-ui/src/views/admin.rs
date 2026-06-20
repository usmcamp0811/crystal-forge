use chrono::Local;
use dioxus::prelude::*;
use std::collections::HashMap;

use crate::api::client::{
    create_admin_user, delete_admin_oidc_mapping, delete_admin_user, fetch_admin_audit_events,
    fetch_admin_oidc_mappings, fetch_admin_users, set_setup_wizard_dismissed, update_admin_user,
    upsert_admin_oidc_mapping,
};
use crate::api::models::{
    AdminAuditEventsParams, AdminCreateUserRequest, AdminUpdateUserRequest,
    AdminUpsertOidcMappingRequest, AdminUserSummary, AuditEvent, IdentitySource, OidcGroupMapping,
    Role,
};
use crate::theme;

const AUDIT_PER_PAGE: i64 = 20;

// ============================================================================
// MOCK DATA STRUCTURES
// ============================================================================

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

#[derive(Clone, Debug)]
struct BackgroundJob {
    id: &'static str,
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

const BACKGROUND_JOBS_MOCK: &[BackgroundJob] = &[
    BackgroundJob {
        id: "j1",
        name: "Cache status poll",
        desc: "Query binary caches to confirm tracked store paths still exist (detect GC eviction).",
        interval: "15m",
        enabled: true,
        last_run: "3m ago",
        last_duration: "4.2s",
        next_run: "in 12m",
        status: "healthy",
        impact: "low",
        note: None,
    },
    BackgroundJob {
        id: "j2",
        name: "GC-eviction reconcile",
        desc: "Flag configs whose derivations were garbage-collected so Scanning marks them needs-build.",
        interval: "1h",
        enabled: true,
        last_run: "24m ago",
        last_duration: "11s",
        next_run: "in 36m",
        status: "healthy",
        impact: "medium",
        note: None,
    },
    BackgroundJob {
        id: "j3",
        name: "CVE DB refresh",
        desc: "Pull latest NVD / advisory feeds into the local vulnerability database.",
        interval: "6h",
        enabled: true,
        last_run: "1h ago",
        last_duration: "38s",
        next_run: "in 5h",
        status: "healthy",
        impact: "low",
        note: None,
    },
    BackgroundJob {
        id: "j4",
        name: "Agent heartbeat sweep",
        desc: "Mark systems offline if no heartbeat past their interval; recompute fleet health.",
        interval: "1m",
        enabled: true,
        last_run: "32s ago",
        last_duration: "0.6s",
        next_run: "in 28s",
        status: "healthy",
        impact: "low",
        note: None,
    },
    BackgroundJob {
        id: "j5",
        name: "Stale build-job reaper",
        desc: "Re-queue or fail builds stuck past their timeout on dead builders.",
        interval: "5m",
        enabled: true,
        last_run: "2m ago",
        last_duration: "1.1s",
        next_run: "in 3m",
        status: "healthy",
        impact: "low",
        note: None,
    },
    BackgroundJob {
        id: "j6",
        name: "Flake poll & sync",
        desc: "Fetch tracked flake repos and enqueue evals for new commits.",
        interval: "5m",
        enabled: true,
        last_run: "4m ago",
        last_duration: "6.8s",
        next_run: "in 1m",
        status: "healthy",
        impact: "medium",
        note: None,
    },
    BackgroundJob {
        id: "j7",
        name: "Session GC",
        desc: "Expire idle sessions and purge revoked tokens.",
        interval: "30m",
        enabled: true,
        last_run: "18m ago",
        last_duration: "0.3s",
        next_run: "in 12m",
        status: "healthy",
        impact: "low",
        note: None,
    },
    BackgroundJob {
        id: "j8",
        name: "Audit log archival",
        desc: "Roll audit events older than retention window to cold storage.",
        interval: "24h",
        enabled: false,
        last_run: "never",
        last_duration: "—",
        next_run: "disabled",
        status: "disabled",
        impact: "medium",
        note: None,
    },
    BackgroundJob {
        id: "j9",
        name: "Cache storage metrics",
        desc: "Pull bucket size / object counts (CloudWatch, atticd) for the Caches view.",
        interval: "1h",
        enabled: true,
        last_run: "41m ago",
        last_duration: "9.4s",
        next_run: "in 19m",
        status: "degraded",
        impact: "medium",
        note: Some("edge-cache poll timed out last run"),
    },
];

const JOB_INTERVALS: &[&str] = &["1m", "5m", "15m", "30m", "1h", "6h", "12h", "24h", "never"];

#[derive(Clone, Debug)]
struct ServerInfo {
    version: &'static str,
    commit: &'static str,
    uptime: &'static str,
    auth_mode: &'static str,
    oidc_issuer: &'static str,
    db_status: &'static str,
    db_size: &'static str,
    sessions: usize,
    tls_expiry: &'static str,
}

const SERVER_INFO_MOCK: ServerInfo = ServerInfo {
    version: "0.8.2",
    commit: "f3a9c01",
    uptime: "18d 4h",
    auth_mode: "OIDC (Keycloak)",
    oidc_issuer: "https://keycloak.acme.io/realms/crystal-forge",
    db_status: "healthy",
    db_size: "2.4 GB",
    sessions: 6,
    tls_expiry: "62d",
};

#[derive(Clone, Debug)]
struct EnvironmentDef {
    name: &'static str,
    color: &'static str,
}

const ENVIRONMENTS_MOCK: &[EnvironmentDef] = &[
    EnvironmentDef {
        name: "production",
        color: "#f87171",
    },
    EnvironmentDef {
        name: "staging",
        color: "#fbbf24",
    },
    EnvironmentDef {
        name: "dev",
        color: "#60a5fa",
    },
    EnvironmentDef {
        name: "edge",
        color: "#a78bfa",
    },
    EnvironmentDef {
        name: "lab",
        color: "#34d399",
    },
];

// ============================================================================
// MAIN COMPONENT
// ============================================================================

#[component]
pub fn AdminView() -> Element {
    let nav = navigator();
    let mut users = use_signal(Vec::<AdminUserSummary>::new);
    let mut user_drafts = use_signal(HashMap::<String, UserEditDraft>::new);

    let mut audit_events = use_signal(Vec::<AuditEvent>::new);
    let mut oidc_mappings = use_signal(Vec::<OidcGroupMapping>::new);
    let mut audit_total = use_signal(|| 0_i64);
    let mut audit_page = use_signal(|| 1_i64);

    let mut users_loading = use_signal(|| true);
    let mut audit_loading = use_signal(|| true);
    let mut users_error = use_signal(|| None::<String>);
    let mut audit_error = use_signal(|| None::<String>);
    let mut oidc_error = use_signal(|| None::<String>);

    let mut user_search = use_signal(String::new);
    let mut user_role_filter = use_signal(|| "all".to_string());

    let mut actor_filter = use_signal(String::new);
    let mut audit_category_filter = use_signal(|| "all".to_string());
    let mut from_filter = use_signal(String::new);
    let mut to_filter = use_signal(String::new);

    let mut active_tab = use_signal(|| "users".to_string());

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
            ServerInfoStrip { users: users.read().clone() }

            // ── Tab card ─────────────────────────────────────────────────────
            div { class: "card", style: "overflow:hidden;",
                // ── Tab bar ──────────────────────────────────────────────────
                div { class: "sd-tabs", style: "padding:0 16px;border-bottom:1px solid var(--cf-card-border);",
                    for (tab_id, tab_label, _icon) in [
                        ("users", "Users", "server"),
                        ("roles", "Roles", "key"),
                        ("oidc", "OIDC Mappings", "link"),
                        ("jobs", "Background Jobs", "sync"),
                        ("audit", "Audit Log", "history"),
                        ("server", "Server", "gear"),
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
                        audit_category_filter: audit_category_filter.clone(),
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
                    ServerTab {}
                }
            }
        }
    }
}

// ============================================================================
// SERVER INFO STRIP
// ============================================================================

#[component]
fn ServerInfoStrip(users: Vec<AdminUserSummary>) -> Element {
    let s = &SERVER_INFO_MOCK;
    let active_users = users.iter().filter(|u| u.enabled).count();

    rsx! {
        div { class: "stat-strip",
            div { class: "stat",
                span { class: "stat-accent", style: "--stat-color:#a78bfa;" }
                div { class: "stat-label", "CF Version" }
                div { class: "stat-value", style: "font-size:20px;", "{s.version}" }
                div { class: "stat-meta mono", "{s.commit} · up {s.uptime}" }
            }
            div { class: "stat",
                span { class: "stat-accent", style: "--stat-color:#34d399;" }
                div { class: "stat-label", "Auth mode" }
                div { class: "stat-value", style: "font-size:16px;", "{s.auth_mode}" }
                div { class: "stat-meta", "{active_users} active users" }
            }
            div { class: "stat",
                span { class: "stat-accent", style: "--stat-color:#60a5fa;" }
                div { class: "stat-label", "Database" }
                div { class: "stat-value", style: "font-size:16px;color:#34d399;", "{s.db_status}" }
                div { class: "stat-meta", "{s.db_size}" }
            }
            div { class: "stat",
                span { class: "stat-accent", style: "--stat-color:#fbbf24;" }
                div { class: "stat-label", "Active sessions" }
                div { class: "stat-value", "{s.sessions}" }
            }
            div { class: "stat",
                span { class: "stat-accent", style: "--stat-color:#f87171;" }
                div { class: "stat-label", "TLS cert" }
                div { class: "stat-value", style: "font-size:20px;", "{s.tls_expiry}" }
                div { class: "stat-meta", "until expiry" }
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
) -> Element {
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
                            th { "Last login" }
                            th { style: "text-align:right;", " " }
                        }
                    }
                    tbody {
                        for user in filtered_users {
                            {
                                let user_id = user.id.clone();
                                let is_service = user.identifier.contains("bot") || user.identifier.contains("audit");
                                let initials = get_user_initials(&user.identifier);

                                rsx! {
                                    tr {
                                        td {
                                            div { style: "display:flex;align-items:center;gap:10px;",
                                                // Avatar
                                                div {
                                                    style: if is_service {
                                                        "width:28px;height:28px;border-radius:50%;background:var(--cf-subtle-bg);display:grid;place-items:center;font-size:11px;font-weight:600;color:var(--cf-text-muted);flex-shrink:0;"
                                                    } else {
                                                        "width:28px;height:28px;border-radius:50%;background:linear-gradient(135deg,#a78bc4,#654a84);display:grid;place-items:center;font-size:11px;font-weight:600;color:#fff;flex-shrink:0;"
                                                    },
                                                    if is_service {
                                                        svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                                            rect { x: "4", y: "4", width: "16", height: "16", rx: "2" }
                                                            rect { x: "9", y: "9", width: "6", height: "6" }
                                                            path { d: "M15 2v2M15 20v2M2 15h2M20 15h2M2.88 2.88l1.42 1.42M19.7 19.7l1.42 1.42M2.88 21.12l1.42-1.42M19.7 4.3l1.42-1.42" }
                                                        }
                                                    } else {
                                                        "{initials}"
                                                    }
                                                }
                                                div {
                                                    div { style: "font-weight:600;font-size:13px;display:flex;align-items:center;gap:6px;",
                                                        "{user.identifier}"
                                                        if is_service {
                                                            span { class: "chip chip-unknown", style: "font-size:9px;", "service" }
                                                        }
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
                                                        {
                                                            let env_def = ENVIRONMENTS_MOCK.iter().find(|e| e.name == env);
                                                            if let Some(def) = env_def {
                                                                rsx! {
                                                                    span {
                                                                        class: "chip",
                                                                        style: "background:color-mix(in oklab, {def.color} 14%, var(--cf-card-bg));border:1px solid {def.color};color:{def.color};font-size:10px;gap:4px;",
                                                                        span { style: "width:4px;height:4px;border-radius:50%;background:{def.color};" }
                                                                        "{env}"
                                                                    }
                                                                }
                                                            } else {
                                                                rsx! { span { class: "chip chip-unknown", style: "font-size:10px;", "{env}" } }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        td {
                                            // Mock MFA status
                                            if user.identifier.contains("mreyes") || user.identifier.contains("jpark") {
                                                span { class: "chip chip-healthy", style: "font-size:10px;", "on" }
                                            } else {
                                                span { class: "chip chip-warning", style: "font-size:10px;", "off" }
                                            }
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
                                                    svg { width: "14", height: "14", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                                        circle { cx: "12", cy: "12", r: "3" }
                                                        path { d: "M12 1v6M12 17v6M3.51 9h6M14.51 9h6M6 20.51l3-3M15 11.51l3-3M3.51 15h6M14.51 15h6M6 3.51l3 3M15 12.51l3 3" }
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
) -> Element {
    let mut mapping_group = use_signal(String::new);
    let mut mapping_role = use_signal(|| "Viewer".to_string());
    let mut mapping_environments = use_signal(String::new);
    let mut mapping_submitting = use_signal(|| false);

    rsx! {
        div { style: "padding:0;",
            // Connection status callout
            div { style: "padding:14px 16px;border-bottom:1px solid var(--cf-divider);",
                div { class: "sd-callout sd-callout-info", style: "font-size:12px;",
                    svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                        path { d: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" }
                        path { d: "M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" }
                    }
                    div {
                        "Connected to "
                        span { class: "mono", "{SERVER_INFO_MOCK.oidc_issuer}" }
                        ". When a user logs in, their IdP groups are matched top-down; the first matching mapping sets their role and environment scope."
                    }
                }
            }

            // Add mapping button
            div { style: "padding:10px 16px;display:flex;justify-content:flex-end;",
                button { class: "btn btn-primary focus-ring",
                    svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:5px;vertical-align:text-bottom;",
                        line { x1: "12", y1: "5", x2: "12", y2: "19" }
                        line { x1: "5", y1: "12", x2: "19", y2: "12" }
                    }
                    "Add mapping"
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
                            // Mock matched users count
                            let matched_users = if mapping.group_name.contains("admins") { 1 }
                                else if mapping.group_name.contains("operators") { 2 }
                                else if mapping.group_name.contains("sre") { 1 }
                                else { 2 };

                            rsx! {
                                tr {
                                    td {
                                        span { class: "mono", style: "font-size:12px;color:var(--cf-text-muted);", "#{idx + 1}" }
                                    }
                                    td {
                                        span { class: "mono", style: "font-weight:600;font-size:13px;", "{mapping.group_name}" }
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
                                                    {
                                                        let env_def = ENVIRONMENTS_MOCK.iter().find(|e| e.name == env);
                                                        if let Some(def) = env_def {
                                                            rsx! {
                                                                span {
                                                                    class: "chip",
                                                                    style: "background:color-mix(in oklab, {def.color} 14%, var(--cf-card-bg));border:1px solid {def.color};color:{def.color};font-size:10px;gap:4px;",
                                                                    span { style: "width:4px;height:4px;border-radius:50%;background:{def.color};" }
                                                                    "{env}"
                                                                }
                                                            }
                                                        } else {
                                                            rsx! { span { class: "chip chip-unknown", style: "font-size:10px;", "{env}" } }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    td { class: "mono", style: "font-size:12px;", "{matched_users}" }
                                    td {
                                        div { class: "row-actions",
                                            button {
                                class: "btn-icon focus-ring",
                                title: "Edit",
                                onclick: {
                                    let mapping_id = mapping.id.clone();
                                    move |_| {
                                        let id = mapping_id.clone();
                                        // Handle delete
                                        spawn(async move {
                                            let _ = delete_admin_oidc_mapping(&id).await;
                                        });
                                    }
                                },
                                                svg { width: "14", height: "14", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                                                    circle { cx: "12", cy: "12", r: "3" }
                                                    path { d: "M12 1v6M12 17v6M3.51 9h6M14.51 9h6M6 20.51l3-3M15 11.51l3-3M3.51 15h6M14.51 15h6M6 3.51l3 3M15 12.51l3 3" }
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
                    div { "Scheduled server-side tasks. Crank intervals down for freshness, up to save resources. Cache polling and GC reconciliation can be heavy on large fleets — schedule deliberately." }
                }
            }

            // Jobs table
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
                            let (status_class, status_label) = match job.status {
                                "healthy" => ("chip-healthy", "healthy"),
                                "degraded" => ("chip-warning", "degraded"),
                                _ => ("chip-unknown", "disabled"),
                            };
                            let (impact_class, impact_label) = match job.impact {
                                "low" => ("chip-healthy", "low load"),
                                "medium" => ("chip-warning", "medium load"),
                                _ => ("chip-critical", "high load"),
                            };

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

// ============================================================================
// AUDIT LOG TAB
// ============================================================================

#[component]
fn AuditTab(
    audit_events: Signal<Vec<AuditEvent>>,
    audit_loading: bool,
    audit_error: Signal<Option<String>>,
    audit_category_filter: Signal<String>,
    actor_filter: Signal<String>,
    audit_page: Signal<i64>,
    can_go_prev: bool,
    can_go_next: bool,
    total_pages: i64,
    audit_total: i64,
) -> Element {
    let filtered_events = {
        let cat_filter = audit_category_filter.read().clone();
        audit_events
            .read()
            .iter()
            .filter(|e| {
                if cat_filter == "all" {
                    true
                } else {
                    // Mock category filtering - in real implementation would check event.action
                    true
                }
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
                        placeholder: "Search actor / action / target…",
                        value: "{actor_filter.read()}",
                        oninput: move |evt| {
                            actor_filter.set(evt.value());
                            audit_page.set(1);
                        }
                    }
                }
                div { class: "seg",
                    for category in ["all", "security", "deploy", "build", "config", "auth"] {
                        button {
                            class: if *audit_category_filter.read() == category { "active" } else { "" },
                            onclick: {
                                let cat = category.to_string();
                                move |_| audit_category_filter.set(cat.clone())
                            },
                            "{category}"
                        }
                    }
                }
                span { class: "filter-count", "{filtered_events.len()} events" }
                button { class: "btn btn-ghost focus-ring", style: "margin-left:auto;",
                    svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:5px;vertical-align:text-bottom;",
                        path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3" }
                    }
                    "Export"
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
                                        td { class: "mono", style: "font-size:11px;color:var(--cf-text-muted);", "10.2.4.18" }
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

#[component]
fn ServerTab() -> Element {
    let nav = navigator();
    let s = &SERVER_INFO_MOCK;

    rsx! {
        div { style: "padding:16px;display:grid;grid-template-columns:1fr 1fr;gap:14px;",
            // Build info card
            div { class: "card", style: "padding:16px;",
                h3 { style: "margin:0 0 12px;font-size:13px;font-weight:600;", "Build info" }
                dl { class: "kv-grid",
                    dt { "Version" }
                    dd { class: "mono", "{s.version}" }
                    dt { "Commit" }
                    dd { class: "mono", "{s.commit}" }
                    dt { "Uptime" }
                    dd { "{s.uptime}" }
                    dt { "Database" }
                    dd {
                        span { class: "chip chip-healthy", "{s.db_status}" }
                        " "
                        span { style: "color:var(--cf-text-muted);", "· {s.db_size}" }
                    }
                }
            }

            // Authentication info card
            div { class: "card", style: "padding:16px;",
                h3 { style: "margin:0 0 12px;font-size:13px;font-weight:600;", "Authentication" }
                dl { class: "kv-grid",
                    dt { "Mode" }
                    dd { "{s.auth_mode}" }
                    dt { "OIDC issuer" }
                    dd { class: "mono", style: "font-size:11px;word-break:break-all;white-space:normal;", "{s.oidc_issuer}" }
                    dt { "Sessions" }
                    dd { "{s.sessions} active" }
                    dt { "TLS expiry" }
                    dd {
                        span { class: "chip chip-healthy", "{s.tls_expiry}" }
                    }
                }
            }

            // Agent heartbeat config card
            HeartbeatConfigCard {}

            // Classification banners card
            div { class: "card", style: "padding:16px;grid-column:1 / -1;",
                div { style: "display:flex;align-items:flex-start;justify-content:space-between;gap:12px;flex-wrap:wrap;",
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
                    // Toggle switch (non-functional mock)
                    div {
                        style: "flex-shrink:0;width:44px;height:24px;border-radius:999px;background:var(--cf-subtle-bg);position:relative;cursor:pointer;",
                        div { style: "position:absolute;top:2px;left:2px;width:20px;height:20px;border-radius:50%;background:#fff;box-shadow:0 1px 3px rgba(0,0,0,0.3);" }
                    }
                }
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
                                    }
                                    nav.push("/");
                                });
                            },
                            svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:5px;vertical-align:text-bottom;",
                                path { d: "M20 12a8 8 0 0 1-14 5.3L3 14m1-4a8 8 0 0 1 14-5.3L21 8M21 3v5h-5M3 21v-5h5" }
                            }
                            "Relaunch Setup Coach"
                        }
                        button { class: "btn btn-ghost focus-ring", "Reset progress" }
                    }
                }
            }

            // Maintenance card
            div { class: "card", style: "padding:16px;grid-column:1 / -1;",
                h3 { style: "margin:0 0 12px;font-size:13px;font-weight:600;", "Maintenance" }
                div { style: "display:flex;gap:8px;flex-wrap:wrap;",
                    button { class: "btn btn-ghost focus-ring",
                        svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:5px;vertical-align:text-bottom;",
                            path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3" }
                        }
                        "Backup database"
                    }
                    button { class: "btn btn-ghost focus-ring",
                        svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:5px;vertical-align:text-bottom;",
                            path { d: "M20 12a8 8 0 0 1-14 5.3L3 14m1-4a8 8 0 0 1 14-5.3L21 8M21 3v5h-5M3 21v-5h5" }
                        }
                        "Reload config"
                    }
                    button { class: "btn btn-ghost focus-ring",
                        svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:5px;vertical-align:text-bottom;",
                            path { d: "M12 8v4l3 3m6-3a9 9 0 1 1-18 0 9 9 0 0 1 18 0z" }
                        }
                        "Export audit log"
                    }
                    button { class: "btn btn-ghost focus-ring", style: "color:#fbbf24;border-color:rgba(251,191,36,0.3);",
                        svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:5px;vertical-align:text-bottom;",
                            path { d: "M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" }
                            line { x1: "12", y1: "9", x2: "12", y2: "13" }
                            line { x1: "12", y1: "17", x2: "12.01", y2: "17" }
                        }
                        "Invalidate all sessions"
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
fn HeartbeatConfigCard() -> Element {
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
                div { style: "display:flex;flex-direction:column;gap:0;border:1px solid var(--cf-divider);border-radius:8px;overflow:hidden;",
                    for (idx, env) in ENVIRONMENTS_MOCK.iter().enumerate() {
                        div {
                            style: if idx > 0 { "display:flex;align-items:center;gap:12px;padding:10px 12px;border-top:1px solid var(--cf-divider);" } else { "display:flex;align-items:center;gap:12px;padding:10px 12px;" },
                            span { style: "width:8px;height:8px;border-radius:50%;background:{env.color};flex-shrink:0;" }
                            div { style: "flex:1;min-width:0;",
                                span { class: "mono", style: "font-size:13px;font-weight:600;", "{env.name}" }
                                span { style: "font-size:11px;color:var(--cf-text-muted);margin-left:8px;", "5 systems" }
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
        AuditAction::UserCreated | AuditAction::UserDeleted | AuditAction::UserRoleAssigned => {
            ("#f87171", "chip-critical", "security")
        }
        AuditAction::SystemDeployRequested | AuditAction::SystemRollbackRequested => {
            ("#a78bfa", "chip-info", "deploy")
        }
        AuditAction::SessionInvalidated => ("#fbbf24", "chip-warning", "auth"),
        _ => ("var(--cf-text-primary)", "chip-unknown", "config"),
    }
}

fn format_action_label(action: &crate::api::models::AuditAction) -> &'static str {
    use crate::api::models::AuditAction;
    match action {
        AuditAction::UserCreated => "user.create",
        AuditAction::UserUpdated => "user.update",
        AuditAction::UserDeleted => "user.delete",
        AuditAction::UserEnabled => "user.enable",
        AuditAction::UserDisabled => "user.disable",
        AuditAction::UserRoleAssigned => "user.role_change",
        AuditAction::UserEnvironmentMembershipUpdated => "user.env_change",
        AuditAction::OidcMappingChanged => "oidc.mapping_edit",
        AuditAction::SystemSyncRequested => "system.sync",
        AuditAction::SystemDeployRequested => "system.deploy",
        AuditAction::SystemRollbackRequested => "system.rollback",
        AuditAction::SessionInvalidated => "auth.session_kill",
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
}

impl UserEditDraft {
    fn from_user(user: &AdminUserSummary) -> Self {
        Self {
            role: role_to_string(&user.role),
            enabled: user.enabled,
            environments: user.environments.join(", "),
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
