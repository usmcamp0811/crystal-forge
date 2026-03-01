//! Builder request authentication using Ed25519 signatures.
//!
//! This module provides authentication for builder API requests using the same
//! pattern as agent authentication:
//! - Builder signs request body with its private Ed25519 key
//! - Server verifies signature using builder's registered public key
//! - X-Builder-ID header contains the builder UUID
//! - X-Signature header contains the base64-encoded signature

use axum::http::{HeaderMap, StatusCode};
use base64::engine::{Engine, general_purpose};
use bytes::Bytes;
use ed25519_dalek::{Signature, Verifier};
use sqlx::PgPool;
use std::future::Future;
use std::pin::Pin;
use tracing::warn;
use uuid::Uuid;

use crate::models::builders::Builder;
use crate::queries::builders::get_builder_by_id;

#[derive(Debug)]
pub struct VerifiedBuilderRequest {
    pub builder_id: Uuid,
    pub signature: Signature,
    pub builder: Builder,
    pub body: Bytes,
}

/// Trait for looking up builders by ID.
///
/// This abstraction enables testing the authentication logic without a real database.
pub trait BuilderLookup: Clone + Send + Sync + 'static {
    fn lookup(
        &self,
        builder_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Option<Builder>>> + Send>>;
}

/// Implementation of BuilderLookup for PgPool (production use).
impl BuilderLookup for PgPool {
    fn lookup(
        &self,
        builder_id: Uuid,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Option<Builder>>> + Send>> {
        let pool = self.clone();
        Box::pin(async move { get_builder_by_id(&pool, &builder_id).await })
    }
}

/// Authenticate a builder API request with replay resistance.
///
/// # Headers
///
/// - `X-Builder-ID`: UUID of the builder
/// - `X-Signature`: Base64-encoded Ed25519 signature of the signed payload
/// - `X-Timestamp`: ISO 8601 timestamp (RFC 3339 format)
///
/// # Signed Payload Format
///
/// The signature is computed over: `{method}\n{path}\n{timestamp}\n{body}`
///
/// Example:
/// ```text
/// POST
/// /api/v1/builders/123/heartbeat
/// 2026-03-01T02:30:00Z
/// {"status":"active"}
/// ```
///
/// # Replay Resistance
///
/// - Timestamp must be within ±5 minutes of server time
/// - Signature binds to specific method + path (prevents cross-endpoint reuse)
///
/// # Errors
///
/// Returns `StatusCode::UNAUTHORIZED` for:
/// - Missing required headers (X-Builder-ID, X-Signature, X-Timestamp)
/// - Invalid builder ID format
/// - Unknown builder (no matching registration)
/// - Invalid signature verification
/// - Builder status is not 'active'
/// - Timestamp outside freshness window (replay attack detected)
///
/// Returns `StatusCode::BAD_REQUEST` for:
/// - Malformed signature (invalid base64 or wrong length)
/// - Malformed timestamp (invalid ISO 8601 format)
///
/// Returns `StatusCode::INTERNAL_SERVER_ERROR` for:
/// - Database errors during lookup
pub async fn authenticate_builder_request_with_lookup<L: BuilderLookup>(
    headers: &HeaderMap,
    body: Bytes,
    method: &str,
    path: &str,
    lookup: &L,
) -> Result<VerifiedBuilderRequest, StatusCode> {
    authenticate_builder_request_with_lookup_options(headers, body, method, path, lookup, false).await
}

async fn authenticate_builder_request_with_lookup_options<L: BuilderLookup>(
    headers: &HeaderMap,
    body: Bytes,
    method: &str,
    path: &str,
    lookup: &L,
    allow_inactive: bool,
) -> Result<VerifiedBuilderRequest, StatusCode> {
    // Extract builder ID from header
    let builder_id_str = headers
        .get("X-Builder-ID")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let builder_id = Uuid::parse_str(builder_id_str)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Extract timestamp from header (required for replay resistance)
    let timestamp_str = headers
        .get("X-Timestamp")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Parse timestamp (ISO 8601 / RFC 3339 format)
    let request_timestamp = chrono::DateTime::parse_from_rfc3339(timestamp_str)
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .with_timezone(&chrono::Utc);

    // Enforce freshness window (±5 minutes) to prevent replay attacks
    let now = chrono::Utc::now();
    let time_diff = (now - request_timestamp).num_seconds().abs();
    const FRESHNESS_WINDOW_SECS: i64 = 5 * 60; // 5 minutes

    if time_diff > FRESHNESS_WINDOW_SECS {
        warn!(
            builder_id = builder_id_str,
            request_timestamp = %request_timestamp,
            server_time = %now,
            diff_secs = time_diff,
            "builder auth rejected: timestamp outside freshness window (possible replay attack)"
        );
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Extract signature from header
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

    // Look up the builder
    let builder = lookup
        .lookup(builder_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let allowed = match builder.status {
        crate::models::builders::BuilderStatus::Active => true,
        crate::models::builders::BuilderStatus::Inactive => allow_inactive,
        crate::models::builders::BuilderStatus::Offline => false,
    };

    if !allowed {
        warn!(
            builder_id = %builder.id,
            status = ?builder.status,
            "builder auth rejected: builder is not active"
        );
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Construct signed payload: method\npath\ntimestamp\nbody
    // This binds the signature to specific endpoint and prevents replay across endpoints
    let body_str = std::str::from_utf8(&body).unwrap_or("");
    let signed_payload = format!("{}\n{}\n{}\n{}", method, path, timestamp_str, body_str);

    // Verify the signature against the full signed payload
    if builder
        .public_key
        .verifying_key()
        .verify(signed_payload.as_bytes(), &signature)
        .is_err()
    {
        warn!(
            builder_id = %builder.id,
            public_key = %builder.public_key.to_base64(),
            method = method,
            path = path,
            "builder auth rejected: signature verification failed"
        );
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(VerifiedBuilderRequest {
        builder_id,
        signature,
        builder,
        body,
    })
}

/// Production entry point using PgPool directly.
pub async fn authenticate_builder_request(
    headers: &HeaderMap,
    body: Bytes,
    method: &str,
    path: &str,
    pool: &PgPool,
) -> Result<VerifiedBuilderRequest, StatusCode> {
    authenticate_builder_request_with_lookup(headers, body, method, path, pool).await
}

/// Production entry point that allows inactive builders to authenticate.
/// Intended only for bootstrap paths (e.g. first heartbeat).
pub async fn authenticate_builder_request_allow_inactive(
    headers: &HeaderMap,
    body: Bytes,
    method: &str,
    path: &str,
    pool: &PgPool,
) -> Result<VerifiedBuilderRequest, StatusCode> {
    authenticate_builder_request_with_lookup_options(headers, body, method, path, pool, true).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use ed25519_dalek::Signer;

    use crate::models::builders::{Builder, BuilderStatus, CreateBuilderRequest};
    use crate::models::public_key::PublicKey;
    use crate::queries::builders::create_builder;
    use crate::test_utils::db::test_pool;

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_authenticate_builder_request_success() {
        let pool = test_pool().await;

        // Create a test builder with a real keypair
        let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let verifying_key = signing_key.verifying_key();
        let public_key_base64 = general_purpose::STANDARD.encode(verifying_key.to_bytes());

        let request = CreateBuilderRequest {
            name: "test-auth-builder".to_string(),
            public_key: Some(public_key_base64),
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: None,
            environment_ids: vec![],
        };

        let (builder, _private_key) = create_builder(&pool, &request)
            .await
            .expect("Failed to create builder");

        // Activate the builder (initial status is inactive)
        sqlx::query("UPDATE builders SET status = 'active' WHERE id = $1")
            .bind(builder.id)
        .execute(&pool)
        .await
        .expect("Failed to activate builder");

        // Create a test request body
        let body = Bytes::from("test request body");

        // Create timestamp
        let timestamp = chrono::Utc::now().to_rfc3339();

        // Create signature payload: {method}\n{path}\n{timestamp}\n{body}
        let method = "POST";
        let path = "/api/v1/test";
        let signature_payload = format!("{}\n{}\n{}\n{}", method, path, timestamp, String::from_utf8_lossy(&body));
        let signature = signing_key.sign(signature_payload.as_bytes());
        let signature_base64 = general_purpose::STANDARD.encode(signature.to_bytes());

        // Create headers
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Builder-ID",
            HeaderValue::from_str(&builder.id.to_string()).unwrap(),
        );
        headers.insert(
            "X-Signature",
            HeaderValue::from_str(&signature_base64).unwrap(),
        );
        headers.insert(
            "X-Timestamp",
            HeaderValue::from_str(&timestamp).unwrap(),
        );

        // Authenticate
        let result = authenticate_builder_request(&headers, body.clone(), method, path, &pool).await;

        assert!(result.is_ok());
        let verified = result.unwrap();
        assert_eq!(verified.builder_id, builder.id);
        assert_eq!(verified.body, body);
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_authenticate_builder_request_inactive_builder() {
        let pool = test_pool().await;

        let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let verifying_key = signing_key.verifying_key();
        let public_key_base64 = general_purpose::STANDARD.encode(verifying_key.to_bytes());

        let request = CreateBuilderRequest {
            name: "inactive-builder".to_string(),
            public_key: Some(public_key_base64),
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: None,
            environment_ids: vec![],
        };

        let (builder, _private_key) = create_builder(&pool, &request)
            .await
            .expect("Failed to create builder");

        // Builder starts as inactive - don't activate it

        let body = Bytes::from("test request body");
        let timestamp = chrono::Utc::now().to_rfc3339();
        let method = "POST";
        let path = "/api/v1/test";
        let signature_payload = format!("{}\n{}\n{}\n{}", method, path, timestamp, String::from_utf8_lossy(&body));
        let signature = signing_key.sign(signature_payload.as_bytes());
        let signature_base64 = general_purpose::STANDARD.encode(signature.to_bytes());

        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Builder-ID",
            HeaderValue::from_str(&builder.id.to_string()).unwrap(),
        );
        headers.insert(
            "X-Signature",
            HeaderValue::from_str(&signature_base64).unwrap(),
        );
        headers.insert(
            "X-Timestamp",
            HeaderValue::from_str(&timestamp).unwrap(),
        );

        // Authenticate - should fail because builder is inactive
        let result = authenticate_builder_request(&headers, body, method, path, &pool).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    #[ignore = "requires running test database"]
    async fn test_authenticate_builder_request_invalid_signature() {
        let pool = test_pool().await;

        let signing_key = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let verifying_key = signing_key.verifying_key();
        let public_key_base64 = general_purpose::STANDARD.encode(verifying_key.to_bytes());

        let request = CreateBuilderRequest {
            name: "invalid-sig-builder".to_string(),
            public_key: Some(public_key_base64),
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: None,
            environment_ids: vec![],
        };

        let (builder, _private_key) = create_builder(&pool, &request)
            .await
            .expect("Failed to create builder");

        sqlx::query("UPDATE builders SET status = 'active' WHERE id = $1")
            .bind(builder.id)
        .execute(&pool)
        .await
        .expect("Failed to activate builder");

        let body = Bytes::from("test request body");
        let timestamp = chrono::Utc::now().to_rfc3339();
        let method = "POST";
        let path = "/api/v1/test";
        let signature_payload = format!("{}\n{}\n{}\n{}", method, path, timestamp, String::from_utf8_lossy(&body));

        // Use a different key to sign (wrong signature)
        let wrong_signing_key = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let signature = wrong_signing_key.sign(signature_payload.as_bytes());
        let signature_base64 = general_purpose::STANDARD.encode(signature.to_bytes());

        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Builder-ID",
            HeaderValue::from_str(&builder.id.to_string()).unwrap(),
        );
        headers.insert(
            "X-Signature",
            HeaderValue::from_str(&signature_base64).unwrap(),
        );
        headers.insert(
            "X-Timestamp",
            HeaderValue::from_str(&timestamp).unwrap(),
        );

        // Authenticate - should fail due to invalid signature
        let result = authenticate_builder_request(&headers, body, method, path, &pool).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_authenticate_builder_request_missing_headers() {
        let pool = test_pool().await;
        let body = Bytes::from("test");
        let method = "POST";
        let path = "/api/v1/test";

        // Missing both headers
        let headers = HeaderMap::new();
        let result = authenticate_builder_request(&headers, body.clone(), method, path, &pool).await;
        assert_eq!(result.unwrap_err(), StatusCode::UNAUTHORIZED);

        // Missing signature
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Builder-ID",
            HeaderValue::from_str(&Uuid::new_v4().to_string()).unwrap(),
        );
        let result = authenticate_builder_request(&headers, body, method, path, &pool).await;
        assert_eq!(result.unwrap_err(), StatusCode::UNAUTHORIZED);
    }
}
