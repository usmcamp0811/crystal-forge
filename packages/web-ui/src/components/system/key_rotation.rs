//! Shared agent public-key logic for the system key-rotation surfaces.
//!
//! Both entry points that replace a system's agent key — the Systems-list
//! "Update Key" row action (`UpdatePublicKeyModal`) and the Agent identity
//! section of `EditSystemModal` — validate and fingerprint the operator's key
//! through this module, so there is exactly one definition of "is this a usable
//! Ed25519 agent public key".
//!
//! Nothing here talks to the network, and nothing here ever handles a private
//! key: key generation stays in [`crate::components::modals::generate_key_pair`]
//! and persistence stays on `PUT /systems/:id/public-key`.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use sha2::{Digest, Sha256};

/// Raw byte length of an Ed25519 public key.
const ED25519_PUBLIC_KEY_LEN: usize = 32;

/// Validate an operator-supplied agent public key.
///
/// Mirrors the server-side `PublicKey::from_base64` contract used by
/// `PUT /systems/:id/public-key` closely enough to catch the common paste
/// mistakes in the browser, while the server stays the authoritative check
/// (it additionally rejects keys that are not valid Ed25519 curve points).
///
/// Returns the trimmed key on success, or an operator-facing message.
pub fn validate_public_key_input(input: &str) -> Result<String, String> {
    let candidate = input.trim();

    if candidate.is_empty() {
        return Err("Public key cannot be empty".to_string());
    }

    let decoded = STANDARD
        .decode(candidate)
        .map_err(|_| "Public key must be a base64-encoded Ed25519 key".to_string())?;

    if decoded.len() != ED25519_PUBLIC_KEY_LEN {
        return Err(format!(
            "Public key must decode to {ED25519_PUBLIC_KEY_LEN} bytes, got {}",
            decoded.len()
        ));
    }

    Ok(candidate.to_string())
}

/// Compute the display fingerprint for a base64 agent public key.
///
/// Byte-for-byte identical to the server's `PublicKey::fingerprint()`
/// (`SHA256:` + unpadded base64 of the SHA256 digest of the raw 32 key bytes),
/// so a locally generated or pasted key can be previewed and shown as the new
/// current fingerprint without a round trip. Returns `None` for input that
/// `validate_public_key_input` rejects.
pub fn public_key_fingerprint(input: &str) -> Option<String> {
    let key = validate_public_key_input(input).ok()?;
    let raw = STANDARD.decode(key).ok()?;
    let digest = Sha256::digest(raw);
    Some(format!("SHA256:{}", STANDARD_NO_PAD.encode(digest)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_valid_public_key(input: &str) -> bool {
        validate_public_key_input(input).is_ok()
    }

    /// Base64 public key for the Ed25519 signing key seeded with 32 `0x07`
    /// bytes — the same fixture the server uses in
    /// `models::public_key::tests::fingerprint_uses_stable_sha256_display_format`.
    fn sample_public_key() -> String {
        use ed25519_dalek::SigningKey;
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        STANDARD.encode(signing_key.verifying_key().to_bytes())
    }

    #[test]
    fn validation_accepts_a_real_ed25519_public_key() {
        let key = sample_public_key();

        assert_eq!(validate_public_key_input(&key).as_deref(), Ok(key.as_str()));
        assert!(is_valid_public_key(&key));
    }

    #[test]
    fn validation_trims_surrounding_whitespace_from_a_paste() {
        let key = sample_public_key();
        let pasted = format!("  {key}\n");

        assert_eq!(
            validate_public_key_input(&pasted).as_deref(),
            Ok(key.as_str())
        );
    }

    #[test]
    fn validation_rejects_empty_input() {
        assert_eq!(
            validate_public_key_input("   "),
            Err("Public key cannot be empty".to_string())
        );
        assert!(!is_valid_public_key(""));
    }

    #[test]
    fn validation_rejects_non_base64_input() {
        // The design mock's OpenSSH-armored placeholder is not what this API
        // accepts; catching it in the browser avoids a confusing server 400.
        assert!(!is_valid_public_key("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA"));
        assert!(!is_valid_public_key("not base64!"));
    }

    #[test]
    fn validation_rejects_correctly_encoded_keys_of_the_wrong_length() {
        // Valid base64, but only 3 decoded bytes.
        assert!(!is_valid_public_key("YWJj"));
        // Valid base64 of 31 bytes.
        assert!(!is_valid_public_key(&STANDARD.encode([1_u8; 31])));
        assert!(is_valid_public_key(&STANDARD.encode([1_u8; 32])));
    }

    #[test]
    fn fingerprint_matches_the_server_display_format() {
        let fingerprint = public_key_fingerprint(&sample_public_key()).expect("valid key");

        assert!(fingerprint.starts_with("SHA256:"));
        // Unpadded base64, exactly as `PublicKey::fingerprint()` emits.
        assert!(!fingerprint.contains('='));
        assert_eq!(fingerprint.len(), "SHA256:".len() + 43);
    }

    #[test]
    fn fingerprint_is_stable_and_key_specific() {
        let a = public_key_fingerprint(&STANDARD.encode([7_u8; 32])).expect("valid key");
        let b = public_key_fingerprint(&STANDARD.encode([7_u8; 32])).expect("valid key");
        let c = public_key_fingerprint(&STANDARD.encode([8_u8; 32])).expect("valid key");

        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn fingerprint_is_none_for_invalid_input() {
        assert_eq!(public_key_fingerprint(""), None);
        assert_eq!(public_key_fingerprint("not base64!"), None);
        assert_eq!(public_key_fingerprint("YWJj"), None);
    }
}
