//! Attestation signature verification and identity validation.
//!
//! Verifies signed attestation envelopes against enrolled agent Ed25519 keys.
//! This service contains no database queries — it operates on in-memory data.

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use cf_protocol::attestation::{AttestationPayload, SignedAttestationEnvelope, VerificationStatus};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

/// Result of verifying an attestation envelope.
#[derive(Debug)]
pub struct VerificationResult {
    pub status: VerificationStatus,
    pub reason: Option<String>,
    /// The canonical payload bytes that were verified (or attempted).
    pub canonical_bytes: Vec<u8>,
    /// The computed payload digest.
    pub payload_digest: Vec<u8>,
    /// The raw signature bytes.
    pub signature_bytes: Vec<u8>,
}

/// Verify a signed attestation envelope against a known public key.
///
/// Steps:
/// 1. Build canonical payload bytes from the payload struct.
/// 2. Verify the payload digest matches.
/// 3. Decode the signature from base64.
/// 4. Verify the Ed25519 signature against the public key.
pub fn verify_envelope(
    envelope: &SignedAttestationEnvelope,
    public_key: &VerifyingKey,
    expected_system_hostname: &str,
) -> VerificationResult {
    // 1. Canonical serialization
    let canonical_bytes = match build_canonical_bytes(&envelope.payload) {
        Ok(bytes) => bytes,
        Err(e) => {
            return VerificationResult {
                status: VerificationStatus::Malformed,
                reason: Some(format!("Failed to build canonical bytes: {e}")),
                canonical_bytes: Vec::new(),
                payload_digest: Vec::new(),
                signature_bytes: Vec::new(),
            };
        }
    };

    // 2. Verify payload digest
    let computed_digest = Sha256::digest(&canonical_bytes);
    let expected_digest_hex = hex::encode(&computed_digest);
    if envelope.payload.payload_digest != expected_digest_hex {
        return VerificationResult {
            status: VerificationStatus::Malformed,
            reason: Some("Payload digest mismatch".to_string()),
            canonical_bytes,
            payload_digest: computed_digest.to_vec(),
            signature_bytes: Vec::new(),
        };
    }

    // 3. Identity check: agent_key_id must match the expected system hostname
    if envelope.key_id != expected_system_hostname {
        return VerificationResult {
            status: VerificationStatus::IdentityMismatch,
            reason: Some(format!(
                "Key ID '{}' does not match expected system '{}'",
                envelope.key_id, expected_system_hostname
            )),
            canonical_bytes,
            payload_digest: computed_digest.to_vec(),
            signature_bytes: Vec::new(),
        };
    }

    // 4. Decode signature
    let signature_bytes = match BASE64.decode(&envelope.signature) {
        Ok(bytes) => bytes,
        Err(e) => {
            return VerificationResult {
                status: VerificationStatus::Malformed,
                reason: Some(format!("Invalid signature encoding: {e}")),
                canonical_bytes,
                payload_digest: computed_digest.to_vec(),
                signature_bytes: Vec::new(),
            };
        }
    };

    let signature = match Signature::from_slice(&signature_bytes) {
        Ok(sig) => sig,
        Err(e) => {
            return VerificationResult {
                status: VerificationStatus::Malformed,
                reason: Some(format!("Invalid signature format: {e}")),
                canonical_bytes,
                payload_digest: computed_digest.to_vec(),
                signature_bytes,
            };
        }
    };

    // 5. Verify signature
    match public_key.verify(&canonical_bytes, &signature) {
        Ok(()) => VerificationResult {
            status: VerificationStatus::Verified,
            reason: None,
            canonical_bytes,
            payload_digest: computed_digest.to_vec(),
            signature_bytes,
        },
        Err(_) => VerificationResult {
            status: VerificationStatus::InvalidSignature,
            reason: Some("Ed25519 signature verification failed".to_string()),
            canonical_bytes,
            payload_digest: computed_digest.to_vec(),
            signature_bytes,
        },
    }
}

/// Build canonical bytes from an attestation payload.
///
/// Uses deterministic JSON serialization with the fixed field order defined
/// by the `AttestationPayload` struct. Both agent and server use this same
/// Rust type from `cf-protocol`.
pub fn build_canonical_bytes(payload: &AttestationPayload) -> Result<Vec<u8>> {
    // Build a copy with payload_digest zeroed out for signing
    // (the digest field is computed over the payload without itself)
    let signing_payload = AttestationPayload {
        payload_digest: "0".repeat(64), // placeholder for canonical computation
        ..payload.clone()
    };
    serde_json::to_vec(&signing_payload).context("serialize canonical payload")
}

/// Compute the payload digest for a given payload (with zeroed digest field).
pub fn compute_payload_digest(payload: &AttestationPayload) -> Result<String> {
    let canonical = build_canonical_bytes(payload)?;
    let digest = Sha256::digest(&canonical);
    Ok(hex::encode(digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;
    use uuid::Uuid;

    fn test_payload() -> AttestationPayload {
        AttestationPayload {
            activation_source: None,
            agent_build_hash: Some("test-hash".to_string()),
            agent_key_id: "test-host".to_string(),
            agent_session_id: None,
            agent_version: "0.1.0".to_string(),
            attestation_id: Uuid::new_v4(),
            boot_id: "test-boot-id".to_string(),
            boot_timestamp: None,
            booted_generation: Some(1),
            current_system_store_path: "/nix/store/test".to_string(),
            current_system_nar_hash: None,
            deployment_authorization_id: None,
            deployment_execution_id: None,
            kernel_version: Some("6.1.0".to_string()),
            monotonic_counter: 1,
            nix_version: Some("2.18.0".to_string()),
            observed_at: Utc::now(),
            payload_digest: String::new(),
            protocol_version: cf_protocol::attestation::ATTESTATION_PROTOCOL_VERSION,
            system_id: "test-host".to_string(),
            system_profile_store_path: None,
        }
    }

    #[test]
    fn verify_valid_signature() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let mut payload = test_payload();
        let digest = compute_payload_digest(&payload).unwrap();
        payload.payload_digest = digest;

        let canonical = build_canonical_bytes(&payload).unwrap();
        let signature = signing_key.sign(&canonical);
        let sig_b64 = BASE64.encode(signature.to_bytes());

        let envelope = SignedAttestationEnvelope {
            payload,
            signature: sig_b64,
            key_id: "test-host".to_string(),
            signature_algorithm: "ed25519-v1".to_string(),
        };

        let result = verify_envelope(&envelope, &verifying_key, "test-host");
        assert_eq!(result.status, VerificationStatus::Verified);
        assert!(result.reason.is_none());
    }

    #[test]
    fn reject_identity_mismatch() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        let mut payload = test_payload();
        let digest = compute_payload_digest(&payload).unwrap();
        payload.payload_digest = digest;

        let canonical = build_canonical_bytes(&payload).unwrap();
        let signature = signing_key.sign(&canonical);
        let sig_b64 = BASE64.encode(signature.to_bytes());

        let envelope = SignedAttestationEnvelope {
            payload,
            signature: sig_b64,
            key_id: "test-host".to_string(),
            signature_algorithm: "ed25519-v1".to_string(),
        };

        let result = verify_envelope(&envelope, &verifying_key, "other-host");
        assert_eq!(result.status, VerificationStatus::IdentityMismatch);
    }

    #[test]
    fn reject_invalid_signature() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let wrong_key = SigningKey::generate(&mut OsRng);
        let wrong_verifying_key = wrong_key.verifying_key();

        let mut payload = test_payload();
        let digest = compute_payload_digest(&payload).unwrap();
        payload.payload_digest = digest;

        let canonical = build_canonical_bytes(&payload).unwrap();
        let signature = signing_key.sign(&canonical);
        let sig_b64 = BASE64.encode(signature.to_bytes());

        let envelope = SignedAttestationEnvelope {
            payload,
            signature: sig_b64,
            key_id: "test-host".to_string(),
            signature_algorithm: "ed25519-v1".to_string(),
        };

        let result = verify_envelope(&envelope, &wrong_verifying_key, "test-host");
        assert_eq!(result.status, VerificationStatus::InvalidSignature);
    }
}
