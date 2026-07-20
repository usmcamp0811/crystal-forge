//! Key pair generation modal for system registration.

use dioxus::prelude::*;

use crate::theme;

/// Generated key pair for a new system.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedKeyPair {
    /// Base64-encoded public key
    pub public_key: String,
    /// Base64-encoded private key
    pub private_key: String,
}

/// Generate a new Ed25519 key pair for system authentication.
pub fn generate_key_pair() -> GeneratedKeyPair {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use ed25519_dalek::SigningKey;
    use wasm_bindgen::JsCast;

    let mut secret = [0_u8; 32];
    let mut filled = false;
    if let Some(win) = web_sys::window() {
        if let Ok(crypto) = win.crypto() {
            if crypto.get_random_values_with_u8_array(&mut secret).is_ok() {
                filled = true;
            }
        }
    }
    if !filled {
        for byte in &mut secret {
            *byte = (js_sys::Math::random() * 256.0) as u8;
        }
    }

    let signing_key = SigningKey::from_bytes(&secret);
    let verifying_key = signing_key.verifying_key();

    GeneratedKeyPair {
        public_key: STANDARD.encode(verifying_key.to_bytes()),
        private_key: STANDARD.encode(signing_key.to_bytes()),
    }
}

/// Modal displaying a generated key pair for a new system.
///
/// Shows both public and private keys with instructions for secure handling.
/// The private key should be stored securely and installed on the target system.
#[component]
pub fn KeyPairModal(
    /// The generated key pair to display (if None, generates a new one)
    keys: Option<GeneratedKeyPair>,
    /// Called when the modal is closed
    on_close: EventHandler<()>,
    /// Called when the user wants to use the public key in a form
    on_use_public_key: EventHandler<()>,
) -> Element {
    let key_pair = keys.unwrap_or_else(generate_key_pair);

    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4 cf-modal-overlay",
            onclick: move |_| on_close.call(()),

            div {
                class: "relative bg-gray-900 rounded-xl border border-gray-700 shadow-2xl p-6 space-y-4 cf-modal-panel-44",
                onclick: |evt| evt.stop_propagation(),

                h3 { class: "text-lg font-semibold text-white", "Generated System Key Pair" }
                p {
                    class: "text-sm {theme::text::SECONDARY}",
                    "Store the private key securely. It is shown only now. Install the private key on the system and register the public key in Crystal Forge."
                }

                div {
                    class: "space-y-2",
                    p { class: "text-xs uppercase tracking-wide text-gray-500", "Public Key" }
                    pre {
                        class: "rounded-lg border border-gray-700 bg-gray-950 p-3 text-xs text-gray-200 font-mono overflow-x-auto",
                        "{key_pair.public_key}"
                    }
                }

                div {
                    class: "space-y-2",
                    p { class: "text-xs uppercase tracking-wide text-gray-500", "Private Key" }
                    pre {
                        class: "rounded-lg border border-gray-700 bg-gray-950 p-3 text-xs text-gray-200 font-mono overflow-x-auto",
                        "{key_pair.private_key}"
                    }
                }

                ul {
                    class: "list-disc pl-5 text-sm {theme::text::SECONDARY} space-y-1",
                    li { "Copy and store the private key in your secret manager." }
                    li { "Install the private key on the target host before starting the agent." }
                    li { "Keep the public key in Crystal Forge for system registration." }
                    li { "Rotate keys immediately if private key exposure is suspected." }
                }

                div {
                    class: "flex flex-col-reverse sm:flex-row sm:justify-end gap-2",
                    button {
                        class: "px-3 py-2 rounded-lg text-sm bg-gray-700 hover:bg-gray-600 text-white",
                        onclick: move |_| on_close.call(()),
                        "Close"
                    }
                    button {
                        class: "px-3 py-2 rounded-lg text-sm font-medium text-white {theme::interactive::PRIMARY_BTN}",
                        onclick: move |_| on_use_public_key.call(()),
                        "Use Public Key"
                    }
                }
            }
        }
    }
}
