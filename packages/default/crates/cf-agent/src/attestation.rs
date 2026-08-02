//! Running-state attestation producer.
//!
//! Builds, signs, and sends running-state attestations to the Crystal Forge
//! server. Manages monotonic counter state with durable file persistence.

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use cf_protocol::attestation::{
    AttestationPayload, AttestationReceipt, SignedAttestationEnvelope, ATTESTATION_PROTOCOL_VERSION,
};
use chrono::Utc;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Default attestation interval (6 hours).
pub const DEFAULT_ATTESTATION_INTERVAL_SECS: u64 = 6 * 60 * 60;

/// Monotonic counter state manager.
///
/// Persists the counter in a durable file location with atomic replacement
/// to prevent counter reuse after agent restart.
pub struct CounterState {
    path: PathBuf,
    boot_id: String,
    counter: i64,
}

impl CounterState {
    /// Load or initialize counter state for the current boot session.
    pub fn load(state_dir: &Path, boot_id: &str) -> Result<Self> {
        let path = state_dir.join("attestation_counter");

        // Try to read existing state
        if let Ok(content) = fs::read_to_string(&path) {
            if let Some((stored_boot_id, counter_str)) = content.trim().split_once(':') {
                if stored_boot_id == boot_id {
                    if let Ok(counter) = counter_str.parse::<i64>() {
                        return Ok(CounterState {
                            path,
                            boot_id: boot_id.to_string(),
                            counter,
                        });
                    }
                }
            }
        }

        // New boot session or corrupted state — start fresh
        Ok(CounterState {
            path,
            boot_id: boot_id.to_string(),
            counter: 0,
        })
    }

    /// Atomically advance the counter and persist the new value.
    pub fn advance(&mut self) -> Result<i64> {
        self.counter += 1;
        self.persist()?;
        Ok(self.counter)
    }

    /// Get the current counter value.
    pub fn current(&self) -> i64 {
        self.counter
    }

    fn persist(&self) -> Result<()> {
        let content = format!("{}:{}", self.boot_id, self.counter);
        let tmp_path = self.path.with_extension("tmp");

        let mut f = fs::File::create(&tmp_path)
            .context("create counter temp file")?;
        f.write_all(content.as_bytes())
            .context("write counter state")?;
        f.sync_all().context("sync counter state")?;

        fs::rename(&tmp_path, &self.path)
            .context("atomic rename counter state")?;

        Ok(())
    }
}

/// Build and sign an attestation envelope.
pub fn build_attestation(
    signing_key: &SigningKey,
    key_id: &str,
    boot_id: &str,
    boot_timestamp: Option<chrono::DateTime<Utc>>,
    counter: i64,
    store_path: &str,
    nar_hash: Option<&str>,
    profile_store_path: Option<&str>,
    booted_generation: Option<i64>,
    kernel_version: Option<&str>,
    nix_version: Option<&str>,
    agent_version: &str,
    agent_build_hash: Option<&str>,
    authorization_id: Option<Uuid>,
    execution_id: Option<Uuid>,
    activation_source: Option<&str>,
) -> Result<SignedAttestationEnvelope> {
    let attestation_id = Uuid::new_v4();

    // Build payload with placeholder digest
    let mut payload = AttestationPayload {
        activation_source: activation_source.map(|s| s.to_string()),
        agent_build_hash: agent_build_hash.map(|s| s.to_string()),
        agent_key_id: key_id.to_string(),
        agent_session_id: None,
        agent_version: agent_version.to_string(),
        attestation_id,
        boot_id: boot_id.to_string(),
        boot_timestamp,
        booted_generation,
        current_system_store_path: store_path.to_string(),
        current_system_nar_hash: nar_hash.map(|s| s.to_string()),
        deployment_authorization_id: authorization_id,
        deployment_execution_id: execution_id,
        kernel_version: kernel_version.map(|s| s.to_string()),
        monotonic_counter: counter,
        nix_version: nix_version.map(|s| s.to_string()),
        observed_at: Utc::now(),
        payload_digest: "0".repeat(64), // placeholder for canonical computation
        protocol_version: ATTESTATION_PROTOCOL_VERSION,
        system_id: key_id.to_string(),
        system_profile_store_path: profile_store_path.map(|s| s.to_string()),
    };

    // Compute canonical bytes (with zeroed digest)
    let canonical_bytes = serde_json::to_vec(&payload)
        .context("serialize canonical payload")?;

    // Compute digest
    let digest = Sha256::digest(&canonical_bytes);
    let digest_hex = hex::encode(digest);
    payload.payload_digest = digest_hex;

    // Re-serialize with actual digest for the canonical form
    // (Note: the canonical bytes for signing use the zeroed digest, which is
    // what verify_envelope also uses via build_canonical_bytes)

    // Sign the canonical bytes (with zeroed digest)
    let signature = signing_key.sign(&canonical_bytes);
    let sig_b64 = BASE64.encode(signature.to_bytes());

    Ok(SignedAttestationEnvelope {
        payload,
        signature: sig_b64,
        key_id: key_id.to_string(),
        signature_algorithm: "ed25519-v1".to_string(),
    })
}

/// Send an attestation to the server.
pub async fn send_attestation(
    client: &reqwest::Client,
    server_url: &str,
    envelope: &SignedAttestationEnvelope,
) -> Result<AttestationReceipt> {
    let url = format!("{}/api/v1/agent/running-state-attestations", server_url);

    let response = client
        .post(&url)
        .json(envelope)
        .send()
        .await
        .context("send attestation request")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Attestation submission failed (HTTP {}): {}", status, body);
    }

    response
        .json::<AttestationReceipt>()
        .await
        .context("parse attestation receipt")
}

/// Determine if an attestation should be sent based on the trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttestationTrigger {
    /// Agent startup.
    Startup,
    /// Boot ID changed (system rebooted).
    BootChange,
    /// System store path changed.
    StorePathChange,
    /// Deployment completed.
    DeploymentComplete,
    /// Deployment failed.
    DeploymentFailed,
    /// Periodic timer.
    Periodic,
}

impl std::fmt::Display for AttestationTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Startup => write!(f, "startup"),
            Self::BootChange => write!(f, "boot_change"),
            Self::StorePathChange => write!(f, "store_path_change"),
            Self::DeploymentComplete => write!(f, "deployment_complete"),
            Self::DeploymentFailed => write!(f, "deployment_failed"),
            Self::Periodic => write!(f, "periodic"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn counter_state_persists_and_advances() {
        let dir = std::env::temp_dir().join(format!("cf-agent-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let boot_id = "test-boot-id";

        let mut counter = CounterState::load(&dir, boot_id).unwrap();
        assert_eq!(counter.current(), 0);

        assert_eq!(counter.advance().unwrap(), 1);
        assert_eq!(counter.advance().unwrap(), 2);

        // Reload should resume
        let reloaded = CounterState::load(&dir, boot_id).unwrap();
        assert_eq!(reloaded.current(), 2);

        // New boot should reset
        let new_boot = CounterState::load(&dir, "new-boot-id").unwrap();
        assert_eq!(new_boot.current(), 0);

        // Cleanup
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_attestation_produces_valid_envelope() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let key_id = "test-host";
        let boot_id = "test-boot-id";
        let store_path = "/nix/store/test-nixos-system";

        let envelope = build_attestation(
            &signing_key,
            key_id,
            boot_id,
            None,
            1,
            store_path,
            None,
            None,
            Some(42),
            Some("6.1.0"),
            Some("2.18.0"),
            "0.1.0",
            None,
            None,
            None,
            Some("startup"),
        )
        .unwrap();

        assert_eq!(envelope.key_id, key_id);
        assert_eq!(envelope.payload.agent_key_id, key_id);
        assert_eq!(envelope.payload.boot_id, boot_id);
        assert_eq!(envelope.payload.current_system_store_path, store_path);
        assert_eq!(envelope.payload.monotonic_counter, 1);
        assert_eq!(
            envelope.payload.protocol_version,
            ATTESTATION_PROTOCOL_VERSION
        );
        assert!(!envelope.signature.is_empty());
        assert_eq!(envelope.payload.payload_digest.len(), 64); // SHA-256 hex
    }
}
