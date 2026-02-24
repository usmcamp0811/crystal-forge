//! Modal for updating a system's public key.

use dioxus::prelude::*;
use uuid::Uuid;

use crate::theme;

#[component]
pub fn UpdatePublicKeyModal(
    system_id: Uuid,
    hostname: String,
    on_cancel: EventHandler<()>,
    on_confirm: EventHandler<String>,
) -> Element {
    let mut new_public_key = use_signal(String::new);
    let mut error_message = use_signal(|| None::<String>);

    let handle_confirm = move |_| {
        let key = new_public_key.read().trim().to_string();

        if key.is_empty() {
            error_message.set(Some("Public key cannot be empty".to_string()));
            return;
        }

        // Basic validation - check if it looks like a base64 encoded key
        if key.len() < 32 {
            error_message.set(Some("Public key is too short".to_string()));
            return;
        }

        error_message.set(None);
        on_confirm.call(key);
    };

    rsx! {
        div {
            class: "fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50",
            onclick: move |_| on_cancel.call(()),

            div {
                class: "bg-gray-800 rounded-lg p-6 max-w-2xl w-full mx-4 border border-gray-700",
                onclick: move |evt| evt.stop_propagation(),

                // Header
                div {
                    class: "flex items-center justify-between mb-4",
                    h3 {
                        class: "text-lg font-semibold {theme::text::PRIMARY}",
                        "Update Public Key"
                    }
                    button {
                        class: "text-gray-400 hover:text-gray-200 transition-colors",
                        onclick: move |_| on_cancel.call(()),
                        "✕"
                    }
                }

                // System info
                div {
                    class: "mb-4 p-3 bg-gray-900 rounded border border-gray-700",
                    p {
                        class: "{theme::text::SECONDARY} text-sm mb-1",
                        "System:"
                    }
                    p {
                        class: "{theme::text::PRIMARY} font-mono",
                        "{hostname}"
                    }
                    p {
                        class: "{theme::text::MUTED} text-xs mt-1",
                        "ID: {system_id}"
                    }
                }

                // Warning
                div {
                    class: "mb-4 p-3 bg-yellow-500/10 border border-yellow-500/30 rounded",
                    p {
                        class: "text-yellow-300 text-sm",
                        "⚠️ Warning: Updating the public key will prevent the system from authenticating until the new key is deployed."
                    }
                }

                // Input field
                div {
                    class: "mb-4",
                    label {
                        class: "block {theme::text::SECONDARY} text-sm font-medium mb-2",
                        "New Public Key (Base64)"
                    }
                    textarea {
                        class: "w-full px-3 py-2 bg-gray-900 border border-gray-700 rounded {theme::text::PRIMARY} font-mono text-sm",
                        class: "focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                        rows: 4,
                        placeholder: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA...",
                        value: "{new_public_key}",
                        oninput: move |evt| {
                            new_public_key.set(evt.value());
                            error_message.set(None);
                        }
                    }
                    p {
                        class: "mt-1 text-xs {theme::text::MUTED}",
                        "Paste the base64-encoded Ed25519 public key"
                    }
                }

                // Error message
                if let Some(error) = error_message.read().as_ref() {
                    div {
                        class: "mb-4 p-3 bg-red-500/10 border border-red-500/30 rounded",
                        p {
                            class: "text-red-300 text-sm",
                            "{error}"
                        }
                    }
                }

                // Actions
                div {
                    class: "flex gap-3 justify-end",
                    button {
                        class: "px-4 py-2 bg-gray-700 hover:bg-gray-600 {theme::text::PRIMARY} rounded transition-colors",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white rounded transition-colors",
                        onclick: handle_confirm,
                        "Update Key"
                    }
                }
            }
        }
    }
}
