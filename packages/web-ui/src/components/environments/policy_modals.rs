//! Policy-related modals for environments.

use dioxus::prelude::*;
use uuid::Uuid;

use super::PolicyOption;
use crate::theme;

/// Props for the edit requirements modal.
#[derive(Props, Clone, PartialEq)]
pub struct EditRequirementsModalProps {
    pub environment_name: String,
    pub policy_library: Vec<PolicyOption>,
    pub selected_policy_ids: Signal<Vec<Uuid>>,
    pub error: Signal<Option<String>>,
    pub on_close: EventHandler<()>,
    pub on_save: EventHandler<()>,
}

/// Modal for editing required policies for an environment.
#[component]
pub fn EditRequirementsModal(props: EditRequirementsModalProps) -> Element {
    let mut selected_policy_ids = props.selected_policy_ids;
    let error = props.error;
    let on_close = props.on_close;
    let on_save = props.on_save;

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4",
            style: "position: fixed; inset: 0; z-index: 60; width: 100vw; height: 100vh; backdrop-filter: blur(6px);",
            onclick: move |_| on_close.call(()),
            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6 space-y-4",
                style: "width: 100%; max-width: 44rem;",
                onclick: |evt| evt.stop_propagation(),

                div {
                    class: "flex items-start justify-between gap-4",
                    div {
                        h3 { class: "text-lg font-semibold text-white", "Environment: {props.environment_name}" }
                        p { class: "text-sm {theme::text::SECONDARY}", "Required policies are hard requirements for this environment." }
                    }
                }

                div {
                    class: "rounded-lg border {theme::surface::CARD_BORDER} bg-gray-900/50 p-3 space-y-3 max-h-[50vh] overflow-y-auto",
                    for policy in props.policy_library.clone() {
                        {
                            let checked = selected_policy_ids.read().contains(&policy.id);
                            rsx! {
                                label {
                                    key: "{policy.id}",
                                    class: "rounded-lg border border-gray-700 px-3 py-2 flex items-start gap-3 cursor-pointer hover:bg-gray-800/60",
                                    style: if checked { "background-color: #1F2E42; border-color: #3E5B82;" } else { "" },
                                    input {
                                        r#type: "checkbox",
                                        checked,
                                        onchange: move |_| {
                                            let mut values = selected_policy_ids.read().clone();
                                            if values.contains(&policy.id) {
                                                values.retain(|id| id != &policy.id);
                                            } else {
                                                values.push(policy.id);
                                            }
                                            selected_policy_ids.set(values);
                                        }
                                    }
                                    div {
                                        p { class: "text-sm text-white", "{policy.name}" }
                                        p { class: "text-xs {theme::text::SECONDARY}", "{policy.description}" }
                                    }
                                }
                            }
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
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                        onclick: move |_| on_save.call(()),
                        "Save Requirements"
                    }
                }
            }
        }
    }
}

/// Props for the policy picker modal.
#[derive(Props, Clone, PartialEq)]
pub struct PolicyPickerModalProps {
    pub title: String,
    pub current_ids: Vec<Uuid>,
    pub policy_library: Vec<PolicyOption>,
    pub on_apply: EventHandler<Vec<Uuid>>,
    pub on_close: EventHandler<()>,
}

/// Modal for picking policies (used during environment creation).
#[component]
pub fn PolicyPickerModal(props: PolicyPickerModalProps) -> Element {
    let mut selected = use_signal(|| props.current_ids.clone());

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4",
            style: "position: fixed; inset: 0; z-index: 60; width: 100vw; height: 100vh; backdrop-filter: blur(6px);",
            onclick: move |_| props.on_close.call(()),
            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6 space-y-4",
                style: "width: 100%; max-width: 44rem;",
                onclick: |evt| evt.stop_propagation(),

                h3 { class: "text-lg font-semibold text-white", "{props.title}" }
                p { class: "text-sm {theme::text::SECONDARY}", "Select policies that will be required for this environment." }

                div {
                    class: "rounded-lg border {theme::surface::CARD_BORDER} bg-gray-900/50 p-3 space-y-3 max-h-[50vh] overflow-y-auto",
                    for policy in props.policy_library.clone() {
                        {
                            let checked = selected.read().contains(&policy.id);
                            rsx! {
                                label {
                                    key: "{policy.id}",
                                    class: "rounded-lg border border-gray-700 px-3 py-2 flex items-start gap-3 cursor-pointer hover:bg-gray-800/60",
                                    style: if checked { "background-color: #1F2E42; border-color: #3E5B82;" } else { "" },
                                    input {
                                        r#type: "checkbox",
                                        checked,
                                        onchange: move |_| {
                                            let mut values = selected.read().clone();
                                            if values.contains(&policy.id) {
                                                values.retain(|id| id != &policy.id);
                                            } else {
                                                values.push(policy.id);
                                            }
                                            selected.set(values);
                                        }
                                    }
                                    div {
                                        p { class: "text-sm text-white", "{policy.name}" }
                                        p { class: "text-xs {theme::text::SECONDARY}", "{policy.description}" }
                                    }
                                }
                            }
                        }
                    }
                }

                div {
                    class: "flex flex-col-reverse sm:flex-row sm:justify-end gap-2",
                    button {
                        class: "px-3 py-2 rounded-lg text-sm bg-gray-700 hover:bg-gray-600 text-white",
                        onclick: move |_| props.on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                        onclick: move |_| props.on_apply.call(selected.read().clone()),
                        "Apply Policies"
                    }
                }
            }
        }
    }
}
