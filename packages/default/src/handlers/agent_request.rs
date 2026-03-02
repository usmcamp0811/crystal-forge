use crate::models::{system_states::SystemState, system_states::SystemStateV1, systems::System};
use crate::queries::systems::get_by_hostname;
use anyhow::Result;
use axum::extract::FromRef;
use axum::{http::HeaderMap, http::StatusCode};
use base64::engine::{Engine, general_purpose};
use bytes::Bytes;
use ed25519_dalek::Signature;
use ed25519_dalek::Verifier;
use sqlx::PgPool;
use uuid::Uuid;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

#[derive(Debug)]
pub struct VerifiedAgentRequest {
    pub key_id: String,
    pub signature: Signature,
    pub system: System,
    pub body: Bytes,
}

/// Trait for looking up systems by hostname.
///
/// This abstraction enables testing the authentication logic without a real database.
/// Production code uses `PgPool` via the impl for `SystemLookup`, while tests can
/// provide mock implementations.
pub trait SystemLookup: Clone + Send + Sync + 'static {
    /// Look up a system by its hostname (key_id).
    ///
    /// Returns `Ok(Some(system))` if found, `Ok(None)` if not found,
    /// or `Err` on database errors.
    fn lookup(
        &self,
        hostname: String,
    ) -> Pin<Box<dyn Future<Output = Result<Option<System>>> + Send>>;
}

/// Implementation of SystemLookup for PgPool (production use).
impl SystemLookup for PgPool {
    fn lookup(
        &self,
        hostname: String,
    ) -> Pin<Box<dyn Future<Output = Result<Option<System>>> + Send>> {
        let pool = self.clone();
        Box::pin(async move { get_by_hostname(&pool, &hostname).await })
    }
}

/// Extract key ID, decode signature, and verify against the system's public key.
///
/// This is the testable version that accepts any SystemLookup implementation.
///
/// # Errors
///
/// Returns `StatusCode::UNAUTHORIZED` for:
/// - Missing X-Key-ID header
/// - Missing X-Signature header
/// - Unknown hostname (no matching system)
/// - Invalid signature verification
///
/// Returns `StatusCode::BAD_REQUEST` for:
/// - Malformed signature (invalid base64 or wrong length)
///
/// Returns `StatusCode::INTERNAL_SERVER_ERROR` for:
/// - Database errors during lookup
pub async fn authenticate_agent_request_with_lookup<L: SystemLookup>(
    headers: &HeaderMap,
    body: Bytes,
    lookup: &L,
) -> Result<VerifiedAgentRequest, StatusCode> {
    let key_id = headers
        .get("X-Key-ID")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?
        .to_string();

    let sig = headers
        .get("X-Signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let signature_bytes = general_purpose::STANDARD
        .decode(sig)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let bytes: [u8; 64] = signature_bytes
        .try_into()
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let signature = Signature::from_bytes(&bytes);

    let system = lookup
        .lookup(key_id.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if system
        .public_key
        .verifying_key()
        .verify(&body, &signature)
        .is_err()
    {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(VerifiedAgentRequest {
        key_id,
        signature,
        system,
        body,
    })
}

/// Extract key ID, decode signature, and fetch the system entry.
/// Returns a VerifiedAgentRequest or an appropriate StatusCode error.
///
/// This is the production entry point that uses PgPool directly.
pub async fn authenticate_agent_request(
    headers: &HeaderMap,
    body: Bytes,
    pool: &PgPool,
) -> Result<VerifiedAgentRequest, StatusCode> {
    authenticate_agent_request_with_lookup(headers, body, pool).await
}

use crate::config::ServerConfig;

/// Shared server state containing authorized signing keys for current-system auth
#[derive(Clone)]
pub struct CFState {
    pub pool: PgPool,
    pub server_config: ServerConfig,
    pub eval_log_channels: Arc<tokio::sync::Mutex<std::collections::HashMap<i32, tokio::sync::broadcast::Sender<String>>>>,
    pub build_log_channels: Arc<tokio::sync::Mutex<std::collections::HashMap<Uuid, tokio::sync::broadcast::Sender<String>>>>,
}

impl CFState {
    pub fn new(pool: PgPool, server_config: ServerConfig) -> Self {
        Self {
            pool,
            server_config,
            eval_log_channels: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            build_log_channels: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

impl FromRef<CFState> for PgPool {
    fn from_ref(state: &CFState) -> PgPool {
        state.pool.clone()
    }
}

impl FromRef<CFState> for ServerConfig {
    fn from_ref(state: &CFState) -> ServerConfig {
        state.server_config.clone()
    }
}

pub fn deserialize_system_state_versioned(
    agent_request: &VerifiedAgentRequest,
) -> Result<(SystemState, bool)> {
    let body = &agent_request.body;

    // Try current version first
    if let Ok(state) = serde_json::from_slice::<SystemState>(body) {
        return Ok((state, true));
    }

    // Try previous versions with fallback
    if let Ok(old_state) = serde_json::from_slice::<SystemStateV1>(body) {
        let converted = SystemState::from_v1(old_state);
        return Ok((converted, false));
    }

    Err(anyhow::anyhow!(
        "Unable to deserialize any known SystemState version from system: {}",
        agent_request.system.hostname
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::public_key::PublicKey;
    use crate::test_utils::builders::SystemStateBuilder;
    use crate::test_utils::crypto::generate_keypair;
    use axum::http::HeaderName;
    use chrono::Utc;
    use ed25519_dalek::Signer;
    use uuid::Uuid;

    /// Mock implementation of SystemLookup for testing.
    #[derive(Clone)]
    struct MockSystemLookup {
        /// The system to return, or None to simulate "not found"
        system: Option<System>,
        /// If true, simulate a database error
        error: bool,
    }

    impl MockSystemLookup {
        fn new(system: Option<System>) -> Self {
            Self {
                system,
                error: false,
            }
        }

        fn with_error() -> Self {
            Self {
                system: None,
                error: true,
            }
        }
    }

    impl SystemLookup for MockSystemLookup {
        fn lookup(
            &self,
            _hostname: String,
        ) -> Pin<Box<dyn Future<Output = Result<Option<System>>> + Send>> {
            let system = self.system.clone();
            let error = self.error;
            Box::pin(async move {
                if error {
                    Err(anyhow::anyhow!("Database error"))
                } else {
                    Ok(system)
                }
            })
        }
    }

    /// Helper to create a test System with a known signing key.
    fn create_test_system(hostname: &str) -> (ed25519_dalek::SigningKey, System) {
        let (signing_key, verifying_key) = generate_keypair();
        let public_key = PublicKey::from_verifying_key(verifying_key);

        let system = System {
            id: Uuid::new_v4(),
            hostname: hostname.to_string(),
            environment_id: None,
            is_active: true,
            public_key,
            flake_id: Some(1),
            derivation: "/nix/store/test-system".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            desired_target: None,
            deployment_policy: "manual".to_string(),
        };

        (signing_key, system)
    }

    /// Helper to create valid headers with a signature.
    fn create_signed_headers(
        hostname: &str,
        body: &[u8],
        signing_key: &ed25519_dalek::SigningKey,
    ) -> HeaderMap {
        let signature = signing_key.sign(body);
        let sig_b64 = general_purpose::STANDARD.encode(signature.to_bytes());

        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-key-id"),
            hostname.parse().unwrap(),
        );
        headers.insert(
            HeaderName::from_static("x-signature"),
            sig_b64.parse().unwrap(),
        );
        headers
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // authenticate_agent_request tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_authenticate_with_valid_signature() {
        let (signing_key, system) = create_test_system("test-host");
        let body = Bytes::from(b"test request body".to_vec());
        let headers = create_signed_headers("test-host", &body, &signing_key);

        let lookup = MockSystemLookup::new(Some(system));
        let result = authenticate_agent_request_with_lookup(&headers, body.clone(), &lookup).await;

        assert!(result.is_ok());
        let verified = result.unwrap();
        assert_eq!(verified.key_id, "test-host");
        assert_eq!(verified.system.hostname, "test-host");
        assert_eq!(verified.body, body);
    }

    #[tokio::test]
    async fn test_authenticate_with_invalid_signature() {
        // Create a system with one key
        let (_signing_key, system) = create_test_system("test-host");

        // Create a different key pair and sign with it
        let (wrong_signing_key, _) = generate_keypair();
        let body = Bytes::from(b"test request body".to_vec());
        let headers = create_signed_headers("test-host", &body, &wrong_signing_key);

        let lookup = MockSystemLookup::new(Some(system));
        let result = authenticate_agent_request_with_lookup(&headers, body, &lookup).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_authenticate_missing_key_id_header() {
        let (signing_key, _system) = create_test_system("test-host");
        let body = Bytes::from(b"test request body".to_vec());

        // Only include signature, missing X-Key-ID
        let signature = signing_key.sign(&body);
        let sig_b64 = general_purpose::STANDARD.encode(signature.to_bytes());
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-signature"),
            sig_b64.parse().unwrap(),
        );

        let lookup = MockSystemLookup::new(None);
        let result = authenticate_agent_request_with_lookup(&headers, body, &lookup).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_authenticate_missing_signature_header() {
        let body = Bytes::from(b"test request body".to_vec());

        // Only include key-id, missing X-Signature
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-key-id"),
            "test-host".parse().unwrap(),
        );

        let lookup = MockSystemLookup::new(None);
        let result = authenticate_agent_request_with_lookup(&headers, body, &lookup).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_authenticate_missing_both_headers() {
        let body = Bytes::from(b"test request body".to_vec());
        let headers = HeaderMap::new(); // Empty headers

        let lookup = MockSystemLookup::new(None);
        let result = authenticate_agent_request_with_lookup(&headers, body, &lookup).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_authenticate_unknown_hostname() {
        let (signing_key, _) = create_test_system("test-host");
        let body = Bytes::from(b"test request body".to_vec());
        let headers = create_signed_headers("unknown-host", &body, &signing_key);

        // Lookup returns None (hostname not found)
        let lookup = MockSystemLookup::new(None);
        let result = authenticate_agent_request_with_lookup(&headers, body, &lookup).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_authenticate_database_error() {
        let (signing_key, _system) = create_test_system("test-host");
        let body = Bytes::from(b"test request body".to_vec());
        let headers = create_signed_headers("test-host", &body, &signing_key);

        // Lookup simulates a database error
        let lookup = MockSystemLookup::with_error();
        let result = authenticate_agent_request_with_lookup(&headers, body, &lookup).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_authenticate_invalid_base64_signature() {
        let body = Bytes::from(b"test request body".to_vec());

        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-key-id"),
            "test-host".parse().unwrap(),
        );
        // Invalid base64 string (not valid base64 at all)
        headers.insert(
            HeaderName::from_static("x-signature"),
            "not-valid-base64!!!".parse().unwrap(),
        );

        let lookup = MockSystemLookup::new(None);
        let result = authenticate_agent_request_with_lookup(&headers, body, &lookup).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_authenticate_signature_wrong_length() {
        let body = Bytes::from(b"test request body".to_vec());

        // Valid base64 but decoded to wrong length (not 64 bytes)
        let short_sig = general_purpose::STANDARD.encode(b"too short");

        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("x-key-id"),
            "test-host".parse().unwrap(),
        );
        headers.insert(
            HeaderName::from_static("x-signature"),
            short_sig.parse().unwrap(),
        );

        let lookup = MockSystemLookup::new(None);
        let result = authenticate_agent_request_with_lookup(&headers, body, &lookup).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_authenticate_signature_tampered_body() {
        let (signing_key, system) = create_test_system("test-host");

        // Sign one body
        let original_body = b"original request body";
        let headers = create_signed_headers("test-host", original_body, &signing_key);

        // But send a different body
        let tampered_body = Bytes::from(b"tampered request body".to_vec());

        let lookup = MockSystemLookup::new(Some(system));
        let result = authenticate_agent_request_with_lookup(&headers, tampered_body, &lookup).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::UNAUTHORIZED);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // deserialize_system_state_versioned tests
    // ═══════════════════════════════════════════════════════════════════════════

    fn create_verified_request(body: Bytes) -> VerifiedAgentRequest {
        let (signing_key, system) = create_test_system("test-host");
        let signature = signing_key.sign(&body);

        VerifiedAgentRequest {
            key_id: "test-host".to_string(),
            signature,
            system,
            body,
        }
    }

    #[test]
    fn test_deserialize_current_version_system_state() {
        let state = SystemStateBuilder::new()
            .hostname("test-host")
            .change_reason("startup")
            .build();
        let body = Bytes::from(serde_json::to_vec(&state).unwrap());

        let request = create_verified_request(body);
        let result = deserialize_system_state_versioned(&request);

        assert!(result.is_ok());
        let (deserialized, version_compatible) = result.unwrap();
        assert!(version_compatible); // Current version returns true
        assert_eq!(deserialized.hostname, "test-host");
        assert_eq!(deserialized.change_reason, "startup");
    }

    #[test]
    fn test_deserialize_v1_system_state_fallback() {
        // Create a V1 SystemState with the old 'context' field
        let v1_state = super::SystemStateV1 {
            id: None,
            hostname: "legacy-host".to_string(),
            context: "agent-startup".to_string(), // V1 used 'context' instead of 'change_reason'
            timestamp: Some(Utc::now()),
            store_path: Some("/nix/store/legacy".to_string()),
            os: Some("24.11".to_string()),
            kernel: Some("6.6.0".to_string()),
            memory_gb: Some(8.0),
            uptime_secs: Some(3600),
            cpu_brand: Some("Old CPU".to_string()),
            cpu_cores: Some(2),
            board_serial: Some("LEGACY-SERIAL".to_string()),
            product_uuid: Some("legacy-uuid".to_string()),
            rootfs_uuid: Some("legacy-rootfs".to_string()),
            chassis_serial: Some("LEGACY-CHASSIS".to_string()),
            bios_version: Some("0.9.0".to_string()),
            cpu_microcode: None,
            network_interfaces: None,
            primary_mac_address: Some("00:11:22:33:44:55".to_string()),
            primary_ip_address: Some("10.0.0.1".to_string()),
            gateway_ip: Some("10.0.0.254".to_string()),
            selinux_status: None,
            tpm_present: Some(false),
            secure_boot_enabled: Some(false),
            fips_mode: Some(false),
            agent_version: Some("0.1.0".to_string()),
            agent_build_hash: None,
            nixos_version: Some("24.11".to_string()),
        };

        let body = Bytes::from(serde_json::to_vec(&v1_state).unwrap());

        let request = create_verified_request(body);
        let result = deserialize_system_state_versioned(&request);

        assert!(result.is_ok());
        let (deserialized, version_compatible) = result.unwrap();
        assert!(!version_compatible); // V1 fallback returns false
        assert_eq!(deserialized.hostname, "legacy-host");
        // V1 'agent-startup' maps to 'startup'
        assert_eq!(deserialized.change_reason, "startup");
    }

    #[test]
    fn test_deserialize_invalid_json() {
        let body = Bytes::from(b"not valid json at all".to_vec());

        let request = create_verified_request(body);
        let result = deserialize_system_state_versioned(&request);

        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_wrong_json_structure() {
        // Valid JSON but not a SystemState structure
        let wrong_json = serde_json::json!({
            "foo": "bar",
            "number": 42
        });
        let body = Bytes::from(serde_json::to_vec(&wrong_json).unwrap());

        let request = create_verified_request(body);
        let result = deserialize_system_state_versioned(&request);

        assert!(result.is_err());
    }
}
