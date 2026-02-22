use chrono::Local;
use dioxus::prelude::*;

use crate::api::client::{fetch_admin_audit_events, fetch_admin_users};
use crate::api::models::{AdminAuditEventsParams, AdminUserSummary, AuditEvent};
use crate::theme;

const AUDIT_PER_PAGE: i64 = 20;

#[component]
pub fn AdminView() -> Element {
    let mut users = use_signal(Vec::<AdminUserSummary>::new);
    let mut audit_events = use_signal(Vec::<AuditEvent>::new);
    let mut audit_total = use_signal(|| 0_i64);
    let mut audit_page = use_signal(|| 1_i64);

    let mut users_loading = use_signal(|| true);
    let mut audit_loading = use_signal(|| true);
    let mut users_error = use_signal(|| None::<String>);
    let mut audit_error = use_signal(|| None::<String>);

    let mut actor_filter = use_signal(String::new);
    let mut action_filter = use_signal(String::new);
    let mut from_filter = use_signal(String::new);
    let mut to_filter = use_signal(String::new);

    {
        let mut users = users.clone();
        let mut users_loading = users_loading.clone();
        let mut users_error = users_error.clone();
        use_effect(move || {
            spawn(async move {
                match fetch_admin_users().await {
                    Ok(next_users) => {
                        users.set(next_users);
                        users_error.set(None);
                    }
                    Err(e) => {
                        users_error.set(Some(format!("Failed to load admin users: {e}")));
                    }
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
                    from: optional_value(from),
                    to: optional_value(to),
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
            class: "space-y-6",
            header {
                class: "space-y-2",
                h1 { class: "{theme::typography::PAGE_TITLE}", "Server Management" }
                p { class: "text-sm {theme::text::SECONDARY}", "Manage users, role assignments, and review recent security-sensitive actions." }
            }

            section {
                class: "space-y-3",
                h2 { class: "text-lg font-semibold text-white", "Users" }
                if *users_loading.read() {
                    div { class: "text-sm {theme::text::SECONDARY}", "Loading users..." }
                } else if let Some(message) = users_error.read().clone() {
                    div {
                        class: "rounded-lg border border-red-500/40 bg-red-950/30 px-4 py-3 text-sm text-red-200",
                        "{message}"
                    }
                } else {
                    div {
                        class: "overflow-auto rounded-xl border {theme::surface::CARD_BORDER}",
                        table {
                            class: "min-w-full text-sm",
                            thead {
                                class: "{theme::surface::CARD_BG} text-left text-xs uppercase tracking-wide {theme::text::MUTED}",
                                tr {
                                    th { class: "px-4 py-3", "Identifier" }
                                    th { class: "px-4 py-3", "Role" }
                                    th { class: "px-4 py-3", "Status" }
                                    th { class: "px-4 py-3", "Environments" }
                                    th { class: "px-4 py-3", "Updated" }
                                }
                            }
                            tbody {
                                class: "divide-y {theme::surface::CARD_BORDER}",
                                for user in users.read().iter() {
                                    tr {
                                        class: "{theme::surface::CARD_BG}",
                                        td { class: "px-4 py-3 text-white", {user.identifier.clone()} }
                                        td { class: "px-4 py-3 {theme::text::SECONDARY}", {format_role(user.role)} }
                                        td { class: "px-4 py-3 {theme::text::SECONDARY}", {format_status(user.enabled)} }
                                        td { class: "px-4 py-3 text-slate-300", {format_user_environments(user)} }
                                        td { class: "px-4 py-3 {theme::text::MUTED}", {format_time(user.updated_at)} }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            section {
                class: "space-y-3",
                h2 { class: "text-lg font-semibold text-white", "Audit Log" }
                div {
                    class: "grid gap-3 sm:grid-cols-2 xl:grid-cols-5",
                    input {
                        class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} px-3 py-2 text-sm text-white",
                        r#type: "text",
                        placeholder: "Filter by actor",
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
                        option { value: "user_role_assigned", "Role assignment" }
                        option { value: "session_invalidated", "Session invalidated" }
                    }
                    input {
                        class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} px-3 py-2 text-sm text-white",
                        r#type: "text",
                        placeholder: "From (RFC3339)",
                        value: "{from_filter.read()}",
                        oninput: move |evt| {
                            from_filter.set(evt.value());
                            audit_page.set(1);
                        }
                    }
                    input {
                        class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} px-3 py-2 text-sm text-white",
                        r#type: "text",
                        placeholder: "To (RFC3339)",
                        value: "{to_filter.read()}",
                        oninput: move |evt| {
                            to_filter.set(evt.value());
                            audit_page.set(1);
                        }
                    }
                    button {
                        class: "rounded-lg border {theme::surface::CARD_BORDER} px-3 py-2 text-sm font-medium text-white {theme::interactive::GHOST_BTN}",
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
                                    class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} px-4 py-3",
                                    div {
                                        class: "flex items-start justify-between gap-4",
                                        div {
                                            p { class: "text-sm text-white", "{event.target}" }
                                            p { class: "text-xs {theme::text::SECONDARY}", "{format_event_actor(event)}" }
                                            p { class: "text-xs {theme::text::MUTED}", "Source: {event.source}" }
                                        }
                                        span { class: "text-xs {theme::text::MUTED}", "{format_time(event.timestamp)}" }
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
                                class: "rounded-md border {theme::surface::CARD_BORDER} px-3 py-1.5 text-xs font-medium text-white {theme::interactive::GHOST_BTN}",
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
                                class: "rounded-md border {theme::surface::CARD_BORDER} px-3 py-1.5 text-xs font-medium text-white {theme::interactive::GHOST_BTN}",
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
        }
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

fn format_status(enabled: bool) -> &'static str {
    if enabled { "Enabled" } else { "Disabled" }
}

fn format_action(event: &AuditEvent) -> &'static str {
    match event.action {
        crate::api::models::AuditAction::UserRoleAssigned => "Role assignment",
        crate::api::models::AuditAction::SessionInvalidated => "Session invalidated",
    }
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
