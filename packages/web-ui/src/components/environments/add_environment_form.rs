//! Add environment form component.

use dioxus::prelude::*;

use super::{
    normalize_color_hex, normalize_optional, required_policy_names, EnvironmentItem,
    NewEnvironmentDraft, PolicyOption,
};
use crate::components::layout::Card;
use crate::theme;

/// Validate a new environment draft.
pub fn validate_environment(
    draft: &NewEnvironmentDraft,
    existing: &[EnvironmentItem],
    policy_library: &[PolicyOption],
) -> Result<(), String> {
    let name = draft.name.trim();
    if name.is_empty() {
        return Err("Environment name is required.".to_string());
    }
    if existing
        .iter()
        .any(|item| item.name.eq_ignore_ascii_case(name))
    {
        return Err("Environment name already exists.".to_string());
    }
    if !super::looks_like_hex_color(&draft.color_hex) {
        return Err("Environment color must be a valid hex value.".to_string());
    }
    if draft.required_policy_ids.is_empty() {
        return Err("At least one required policy must be selected.".to_string());
    }
    if draft
        .required_policy_ids
        .iter()
        .any(|id| !policy_library.iter().any(|p| p.id == *id))
    {
        return Err("One or more selected policies are invalid.".to_string());
    }
    Ok(())
}

/// Props for the add environment form.
#[derive(Props, Clone, PartialEq)]
pub struct AddEnvironmentFormProps {
    pub draft: Signal<NewEnvironmentDraft>,
    pub error: Signal<Option<String>>,
    pub policy_library: Vec<PolicyOption>,
    pub default_required_policy: uuid::Uuid,
    pub on_cancel: EventHandler<()>,
    pub on_submit: EventHandler<NewEnvironmentDraft>,
    pub on_choose_policies: EventHandler<()>,
}

/// Form for adding a new environment.
#[component]
pub fn AddEnvironmentForm(props: AddEnvironmentFormProps) -> Element {
    let mut draft = props.draft;
    let error = props.error;
    let on_cancel = props.on_cancel;
    let on_submit = props.on_submit;
    let on_choose_policies = props.on_choose_policies;
    let mut show_policies_callout = use_signal(|| true);

    rsx! {
        Card {
            title: Some("Create Environment".to_string()),
            children: rsx! {
                div {
                    class: "space-y-4",
                    div {
                        class: "grid grid-cols-1 md:grid-cols-3 gap-4",
                        label {
                            class: "space-y-2",
                            span { class: "text-xs uppercase tracking-wide text-gray-500", "Name" }
                            input {
                                class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                value: "{draft.read().name}",
                                placeholder: "lan",
                                oninput: move |evt| {
                                    let mut next = draft.read().clone();
                                    next.name = evt.value();
                                    draft.set(next);
                                }
                            }
                        }
                        label {
                            class: "space-y-2",
                            span { class: "text-xs uppercase tracking-wide text-gray-500", "Description (optional)" }
                            input {
                                class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                                value: "{draft.read().description}",
                                placeholder: "Local area systems",
                                oninput: move |evt| {
                                    let mut next = draft.read().clone();
                                    next.description = evt.value();
                                    draft.set(next);
                                }
                            }
                        }
                        label {
                            class: "space-y-2",
                            span { class: "text-xs uppercase tracking-wide text-gray-500", "Color" }
                            input {
                                r#type: "color",
                                class: "w-full h-10 rounded-lg border {theme::surface::CARD_BORDER} bg-gray-900 cursor-pointer",
                                value: "{draft.read().color_hex}",
                                oninput: move |evt| {
                                    let mut next = draft.read().clone();
                                    next.color_hex = evt.value();
                                    draft.set(next);
                                }
                            }
                        }
                    }

                    div {
                        class: "relative rounded-lg border {theme::surface::CARD_BORDER} bg-gray-900/50 p-4 space-y-3 overflow-visible",
                        div {
                            class: "flex items-center justify-between gap-2",
                            p { class: "text-xs uppercase tracking-wide text-gray-500", "Required Policies (all mandatory)" }
                            button {
                                class: "text-xs text-blue-300 hover:text-blue-200 px-2 py-1 rounded hover:bg-blue-500/10 transition-colors",
                                onclick: move |_| {
                                    show_policies_callout.set(false);
                                    on_choose_policies.call(())
                                },
                                "Choose Policies"
                            }
                        }
                        if show_policies_callout() {
                            div {
                                "data-testid": "setup-coach-environment-policies-callout",
                                style: "position:absolute; right:12px; top:46px; width:min(420px, 92vw); z-index:70; background:rgba(30,64,175,0.94); border:1px solid rgba(96,165,250,0.75); border-radius:10px; padding:8px 10px; color:#dbeafe; font-size:12px; box-shadow:0 10px 24px rgba(15,23,42,0.45);",
                                div {
                                    style: "position:absolute; top:-6px; right:18px; width:10px; height:10px; background:rgba(30,64,175,0.94); border-left:1px solid rgba(96,165,250,0.75); border-top:1px solid rgba(96,165,250,0.75); transform:rotate(45deg);"
                                }
                                p { style: "margin:0; color:#eff6ff; font-weight:600;", "Next action" }
                                p { style: "margin:2px 0 0 0;", "Think of policies as safety rules for this environment." }
                                p { style: "margin:2px 0 0 0;", "If a system does not meet these rules, deployment can be blocked until it does." }
                                p { style: "margin:2px 0 0 0;", "You can add or remove policies later for each environment." }
                            }
                        }
                        div {
                            class: "flex flex-wrap gap-2",
                            for name in required_policy_names(&draft.read().required_policy_ids, &props.policy_library) {
                                span {
                                    class: "inline-flex px-2 py-1 text-xs rounded border text-blue-100 cf-chip-blue",
                                    "{name}"
                                }
                            }
                            if draft.read().required_policy_ids.is_empty() {
                                span { class: "text-xs text-gray-500", "No required policies selected" }
                            }
                        }
                    }

                    if let Some(message) = error.read().clone() {
                        p { class: "text-sm text-red-300", "{message}" }
                    }

                    div {
                        class: "flex flex-col-reverse sm:flex-row sm:justify-end gap-2",
                        button {
                            class: "px-3 py-2 rounded-lg text-sm bg-gray-700 hover:bg-gray-600 text-white",
                            onclick: move |_| on_cancel.call(()),
                            "Cancel"
                        }
                        button {
                            class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                            onclick: move |_| {
                                let next = draft.read().clone();
                                on_submit.call(next);
                            },
                            "Save Environment"
                        }
                    }
                }
            }
        }
    }
}
