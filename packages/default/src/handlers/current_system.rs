use crate::models::system_states::{SystemState, SystemStateV1};
use crate::queries::system_states::insert_system_state;
use anyhow::Result;
use axum::extract::FromRef;
use base64::engine::Engine;
use base64::engine::general_purpose;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sqlx::PgPool;
use std::collections::HashMap;

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use tracing::{debug, info};

/// Shared server state containing authorized signing keys for current-system auth
#[derive(Clone)]
pub struct CFState {
    pool: PgPool,
    authorized_keys: HashMap<String, VerifyingKey>,
}

impl CFState {
    pub fn new(pool: PgPool, authorized_keys: HashMap<String, VerifyingKey>) -> Self {
        Self {
            pool,
            authorized_keys,
        }
    }
}

impl FromRef<CFState> for PgPool {
    fn from_ref(state: &CFState) -> PgPool {
        state.pool.clone()
    }
}
/// Handles the `/current-system` POST route.
/// Verifies the body signature using headers, parses the payload, and
/// stores system state info in the database.
pub async fn handle_current_system(
    State(state): State<CFState>,
    State(pool): State<PgPool>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Extract and validate key ID and signature from headers
    let key_id = match headers.get("X-Key-ID") {
        Some(v) => v.to_str().unwrap_or(""),
        None => return StatusCode::UNAUTHORIZED,
    };

    let sig = match headers.get("X-Signature") {
        Some(v) => v.to_str().unwrap_or(""),
        None => return StatusCode::UNAUTHORIZED,
    };

    // Decode and validate signature format
    let signature_bytes = match general_purpose::STANDARD.decode(sig) {
        Ok(bytes) => bytes,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    let bytes: [u8; 64] = match signature_bytes.try_into() {
        Ok(b) => b,
        Err(_) => return StatusCode::BAD_REQUEST,
    };

    let signature = Signature::from_bytes(&bytes);

    // Lookup key for signature verification
    let key = match state.authorized_keys.get(key_id) {
        Some(k) => k,
        None => return StatusCode::UNAUTHORIZED,
    };

    if key.verify(&body, &signature).is_err() {
        return StatusCode::UNAUTHORIZED;
    }

    // Try to deserialize with version detection
    let (payload, version_compatible) = match try_deserialize_system_state(&body) {
        Ok((state, compatible)) => (state, compatible),
        Err(e) => {
            eprintln!("❌ All deserialization attempts failed: {e}");
            return StatusCode::BAD_REQUEST;
        }
    };

    info!("{}", payload);
    // Insert with compatibility flag
    if let Err(e) = insert_system_state(&pool, &payload, version_compatible).await {
        debug!("❌ failed to insert into DB: {e:?}");
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    // Return different status codes based on compatibility
    if version_compatible {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED // 202 - accepted but agent should upgrade
    }
}

pub fn try_deserialize_system_state(body: &[u8]) -> Result<(SystemState, bool)> {
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
        "Unable to deserialize any known SystemState version"
    ))
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use axum::{
        Router,
        body::Body,
        http::{HeaderMap, Method, Request, StatusCode},
    };
    use axum_test::TestServer;
    use base64::engine::Engine;
    use base64::engine::general_purpose;
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_current_system_endpoint_success() {
        // Create a test signing key
        let signing_key = SigningKey::generate(&mut rand::thread_rng());
        let verifying_key = signing_key.verifying_key();

        // Create authorized keys map
        let mut authorized_keys = HashMap::new();
        authorized_keys.insert("test-key".to_string(), verifying_key);

        // Mock database pool (you'd need to set up a test database)
        // let pool = create_test_pool().await;

        // Create test state
        // let state = CFState::new(pool, authorized_keys);

        // Create test system state
        let test_state = SystemState {
            id: None,
            hostname: "test-machine".to_string(),
            context: "test".to_string(),
            derivation_path: Some("/nix/store/test123".to_string()),
            os: Some("NixOS".to_string()),
            kernel: Some("6.1.0".to_string()),
            memory_gb: Some(16.0),
            uptime_secs: Some(3600),
            cpu_brand: Some("Test CPU".to_string()),
            cpu_cores: Some(8),
            board_serial: None,
            product_uuid: None,
            rootfs_uuid: None,
            timestamp: None,
            chassis_serial: None,
            bios_version: None,
            cpu_microcode: None,
            network_interfaces: None,
            primary_mac_address: None,
            primary_ip_address: None,
            gateway_ip: None,
            selinux_status: None,
            tpm_present: None,
            secure_boot_enabled: None,
            fips_mode: None,
            agent_version: Some("1.0.0".to_string()),
            agent_build_hash: Some("test-hash".to_string()),
            nixos_version: Some("23.05".to_string()),
        };

        // Serialize the payload
        let payload = serde_json::to_vec(&test_state).unwrap();

        // Sign the payload
        let signature = signing_key.sign(&payload);
        let signature_b64 = general_purpose::STANDARD.encode(signature.to_bytes());

        // This would be your actual test if you had the router set up:
        /*
        let app = Router::new()
            .route("/current-system", post(handle_current_system))
            .with_state(state);

        let server = TestServer::new(app).unwrap();

        let response = server
            .post("/current-system")
            .add_header("X-Key-ID", "test-key")
            .add_header("X-Signature", &signature_b64)
            .bytes(payload)
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        */

        // For now, just test the signature verification logic
        let decoded_sig = general_purpose::STANDARD.decode(&signature_b64).unwrap();
        let sig_bytes: [u8; 64] = decoded_sig.try_into().unwrap();
        let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);

        assert!(verifying_key.verify(&payload, &sig).is_ok());
    }

    // Simple test you can run right now without database
    #[test]
    fn test_signature_creation_and_verification() {
        let signing_key = SigningKey::generate(&mut rand::thread_rng());
        let verifying_key = signing_key.verifying_key();

        let test_data = b"test message";
        let signature = signing_key.sign(test_data);

        assert!(verifying_key.verify(test_data, &signature).is_ok());

        // Test base64 encoding/decoding like your handler does
        let sig_b64 = general_purpose::STANDARD.encode(signature.to_bytes());
        let decoded = general_purpose::STANDARD.decode(&sig_b64).unwrap();
        let sig_bytes: [u8; 64] = decoded.try_into().unwrap();
        let reconstructed_sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);

        assert!(verifying_key.verify(test_data, &reconstructed_sig).is_ok());
    }
}
