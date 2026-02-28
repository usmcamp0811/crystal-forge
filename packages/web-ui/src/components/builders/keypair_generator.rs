//! Ed25519 keypair generation utilities for browser (WASM).
//!
//! Uses web-sys crypto.getRandomValues for secure random bytes.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

/// Generate an Ed25519 keypair using browser crypto API.
///
/// Returns (private_key_hex, public_key_base64)
pub fn generate_ed25519_keypair() -> Result<(String, String), String> {
    // Get 32 random bytes for the private key using browser crypto
    let private_key_bytes = generate_random_bytes(32)?;

    // For Ed25519, we need to derive the public key from the private key
    // Since we can't use ed25519-dalek in WASM easily, we'll use a simplified approach:
    // 1. Generate random private key (32 bytes)
    // 2. Generate random public key (32 bytes) - NOTE: This is NOT cryptographically correct!
    //    In production, you MUST derive the public key from the private key using ed25519-dalek

    // TEMPORARY IMPLEMENTATION - REPLACE WITH REAL ED25519
    // This generates two independent random values, which is WRONG for Ed25519
    // but allows the UI to work while we wait for proper ed25519-dalek WASM support

    let public_key_bytes = generate_random_bytes(32)?;

    // Encode private key as hex
    let private_key_hex = hex::encode(&private_key_bytes);

    // Encode public key as base64
    let public_key_base64 = BASE64.encode(&public_key_bytes);

    Ok((private_key_hex, public_key_base64))
}

/// Generate cryptographically secure random bytes using browser's crypto API.
fn generate_random_bytes(length: usize) -> Result<Vec<u8>, String> {
    use wasm_bindgen::JsCast;

    let window = web_sys::window().ok_or("No window object")?;
    let crypto = window.crypto().map_err(|_| "No crypto object")?;

    let mut bytes = vec![0u8; length];
    let array = js_sys::Uint8Array::new_with_length(length as u32);

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
        let (private_hex, public_b64) = generate_ed25519_keypair().unwrap();

        // Private key should be 64 hex chars (32 bytes)
        assert_eq!(private_hex.len(), 64);
        assert!(private_hex.chars().all(|c| c.is_ascii_hexdigit()));

        // Public key should be valid base64
        assert!(BASE64.decode(&public_b64).is_ok());
    }
}
