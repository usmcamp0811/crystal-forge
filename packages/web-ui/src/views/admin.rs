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
    let mut user_status_filter = use_signal(|| "all".to_string());

    let mut actor_filter = use_signal(String::new);
    let mut action_filter = use_signal(String::new);
    let mut from_filter = use_signal(String::new);
    let mut to_filter = use_signal(String::new);

    let mut create_email = use_signal(String::new);
    let mut create_display_name = use_signal(String::new);
    let mut create_password = use_signal(String::new);
    let mut create_role = use_signal(|| "Viewer".to_string());
    let mut create_environments = use_signal(String::new);
    let mut create_submitting = use_signal(|| false);

    let mut mapping_group = use_signal(String::new);
    let mut mapping_role = use_signal(|| "Viewer".to_string());
    let mut mapping_environments = use_signal(String::new);
    let mut mapping_submitting = use_signal(|| false);

    // Password reset modal state
    let mut reset_password_user: Signal<Option<AdminUserSummary>> = use_signal(|| None);
    let mut reset_password_value = use_signal(String::new);
    let mut reset_password_submitting = use_signal(|| false);
    let mut reset_password_error = use_signal(|| None::<String>);

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

    {
        let mut audit_events = audit_events.clone();
        let mut audit_total = audit_total.clone();
        let mut audit_loading = audit_loading.clone();
        let mut audit_error = audit_error.clone();
        use_effect(move || {
            let actor = actor_filter.read().clone();
            let action = action_filter.read().clone();
            let from = from_filter.read().clone();
            let to = to_filter.read().clone();
            let page = *audit_page.read();

            audit_loading.set(true);

            spawn(async move {
                let params = AdminAuditEventsParams {
                    actor: optional_value(actor),
                    action: optional_value(action),
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

    let mut active_tab = use_signal(|| "users".to_string());

    rsx! {
        div {
            class: "space-y-4",

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
            div { class: "stat-strip",
                div { class: "stat",
                    span { class: "stat-accent", style: "--stat-color:#a78bfa;" }
                    div { class: "stat-label", "CF Version" }
                    div { class: "stat-value", style: "font-size:20px;", "—" }
                    div { class: "stat-meta mono",
                        span { style: "color:var(--cf-text-muted);font-size:11px;", "not implemented yet" }
                    }
                }
                div { class: "stat",
                    span { class: "stat-accent", style: "--stat-color:#34d399;" }
                    div { class: "stat-label", "Auth mode" }
                    div { class: "stat-value", style: "font-size:16px;", "—" }
                    div { class: "stat-meta",
                        "{users.read().iter().filter(|u| u.enabled).count()} active users"
                    }
                }
                div { class: "stat",
                    span { class: "stat-accent", style: "--stat-color:#60a5fa;" }
                    div { class: "stat-label", "Database" }
                    div { class: "stat-value", style: "font-size:16px;color:var(--cf-text-muted);", "—" }
                    div { class: "stat-meta",
                        span { style: "color:var(--cf-text-muted);font-size:11px;", "not implemented yet" }
                    }
                }
                div { class: "stat",
                    span { class: "stat-accent", style: "--stat-color:#fbbf24;" }
                    div { class: "stat-label", "Active sessions" }
                    div { class: "stat-value", "—" }
                }
                div { class: "stat",
                    span { class: "stat-accent", style: "--stat-color:#f87171;" }
                    div { class: "stat-label", "TLS cert" }
                    div { class: "stat-value", style: "font-size:20px;", "—" }
                    div { class: "stat-meta",
                        span { style: "color:var(--cf-text-muted);font-size:11px;", "not implemented yet" }
                    }
                }
                } // end jobs tab

                // ── Audit Log tab ───────────────────────────────────────────
                if active_tab.read().as_str() == "audit" {
                div { style: "padding:16px;",
                div { class: "space-y-3",
                h2 { class: "text-lg font-semibold text-white", "Audit Log" }
                div {
                    class: "flex flex-col gap-3",
                    div {
                        class: "grid gap-3 sm:grid-cols-2",
                        input {
                            class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} px-3 py-2 text-sm text-white {theme::interactive::FOCUS_RING}",
                            r#type: "text",
                            placeholder: "Filter by actor...",
                            value: "{actor_filter.read()}",
                            oninput: move |evt| {
                                actor_filter.set(evt.value());
                                audit_page.set(1);
                            }
                        }
                        select {
                            class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} px-3 py-2 text-sm text-white",
                            value: "{action_filter.read()}",
                            onchange: move |evt| {
                                action_filter.set(evt.value());
                                audit_page.set(1);
                            },
                            option { value: "", "All actions" }
                            option { value: "user_created", "User created" }
                            option { value: "user_updated", "User updated" }
                            option { value: "user_deleted", "User deleted" }
                            option { value: "user_enabled", "User enabled" }
                            option { value: "user_disabled", "User disabled" }
                            option { value: "user_role_assigned", "Role assignment" }
                            option { value: "user_environment_membership_updated", "Environment membership" }
                            option { value: "oidc_mapping_changed", "OIDC mapping" }
                            option { value: "system_sync_requested", "System sync requested" }
                            option { value: "system_rollback_requested", "System rollback requested" }
                            option { value: "session_invalidated", "Session invalidated" }
                        }
                    }
                    div {
                        class: "flex flex-wrap gap-3 items-end",
                        div {
                            class: "flex flex-col gap-1 flex-1 min-w-[200px]",
                            label {
                                class: "text-xs {theme::text::MUTED}",
                                "Start date"
                            }
                            input {
                                class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} px-3 py-2 text-sm text-white {theme::interactive::FOCUS_RING}",
                                r#type: "datetime-local",
                                value: "{from_filter.read()}",
                                oninput: move |evt| {
                                    from_filter.set(evt.value());
                                    audit_page.set(1);
                                }
                            }
                        }
                        div {
                            class: "flex flex-col gap-1 flex-1 min-w-[200px]",
                            label {
                                class: "text-xs {theme::text::MUTED}",
                                "End date"
                            }
                            input {
                                class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} px-3 py-2 text-sm text-white {theme::interactive::FOCUS_RING}",
                                r#type: "datetime-local",
                                value: "{to_filter.read()}",
                                oninput: move |evt| {
                                    to_filter.set(evt.value());
                                    audit_page.set(1);
                                }
                            }
                        }
                        button {
                            class: "rounded-lg bg-gray-800 border {theme::surface::CARD_BORDER} px-4 py-2 text-sm font-medium text-white hover:bg-gray-700 transition-colors",
                            onclick: move |_| {
                                actor_filter.set(String::new());
                                action_filter.set(String::new());
                                from_filter.set(String::new());
                                to_filter.set(String::new());
                                audit_page.set(1);
                            },
                            "Clear filters"
                        }
                    }
                }

                if *audit_loading.read() {
                    div { class: "text-sm {theme::text::SECONDARY}", "Loading audit events..." }
                } else if let Some(message) = audit_error.read().clone() {
                    div {
                        class: "rounded-lg border border-red-500/40 bg-red-950/30 px-4 py-3 text-sm text-red-200",
                        "{message}"
                    }
                } else {
                    if audit_events.read().is_empty() {
                        div { class: "text-sm {theme::text::SECONDARY}", "No audit events match the selected filters." }
                    } else {
                        div {
                            class: "space-y-2",
                            for event in audit_events.read().iter() {
                                div {
                                    class: "rounded-xl border {theme::surface::CARD_BORDER} bg-gray-900/60 shadow-sm px-4 py-3 hover:bg-gray-900/80 transition-colors",
                                    div {
                                        class: "flex items-start justify-between gap-4",
                                        div {
                                            class: "space-y-1",
                                            p { class: "text-sm font-medium text-white", "{event.target}" }
                                            p { class: "text-xs {theme::text::SECONDARY}", "{format_event_actor(event)}" }
                                            p { class: "text-xs {theme::text::MUTED}", "Source: {event.source}" }
                                        }
                                        span { class: "text-xs {theme::text::MUTED} whitespace-nowrap", "{format_time(event.timestamp)}" }
                                    }
                                }
                            }
                        }
                    }

                    div {
                        class: "flex items-center justify-between pt-2",
                        p {
                            class: "text-xs {theme::text::MUTED}",
                            "Page {audit_page.read()} of {total_pages} ({audit_total.read()} total)"
                        }
                        div {
                            class: "flex items-center gap-2",
                            button {
                                class: "rounded-md bg-gray-800 border {theme::surface::CARD_BORDER} px-4 py-1.5 text-xs font-medium text-white hover:bg-gray-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
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
                                class: "rounded-md bg-gray-800 border {theme::surface::CARD_BORDER} px-4 py-1.5 text-xs font-medium text-white hover:bg-gray-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: !can_go_next,
                                onclick: move |_| {
                                    let current = *audit_page.read();
                                    if current < total_pages {
                                        audit_page.set(current + 1);
                                    }
                                },
                                "Next"
                            }
                        }
                    }
                }
                }
                } // end audit tab

                // ── Server tab ──────────────────────────────────────────────
                if active_tab.read().as_str() == "server" {
                div { style: "padding:16px;display:grid;grid-template-columns:1fr 1fr;gap:14px;",

                    // Build info card
                    div { class: "card", style: "padding:16px;",
                        h3 { style: "margin:0 0 12px;font-size:13px;font-weight:600;", "Build info" }
                        dl { class: "kv-grid",
                            dt { "Version" }
                            dd { class: "mono",
                                span { style: "color:var(--cf-text-muted);font-size:11px;", "not implemented yet — TASK-336.5" }
                            }
                            dt { "Commit" }
                            dd { class: "mono",
                                span { style: "color:var(--cf-text-muted);font-size:11px;", "not implemented yet" }
                            }
                            dt { "Uptime" }
                            dd {
                                span { style: "color:var(--cf-text-muted);font-size:11px;", "not implemented yet" }
                            }
                            dt { "Database" }
                            dd {
                                span { style: "color:var(--cf-text-muted);font-size:11px;", "not implemented yet" }
                            }
                        }
                    }

                    // Authentication info card
                    div { class: "card", style: "padding:16px;",
                        h3 { style: "margin:0 0 12px;font-size:13px;font-weight:600;", "Authentication" }
                        dl { class: "kv-grid",
                            dt { "Mode" }
                            dd {
                                span { style: "color:var(--cf-text-muted);font-size:11px;", "not implemented yet — TASK-336.5" }
                            }
                            dt { "OIDC issuer" }
                            dd { class: "mono", style: "font-size:11px;word-break:break-all;white-space:normal;",
                                span { style: "color:var(--cf-text-muted);font-size:11px;", "not implemented yet" }
                            }
                            dt { "Sessions" }
                            dd {
                                span { style: "color:var(--cf-text-muted);font-size:11px;", "not implemented yet" }
                            }
                            dt { "TLS expiry" }
                            dd {
                                span { style: "color:var(--cf-text-muted);font-size:11px;", "not implemented yet" }
                            }
                        }
                    }

                    // Heartbeat config (not implemented yet)
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
                            div {
                                "Lower intervals detect drift and outages faster but add load. Each environment can override the global default. "
                                span { style: "color:var(--cf-text-muted);", "Heartbeat config API not implemented yet — tracked in TASK-336.6." }
                            }
                        }
                        div { style: "display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:16px;opacity:0.45;pointer-events:none;",
                            div { class: "field",
                                label { "Global default" }
                                select { class: "input", disabled: true,
                                    option { "Every 30s" }
                                }
                                div { class: "help", "Applied to any environment without an override." }
                            }
                            div { class: "field",
                                label { "Mark stale after" }
                                select { class: "input", disabled: true,
                                    option { "3 missed (90s)" }
                                }
                            }
                            div { class: "field",
                                label { "Mark offline after" }
                                select { class: "input", disabled: true,
                                    option { "5 missed (2m30s)" }
                                }
                            }
                        }
                    }

                    // Classification banners (not implemented yet)
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
                                    "Display a CNSS/DoD classification marking at the top and bottom of every screen. "
                                    span { style: "color:var(--cf-text-muted);", "Not implemented yet — tracked in TASK-336.7." }
                                }
                            }
                            div { style: "flex-shrink:0;width:44px;height:24px;border-radius:999px;background:var(--cf-subtle-bg);cursor:not-allowed;opacity:0.45;", title: "not implemented yet" }
                        }
                    }

                    // Onboarding card (real)
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
                        }
                    }

                    // Maintenance card (not implemented yet)
                    div { class: "card", style: "padding:16px;grid-column:1 / -1;",
                        h3 { style: "margin:0 0 12px;font-size:13px;font-weight:600;", "Maintenance" }
                        div { style: "font-size:12px;color:var(--cf-text-muted);margin-bottom:10px;",
                            "Backup, reload, and session management actions. Not implemented yet — tracked in TASK-336.8."
                        }
                        div { style: "display:flex;gap:8px;flex-wrap:wrap;",
                            button { class: "btn btn-ghost focus-ring", disabled: true, title: "not implemented yet",
                                svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:5px;vertical-align:text-bottom;",
                                    path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3" }
                                }
                                "Backup database"
                            }
                            button { class: "btn btn-ghost focus-ring", disabled: true, title: "not implemented yet",
                                svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:5px;vertical-align:text-bottom;",
                                    path { d: "M20 12a8 8 0 0 1-14 5.3L3 14m1-4a8 8 0 0 1 14-5.3L21 8M21 3v5h-5M3 21v-5h5" }
                                }
                                "Reload config"
                            }
                            button { class: "btn btn-ghost focus-ring", disabled: true, title: "not implemented yet",
                                svg { width: "13", height: "13", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round", style: "margin-right:5px;vertical-align:text-bottom;",
                                    path { d: "M12 8v4l3 3m6-3a9 9 0 1 1-18 0 9 9 0 0 1 18 0z" }
                                }
                                "Export audit log"
                            }
                            button { class: "btn btn-ghost focus-ring", style: "color:#fbbf24;border-color:rgba(251,191,36,0.3);", disabled: true, title: "not implemented yet",
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
                } // end server tab

            } // end tab card
        } // end outer div

        // Password Reset Modal (outside page container for proper z-index layering)
        if let Some(target_user) = reset_password_user.read().clone() {
            div {
                class: "fixed inset-0 z-50 flex items-center justify-center bg-black/60",
                onclick: move |_| reset_password_user.set(None),
                div {
                    class: "bg-gray-900 border {theme::surface::CARD_BORDER} rounded-xl shadow-xl w-full max-w-md mx-4",
                    onclick: move |evt| evt.stop_propagation(),
                    div {
                        class: "px-6 py-4 border-b {theme::surface::CARD_BORDER}",
                        h3 { class: "text-lg font-semibold text-white", "Reset Password" }
                        p { class: "text-sm {theme::text::SECONDARY} mt-1", "Set a new password for {target_user.identifier}" }
                    }
                    div {
                        class: "px-6 py-4 space-y-4",
                        if let Some(error) = reset_password_error.read().clone() {
                            div {
                                class: "rounded-lg border border-red-500/40 bg-red-950/30 px-4 py-3 text-sm text-red-200",
                                "{error}"
                            }
                        }
                        div {
                            label {
                                class: "block text-sm font-medium {theme::text::SECONDARY} mb-2",
                                "New Password"
                            }
                            input {
                                class: "w-full rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} px-4 py-2 text-sm text-white {theme::interactive::FOCUS_RING}",
                                r#type: "password",
                                placeholder: "Minimum 8 characters",
                                value: "{reset_password_value.read()}",
                                oninput: move |evt| reset_password_value.set(evt.value())
                            }
                        }
                        // Password strength indicator
                        {
                            let password = reset_password_value.read().clone();
                            let strength = password_strength(&password);
                            rsx! {
                                div {
                                    class: "space-y-2",
                                    div {
                                        class: "flex gap-1",
                                        for i in 0..4 {
                                            div {
                                                class: "h-1 flex-1 rounded-full transition-colors",
                                                style: if i < strength {
                                                    match strength {
                                                        1 => "background-color: #ef4444;",
                                                        2 => "background-color: #f97316;",
                                                        3 => "background-color: #eab308;",
                                                        _ => "background-color: #22c55e;",
                                                    }
                                                } else {
                                                    "background-color: #374151;"
                                                }
                                            }
                                        }
                                    }
                                    p {
                                        class: "text-xs {theme::text::MUTED}",
                                        {password_strength_label(strength)}
                                    }
                                }
                            }
                        }
                    }
                    div {
                        class: "px-6 py-4 border-t {theme::surface::CARD_BORDER} flex justify-end gap-3",
                        button {
                            class: "rounded-lg bg-gray-800 border {theme::surface::CARD_BORDER} px-4 py-2 text-sm font-medium text-white hover:bg-gray-700 transition-colors",
                            onclick: move |_| reset_password_user.set(None),
                            "Cancel"
                        }
                        button {
                            class: "rounded-lg px-4 py-2 text-sm font-medium text-white {theme::interactive::PRIMARY_BTN} disabled:opacity-50",
                            disabled: *reset_password_submitting.read() || reset_password_value.read().len() < 8,
                            onclick: {
                                let user_id = target_user.id.clone();
                                move |_| {
                                    let password = reset_password_value.read().clone();
                                    if password.len() < 8 {
                                        reset_password_error.set(Some("Password must be at least 8 characters".to_string()));
                                        return;
                                    }

                                    let user_id = user_id.clone();
                                    let mut reset_password_submitting = reset_password_submitting.clone();
                                    let mut reset_password_error = reset_password_error.clone();
                                    let mut reset_password_user = reset_password_user.clone();
                                    let mut users = users.clone();
                                    let mut user_drafts = user_drafts.clone();
                                    let mut users_error = users_error.clone();

                                    reset_password_submitting.set(true);
                                    spawn(async move {
                                        let request = AdminUpdateUserRequest {
                                            role: None,
                                            enabled: None,
                                            environments: None,
                                            password: Some(password),
                                        };
                                        match update_admin_user(&user_id, &request).await {
                                            Ok(_) => {
                                                refresh_users(users, user_drafts, users_error).await;
                                                reset_password_user.set(None);
                                            }
                                            Err(e) => {
                                                reset_password_error.set(Some(format!("Failed to reset password: {e}")));
                                            }
                                        }
                                        reset_password_submitting.set(false);
                                    });
                                }
                            },
                            if *reset_password_submitting.read() { "Resetting..." } else { "Reset Password" }
                        }
                    }
                }
            }
        }
    }
}

fn password_strength(password: &str) -> usize {
    if password.is_empty() {
        return 0;
    }
    let mut score = 0;
    if password.len() >= 8 {
        score += 1;
    }
    if password.len() >= 12 {
        score += 1;
    }
    if password.chars().any(|c| c.is_ascii_digit()) {
        score += 1;
    }
    if password.chars().any(|c| !c.is_ascii_alphanumeric()) {
        score += 1;
    }
    score
}

fn password_strength_label(strength: usize) -> &'static str {
    match strength {
        0 => "Enter a password",
        1 => "Weak",
        2 => "Fair",
        3 => "Good",
        _ => "Strong",
    }
}

fn format_role(role: Option<crate::api::models::Role>) -> &'static str {
    match role {
        Some(crate::api::models::Role::Admin) => "Admin",
        Some(crate::api::models::Role::Operator) => "Operator",
        Some(crate::api::models::Role::Viewer) => "Viewer",
        None => "Unassigned",
    }
}

fn format_action(event: &AuditEvent) -> &'static str {
    match event.action {
        crate::api::models::AuditAction::UserCreated => "User created",
        crate::api::models::AuditAction::UserUpdated => "User updated",
        crate::api::models::AuditAction::UserDeleted => "User deleted",
        crate::api::models::AuditAction::UserEnabled => "User enabled",
        crate::api::models::AuditAction::UserDisabled => "User disabled",
        crate::api::models::AuditAction::UserRoleAssigned => "Role assignment",
        crate::api::models::AuditAction::UserEnvironmentMembershipUpdated => {
            "Environment membership"
        }
        crate::api::models::AuditAction::OidcMappingChanged => "OIDC mapping change",
        crate::api::models::AuditAction::SystemSyncRequested => "System sync requested",
        crate::api::models::AuditAction::SystemDeployRequested => "System deploy requested",
        crate::api::models::AuditAction::SystemRollbackRequested => "System rollback requested",
        crate::api::models::AuditAction::SessionInvalidated => "Session invalidated",
    }
}

fn format_environments(values: &[String]) -> String {
    if values.is_empty() {
        "All / Unscoped".to_string()
    } else {
        values.join(", ")
    }
}

fn users_render_state(users_loading: bool, users_error: Option<&str>) -> UsersRenderState {
    users_render_state_with_data(users_loading, users_error, false)
}

fn users_render_state_with_data(
    users_loading: bool,
    users_error: Option<&str>,
    has_loaded_users: bool,
) -> UsersRenderState {
    if users_loading {
        return UsersRenderState::Loading;
    }

    if users_error.is_some() && !has_loaded_users {
        return UsersRenderState::Error;
    }

    UsersRenderState::Table
}

fn users_error_message(users_error: Option<String>) -> String {
    users_error.unwrap_or_else(|| "Failed to load users".to_string())
}

fn filtered_admin_users(
    users: &[AdminUserSummary],
    search: &str,
    status_filter: &str,
) -> Vec<AdminUserSummary> {
    users
        .iter()
        .filter(|user| user_matches_filters(user, search, status_filter))
        .cloned()
        .collect()
}

fn user_matches_filters(user: &AdminUserSummary, search: &str, status_filter: &str) -> bool {
    let search = search.trim().to_ascii_lowercase();
    if !search.is_empty() && !user.identifier.to_ascii_lowercase().contains(&search) {
        return false;
    }

    match status_filter {
        "enabled" => user.enabled,
        "disabled" => !user.enabled,
        _ => true,
    }
}

#[cfg(target_arch = "wasm32")]
fn confirm_user_delete(identifier: &str) -> bool {
    web_sys::window()
        .and_then(|window| {
            window
                .confirm_with_message(&format!("Delete user {identifier}? This cannot be undone."))
                .ok()
        })
        .unwrap_or(false)
}

#[cfg(not(target_arch = "wasm32"))]
fn confirm_user_delete(_identifier: &str) -> bool {
    true
}

fn format_event_actor(event: &AuditEvent) -> String {
    format!(
        "{} by {}",
        format_action(event),
        event.actor.clone().unwrap_or_else(|| "system".to_string())
    )
}

fn format_user_environments(user: &AdminUserSummary) -> String {
    let environments = user.environments.clone();
    if environments.is_empty() {
        "All / Unscoped".to_string()
    } else {
        environments.join(", ")
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

/// Convert datetime-local input value to RFC3339 format.
/// datetime-local format: "2026-02-15T18:19"
/// RFC3339 format: "2026-02-15T18:19:00Z"
fn datetime_local_to_rfc3339(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    // datetime-local gives us "YYYY-MM-DDTHH:MM", we need to add seconds and timezone
    Some(format!("{trimmed}:00Z"))
}

fn role_from_string(value: &str) -> Role {
    match value {
        "Admin" => Role::Admin,
        "Operator" => Role::Operator,
        _ => Role::Viewer,
    }
}

fn parse_environments(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn validate_and_parse_environments(value: &str) -> Result<Vec<String>, String> {
    let parsed = parse_environments(value);
    for entry in &parsed {
        if entry.contains('*') {
            return Err(
                "Wildcard patterns are not supported yet; use exact environment names.".to_string(),
            );
        }

        let valid = entry
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'));
        if !valid {
            return Err(format!(
                "Invalid environment '{}': only letters, numbers, '-', '_', and '.' are allowed.",
                entry
            ));
        }
    }

    Ok(parsed)
}

#[derive(Debug, Clone)]
struct UserEditDraft {
    role: String,
    enabled: bool,
    environments: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UsersRenderState {
    Loading,
    Error,
    Table,
}

impl UserEditDraft {
    fn from_user(user: &AdminUserSummary) -> Self {
        Self {
            role: editable_role_label(user.role).to_string(),
            enabled: user.enabled,
            environments: user.environments.join(", "),
        }
    }
}

fn editable_role_label(role: Option<Role>) -> &'static str {
    match role {
        Some(Role::Admin) => "Admin",
        Some(Role::Operator) => "Operator",
        Some(Role::Viewer) | None => "Viewer",
    }
}

fn identity_source_label(source: IdentitySource) -> &'static str {
    match source {
        IdentitySource::LocalManaged => "Local",
        IdentitySource::OidcDerived => "OIDC",
    }
}

fn identity_source_badge_class(source: IdentitySource) -> &'static str {
    match source {
        IdentitySource::LocalManaged => "border border-cyan-500/40 bg-cyan-950/40 text-cyan-200",
        IdentitySource::OidcDerived => "border border-amber-500/40 bg-amber-950/40 text-amber-200",
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
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample_user(role: Option<Role>, environments: Vec<&str>) -> AdminUserSummary {
        AdminUserSummary {
            id: "user-1".to_string(),
            identifier: "user@example.com".to_string(),
            identity_source: IdentitySource::LocalManaged,
            role,
            enabled: true,
            environments: environments.into_iter().map(ToString::to_string).collect(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn user_edit_draft_reflects_user_fields() {
        let user = sample_user(Some(Role::Operator), vec!["staging", "prod"]);
        let draft = UserEditDraft::from_user(&user);

        assert_eq!(draft.role, "Operator");
        assert_eq!(draft.environments, "staging, prod");
        assert!(draft.enabled);
    }

    #[test]
    fn editable_role_label_defaults_to_viewer_when_unassigned() {
        assert_eq!(editable_role_label(None), "Viewer");
    }

    #[test]
    fn format_environments_shows_unscoped_when_empty() {
        assert_eq!(format_environments(&[]), "All / Unscoped");
    }

    #[test]
    fn validate_and_parse_environments_rejects_wildcards() {
        let err = validate_and_parse_environments("company-*").expect_err("wildcard must fail");
        assert!(err.contains("Wildcard patterns are not supported"));
    }

    #[test]
    fn validate_and_parse_environments_accepts_expected_tokens() {
        let values =
            validate_and_parse_environments("prod-west, staging_1, qa.env").expect("valid names");
        assert_eq!(
            values,
            vec![
                "prod-west".to_string(),
                "staging_1".to_string(),
                "qa.env".to_string()
            ]
        );
    }

    #[test]
    fn users_render_state_prioritizes_loading_then_error_then_table() {
        assert_eq!(
            users_render_state_with_data(true, Some("boom"), true),
            UsersRenderState::Loading
        );
        assert_eq!(
            users_render_state_with_data(false, Some("boom"), false),
            UsersRenderState::Error
        );
        assert_eq!(
            users_render_state_with_data(false, Some("boom"), true),
            UsersRenderState::Table
        );
        assert_eq!(users_render_state(false, None), UsersRenderState::Table);
    }

    #[test]
    fn users_error_message_has_safe_fallback() {
        assert_eq!(
            users_error_message(Some("custom".to_string())),
            "custom".to_string()
        );
        assert_eq!(
            users_error_message(None),
            "Failed to load users".to_string()
        );
    }

    #[test]
    fn user_matches_filters_respects_search_and_status() {
        let mut user = sample_user(Some(Role::Viewer), vec![]);
        user.identifier = "alice@example.com".to_string();
        user.enabled = false;

        assert!(user_matches_filters(&user, "alice", "all"));
        assert!(!user_matches_filters(&user, "bob", "all"));
        assert!(user_matches_filters(&user, "", "disabled"));
        assert!(!user_matches_filters(&user, "", "enabled"));
    }
}
