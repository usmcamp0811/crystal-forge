//! Ed25519 keypair generation utilities for browser (WASM).
//!
//! Uses web-sys crypto.getRandomValues for secure random bytes.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::SigningKey;

/// Generate an Ed25519 keypair using browser crypto API.
///
/// Returns (private_key_base64, public_key_base64) matching the format expected by builder runtime.
pub fn generate_ed25519_keypair() -> Result<(String, String), String> {
    // Generate a 32-byte private key and derive its public key.
    let private_key_bytes = generate_random_bytes(32)?;
    let private_key_array: [u8; 32] = private_key_bytes
        .as_slice()
        .try_into()
        .map_err(|_| "Failed to convert private key bytes")?;
    let signing_key = SigningKey::from_bytes(&private_key_array);
    let public_key_bytes = signing_key.verifying_key().to_bytes();

    // Encode both keys as base64 (required by builder runtime and server API)
    let private_key_base64 = BASE64.encode(&private_key_bytes);
    let public_key_base64 = BASE64.encode(&public_key_bytes);

    Ok((private_key_base64, public_key_base64))
}

/// Generate cryptographically secure random bytes using browser's crypto API.
fn generate_random_bytes(length: usize) -> Result<Vec<u8>, String> {
    let window = web_sys::window().ok_or("No window object")?;
    let crypto = window.crypto().map_err(|_| "No crypto object")?;

    let mut bytes = vec![0u8; length];
    crypto
        .get_random_values_with_u8_array(&mut bytes)
        .map_err(|_| "Failed to generate random values")?;

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests won't run in WASM without wasm-pack test setup
    // They're here for documentation purposes

    #[test]
    #[ignore] // Requires browser environment
    fn test_keypair_format() {
        let (private_b64, public_b64) = generate_ed25519_keypair().unwrap();

        // Both keys should be valid base64 encoding of 32 bytes
        // Base64 encoding of 32 bytes is 44 characters (with padding) or 43 without
        assert!(private_b64.len() == 43 || private_b64.len() == 44);
        let private_decoded = BASE64.decode(&private_b64).unwrap();
        assert_eq!(private_decoded.len(), 32);

        assert!(public_b64.len() == 43 || public_b64.len() == 44);
        let public_decoded = BASE64.decode(&public_b64).unwrap();
        assert_eq!(public_decoded.len(), 32);
    }
}
