//! Deterministic ed25519 key helpers for tests.
//!
//! Provides convenience functions for generating key pairs and `PublicKey`
//! instances without importing low-level crypto crates in every test file.

use crate::models::public_key::PublicKey;
use ed25519_dalek::{SigningKey, VerifyingKey};

/// Generate a random ed25519 signing key (and its verifying half).
///
/// Uses `rand::thread_rng()` — fine for tests, never for production secrets.
pub fn generate_keypair() -> (SigningKey, VerifyingKey) {
    let signing = SigningKey::generate(&mut rand::thread_rng());
    let verifying = signing.verifying_key();
    (signing, verifying)
}

/// Produce a [`PublicKey`] wrapper from a fresh random key pair.
///
/// Returns both the signing key (so the caller can create signatures) and
/// the wrapped `PublicKey`.
pub fn test_public_key() -> (SigningKey, PublicKey) {
    let (signing, verifying) = generate_keypair();
    (signing, PublicKey::from_verifying_key(verifying))
}

/// Produce a base64-encoded public key string suitable for constructing a
/// `System` via `PublicKey::from_base64`.
pub fn test_public_key_base64() -> (SigningKey, String) {
    let (signing, pk) = test_public_key();
    (signing, pk.to_base64())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_keypair_produces_valid_pair() {
        let (signing, verifying) = generate_keypair();
        // The verifying key derived from the signing key must match.
        assert_eq!(signing.verifying_key().to_bytes(), verifying.to_bytes());
    }

    #[test]
    fn test_public_key_round_trips_via_base64() {
        let (_signing, pk) = test_public_key();
        let b64 = pk.to_base64();
        let recovered = PublicKey::from_base64(&b64, "test").unwrap();
        assert_eq!(pk.to_bytes(), recovered.to_bytes());
    }

    #[test]
    fn test_public_key_base64_is_valid() {
        let (_signing, b64) = test_public_key_base64();
        // Must successfully parse.
        let pk = PublicKey::from_base64(&b64, "test").unwrap();
        assert_eq!(pk.to_bytes().len(), 32);
    }
}
