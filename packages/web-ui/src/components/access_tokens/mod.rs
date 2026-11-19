//! Access tokens card of the Profile view (TASK-534.3).
//!
//! The card lets any signed-in user list, create, inspect, rotate, and revoke
//! their own `cfat_v1_` access tokens. It performs no role check: the server
//! decides what a user may grant.
//!
//! # Secret handling
//!
//! The plaintext secret of a new or rotated token lives only in the local
//! state of `CreateTokenDialog` or `RotateDialog`. It is
//! never placed in a context, URL, browser storage, log, or `Debug` output.
//! Closing the dialog, unmounting the card, or a change of the auth generation
//! drops it. The Profile view keys this card by auth generation, so sign-out
//! and user changes remount the card and discard all token state.

mod create;
mod logic;
mod manage;

use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use uuid::Uuid;

use crate::api::client::{
    AccessTokenApiError, AccessTokenListQuery, fetch_access_token_policy, fetch_access_tokens,
};
use crate::api::models::{AccessTokenDto, AccessTokenStatus};
use crate::components::icon::{Icon, IconName};
use crate::state::app_state::AppState;

use create::CreateTokenDialog;
use logic::{
    LIST_PAGE_SIZE, StatusFilter, audience_label, can_manage, create_disabled_reason,
    environment_summary, feature_disabled_message, format_date, last_used_text, merge_page,
    relative_expiry, scope_summary, status_presentation,
};
use manage::{RevokeDialog, RotateDialog, TokenDetailsDrawer};

/// Delay between the last keystroke in the name search and the request, in
/// milliseconds. Keeps typing from sending one request per character.
const SEARCH_DEBOUNCE_MS: u32 = 250;

/// Dialog opened over the card. At most one is open.
#[derive(Clone, PartialEq)]
enum Dialog {
    None,
    Create,
    Details(Uuid),
    Revoke(AccessTokenDto),
    Rotate(AccessTokenDto),
}

/// Identifies the session and request that a list response answers.
///
/// A response applies only when its stamp equals the current stamp. A newer
/// request, a sign-out, or a user change makes the older response stale.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RequestStamp {
    user_id: Option<String>,
    generation: u64,
    seq: u64,
}

/// Reads the current stamp without subscribing the caller to `app_state`.
fn current_stamp(app_state: Signal<AppState>, seq: u64) -> RequestStamp {
    let state = app_state.peek();
    RequestStamp {
        user_id: state
            .auth
            .as_ref()
            .and_then(|ctx| ctx.user.as_ref())
            .map(|user| user.id.clone()),
        generation: state.auth_generation,
        seq,
    }
}

/// Renders the Access tokens card with its list, filters, and dialogs.
#[component]
pub fn AccessTokensCard() -> Element {
    let app_state = use_context::<Signal<AppState>>();
    let mut policy = use_resource(|| async { fetch_access_token_policy().await });
    let mut items = use_signal(Vec::<AccessTokenDto>::new);
    let mut next_offset = use_signal(|| None::<i64>);
    let mut list_loading = use_signal(|| true);
    let mut loading_more = use_signal(|| false);
    let mut list_error = use_signal(|| None::<AccessTokenApiError>);
    let mut query = use_signal(String::new);
    let mut status_filter = use_signal(|| StatusFilter::All);
    let mut reload = use_signal(|| 0u32);
    let mut list_seq = use_signal(|| 0u64);
    let mut dialog = use_signal(|| Dialog::None);
    let mut notice = use_signal(|| None::<String>);

    // Loads page one when the filters change, after a mutation, and on retry.
    // Rows stay visible while the next page one loads. A response applies only
    // when no newer request, sign-out, or user change happened.
    use_effect(move || {
        let search = query().trim().to_string();
        let filter = status_filter();
        let _ = reload();
        let seq = list_seq.peek().wrapping_add(1);
        list_seq.set(seq);
        loading_more.set(false);
        list_loading.set(true);
        let stamp = current_stamp(app_state, seq);
        spawn(async move {
            if !search.is_empty() {
                TimeoutFuture::new(SEARCH_DEBOUNCE_MS).await;
                if current_stamp(app_state, list_seq()) != stamp {
                    return;
                }
            }
            let result = fetch_access_tokens(&AccessTokenListQuery {
                status: filter.param(),
                q: (!search.is_empty()).then_some(search),
                limit: LIST_PAGE_SIZE,
                offset: 0,
            })
            .await;
            if current_stamp(app_state, list_seq()) != stamp {
                return;
            }
            match result {
                Ok(page) => {
                    items.set(page.items);
                    next_offset.set(page.next_offset);
                    list_error.set(None);
                }
                Err(error) => {
                    items.set(Vec::new());
                    next_offset.set(None);
                    list_error.set(Some(error));
                }
            }
            list_loading.set(false);
        });
    });

    let mut load_more = move || {
        let Some(offset) = next_offset() else {
            return;
        };
        if loading_more() {
            return;
        }
        loading_more.set(true);
        let search = query().trim().to_string();
        let filter = status_filter();
        let stamp = current_stamp(app_state, list_seq());
        spawn(async move {
            let result = fetch_access_tokens(&AccessTokenListQuery {
                status: filter.param(),
                q: (!search.is_empty()).then_some(search),
                limit: LIST_PAGE_SIZE,
                offset,
            })
            .await;
            if current_stamp(app_state, list_seq()) != stamp {
                return;
            }
            match result {
                Ok(page) => {
                    items.with_mut(|current| merge_page(current, page.items));
                    next_offset.set(page.next_offset);
                    list_error.set(None);
                }
                Err(error) => list_error.set(Some(error)),
            }
            loading_more.set(false);
        });
    };

    // A mutation changed, or may have changed, the token list.
    let on_changed = move |text: String| {
        notice.set(Some(text));
        reload.set(reload() + 1);
    };

    let now = Utc::now();
    let policy_state = (*policy.read()).clone();
    let disabled_message = feature_disabled_message(policy_state.as_ref(), list_error().as_ref());
    let (can_create, create_reason) = match &policy_state {
        None => (false, None),
        Some(Err(_)) => (false, Some("Token limits could not be loaded.".to_string())),
        Some(Ok(loaded)) => {
            let reason = create_disabled_reason(loaded);
            (reason.is_none(), reason)
        }
    };
    let policy_failed = matches!(&policy_state, Some(Err(_))) && disabled_message.is_none();
    let filters_active = !query().trim().is_empty() || status_filter() != StatusFilter::All;
    let mcp = policy_state
        .as_ref()
        .and_then(|loaded| loaded.as_ref().ok())
        .map(|loaded| loaded.mcp.clone());

    rsx! {
        section {
            class: "card cfat-card",
            "data-testid": "access-tokens-card",
            aria_labelledby: "cfat-title",
            style: "padding: 18px; grid-column: 1 / -1;",

            div { class: "cfat-head",
                div { class: "cfat-head-copy",
                    h3 { id: "cfat-title", class: "cfat-title",
                        Icon { name: IconName::Key, size: 13 }
                        "Access tokens"
                        span { class: "chip chip-info", style: "font-size: 9px;", "MCP" }
                    }
                    p { class: "cfat-lead",
                        "Connect a coding agent to your CVE and POA&M work. No administrator approval is needed for permissions you already have. A token never exceeds your role or assigned environments."
                    }
                    p { class: "help cfat-note",
                        Icon { name: IconName::Info, size: 11 }
                        span { "Tokens survive browser sign-out, but not revocation, account disable, owner permission removal, or expiry." }
                    }
                }
                button {
                    class: "btn btn-primary focus-ring",
                    disabled: !can_create,
                    "aria-describedby": create_reason.as_ref().map(|_| "cfat-create-reason"),
                    onclick: move |_| {
                        notice.set(None);
                        dialog.set(Dialog::Create);
                    },
                    Icon { name: IconName::Plus, size: 13 }
                    "Create access token"
                }
            }
            if let Some(reason) = create_reason.clone() {
                p { id: "cfat-create-reason", class: "help", role: "status", "{reason}" }
            }
            if policy_failed {
                button {
                    class: "btn btn-ghost focus-ring xs",
                    onclick: move |_| policy.restart(),
                    "Retry loading limits"
                }
            }

            div { class: "cfat-live", role: "status", aria_live: "polite",
                if let Some(text) = notice() {
                    "{text}"
                }
            }

            if let Some(message) = disabled_message.clone() {
                div { class: "cfat-empty", role: "status",
                    Icon { name: IconName::Key, size: 20 }
                    div { class: "cfat-empty-title", "Access tokens are disabled" }
                    div { class: "help", "{message}" }
                }
            } else {
                div { class: "cfat-filters",
                    label { class: "cfat-search",
                        span { class: "sr-only", "Search tokens by name" }
                        input {
                            class: "input focus-ring",
                            r#type: "search",
                            placeholder: "Search by name",
                            autocomplete: "off",
                            value: query(),
                            oninput: move |event| query.set(event.value()),
                        }
                    }
                    label { class: "cfat-filter",
                        "Status"
                        select {
                            class: "input focus-ring",
                            value: status_filter().value(),
                            onchange: move |event| status_filter.set(StatusFilter::from_value(&event.value())),
                            for option_filter in StatusFilter::ALL {
                                option { value: option_filter.value(), "{option_filter.label()}" }
                            }
                        }
                    }
                    if list_loading() && !items().is_empty() {
                        span { class: "help", "Refreshing…" }
                    }
                }

                if let Some(error) = list_error() {
                    div { class: "sd-callout sd-callout-danger cfat-banner", role: "alert",
                        span { "Could not load access tokens: {error}" }
                        button {
                            class: "btn btn-ghost focus-ring xs",
                            onclick: move |_| reload.set(reload() + 1),
                            "Retry"
                        }
                    }
                }

                if list_loading() && items().is_empty() {
                    p { class: "help", role: "status", "Loading access tokens…" }
                } else if items().is_empty() && list_error().is_none() {
                    if filters_active {
                        div { class: "cfat-empty",
                            div { class: "help", "No tokens match." }
                            button {
                                class: "btn btn-ghost focus-ring xs",
                                onclick: move |_| {
                                    query.set(String::new());
                                    status_filter.set(StatusFilter::All);
                                },
                                "Clear filters"
                            }
                        }
                    } else {
                        div { class: "cfat-empty",
                            Icon { name: IconName::Key, size: 20 }
                            div { class: "cfat-empty-title", "No access tokens yet" }
                            div { class: "help",
                                "Create one to let an MCP coding agent read CVEs and POA&Ms in your environments."
                            }
                        }
                    }
                } else if !items().is_empty() {
                    div { class: "cfat-table-wrap",
                        table { class: "sys-table", aria_label: "Access tokens",
                            thead {
                                tr {
                                    th { "Name" }
                                    th { "Audience" }
                                    th { "Environments" }
                                    th { "Scopes" }
                                    th { "Created" }
                                    th { "Last used" }
                                    th { "Expires" }
                                    th { "Status" }
                                    th { span { class: "sr-only", "Actions" } }
                                }
                            }
                            tbody {
                                for token in items() {
                                    TokenRow {
                                        key: "{token.id}",
                                        token: token.clone(),
                                        now,
                                        on_open: move |request: Dialog| {
                                            notice.set(None);
                                            dialog.set(request);
                                        },
                                    }
                                }
                            }
                        }
                    }
                    div { class: "cfat-cards",
                        for token in items() {
                            TokenCard {
                                key: "{token.id}",
                                token: token.clone(),
                                now,
                                on_open: move |request: Dialog| {
                                    notice.set(None);
                                    dialog.set(request);
                                },
                            }
                        }
                    }
                    if next_offset().is_some() {
                        button {
                            class: "btn btn-ghost focus-ring cfat-more",
                            disabled: loading_more(),
                            onclick: move |_| load_more(),
                            if loading_more() {
                                "Loading…"
                            } else {
                                "Load more"
                            }
                        }
                    }
                }
            }

            match dialog() {
                Dialog::None => rsx! {},
                Dialog::Create => match &policy_state {
                    Some(Ok(loaded)) => rsx! {
                        CreateTokenDialog {
                            policy: loaded.clone(),
                            on_close: move |_| dialog.set(Dialog::None),
                            on_changed,
                        }
                    },
                    _ => rsx! {},
                },
                Dialog::Details(id) => rsx! {
                    TokenDetailsDrawer {
                        token_id: id,
                        on_close: move |_| dialog.set(Dialog::None),
                        on_rotate: move |token: AccessTokenDto| dialog.set(Dialog::Rotate(token)),
                        on_revoke: move |token: AccessTokenDto| dialog.set(Dialog::Revoke(token)),
                    }
                },
                Dialog::Revoke(token) => rsx! {
                    RevokeDialog {
                        token,
                        on_close: move |_| dialog.set(Dialog::None),
                        on_changed,
                    }
                },
                Dialog::Rotate(token) => rsx! {
                    RotateDialog {
                        token,
                        mcp: mcp.clone(),
                        on_close: move |_| dialog.set(Dialog::None),
                        on_changed,
                    }
                },
            }
        }
    }
}

/// Renders a status as text plus a glyph. Color only reinforces the text.
#[component]
fn TokenStatusChip(status: AccessTokenStatus) -> Element {
    let presentation = status_presentation(status);
    rsx! {
        span { class: "chip {presentation.chip_class} cfat-status",
            Icon { name: presentation.icon, size: 10 }
            "{presentation.label}"
        }
    }
}

/// Renders the Details, Rotate, and Revoke buttons of one token.
///
/// Rotate and Revoke exist only for a token that can authenticate. Every
/// accessible name includes the token name, so rows stay distinguishable.
#[component]
fn TokenActions(token: AccessTokenDto, on_open: EventHandler<Dialog>) -> Element {
    let manage = can_manage(token.status);
    let id = token.id;
    let for_rotate = token.clone();
    let for_revoke = token.clone();
    rsx! {
        div { class: "cfat-row-actions",
            button {
                class: "btn btn-ghost focus-ring xs",
                aria_label: "Details for token {token.name}",
                onclick: move |_| on_open.call(Dialog::Details(id)),
                "Details"
            }
            if manage {
                button {
                    class: "btn btn-ghost focus-ring xs",
                    aria_label: "Rotate token {token.name}",
                    onclick: move |_| on_open.call(Dialog::Rotate(for_rotate.clone())),
                    "Rotate"
                }
                button {
                    class: "btn btn-ghost focus-ring xs cfat-danger",
                    aria_label: "Revoke token {token.name}",
                    onclick: move |_| on_open.call(Dialog::Revoke(for_revoke.clone())),
                    "Revoke"
                }
            }
        }
    }
}

/// Renders one token as a table row.
#[component]
fn TokenRow(token: AccessTokenDto, now: DateTime<Utc>, on_open: EventHandler<Dialog>) -> Element {
    let environments = environment_summary(&token.environments);
    let scopes = scope_summary(&token.scopes);
    let scope_list = token.scopes.join(", ");
    let created = format_date(token.created_at);
    let last_used = last_used_text(token.last_used_at, now);
    let expires = format_date(token.expires_at);
    let expires_relative = relative_expiry(token.expires_at, now);
    let inactive = !can_manage(token.status);
    rsx! {
        tr {
            class: if inactive { "cfat-row cfat-row-inactive" } else { "cfat-row" },
            "data-testid": "access-token-row",
            td {
                div { class: "cfat-name", "{token.name}" }
                div { class: "mono cfat-hint", "{token.display_hint}" }
            }
            td {
                span { class: "chip chip-unknown", style: "font-size: 10px;", "{audience_label(&token.audience)}" }
            }
            td { "{environments}" }
            td { title: "{scope_list}", "{scopes}" }
            td { "{created}" }
            td { "{last_used}" }
            td {
                div { "{expires}" }
                div { class: "cfat-hint", "{expires_relative}" }
            }
            td { TokenStatusChip { status: token.status } }
            td { TokenActions { token: token.clone(), on_open } }
        }
    }
}

/// Renders one token as a labeled card for narrow screens.
#[component]
fn TokenCard(token: AccessTokenDto, now: DateTime<Utc>, on_open: EventHandler<Dialog>) -> Element {
    let environments = environment_summary(&token.environments);
    let scopes = scope_summary(&token.scopes);
    let last_used = last_used_text(token.last_used_at, now);
    let expires = format_date(token.expires_at);
    let expires_relative = relative_expiry(token.expires_at, now);
    rsx! {
        div { class: "cfat-card-item", "data-testid": "access-token-card-item",
            div { class: "cfat-card-top",
                strong { "{token.name}" }
                TokenStatusChip { status: token.status }
            }
            div { class: "mono cfat-hint", "{token.display_hint}" }
            div { "Environments: {environments}" }
            div { class: "cfat-hint",
                "{audience_label(&token.audience)} · {scopes} · Last used: {last_used} · Expires {expires} ({expires_relative})"
            }
            TokenActions { token: token.clone(), on_open }
        }
    }
}
