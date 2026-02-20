//! Add environment form component.

use dioxus::prelude::*;

use super::{
    EnvironmentItem, NewEnvironmentDraft, PolicyOption, normalize_color_hex, normalize_optional,
    required_policy_names, validate_environment as validate_env,
};
use crate::components::layout::Card;
use crate::theme;

/// Validate a new environment draft.
pub fn validate_environment(
    draft: &NewEnvironmentDraft,
    existing: &[EnvironmentItem],
    policy_library: &[PolicyOption],
) -> Result<(), String> {
    validate_env(draft, existing, policy_library)
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
                            class: "flex flex-wrap gap-2",
                            for name in required_policy_names(&draft.read().required_policy_ids, &props.policy_library) {
                                span {
                                    class: "inline-flex px-2 py-1 text-xs rounded border text-blue-100",
                                    style: "background-color: #253449; border-color: #3E5B82;",
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
