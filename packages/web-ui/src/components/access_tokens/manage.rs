//! Details drawer and the revoke and rotate confirmation dialogs.

use chrono::Utc;
use dioxus::prelude::*;
use uuid::Uuid;

use crate::api::client::{fetch_access_token, revoke_access_token, rotate_access_token};
use crate::api::models::{
    AccessTokenDto, AccessTokenMcpDto, AccessTokenStatus, CreatedAccessTokenDto,
};
use crate::components::dialog_focus::{
    DialogFocusBoundary, DialogFocusRestore, DialogFocusSentinel, DialogInitialFocus,
};
use crate::components::icon::{Icon, IconName};

use super::create::RevealView;
use super::logic::{
    REVOKE_UNKNOWN_OUTCOME, ROTATE_UNKNOWN_OUTCOME, event_label, failure_message, format_date,
    format_short_date, is_live, last_used_short, last_used_title, no_effective_access_text,
    relative_expiry, unavailable_environments_text, unpermitted_scopes_text,
};
use super::{EnvChip, EnvChips, Spinner, TokenStatusChip};

/// DOM id of the details drawer.
const DRAWER_ID: &str = "cfat-details-dialog";
/// DOM id of the revoke dialog.
const REVOKE_ID: &str = "cfat-revoke-dialog";
/// DOM id of the rotate dialog.
const ROTATE_ID: &str = "cfat-rotate-dialog";

/// Shows one token with its grant, its current effective access, and recent
/// events.
///
/// Granted access and effective access are separate sections. An environment
/// that the owner lost carries explicit text. Rotate and Revoke appear only
/// for a live token. `owner` is the name of the signed-in user, who owns every
/// token the card lists.
#[component]
pub(super) fn TokenDetailsDrawer(
    token_id: Uuid,
    owner: String,
    on_close: EventHandler<()>,
    on_rotate: EventHandler<AccessTokenDto>,
    on_revoke: EventHandler<AccessTokenDto>,
) -> Element {
    let mut detail = use_resource(move || async move { fetch_access_token(token_id).await });
    let now = Utc::now();
    let (title, subtitle) = match &*detail.read() {
        Some(Ok(loaded)) => (
            loaded.token.name.clone(),
            Some(format!("{} · MCP", loaded.token.display_hint)),
        ),
        _ => ("Token details".to_string(), None),
    };

    let content = match &*detail.read() {
        None => rsx! {
            div { class: "modal-body",
                p { class: "help", role: "status", "Loading token details…" }
            }
        },
        Some(Err(error)) => rsx! {
            div { class: "modal-body",
                div { class: "sd-callout sd-callout-danger", role: "alert",
                    span { "Could not load token details: {error}" }
                    button {
                        class: "btn btn-ghost focus-ring xs",
                        onclick: move |_| detail.restart(),
                        "Retry"
                    }
                }
            }
        },
        Some(Ok(loaded)) => {
            let token = loaded.token.clone();
            let for_rotate = token.clone();
            let for_revoke = token.clone();
            let live = is_live(token.status);
            let created = format_short_date(token.created_at);
            let created_title = format_date(token.created_at);
            let last_used = last_used_short(token.last_used_at);
            let last_used_hint = last_used_title(token.last_used_at, now);
            let expires = format_short_date(token.expires_at);
            let expires_title = format!(
                "{} ({})",
                format_date(token.expires_at),
                relative_expiry(token.expires_at, now)
            );
            // Effective environments carry no color. The granted list does,
            // so the dot of an effective chip is looked up by id there.
            let effective_environments: Vec<_> = if live {
                loaded
                    .effective
                    .environments
                    .iter()
                    .filter(|env| env.available)
                    .map(|env| {
                        let color = token
                            .environments
                            .iter()
                            .find(|granted| granted.id == env.id)
                            .and_then(|granted| granted.color.clone());
                        (env.id, env.name.clone(), color)
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let unavailable_names: Vec<&str> = loaded
                .effective
                .environments
                .iter()
                .filter(|env| !env.available)
                .map(|env| env.name.as_str())
                .collect();
            let unavailable_text = unavailable_environments_text(&unavailable_names);
            let unpermitted: Vec<&str> = loaded
                .effective
                .scopes
                .iter()
                .filter(|scope| !scope.permitted)
                .map(|scope| scope.scope.as_str())
                .collect();
            let unpermitted_text = unpermitted_scopes_text(&unpermitted);
            let no_access_text = no_effective_access_text(token.status);
            let events = loaded.recent_events.clone();
            rsx! {
                div { class: "modal-body",
                    div { TokenStatusChip { status: token.status } }
                    if token.status == AccessTokenStatus::OwnerSuspended {
                        div { class: "sd-callout sd-callout-warn",
                            Icon { name: IconName::Warn, size: 13 }
                            div { "None of this token's environments are currently assigned to you, so it grants no access. Create a new token for your current environments." }
                        }
                    }
                    dl { class: "kv-grid",
                        dt { "Owner" }
                        dd { "{owner} (you)" }
                        dt { "Audience" }
                        dd { class: "mono", "{token.audience}" }
                        dt { "Created" }
                        dd { title: "{created_title}", "{created}" }
                        dt { "Last used" }
                        dd { title: "{last_used_hint}", "{last_used}" }
                        dt { "Expires" }
                        dd { title: "{expires_title}", "{expires}" }
                    }
                    section { aria_labelledby: "cfat-granted-title",
                        h3 { id: "cfat-granted-title", class: "cfat-subtitle", "Granted at creation" }
                        div { class: "cfat-stack",
                            div { EnvChips { environments: token.environments.clone() } }
                            div { class: "cfat-chips",
                                for scope in token.scopes.iter().cloned() {
                                    span { key: "{scope}", class: "chip chip-info mono cfat-chip", "{scope}" }
                                }
                            }
                        }
                    }
                    section { aria_labelledby: "cfat-effective-title",
                        h3 { id: "cfat-effective-title", class: "cfat-subtitle", "Effective now" }
                        if effective_environments.is_empty() {
                            div { class: "cfat-muted-line", "{no_access_text}" }
                        } else {
                            div { class: "cfat-stack",
                                span { class: "cfat-env-chips",
                                    for (id, name, color) in effective_environments {
                                        EnvChip { key: "{id}", name, color }
                                    }
                                }
                            }
                        }
                        if let Some(text) = unavailable_text {
                            div { class: "help cfat-help-tight", "{text}" }
                        }
                        if let Some(text) = unpermitted_text {
                            div { class: "help cfat-help-tight", "{text}" }
                        }
                        div { class: "help cfat-help-tight",
                            "Effective access is your current role and environments intersected with the grant. It can shrink, never grow."
                        }
                    }
                    section { aria_labelledby: "cfat-events-title",
                        h3 { id: "cfat-events-title", class: "cfat-subtitle", "Recent activity" }
                        if events.is_empty() {
                            div { class: "help", "No recorded events." }
                        } else {
                            ul { class: "cfat-events",
                                for (index, event) in events.into_iter().enumerate() {
                                    li { key: "{index}", title: "{format_date(event.created_at)}",
                                        "{event_label(&event.action)} · {format_short_date(event.created_at)}"
                                    }
                                }
                            }
                        }
                    }
                }
                if live {
                    div { class: "modal-foot",
                        button {
                            class: "btn btn-ghost focus-ring",
                            aria_label: "Rotate {token.name}",
                            onclick: move |_| on_rotate.call(for_rotate.clone()),
                            Icon { name: IconName::Sync, size: 12 }
                            "Rotate"
                        }
                        button {
                            class: "btn btn-ghost focus-ring cfat-danger cfat-danger-line",
                            aria_label: "Revoke {token.name}",
                            onclick: move |_| on_revoke.call(for_revoke.clone()),
                            Icon { name: IconName::Trash, size: 12 }
                            "Revoke"
                        }
                    }
                }
            }
        }
    };

    rsx! {
        div { class: "modal-backdrop cfat-drawer-backdrop", onclick: move |_| on_close.call(()),
            aside {
                id: DRAWER_ID,
                class: "cfat-drawer",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "cfat-details-title",
                tabindex: "-1",
                onclick: move |event| event.stop_propagation(),
                onkeydown: move |event| {
                    if event.key() == Key::Escape {
                        event.prevent_default();
                        on_close.call(());
                    }
                },
                DialogFocusRestore {}
                DialogInitialFocus { dialog_id: DRAWER_ID.to_string() }
                DialogFocusSentinel { dialog_id: DRAWER_ID.to_string(), boundary: DialogFocusBoundary::Last }
                div { class: "modal-head cfat-drawer-head",
                    div { class: "cfat-drawer-head-copy",
                        h2 { id: "cfat-details-title", "{title}" }
                        if let Some(text) = subtitle {
                            p { class: "mono", "{text}" }
                        }
                    }
                    button {
                        class: "btn btn-icon focus-ring",
                        r#type: "button",
                        aria_label: "Close details",
                        onclick: move |_| on_close.call(()),
                        Icon { name: IconName::X, size: 14 }
                    }
                }
                {content}
                DialogFocusSentinel { dialog_id: DRAWER_ID.to_string(), boundary: DialogFocusBoundary::First }
            }
        }
    }
}

/// Asks for confirmation, then revokes a token.
///
/// Revocation is irreversible. The confirm button is not the initial focus
/// target. The dialog cannot close while the request runs. Revocation is
/// idempotent on the server, so a repeated request after a lost response is
/// safe. `on_changed` receives a status message after success.
#[component]
pub(super) fn RevokeDialog(
    token: AccessTokenDto,
    on_close: EventHandler<()>,
    on_changed: EventHandler<String>,
) -> Element {
    let mut pending = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let id = token.id;
    let name = token.name.clone();

    let mut confirm = move || {
        if pending() {
            return;
        }
        pending.set(true);
        error.set(None);
        let name = name.clone();
        spawn(async move {
            match revoke_access_token(id).await {
                Ok(_) => {
                    on_changed.call(format!("Revoked \"{name}\"."));
                    on_close.call(());
                }
                Err(failure) => {
                    error.set(Some(failure_message(&failure, REVOKE_UNKNOWN_OUTCOME)));
                    pending.set(false);
                }
            }
        });
    };

    rsx! {
        div { class: "modal-backdrop cfat-backdrop",
            onclick: move |_| {
                if !pending() {
                    on_close.call(());
                }
            },
            div {
                id: REVOKE_ID,
                class: "modal cfat-modal cfat-modal-narrow",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "cfat-revoke-title",
                tabindex: "-1",
                onclick: move |event| event.stop_propagation(),
                onkeydown: move |event| {
                    if event.key() == Key::Escape && !pending() {
                        event.prevent_default();
                        on_close.call(());
                    }
                },
                DialogFocusRestore {}
                DialogInitialFocus { dialog_id: REVOKE_ID.to_string() }
                DialogFocusSentinel { dialog_id: REVOKE_ID.to_string(), boundary: DialogFocusBoundary::Last }
                div { class: "modal-head",
                    h2 { id: "cfat-revoke-title", "Revoke \"{token.name}\"?" }
                }
                div { class: "modal-body",
                    p { class: "cfat-confirm-text",
                        "This immediately blocks new requests from any agent using this token. A tool call already in progress may finish. Revocation cannot be undone."
                    }
                    if let Some(message) = error() {
                        div { class: "sd-callout sd-callout-danger", role: "alert", "{message}" }
                    }
                }
                div { class: "modal-foot",
                    div { class: "sr-only", role: "status", aria_live: "polite",
                        if pending() {
                            "Revoking token…"
                        }
                    }
                    button {
                        class: "btn btn-ghost focus-ring",
                        disabled: pending(),
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "btn btn-danger focus-ring",
                        disabled: pending(),
                        onclick: move |_| confirm(),
                        if pending() {
                            Spinner { size: 12 }
                            "Revoking…"
                        } else {
                            "Revoke token"
                        }
                    }
                }
                DialogFocusSentinel { dialog_id: REVOKE_ID.to_string(), boundary: DialogFocusBoundary::First }
            }
        }
    }
}

/// Asks for confirmation, then replaces a token with one of the same grant.
///
/// The server revokes the old token and issues the replacement in one
/// transaction. After success the dialog shows [`RevealView`] for the
/// replacement. The dialog cannot close while the request runs, and it never
/// retries: a repeated rotation after a lost response could revoke the
/// replacement the user never saw. `mcp` describes the endpoint line of the
/// reveal view. `on_changed` receives a status message whenever the token list
/// changed or may have changed.
#[component]
pub(super) fn RotateDialog(
    token: AccessTokenDto,
    mcp: Option<AccessTokenMcpDto>,
    on_close: EventHandler<()>,
    on_changed: EventHandler<String>,
) -> Element {
    let mut pending = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut issued = use_signal(|| None::<CreatedAccessTokenDto>);
    let id = token.id;
    let name = token.name.clone();

    let mut confirm = move || {
        if pending() {
            return;
        }
        pending.set(true);
        error.set(None);
        let name = name.clone();
        spawn(async move {
            match rotate_access_token(id).await {
                Ok(created) => {
                    issued.set(Some(created));
                    on_changed.call(format!("Rotated \"{name}\"."));
                }
                Err(failure) => {
                    if failure.outcome_unknown() {
                        on_changed.call(
                            "Check the access token list: the rotate request has an unknown outcome."
                                .to_string(),
                        );
                    }
                    error.set(Some(failure_message(&failure, ROTATE_UNKNOWN_OUTCOME)));
                }
            }
            pending.set(false);
        });
    };

    let revealed = issued.read().is_some();

    rsx! {
        div { class: "modal-backdrop cfat-backdrop",
            onclick: move |_| {
                if !pending() && issued.read().is_none() {
                    on_close.call(());
                }
            },
            div {
                id: ROTATE_ID,
                class: if revealed { "modal cfat-modal cfat-modal-reveal" } else { "modal cfat-modal cfat-modal-narrow" },
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: if revealed { "cfat-reveal-title" } else { "cfat-rotate-title" },
                tabindex: "-1",
                onclick: move |event| event.stop_propagation(),
                onkeydown: move |event| {
                    if event.key() == Key::Escape && !pending() && issued.read().is_none() {
                        event.prevent_default();
                        on_close.call(());
                    }
                },
                DialogFocusRestore {}
                DialogInitialFocus { dialog_id: ROTATE_ID.to_string() }
                DialogFocusSentinel { dialog_id: ROTATE_ID.to_string(), boundary: DialogFocusBoundary::Last }
                if let Some(created) = issued() {
                    RevealView {
                        issued: created,
                        mcp: mcp.clone(),
                        rotated_from: Some(token.display_hint.clone()),
                        dialog_id: ROTATE_ID.to_string(),
                        on_done: move |_| {
                            issued.set(None);
                            on_close.call(());
                        },
                    }
                } else {
                    div { class: "modal-head",
                        h2 { id: "cfat-rotate-title", "Rotate \"{token.name}\"?" }
                    }
                    div { class: "modal-body",
                        p { class: "cfat-confirm-text",
                            "A replacement token with the same grant is issued and the current token is revoked at the same moment. The agent stops connecting until you update it with the new secret."
                        }
                        div { class: "sd-callout sd-callout-info",
                            Icon { name: IconName::Info, size: 13 }
                            div { "Need zero downtime? Create a second token, update the agent, then revoke this one." }
                        }
                        if let Some(message) = error() {
                            div { class: "sd-callout sd-callout-danger", role: "alert", "{message}" }
                        }
                    }
                    div { class: "modal-foot",
                        div { class: "sr-only", role: "status", aria_live: "polite",
                            if pending() {
                                "Rotating token…"
                            }
                        }
                        button {
                            class: "btn btn-ghost focus-ring",
                            disabled: pending(),
                            onclick: move |_| on_close.call(()),
                            "Cancel"
                        }
                        button {
                            class: "btn btn-primary focus-ring",
                            disabled: pending(),
                            onclick: move |_| confirm(),
                            if pending() {
                                Spinner { size: 12 }
                                "Rotating…"
                            } else {
                                "Rotate token"
                            }
                        }
                    }
                }
                DialogFocusSentinel { dialog_id: ROTATE_ID.to_string(), boundary: DialogFocusBoundary::First }
            }
        }
    }
}
