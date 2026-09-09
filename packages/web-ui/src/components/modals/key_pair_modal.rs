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

#[cfg(any(test, target_arch = "wasm32"))]
fn key_pair_from_secret(secret: [u8; 32]) -> GeneratedKeyPair {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use ed25519_dalek::SigningKey;

    let signing_key = SigningKey::from_bytes(&secret);
    let verifying_key = signing_key.verifying_key();

    GeneratedKeyPair {
        public_key: STANDARD.encode(verifying_key.to_bytes()),
        private_key: STANDARD.encode(signing_key.to_bytes()),
    }
}

/// Generate a new Ed25519 key pair for system authentication.
///
/// Generation fails closed when browser Web Crypto is unavailable. An
/// authentication private key must never fall back to a non-cryptographic RNG.
pub fn generate_key_pair() -> Result<GeneratedKeyPair, String> {
    #[cfg(target_arch = "wasm32")]
    {
        let window = web_sys::window().ok_or_else(|| {
            "Secure key generation is unavailable because this browser has no window context."
                .to_string()
        })?;
        let crypto = window.crypto().map_err(|_| {
            "Secure key generation requires the browser Web Crypto API.".to_string()
        })?;
        let mut secret = [0_u8; 32];
        crypto
            .get_random_values_with_u8_array(&mut secret)
            .map_err(|_| {
                "Secure random key generation failed. Check browser security settings and try again."
                    .to_string()
            })?;
        Ok(key_pair_from_secret(secret))
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        Err("Secure key generation is available only in a browser with Web Crypto.".to_string())
    }
}

/// Modal displaying a generated key pair for a new system.
///
/// Shows both public and private keys with instructions for secure handling.
/// The private key should be stored securely and installed on the target system.
#[component]
pub fn KeyPairModal(
    /// The securely generated key pair to display.
    keys: GeneratedKeyPair,
    /// Called when the modal is closed
    on_close: EventHandler<()>,
    /// Called when the user wants to use the public key in a form
    on_use_public_key: EventHandler<()>,
) -> Element {
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
                        "{keys.public_key}"
                    }
                }

                div {
                    class: "space-y-2",
                    p { class: "text-xs uppercase tracking-wide text-gray-500", "Private Key" }
                    pre {
                        class: "rounded-lg border border-gray-700 bg-gray-950 p-3 text-xs text-gray-200 font-mono overflow-x-auto",
                        "{keys.private_key}"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_secret_derives_matching_ed25519_keypair() {
        use base64::Engine;
        use base64::engine::general_purpose::STANDARD;
        use ed25519_dalek::SigningKey;

        let secret = [7_u8; 32];
        let pair = key_pair_from_secret(secret);
        let signing_key = SigningKey::from_bytes(&secret);

        assert_eq!(pair.private_key, STANDARD.encode(secret));
        assert_eq!(
            pair.public_key,
            STANDARD.encode(signing_key.verifying_key().to_bytes())
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn generation_fails_closed_outside_browser_web_crypto() {
        assert!(generate_key_pair().is_err());
    }
}
