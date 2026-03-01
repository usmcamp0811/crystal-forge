//! Edit environment modal component.

use dioxus::prelude::*;

use super::{normalize_color_hex, normalize_optional, EditEnvironmentDraft, EnvironmentItem};
use crate::theme;

/// Validate an environment edit draft.
pub fn validate_environment_edit(
    draft: &EditEnvironmentDraft,
    existing: &[EnvironmentItem],
) -> Result<(), String> {
    let name = draft.name.trim();
    if name.is_empty() {
        return Err("Environment name is required.".to_string());
    }
    if existing
        .iter()
        .any(|item| item.id != draft.id && item.name.eq_ignore_ascii_case(name))
    {
        return Err("Environment name already exists.".to_string());
    }
    if !super::looks_like_hex_color(&draft.color_hex) {
        return Err("Environment color must be a valid hex value.".to_string());
    }
    Ok(())
}

/// Props for the edit environment modal.
#[derive(Props, Clone, PartialEq)]
pub struct EditEnvironmentModalProps {
    pub draft: Signal<Option<EditEnvironmentDraft>>,
    pub error: Signal<Option<String>>,
    pub on_close: EventHandler<()>,
    pub on_save: EventHandler<EditEnvironmentDraft>,
}

/// Modal for editing environment metadata.
#[component]
pub fn EditEnvironmentModal(props: EditEnvironmentModalProps) -> Element {
    let mut draft = props.draft;
    let error = props.error;
    let on_close = props.on_close;
    let on_save = props.on_save;

    let Some(editing) = draft.read().clone() else {
        return rsx! {};
    };

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4 cf-modal-overlay",
            onclick: move |_| on_close.call(()),
            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6 space-y-4 cf-modal-panel-32",
                onclick: |evt| evt.stop_propagation(),

                h3 { class: "text-lg font-semibold text-white", "Edit Environment" }
                p { class: "text-sm {theme::text::SECONDARY}", "Update environment name and description." }

                label {
                    class: "space-y-2 block",
                    span { class: "text-xs uppercase tracking-wide text-gray-500", "Name" }
                    input {
                        class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                        value: "{editing.name}",
                        oninput: move |evt| {
                            let current = draft.read().clone();
                            if let Some(mut next) = current {
                                next.name = evt.value();
                                draft.set(Some(next));
                            }
                        }
                    }
                }

                label {
                    class: "space-y-2 block",
                    span { class: "text-xs uppercase tracking-wide text-gray-500", "Description" }
                    input {
                        class: "w-full rounded-lg px-3 py-2 text-sm {theme::interactive::INPUT} {theme::interactive::FOCUS_RING} {theme::text::SECONDARY}",
                        value: "{editing.description}",
                        oninput: move |evt| {
                            let current = draft.read().clone();
                            if let Some(mut next) = current {
                                next.description = evt.value();
                                draft.set(Some(next));
                            }
                        }
                    }
                }

                label {
                    class: "space-y-2 block",
                    span { class: "text-xs uppercase tracking-wide text-gray-500", "Color" }
                    input {
                        r#type: "color",
                        class: "w-full h-10 rounded-lg border {theme::surface::CARD_BORDER} bg-gray-900 cursor-pointer",
                        value: "{editing.color_hex}",
                        oninput: move |evt| {
                            let current = draft.read().clone();
                            if let Some(mut next) = current {
                                next.color_hex = evt.value();
                                draft.set(Some(next));
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
                        onclick: move |_| {
                            if let Some(next) = draft.read().clone() {
                                on_save.call(next);
                            }
                        },
                        "Save Changes"
                    }
                }
            }
        }
    }
}
