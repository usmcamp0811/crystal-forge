//! Create dialog and the one-time secret reveal view.
//!
//! SECURITY: [`RevealView`] receives the plaintext secret as a prop from the
//! dialog that owns it ([`CreateTokenDialog`] here, `RotateDialog` in
//! `manage`). The secret is rendered only when the user chooses Show, and it
//! reaches the clipboard only when the user presses Copy.

use chrono::Utc;
use dioxus::prelude::*;

use crate::api::client::create_access_token;
use crate::api::models::{AccessTokenMcpDto, AccessTokenPolicyDto, CreatedAccessTokenDto, Role};
use crate::components::dialog_focus::{
    DialogFocusBoundary, DialogFocusRestore, DialogFocusSentinel, DialogInitialFocus,
};
use crate::components::icon::{Icon, IconName};

use super::Spinner;
use super::logic::{
    AUTH_HEADER_EXAMPLE, CreateErrors, build_create_request, create_failure, create_intro,
    env_dot_color, environments_help, expiry_date, format_short_date, initial_form, lifetime_label,
    masked_token, mcp_endpoint_text, production_write_confirmation_text, production_write_selected,
    review_note, review_summary, scope_unavailable_text, set_selected, validate_create,
};

/// DOM id of the create dialog. Focus handling addresses the dialog by id.
const DIALOG_ID: &str = "cfat-create-dialog";

/// Renders one numbered section of the create form as a fieldset.
///
/// The number reads as muted text before the title, as in the design. A
/// disabled section disables every control inside it.
#[component]
fn Section(
    n: &'static str,
    title: &'static str,
    #[props(default = false)] disabled: bool,
    children: Element,
) -> Element {
    rsx! {
        fieldset { class: "cfat-fieldset", disabled,
            legend {
                span { class: "cfat-legend-n", "{n}." }
                "{title}"
            }
            {children}
        }
    }
}

/// Renders the create form, then the one-time reveal view after success.
///
/// The form keeps its values when a request fails. While a request is in
/// flight the dialog cannot close: closing would drop the response and lose
/// the only copy of the secret. The dialog never retries a request. After a
/// success the dialog closes only through [`RevealView`]'s Done button.
///
/// `user_role` only selects wording, as in the design. The server decides
/// what the user may grant. `existing_names` holds the lowercase names of the
/// owner's non-revoked tokens that the card has loaded; a repeat gets a
/// message before the request, and the server still decides.
///
/// When the selection combines a write scope with a production environment,
/// the Review section shows the production write confirmation, which names the
/// selected write capabilities and production environments. The
/// form requires it before the request, and the server requires
/// `confirm_production_write` too.
///
/// `on_changed` receives a status message whenever the token list changed or
/// may have changed.
#[component]
pub(super) fn CreateTokenDialog(
    policy: AccessTokenPolicyDto,
    user_role: Role,
    existing_names: Vec<String>,
    on_close: EventHandler<()>,
    on_changed: EventHandler<String>,
) -> Element {
    let initial = initial_form(&policy);
    let mut form = use_signal(move || initial);
    let mut errors = use_signal(CreateErrors::default);
    let mut pending = use_signal(|| false);
    let mut issued = use_signal(|| None::<CreatedAccessTokenDto>);

    let submit_policy = policy.clone();
    let mut submit = move || {
        if pending() {
            return;
        }
        let found = validate_create(&form.read(), &submit_policy, &existing_names);
        if !found.is_empty() {
            errors.set(found);
            return;
        }
        errors.set(CreateErrors::default());
        let request = build_create_request(&form.read(), &submit_policy);
        pending.set(true);
        spawn(async move {
            match create_access_token(&request).await {
                Ok(created) => {
                    issued.set(Some(created));
                    on_changed.call(format!("Created \"{}\".", request.name));
                }
                Err(error) => {
                    if error.outcome_unknown() {
                        on_changed.call(
                            "Check the access token list: the create request has an unknown outcome."
                                .to_string(),
                        );
                    }
                    errors.set(create_failure(&error));
                }
            }
            pending.set(false);
        });
    };

    let revealed = issued.read().is_some();
    let values = form.read().clone();
    let current_errors = errors.read().clone();
    let summary = review_summary(
        values.environment_ids.len(),
        &values.scopes,
        values.expires_in_days,
    );
    let expiry = expiry_date(Utc::now(), values.expires_in_days);
    let production_write = production_write_selected(&values, &policy);
    let production_confirmation = production_write_confirmation_text(&values, &policy);

    rsx! {
        div { class: "modal-backdrop cfat-backdrop",
            div {
                id: DIALOG_ID,
                class: if revealed { "modal cfat-modal cfat-modal-reveal" } else { "modal cfat-modal" },
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: if revealed { "cfat-reveal-title" } else { "cfat-create-title" },
                tabindex: "-1",
                onclick: move |event| event.stop_propagation(),
                onkeydown: move |event| {
                    if event.key() == Key::Escape && !pending() && issued.read().is_none() {
                        event.prevent_default();
                        on_close.call(());
                    }
                },
                DialogFocusRestore {}
                DialogInitialFocus { dialog_id: DIALOG_ID.to_string() }
                DialogFocusSentinel { dialog_id: DIALOG_ID.to_string(), boundary: DialogFocusBoundary::Last }

                if let Some(created) = issued() {
                    RevealView {
                        issued: created,
                        mcp: Some(policy.mcp.clone()),
                        rotated_from: None::<String>,
                        dialog_id: DIALOG_ID.to_string(),
                        on_done: move |_| {
                            issued.set(None);
                            on_close.call(());
                        },
                    }
                } else {
                    div { class: "modal-head",
                        // The head is a flex row, so the title and its description share one block.
                        div {
                            h2 { id: "cfat-create-title",
                                span { class: "cfat-ico", Icon { name: IconName::Key, size: 14 } }
                                "Create access token"
                            }
                            p { "{create_intro(user_role)}" }
                        }
                    }
                    div { class: "modal-body", style: "gap: 20px;",
                        if let Some(message) = current_errors.general.clone() {
                            div { class: "sd-callout sd-callout-danger", role: "alert", "{message}" }
                        }

                        Section { n: "1", title: "Details", disabled: pending(),
                            div { class: "field",
                                label { r#for: "cfat-name", "Token name *" }
                                input {
                                    id: "cfat-name",
                                    class: "input focus-ring",
                                    r#type: "text",
                                    placeholder: "OpenCode - Sledge",
                                    autocomplete: "off",
                                    required: true,
                                    "maxlength": "120",
                                    "aria-invalid": current_errors.name.as_ref().map(|_| "true"),
                                    "aria-describedby": "cfat-name-error",
                                    value: values.name.clone(),
                                    oninput: move |event| form.write().name = event.value(),
                                }
                                div { id: "cfat-name-error", role: "alert", class: "cfat-error",
                                    if let Some(message) = current_errors.name.clone() {
                                        "{message}"
                                    }
                                }
                            }
                            div { class: "cfat-two-col",
                                div { class: "field",
                                    label { r#for: "cfat-audience", "Use with *" }
                                    select {
                                        id: "cfat-audience",
                                        class: "input focus-ring",
                                        style: "font-size: 12px;",
                                        disabled: true,
                                        option { value: "mcp", selected: true, "MCP server" }
                                    }
                                    div { class: "help", "Only MCP in v1. CLI tokens come later with their own scopes." }
                                }
                                div { class: "field",
                                    label { id: "cfat-expiry-label", "Expiration *" }
                                    div {
                                        class: "seg",
                                        role: "radiogroup",
                                        aria_labelledby: "cfat-expiry-label",
                                        style: "width: fit-content;",
                                        for days in policy.lifetime_options_days.iter().copied() {
                                            button {
                                                key: "{days}",
                                                r#type: "button",
                                                role: "radio",
                                                aria_checked: days == values.expires_in_days,
                                                class: if days == values.expires_in_days { "active" } else { "" },
                                                onclick: move |_| form.write().expires_in_days = days,
                                                "{lifetime_label(days)}"
                                            }
                                        }
                                    }
                                    div { id: "cfat-expiry-help", class: "help",
                                        "Expires {expiry}. Maximum {policy.max_lifetime_days} days; never extended by use."
                                    }
                                    div { id: "cfat-expiry-error", role: "alert", class: "cfat-error",
                                        if let Some(message) = current_errors.expires.clone() {
                                            "{message}"
                                        }
                                    }
                                }
                            }
                        }

                        Section { n: "2", title: "Environments", disabled: pending(),
                            p { class: "cfat-env-help", "{environments_help(user_role)}" }
                            if policy.environments.is_empty() {
                                div { class: "sd-callout sd-callout-info",
                                    Icon { name: IconName::Info, size: 13 }
                                    div { "You have no assigned environments, so a token would grant nothing. Ask an admin to assign one." }
                                }
                            } else {
                                div { class: "cfat-env-list",
                                    for env in policy.environments.iter().cloned() {
                                        label {
                                            key: "{env.id}",
                                            class: if values.environment_ids.contains(&env.id) { "cfat-env cfat-env-on" } else { "cfat-env" },
                                            input {
                                                r#type: "checkbox",
                                                checked: values.environment_ids.contains(&env.id),
                                                onchange: {
                                                    let id = env.id;
                                                    move |event: FormEvent| set_selected(&mut form.write().environment_ids, id, event.checked())
                                                },
                                            }
                                            span {
                                                class: "cfat-dot cfat-dot-lg",
                                                aria_hidden: "true",
                                                style: format!("background: {};", env_dot_color(env.color.as_deref())),
                                            }
                                            "{env.name}"
                                        }
                                    }
                                }
                            }
                            div { role: "alert", class: "cfat-error",
                                if let Some(message) = current_errors.environments.clone() {
                                    "{message}"
                                }
                            }
                            div { class: "cfat-fine",
                                "No \"All environments\" option. Environments you are later removed from stop working on this token automatically."
                            }
                        }

                        Section { n: "3", title: "Capabilities", disabled: pending(),
                            div { class: "cfat-check-grid",
                                for scope in policy.scopes.iter().cloned() {
                                    label {
                                        key: "{scope.scope}",
                                        class: if scope.available { "cfat-check" } else { "cfat-check cfat-check-off" },
                                        input {
                                            r#type: "checkbox",
                                            disabled: !scope.available,
                                            checked: scope.available && values.scopes.contains(&scope.scope),
                                            onchange: {
                                                let name = scope.scope.clone();
                                                move |event: FormEvent| set_selected(&mut form.write().scopes, name.clone(), event.checked())
                                            },
                                        }
                                        span { class: "cfat-check-text",
                                            span { class: "cfat-check-title", "{scope.label}" }
                                            span { class: "mono cfat-check-key", "{scope.scope}" }
                                            if let Some(reason) = scope_unavailable_text(&scope) {
                                                span { class: "cfat-check-why",
                                                    Icon { name: IconName::Info, size: 10 }
                                                    "{reason}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            div { role: "alert", class: "cfat-error",
                                if let Some(message) = current_errors.scopes.clone() {
                                    "{message}"
                                }
                            }
                        }

                        Section { n: "4", title: "Review", disabled: pending(),
                            div { class: "cfat-review",
                                div {
                                    strong { "Effective access: " }
                                    "{summary}"
                                }
                                div { class: "help", "{review_note(user_role)}" }
                            }
                            if production_write {
                                label { class: "sd-callout sd-callout-warn", style: "cursor: pointer;",
                                    input {
                                        r#type: "checkbox",
                                        style: "margin-top: 2px;",
                                        checked: values.confirm_production,
                                        "aria-invalid": current_errors.production.as_ref().map(|_| "true"),
                                        "aria-describedby": "cfat-prod-err",
                                        onchange: move |event: FormEvent| form.write().confirm_production = event.checked(),
                                    }
                                    div { "{production_confirmation}" }
                                }
                            }
                            div { id: "cfat-prod-err", role: "alert", class: "cfat-error",
                                if let Some(message) = current_errors.production.clone() {
                                    "{message}"
                                }
                            }
                        }
                    }
                    div { class: "modal-foot",
                        div { class: "sr-only", role: "status", aria_live: "polite",
                            if pending() {
                                "Creating token…"
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
                            disabled: pending() || policy.environments.is_empty(),
                            onclick: move |_| submit(),
                            if pending() {
                                Spinner { size: 12 }
                                "Creating…"
                            } else {
                                Icon { name: IconName::Key, size: 13 }
                                "Create token"
                            }
                        }
                    }
                }

                DialogFocusSentinel { dialog_id: DIALOG_ID.to_string(), boundary: DialogFocusBoundary::First }
            }
        }
    }
}

/// Shows the new secret once and requires an acknowledgement before Done.
///
/// The view has no way back to the form. The owner dialog must not close it
/// through Escape or a backdrop click, so a stray key press cannot lose the
/// secret. Copy runs only on user action.
///
/// `mcp` describes the endpoint line. `rotated_from` holds the display hint of
/// the token that a rotation revoked. A response that names a replaced token
/// without a hint still reads as a rotation.
#[component]
pub(super) fn RevealView(
    issued: CreatedAccessTokenDto,
    mcp: Option<AccessTokenMcpDto>,
    rotated_from: Option<String>,
    dialog_id: String,
    on_done: EventHandler<()>,
) -> Element {
    let mut shown = use_signal(|| false);
    let mut saved = use_signal(|| false);
    // Holds the confirmation or the failure message of the last Copy.
    let mut copy_result = use_signal(|| None::<Result<String, String>>);

    let endpoint = mcp_endpoint_text(mcp.as_ref());
    let endpoint_url = mcp.as_ref().and_then(|status| status.url.clone());
    let config_example = format!(
        "export CF_MCP_TOKEN=\"<paste token>\"   # from your secret store\nURL:    {endpoint}\nHeader: {AUTH_HEADER_EXAMPLE}"
    );
    let token_for_copy = issued.token.clone();
    let token_text = if shown() {
        issued.token.clone()
    } else {
        masked_token(issued.metadata.id)
    };
    let rotated = rotated_from.is_some() || issued.replaced_token_id.is_some();
    // The hint names the revoked token. Without it, the sentence still states
    // that the response replaced a token.
    let previous_unnamed = rotated_from.is_none() && issued.replaced_token_id.is_some();
    let previous_hint = rotated_from;
    let expires = format_short_date(issued.metadata.expires_at);

    rsx! {
        DialogInitialFocus { dialog_id }
        div { class: "modal-head",
            // The head is a flex row, so the title and its description share one block.
            div {
                h2 { id: "cfat-reveal-title",
                    span { class: "cfat-ico cfat-ico-ok", Icon { name: IconName::Check, size: 14 } }
                    if rotated {
                        "Access token rotated"
                    } else {
                        "Access token created"
                    }
                }
                p { "{issued.metadata.name} · expires {expires}" }
            }
        }
        div { class: "modal-body",
            div { class: "sd-callout sd-callout-warn", role: "alert",
                Icon { name: IconName::Warn, size: 13 }
                div {
                    strong { "This is the only time the secret will be displayed." }
                    " Store it in your agent's secret store now. Crystal Forge keeps only a hash and cannot show it again."
                    if let Some(hint) = previous_hint {
                        " The previous token "
                        span { class: "mono", "{hint}" }
                        " was revoked; agents using it stop working until updated."
                    }
                    if previous_unnamed {
                        " The previous token was revoked; agents using it stop working until updated."
                    }
                }
            }
            div { class: "field",
                label { r#for: "cfat-secret", "Token" }
                div { class: "cfat-secret-row",
                    input {
                        id: "cfat-secret",
                        class: "input focus-ring mono",
                        r#type: "text",
                        readonly: true,
                        autocomplete: "off",
                        spellcheck: "false",
                        value: token_text,
                    }
                    button {
                        class: "btn btn-ghost focus-ring xs",
                        r#type: "button",
                        aria_pressed: shown(),
                        aria_label: if shown() { "Hide token" } else { "Show token" },
                        onclick: move |_| shown.set(!shown()),
                        if shown() {
                            "Hide"
                        } else {
                            "Show"
                        }
                    }
                    button {
                        class: "btn btn-primary focus-ring xs",
                        r#type: "button",
                        aria_label: "Copy token",
                        onclick: move |_| {
                            let value = token_for_copy.clone();
                            spawn(async move {
                                let outcome = copy_text(&value)
                                    .await
                                    .map(|_| "Token copied to clipboard.".to_string());
                                copy_result.set(Some(outcome));
                            });
                        },
                        Icon { name: IconName::File, size: 11 }
                        "Copy"
                    }
                }
                if shown() {
                    div { class: "help", "Shown on screen. Mind anyone who can see your display." }
                }
            }
            div { class: "field",
                label { r#for: "cfat-endpoint", "MCP endpoint" }
                div { class: "cfat-secret-row",
                    input {
                        id: "cfat-endpoint",
                        class: "input focus-ring mono",
                        r#type: "text",
                        readonly: true,
                        value: endpoint.clone(),
                    }
                    if let Some(url) = endpoint_url.clone() {
                        button {
                            class: "btn btn-ghost focus-ring xs",
                            r#type: "button",
                            aria_label: "Copy endpoint",
                            onclick: move |_| {
                                let value = url.clone();
                                spawn(async move {
                                    let outcome = copy_text(&value)
                                        .await
                                        .map(|_| "Endpoint copied to clipboard.".to_string());
                                    copy_result.set(Some(outcome));
                                });
                            },
                            Icon { name: IconName::File, size: 11 }
                            "Copy"
                        }
                    }
                }
            }
            div { class: "field",
                label { "Client configuration (generic)" }
                pre { class: "mono cfat-pre", "{config_example}" }
                div { class: "help",
                    "Any MCP client that supports a static HTTP bearer header. Never put the token in a URL."
                }
            }
            div { class: "cfat-live", role: "status", aria_live: "polite",
                match copy_result() {
                    Some(Ok(text)) => rsx! { "{text}" },
                    Some(Err(text)) => rsx! { span { class: "cfat-error", "{text}" } },
                    None => rsx! {},
                }
            }
            label { class: "cfat-ack",
                input {
                    r#type: "checkbox",
                    checked: saved(),
                    onchange: move |event| saved.set(event.checked()),
                }
                "I have saved the token in my agent's secret store."
            }
        }
        div { class: "modal-foot",
            button {
                class: "btn btn-primary focus-ring",
                disabled: !saved(),
                style: if saved() { "opacity: 1;" } else { "opacity: 0.5;" },
                onclick: move |_| on_done.call(()),
                "Done"
            }
        }
    }
}

/// Copies `value` to the browser clipboard. A failure is returned and never
/// reported as a successful copy; the user can still copy the shown secret
/// by hand.
async fn copy_text(value: &str) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen_futures::JsFuture;
        let window = web_sys::window().ok_or_else(|| {
            "Clipboard access is unavailable in this browser context.".to_string()
        })?;
        JsFuture::from(window.navigator().clipboard().write_text(value))
            .await
            .map_err(|_| "Copy failed. Show the value and copy it manually.".to_string())?;
        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = value;
        Err("Clipboard access is available only in a browser.".to_string())
    }
}
