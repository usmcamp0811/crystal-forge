use crate::handlers::agent_request::deserialize_system_state_versioned;
use crate::handlers::agent_request::{
    CFState, SystemLookup, authenticate_agent_request_with_lookup,
};
use crate::models::system_states::SystemState;
use crate::queries::system_states::insert_system_state;
use anyhow::Result;
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use sqlx::PgPool;
use std::future::Future;
use std::pin::Pin;
use tracing::{debug, info};

type InsertFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

async fn update_with_lookup_and_insert<L, I>(
    headers: HeaderMap,
    body: Bytes,
    lookup: &L,
    insert: I,
) -> StatusCode
where
    L: SystemLookup,
    I: for<'a> Fn(&'a SystemState, bool) -> InsertFuture<'a>,
{
    let agent_request = match authenticate_agent_request_with_lookup(&headers, body, lookup).await {
        Ok(req) => req,
        Err(status) => return status,
    };

    let (payload, version_compatible) = match deserialize_system_state_versioned(&agent_request) {
        Ok((state, compatible)) => (state, compatible),
        Err(e) => {
            debug!("All deserialization attempts failed: {e}");
            return StatusCode::BAD_REQUEST;
        }
    };

    info!(
        "System state received from {}: {}",
        agent_request.system.hostname, payload
    );

    if let Err(e) = insert(&payload, version_compatible).await {
        debug!("failed to insert into DB: {e:?}");
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    if version_compatible {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED
    }
}

/// Handles the `/current-system` POST route.
/// Verifies the body signature using headers, parses the payload, and
/// stores system state info in the database.
pub async fn update(
    State(_state): State<CFState>,
    State(pool): State<PgPool>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let pool = pool.clone();
    update_with_lookup_and_insert(headers, body, &pool, |payload, version_compatible| {
        let pool = pool.clone();
        Box::pin(async move { insert_system_state(&pool, payload, version_compatible).await })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::agent_request::SystemLookup;
    use crate::models::{public_key::PublicKey, system_states::SystemStateV1, systems::System};
    use crate::test_utils::{builders::SystemStateBuilder, crypto::generate_keypair};
    use axum::http::HeaderName;
    use base64::engine::{Engine, general_purpose};
    use chrono::Utc;
    use ed25519_dalek::Signer;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Mutex;
    use uuid::Uuid;

    #[derive(Clone)]
    struct MockLookup {
        system: Option<System>,
    }

    impl MockLookup {
        fn new(system: Option<System>) -> Self {
            Self { system }
        }
    }

    impl SystemLookup for MockLookup {
        fn lookup(
            &self,
            _hostname: String,
        ) -> Pin<Box<dyn Future<Output = Result<Option<System>>> + Send>> {
            let system = self.system.clone();
            Box::pin(async move { Ok(system) })
        }
    }

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
            system_configuration_name: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            desired_target: None,
            deployment_policy: "manual".to_string(),
        };

        (signing_key, system)
    }

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
            hostname.parse().expect("valid hostname header"),
        );
        headers.insert(
            HeaderName::from_static("x-signature"),
            sig_b64.parse().expect("valid signature header"),
        );
        headers
    }

    #[tokio::test]
    async fn test_valid_state_update_request_returns_ok_and_calls_insert() {
        let (signing_key, system) = create_test_system("test-host");
        let state = SystemStateBuilder::new()
            .hostname("test-host")
            .change_reason("startup")
            .build();
        let body_bytes = serde_json::to_vec(&state).expect("serialize current state");
        let headers = create_signed_headers("test-host", &body_bytes, &signing_key);
        let body = Bytes::from(body_bytes);

        let lookup = MockLookup::new(Some(system));
        let insert_calls = Arc::new(AtomicUsize::new(0));
        let last_compat = Arc::new(Mutex::new(None));

        let status = update_with_lookup_and_insert(headers, body, &lookup, {
            let insert_calls = Arc::clone(&insert_calls);
            let last_compat = Arc::clone(&last_compat);
            move |_payload, version_compatible| {
                insert_calls.fetch_add(1, Ordering::SeqCst);
                let last_compat = Arc::clone(&last_compat);
                Box::pin(async move {
                    *last_compat.lock().await = Some(version_compatible);
                    Ok(())
                })
            }
        })
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(insert_calls.load(Ordering::SeqCst), 1);
        assert_eq!(*last_compat.lock().await, Some(true));
    }

    #[tokio::test]
    async fn test_invalid_payload_format_returns_bad_request() {
        let (signing_key, system) = create_test_system("test-host");
        let body = Bytes::from_static(b"not valid json");
        let headers = create_signed_headers("test-host", &body, &signing_key);

        let lookup = MockLookup::new(Some(system));
        let insert_calls = Arc::new(AtomicUsize::new(0));

        let status = update_with_lookup_and_insert(headers, body, &lookup, {
            let insert_calls = Arc::clone(&insert_calls);
            move |_payload, _version_compatible| {
                insert_calls.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move { Ok(()) })
            }
        })
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(insert_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_version_compatibility_current_vs_v1_status_codes() {
        let (signing_key, system) = create_test_system("test-host");
        let lookup = MockLookup::new(Some(system));

        let current = SystemStateBuilder::new()
            .hostname("test-host")
            .change_reason("startup")
            .build();
        let current_bytes = serde_json::to_vec(&current).expect("serialize current state");
        let current_headers = create_signed_headers("test-host", &current_bytes, &signing_key);
        let current_status = update_with_lookup_and_insert(
            current_headers,
            Bytes::from(current_bytes),
            &lookup,
            |_payload, _version_compatible| Box::pin(async move { Ok(()) }),
        )
        .await;

        let v1 = SystemStateV1 {
            id: None,
            hostname: "test-host".to_string(),
            context: "agent-startup".to_string(),
            timestamp: None,
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
        let v1_bytes = serde_json::to_vec(&v1).expect("serialize v1 state");
        let v1_headers = create_signed_headers("test-host", &v1_bytes, &signing_key);
        let v1_status = update_with_lookup_and_insert(
            v1_headers,
            Bytes::from(v1_bytes),
            &lookup,
            |_payload, _version_compatible| Box::pin(async move { Ok(()) }),
        )
        .await;

        assert_eq!(current_status, StatusCode::OK);
        assert_eq!(v1_status, StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn test_insert_failure_returns_internal_server_error() {
        let (signing_key, system) = create_test_system("test-host");
        let state = SystemStateBuilder::new()
            .hostname("test-host")
            .change_reason("startup")
            .build();
        let body_bytes = serde_json::to_vec(&state).expect("serialize current state");
        let headers = create_signed_headers("test-host", &body_bytes, &signing_key);

        let lookup = MockLookup::new(Some(system));
        let status = update_with_lookup_and_insert(
            headers,
            Bytes::from(body_bytes),
            &lookup,
            |_payload, _version_compatible| {
                Box::pin(async move { Err(anyhow::anyhow!("simulated insert failure")) })
            },
        )
        .await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }
}
