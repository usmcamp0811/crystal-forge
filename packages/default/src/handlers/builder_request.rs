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

/// Authenticate a builder API request.
///
/// # Headers
///
/// - `X-Builder-ID`: UUID of the builder
/// - `X-Signature`: Base64-encoded Ed25519 signature of the request body
///
/// # Errors
///
/// Returns `StatusCode::UNAUTHORIZED` for:
/// - Missing X-Builder-ID header
/// - Missing X-Signature header
/// - Invalid builder ID format
/// - Unknown builder (no matching registration)
/// - Invalid signature verification
/// - Builder status is not 'active'
///
/// Returns `StatusCode::BAD_REQUEST` for:
/// - Malformed signature (invalid base64 or wrong length)
///
/// Returns `StatusCode::INTERNAL_SERVER_ERROR` for:
/// - Database errors during lookup
pub async fn authenticate_builder_request_with_lookup<L: BuilderLookup>(
    headers: &HeaderMap,
    body: Bytes,
    lookup: &L,
) -> Result<VerifiedBuilderRequest, StatusCode> {
    authenticate_builder_request_with_lookup_options(headers, body, lookup, false).await
}

async fn authenticate_builder_request_with_lookup_options<L: BuilderLookup>(
    headers: &HeaderMap,
    body: Bytes,
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

    // Verify the signature
    if builder
        .public_key
        .verifying_key()
        .verify(&body, &signature)
        .is_err()
    {
        warn!(
            builder_id = %builder.id,
            public_key = %builder.public_key.to_base64(),
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
    pool: &PgPool,
) -> Result<VerifiedBuilderRequest, StatusCode> {
    authenticate_builder_request_with_lookup(headers, body, pool).await
}

/// Production entry point that allows inactive builders to authenticate.
/// Intended only for bootstrap paths (e.g. first heartbeat).
pub async fn authenticate_builder_request_allow_inactive(
    headers: &HeaderMap,
    body: Bytes,
    pool: &PgPool,
) -> Result<VerifiedBuilderRequest, StatusCode> {
    authenticate_builder_request_with_lookup_options(headers, body, pool, true).await
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
            public_key: public_key_base64,
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: None,
            environment_ids: vec![],
        };

        let builder = create_builder(&pool, &request)
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

        // Sign the body
        let signature = signing_key.sign(&body);
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

        // Authenticate
        let result = authenticate_builder_request(&headers, body.clone(), &pool).await;

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
            public_key: public_key_base64,
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: None,
            environment_ids: vec![],
        };

        let builder = create_builder(&pool, &request)
            .await
            .expect("Failed to create builder");

        // Builder starts as inactive - don't activate it

        let body = Bytes::from("test request body");
        let signature = signing_key.sign(&body);
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

        // Authenticate - should fail because builder is inactive
        let result = authenticate_builder_request(&headers, body, &pool).await;

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
            public_key: public_key_base64,
            max_cpu_cores: None,
            max_memory_mb: None,
            max_concurrent_jobs: None,
            environment_ids: vec![],
        };

        let builder = create_builder(&pool, &request)
            .await
            .expect("Failed to create builder");

        sqlx::query("UPDATE builders SET status = 'active' WHERE id = $1")
            .bind(builder.id)
        .execute(&pool)
        .await
        .expect("Failed to activate builder");

        let body = Bytes::from("test request body");

        // Use a different key to sign (wrong signature)
        let wrong_signing_key = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let signature = wrong_signing_key.sign(&body);
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

        // Authenticate - should fail due to invalid signature
        let result = authenticate_builder_request(&headers, body, &pool).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_authenticate_builder_request_missing_headers() {
        let pool = test_pool().await;
        let body = Bytes::from("test");

        // Missing both headers
        let headers = HeaderMap::new();
        let result = authenticate_builder_request(&headers, body.clone(), &pool).await;
        assert_eq!(result.unwrap_err(), StatusCode::UNAUTHORIZED);

        // Missing signature
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Builder-ID",
            HeaderValue::from_str(&Uuid::new_v4().to_string()).unwrap(),
        );
        let result = authenticate_builder_request(&headers, body, &pool).await;
        assert_eq!(result.unwrap_err(), StatusCode::UNAUTHORIZED);
    }
}
