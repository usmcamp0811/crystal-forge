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
                        class: "rounded-lg border {theme::surface::CARD_BORDER} bg-gray-900/50 p-4 space-y-3",
                        div {
                            class: "flex items-center justify-between gap-2",
                            p { class: "text-xs uppercase tracking-wide text-gray-500", "Required Policies (all mandatory)" }
                            button {
                                class: "text-xs text-blue-300 hover:text-blue-200 px-2 py-1 rounded hover:bg-blue-500/10 transition-colors",
                                onclick: move |_| on_choose_policies.call(()),
                                "Choose Policies"
                            }
                        }
                        div {
                            class: "rounded-md border {theme::surface::CARD_BORDER} bg-gray-950/40 px-3 py-2 space-y-1",
                            p {
                                class: "text-xs font-medium {theme::text::SECONDARY}",
                                "New here? Think of policies as required safety rules for this environment."
                            }
                            p {
                                class: "text-xs {theme::text::MUTED}",
                                "If a system in this environment does not meet these rules, deployment can be blocked until it does."
                            }
                            p {
                                class: "text-xs {theme::text::MUTED}",
                                "You can change this anytime by adding or removing policies per environment."
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
