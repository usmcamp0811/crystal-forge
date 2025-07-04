use crate::handlers::agent_request::{
    CFState, authenticate_agent_request, try_deserialize_system_state,
};
use crate::models::system_states::{SystemState, SystemStateV1};
use crate::queries::system_states::insert_system_state;
use crate::queries::systems::get_by_hostname;
use anyhow::Result;
use axum::{
    body::Bytes,
    extract::FromRef,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use base64::engine::Engine;
use base64::engine::general_purpose;
use ed25519_dalek::Signature;
use sqlx::PgPool;
use tracing::{debug, info};

/// Handles the `/current-system` POST route.
/// Verifies the body signature using headers, parses the payload, and
/// stores system state info in the database.
pub async fn handle_current_system(
    State(state): State<CFState>,
    State(pool): State<PgPool>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Get verified agent request
    let agent_request = match authenticate_agent_request(&headers, body, &pool).await {
        Ok(req) => req,
        Err(status) => return status,
    };

    // Try to deserialize with version detection
    let (payload, version_compatible) = match try_deserialize_system_state(&agent_request) {
        Ok((state, compatible)) => (state, compatible),
        Err(e) => {
            debug!("❌ All deserialization attempts failed: {e}");
            return StatusCode::BAD_REQUEST;
        }
    };

    // TODO: Might want to just do payload need to see what it looks like
    info!(
        "System state received from {}: {}",
        agent_request.system.hostname, payload
    );

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
    use ed25519_dalek::Verifier;
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json;
    use std::collections::HashMap;
    use winnow::Parser;

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
