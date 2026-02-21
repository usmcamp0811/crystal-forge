//! Policy editor modal for creating and editing policy definitions.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::theme;

use super::types::{PolicyDefinition, PolicyFormat};

/// Modal for creating or editing a policy definition.
#[component]
pub fn PolicyEditorModal(
    editing_policy_id: Signal<Option<Uuid>>,
    edit_name: Signal<String>,
    edit_description: Signal<String>,
    edit_body: Signal<String>,
    edit_format: Signal<PolicyFormat>,
    policy_library: Signal<Vec<PolicyDefinition>>,
    on_close: EventHandler<()>,
) -> Element {
    let is_editing = editing_policy_id.read().is_some();
    let title = if is_editing {
        "Edit Policy"
    } else {
        "Create Policy"
    };
    let action_label = if is_editing {
        "Save Changes"
    } else {
        "Create Policy"
    };

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-6",
            style: "position: fixed; inset: 0; z-index: 50; width: 100vw; height: 100vh; backdrop-filter: blur(6px);",
            onclick: move |_| on_close.call(()),

            div {
                class: "{theme::surface::CARD_BG} border border-violet-500/30 rounded-2xl p-6 shadow-xl shadow-violet-900/20",
                style: "width: 85vw; max-width: 64rem; display: flex; flex-direction: column; gap: 1.5rem;",
                onclick: |evt| evt.stop_propagation(),

                // Header
                div {
                    class: "flex items-center justify-between",
                    div {
                        class: "flex items-center gap-3",
                        div {
                            class: "w-10 h-10 rounded-lg bg-violet-500/20 flex items-center justify-center",
                            svg {
                                class: "w-5 h-5 text-violet-400",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                                }
                            }
                        }
                        div {
                            h3 { class: "text-white text-lg font-semibold", "{title}" }
                            p { class: "text-xs {theme::text::MUTED}", "Define the policy metadata and TOML/JSON body." }
                        }
                    }
                    button {
                        class: "p-2 rounded-lg text-gray-400 hover:text-white hover:bg-violet-500/10 transition-colors",
                        onclick: move |_| on_close.call(()),
                        svg {
                            class: "w-5 h-5",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M6 18L18 6M6 6l12 12"
                            }
                        }
                    }
                }

                // Form content
                div {
                    class: "grid grid-cols-1 lg:grid-cols-[280px_1fr] gap-6 items-start",

                    // Left column - metadata
                    div {
                        class: "space-y-4",
                        div {
                            class: "space-y-2",
                            label { class: "text-xs text-violet-300/70 font-medium", "Policy Name" }
                            input {
                                class: "w-full rounded-lg border border-gray-700 bg-gray-950/50 px-3 py-2 text-sm text-gray-100 focus:outline-none focus:ring-2 focus:ring-violet-500/40 focus:border-violet-500/50",
                                placeholder: "e.g., Require SSH Enabled",
                                value: "{edit_name}",
                                oninput: move |event| edit_name.set(event.value()),
                            }
                        }
                        div {
                            class: "space-y-2",
                            label { class: "text-xs text-violet-300/70 font-medium", "Description" }
                            textarea {
                                class: "w-full rounded-lg border border-gray-700 bg-gray-950/50 px-3 py-2 text-sm text-gray-100 focus:outline-none focus:ring-2 focus:ring-violet-500/40 focus:border-violet-500/50 resize-none",
                                placeholder: "Describe what this policy enforces...",
                                rows: "4",
                                value: "{edit_description}",
                                oninput: move |event| edit_description.set(event.value()),
                            }
                        }
                        div {
                            class: "space-y-2",
                            label { class: "text-xs text-violet-300/70 font-medium", "Format" }
                            div {
                                class: "flex gap-2",
                                button {
                                    class: "px-3 py-1.5 rounded-md text-xs border transition-colors",
                                    class: if *edit_format.read() == PolicyFormat::Toml {
                                        "bg-violet-500/20 border-violet-500 text-violet-300"
                                    } else {
                                        "bg-gray-950/50 border-gray-700 text-gray-400 hover:border-gray-600"
                                    },
                                    onclick: move |_| edit_format.set(PolicyFormat::Toml),
                                    "TOML"
                                }
                                button {
                                    class: "px-3 py-1.5 rounded-md text-xs border transition-colors",
                                    class: if *edit_format.read() == PolicyFormat::Json {
                                        "bg-violet-500/20 border-violet-500 text-violet-300"
                                    } else {
                                        "bg-gray-950/50 border-gray-700 text-gray-400 hover:border-gray-600"
                                    },
                                    onclick: move |_| edit_format.set(PolicyFormat::Json),
                                    "JSON"
                                }
                            }
                        }
                    }

                    // Right column - code editor
                    div {
                        class: "space-y-3 flex flex-col",
                        label { class: "text-xs text-violet-300/70 font-medium", "Policy Definition" }
                        div {
                            class: "rounded-lg border border-gray-700 bg-gray-950/70 overflow-hidden",
                            textarea {
                                class: "w-full bg-transparent px-3 py-3 text-sm text-gray-100 font-mono focus:outline-none resize-none",
                                style: "min-height: 280px;",
                                rows: "12",
                                value: "{edit_body}",
                                oninput: move |event| edit_body.set(event.value()),
                                spellcheck: "false",
                            }
                        }
                    }
                }

                // Footer
                div {
                    class: "flex justify-end items-center gap-3 pt-4 border-t border-gray-800",
                    button {
                        class: "px-4 py-2 rounded-lg text-sm text-gray-300 border border-gray-700 hover:bg-gray-800 transition-colors",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-4 py-2 rounded-lg text-sm font-semibold bg-violet-600 hover:bg-violet-500 text-white transition-colors shadow-lg shadow-violet-900/30",
                        onclick: move |_| {
                            let name = edit_name.read().clone();
                            let description = edit_description.read().clone();
                            let body = edit_body.read().clone();
                            let format = *edit_format.read();
                            let new_id = editing_policy_id.read().unwrap_or_else(Uuid::new_v4);
                            let mut library = policy_library.read().clone();
                            let is_existing = library.iter().any(|policy| policy.id == new_id);

                            if is_existing {
                                library = library
                                    .into_iter()
                                    .map(|policy| {
                                        if policy.id == new_id {
                                            PolicyDefinition {
                                                id: new_id,
                                                name: name.clone(),
                                                description: description.clone(),
                                                format,
                                                body: body.clone(),
                                            }
                                        } else {
                                            policy
                                        }
                                    })
                                    .collect();
                            } else {
                                library.push(PolicyDefinition {
                                    id: new_id,
                                    name: name.clone(),
                                    description: description.clone(),
                                    format,
                                    body: body.clone(),
                                });
                            }
                            policy_library.set(library);
                            on_close.call(());
                        },
                        "{action_label}"
                    }
                }
            }
        }
    }
}
