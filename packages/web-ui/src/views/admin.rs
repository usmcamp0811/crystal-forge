use chrono::Local;
use dioxus::prelude::*;
use std::collections::HashMap;

use crate::api::client::{
    create_admin_user, delete_admin_oidc_mapping, fetch_admin_audit_events,
    fetch_admin_oidc_mappings, fetch_admin_users, update_admin_user, upsert_admin_oidc_mapping,
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

    let mut actor_filter = use_signal(String::new);
    let mut action_filter = use_signal(String::new);
    let mut from_filter = use_signal(String::new);
    let mut to_filter = use_signal(String::new);

    let mut create_email = use_signal(String::new);
    let mut create_display_name = use_signal(String::new);
    let mut create_role = use_signal(|| "Viewer".to_string());
    let mut create_environments = use_signal(String::new);
    let mut create_submitting = use_signal(|| false);

    let mut mapping_group = use_signal(String::new);
    let mut mapping_role = use_signal(|| "Viewer".to_string());
    let mut mapping_environments = use_signal(String::new);
    let mut mapping_submitting = use_signal(|| false);

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
                p {
                    class: "text-xs {theme::text::SECONDARY}",
                    "Role and environment membership changes take effect after the user signs in again."
                }
                p {
                    class: "text-xs {theme::text::SECONDARY}",
                    "Users sourced from OIDC are IdP-derived and their role/memberships are managed through OIDC group mappings."
                }

                div {
                    class: "rounded-xl border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} p-4 space-y-3",
                    h3 { class: "text-sm font-semibold text-white", "Create user" }
                    div {
                        class: "grid gap-3 sm:grid-cols-2 xl:grid-cols-4",
                        input {
                            class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} px-3 py-2 text-sm text-white",
                            r#type: "email",
                            placeholder: "Email",
                            value: "{create_email.read()}",
                            oninput: move |evt| create_email.set(evt.value())
                        }
                        input {
                            class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} px-3 py-2 text-sm text-white",
                            r#type: "text",
                            placeholder: "Display name (optional)",
                            value: "{create_display_name.read()}",
                            oninput: move |evt| create_display_name.set(evt.value())
                        }
                        select {
                            class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} px-3 py-2 text-sm text-white",
                            value: "{create_role.read()}",
                            onchange: move |evt| create_role.set(evt.value()),
                            option { value: "Admin", "Admin" }
                            option { value: "Operator", "Operator" }
                            option { value: "Viewer", "Viewer" }
                        }
                        input {
                            class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} px-3 py-2 text-sm text-white",
                            r#type: "text",
                            placeholder: "Environments (comma-separated)",
                            value: "{create_environments.read()}",
                            oninput: move |evt| create_environments.set(evt.value())
                        }
                    }
                    div {
                        class: "flex justify-end",
                        button {
                            class: "rounded-lg px-3 py-2 text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                            disabled: *create_submitting.read(),
                            onclick: move |_| {
                                let email = create_email.read().clone();
                                let display_name = create_display_name.read().clone();
                                let role = role_from_string(&create_role.read());
                                let environments = parse_environments(&create_environments.read());

                                let request = AdminCreateUserRequest {
                                    email,
                                    display_name: optional_value(display_name),
                                    role,
                                    environments,
                                };

                                let mut users = users.clone();
                                let mut user_drafts = user_drafts.clone();
                                let mut users_error = users_error.clone();
                                let mut create_submitting = create_submitting.clone();
                                let mut create_email = create_email.clone();
                                let mut create_display_name = create_display_name.clone();
                                let mut create_environments = create_environments.clone();

                                create_submitting.set(true);
                                spawn(async move {
                                    match create_admin_user(&request).await {
                                        Ok(_) => {
                                            refresh_users(users, user_drafts, users_error).await;
                                            create_email.set(String::new());
                                            create_display_name.set(String::new());
                                            create_environments.set(String::new());
                                        }
                                        Err(e) => users_error.set(Some(format!("Failed to create user: {e}"))),
                                    }
                                    create_submitting.set(false);
                                });
                            },
                            if *create_submitting.read() { "Creating..." } else { "Create user" }
                        }
                    }
                }

                if users_render_state(*users_loading.read(), users_error.read().as_deref())
                    == UsersRenderState::Loading
                {
                    div { class: "text-sm {theme::text::SECONDARY}", "Loading users..." }
                } else if users_render_state(*users_loading.read(), users_error.read().as_deref())
                    == UsersRenderState::Error
                {
                    div {
                        class: "rounded-lg border border-red-500/40 bg-red-950/30 px-4 py-3 text-sm text-red-200",
                        "{users_error_message(users_error.read().clone())}"
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
                                    th { class: "px-4 py-3", "Source" }
                                    th { class: "px-4 py-3", "Role" }
                                    th { class: "px-4 py-3", "Status" }
                                    th { class: "px-4 py-3", "Environments" }
                                    th { class: "px-4 py-3", "Updated" }
                                    th { class: "px-4 py-3", "Actions" }
                                }
                            }
                            tbody {
                                class: "divide-y {theme::surface::CARD_BORDER}",
                                for user in users.read().iter() {
                                    {
                                        let user_id = user.id.clone();
                                        let draft = user_drafts
                                            .read()
                                            .get(&user_id)
                                            .cloned()
                                            .unwrap_or_else(|| UserEditDraft::from_user(user));

                                        rsx! {
                                            tr {
                                                class: "{theme::surface::CARD_BG}",
                                                td { class: "px-4 py-3 text-white", {user.identifier.clone()} }
                                                td {
                                                    class: "px-4 py-3",
                                                    span {
                                                        class: "inline-flex rounded-full px-2 py-1 text-xs font-medium {identity_source_badge_class(user.identity_source)}",
                                                        "{identity_source_label(user.identity_source)}"
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3",
                                                    {
                                                        let is_oidc_derived = user.identity_source == IdentitySource::OidcDerived;
                                                        rsx! {
                                                    select {
                                                        class: "rounded-md border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} px-2 py-1 text-xs text-white",
                                                        value: "{draft.role}",
                                                        disabled: is_oidc_derived,
                                                        onchange: {
                                                            let mut user_drafts = user_drafts.clone();
                                                            let user_id = user_id.clone();
                                                            move |evt| {
                                                                let mut drafts = user_drafts.write();
                                                                if let Some(entry) = drafts.get_mut(&user_id) {
                                                                    entry.role = evt.value();
                                                                }
                                                            }
                                                        },
                                                        option { value: "Admin", "Admin" }
                                                        option { value: "Operator", "Operator" }
                                                        option { value: "Viewer", "Viewer" }
                                                    }
                                                        }
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3",
                                                    {
                                                        let is_oidc_derived = user.identity_source == IdentitySource::OidcDerived;
                                                        rsx! {
                                                    label { class: "inline-flex items-center gap-2 text-xs {theme::text::SECONDARY}",
                                                        input {
                                                            r#type: "checkbox",
                                                            checked: draft.enabled,
                                                            disabled: is_oidc_derived,
                                                            onchange: {
                                                                let mut user_drafts = user_drafts.clone();
                                                                let user_id = user_id.clone();
                                                                move |evt| {
                                                                    let mut drafts = user_drafts.write();
                                                                    if let Some(entry) = drafts.get_mut(&user_id) {
                                                                        entry.enabled = evt.checked();
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        if draft.enabled { "Enabled" } else { "Disabled" }
                                                    }
                                                        }
                                                    }
                                                }
                                                td {
                                                    class: "px-4 py-3",
                                                    input {
                                                        class: "w-full rounded-md border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} px-2 py-1 text-xs text-white",
                                                        r#type: "text",
                                                        value: "{draft.environments}",
                                                        disabled: user.identity_source == IdentitySource::OidcDerived,
                                                        oninput: {
                                                            let mut user_drafts = user_drafts.clone();
                                                            let user_id = user_id.clone();
                                                            move |evt| {
                                                                let mut drafts = user_drafts.write();
                                                                if let Some(entry) = drafts.get_mut(&user_id) {
                                                                    entry.environments = evt.value();
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                                td { class: "px-4 py-3 {theme::text::MUTED}", {format_time(user.updated_at)} }
                                                td {
                                                    class: "px-4 py-3",
                                                    button {
                                                        class: "rounded-md border {theme::surface::CARD_BORDER} px-2 py-1 text-xs font-medium text-white {theme::interactive::GHOST_BTN}",
                                                        disabled: user.identity_source == IdentitySource::OidcDerived,
                                                        onclick: {
                                                            let user_id = user_id.clone();
                                                            let mut users = users.clone();
                                                            let mut user_drafts = user_drafts.clone();
                                                            let mut users_error = users_error.clone();
                                                            move |_| {
                                                                let draft = user_drafts
                                                                    .read()
                                                                    .get(&user_id)
                                                                    .cloned();
                                                                let Some(draft) = draft else {
                                                                    return;
                                                                };

                                                                let request = AdminUpdateUserRequest {
                                                                    role: Some(role_from_string(&draft.role)),
                                                                    enabled: Some(draft.enabled),
                                                                    environments: Some(parse_environments(&draft.environments)),
                                                                };

                                                                let user_id = user_id.clone();
                                                                let mut users = users.clone();
                                                                let mut user_drafts = user_drafts.clone();
                                                                let mut users_error = users_error.clone();
                                                                spawn(async move {
                                                                    match update_admin_user(&user_id, &request).await {
                                                                        Ok(_) => refresh_users(users, user_drafts, users_error).await,
                                                                        Err(e) => users_error.set(Some(format!("Failed to update user: {e}"))),
                                                                    }
                                                                });
                                                            }
                                                        },
                                                        if user.identity_source == IdentitySource::OidcDerived {
                                                            "IdP managed"
                                                        } else {
                                                            "Save"
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

            section {
                class: "space-y-3",
                h2 { class: "text-lg font-semibold text-white", "OIDC Mappings" }
                p {
                    class: "text-xs {theme::text::SECONDARY}",
                    "IdP-derived role and environments are resolved from these group mappings at login."
                }

                div {
                    class: "rounded-xl border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} p-4 space-y-3",
                    div {
                        class: "grid gap-3 sm:grid-cols-3",
                        input {
                            class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} px-3 py-2 text-sm text-white",
                            r#type: "text",
                            placeholder: "OIDC group name",
                            value: "{mapping_group.read()}",
                            oninput: move |evt| mapping_group.set(evt.value())
                        }
                        select {
                            class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} px-3 py-2 text-sm text-white",
                            value: "{mapping_role.read()}",
                            onchange: move |evt| mapping_role.set(evt.value()),
                            option { value: "Admin", "Admin" }
                            option { value: "Operator", "Operator" }
                            option { value: "Viewer", "Viewer" }
                        }
                        input {
                            class: "rounded-lg border {theme::surface::CARD_BORDER} {theme::surface::CARD_BG} px-3 py-2 text-sm text-white",
                            r#type: "text",
                            placeholder: "Environments (comma-separated)",
                            value: "{mapping_environments.read()}",
                            oninput: move |evt| mapping_environments.set(evt.value())
                        }
                    }
                    div {
                        class: "flex justify-end",
                        button {
                            class: "rounded-lg px-3 py-2 text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                            disabled: *mapping_submitting.read(),
                            onclick: move |_| {
                                let request = AdminUpsertOidcMappingRequest {
                                    group_name: mapping_group.read().clone(),
                                    role: Some(role_from_string(&mapping_role.read())),
                                    environments: parse_environments(&mapping_environments.read()),
                                };

                                let mut oidc_mappings = oidc_mappings.clone();
                                let mut oidc_error = oidc_error.clone();
                                let mut mapping_group = mapping_group.clone();
                                let mut mapping_environments = mapping_environments.clone();
                                let mut mapping_submitting = mapping_submitting.clone();
                                mapping_submitting.set(true);

                                spawn(async move {
                                    match upsert_admin_oidc_mapping(&request).await {
                                        Ok(_) => match fetch_admin_oidc_mappings().await {
                                            Ok(next) => {
                                                oidc_mappings.set(next);
                                                oidc_error.set(None);
                                                mapping_group.set(String::new());
                                                mapping_environments.set(String::new());
                                            }
                                            Err(e) => {
                                                oidc_error.set(Some(format!("Failed to reload OIDC mappings: {e}")));
                                            }
                                        },
                                        Err(e) => oidc_error.set(Some(format!("Failed to save OIDC mapping: {e}"))),
                                    }
                                    mapping_submitting.set(false);
                                });
                            },
                            if *mapping_submitting.read() { "Saving..." } else { "Save mapping" }
                        }
                    }
                }

                if let Some(message) = oidc_error.read().clone() {
                    div {
                        class: "rounded-lg border border-red-500/40 bg-red-950/30 px-4 py-3 text-sm text-red-200",
                        "{message}"
                    }
                }

                div {
                    class: "overflow-auto rounded-xl border {theme::surface::CARD_BORDER}",
                    table {
                        class: "min-w-full text-sm",
                        thead {
                            class: "{theme::surface::CARD_BG} text-left text-xs uppercase tracking-wide {theme::text::MUTED}",
                            tr {
                                th { class: "px-4 py-3", "Group" }
                                th { class: "px-4 py-3", "Role" }
                                th { class: "px-4 py-3", "Environments" }
                                th { class: "px-4 py-3", "Updated" }
                                th { class: "px-4 py-3", "Actions" }
                            }
                        }
                        tbody {
                            class: "divide-y {theme::surface::CARD_BORDER}",
                            for mapping in oidc_mappings.read().iter() {
                                tr {
                                    class: "{theme::surface::CARD_BG}",
                                    td { class: "px-4 py-3 text-white", "{mapping.group_name}" }
                                    td { class: "px-4 py-3 {theme::text::SECONDARY}", "{editable_role_label(mapping.role)}" }
                                    td { class: "px-4 py-3 text-slate-300", "{format_environments(&mapping.environments)}" }
                                    td { class: "px-4 py-3 {theme::text::MUTED}", "{format_time(mapping.updated_at)}" }
                                    td {
                                        class: "px-4 py-3",
                                        button {
                                            class: "rounded-md border border-red-500/40 px-2 py-1 text-xs font-medium text-red-200 hover:bg-red-950/40",
                                            onclick: {
                                                let mapping_id = mapping.id.clone();
                                                let mut oidc_mappings = oidc_mappings.clone();
                                                let mut oidc_error = oidc_error.clone();
                                                move |_| {
                                                    let mapping_id = mapping_id.clone();
                                                    let mut oidc_mappings = oidc_mappings.clone();
                                                    let mut oidc_error = oidc_error.clone();
                                                    spawn(async move {
                                                        match delete_admin_oidc_mapping(&mapping_id).await {
                                                            Ok(()) => match fetch_admin_oidc_mappings().await {
                                                                Ok(next) => {
                                                                    oidc_mappings.set(next);
                                                                    oidc_error.set(None);
                                                                }
                                                                Err(e) => {
                                                                    oidc_error.set(Some(format!("Failed to reload OIDC mappings: {e}")));
                                                                }
                                                            },
                                                            Err(e) => oidc_error.set(Some(format!("Failed to delete OIDC mapping: {e}"))),
                                                        }
                                                    });
                                                }
                                            },
                                            "Delete"
                                        }
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
                        option { value: "user_created", "User created" }
                        option { value: "user_updated", "User updated" }
                        option { value: "user_enabled", "User enabled" }
                        option { value: "user_disabled", "User disabled" }
                        option { value: "user_role_assigned", "Role assignment" }
                        option { value: "user_environment_membership_updated", "Environment membership" }
                        option { value: "oidc_mapping_changed", "OIDC mapping" }
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

fn format_action(event: &AuditEvent) -> &'static str {
    match event.action {
        crate::api::models::AuditAction::UserCreated => "User created",
        crate::api::models::AuditAction::UserUpdated => "User updated",
        crate::api::models::AuditAction::UserEnabled => "User enabled",
        crate::api::models::AuditAction::UserDisabled => "User disabled",
        crate::api::models::AuditAction::UserRoleAssigned => "Role assignment",
        crate::api::models::AuditAction::UserEnvironmentMembershipUpdated => {
            "Environment membership"
        }
        crate::api::models::AuditAction::OidcMappingChanged => "OIDC mapping change",
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
    if users_loading {
        return UsersRenderState::Loading;
    }

    if users_error.is_some() {
        return UsersRenderState::Error;
    }

    UsersRenderState::Table
}

fn users_error_message(users_error: Option<String>) -> String {
    users_error.unwrap_or_else(|| "Failed to load users".to_string())
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
    fn users_render_state_prioritizes_loading_then_error_then_table() {
        assert_eq!(
            users_render_state(true, Some("boom")),
            UsersRenderState::Loading
        );
        assert_eq!(
            users_render_state(false, Some("boom")),
            UsersRenderState::Error
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
}
