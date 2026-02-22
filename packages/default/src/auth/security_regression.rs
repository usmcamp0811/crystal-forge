use crate::auth::oidc::{ClaimExtractor, IdTokenClaims, JwtValidator};
use crate::config::ClaimMappingConfig;
use crate::handlers::agent_request::{
    SystemLookup, authenticate_agent_request_with_lookup,
};
use crate::handlers::api::auth_oidc::OidcError;
use crate::models::public_key::PublicKey;
use crate::models::systems::System;
use anyhow::Result;
use axum::body::Bytes;
use axum::http::HeaderMap;
use axum::http::HeaderName;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use base64::Engine;
use base64::engine::general_purpose;
use chrono::Utc;
use ed25519_dalek::Signer;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use openidconnect::core::CoreJsonWebKeySet;
use serde_json::json;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Clone)]
struct MockLookup {
    system: Option<System>,
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

fn test_system(hostname: &str, key: ed25519_dalek::VerifyingKey) -> System {
    System {
        id: Uuid::new_v4(),
        hostname: hostname.to_string(),
        environment_id: None,
        is_active: true,
        public_key: PublicKey::from_verifying_key(key),
        flake_id: Some(1),
        derivation: "/nix/store/test-system".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        desired_target: None,
        deployment_policy: "manual".to_string(),
    }
}

#[test]
fn security_oidc_error_unverified_email_maps_to_forbidden() {
    let response = OidcError::UnverifiedEmail.into_response();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[test]
fn security_role_mapping_failure_uses_empty_roles() {
    let mut claims = HashMap::new();
    claims.insert("roles".to_string(), json!({"unexpected": "shape"}));

    let config = ClaimMappingConfig {
        roles_claim: "roles".to_string(),
        ..ClaimMappingConfig::default()
    };

    let extractor = ClaimExtractor::new(config);
    let user_info = extractor
        .extract_user_info(None, None, None, None, None, None, &claims, "subject-2".to_string())
        .unwrap();

    assert!(user_info.roles.is_empty());
}

#[test]
fn security_token_validation_rejects_non_rsa_algorithm() {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
    let claims = IdTokenClaims {
        iss: "https://issuer.example.com".to_string(),
        sub: "user-1".to_string(),
        aud: vec!["cf-client".to_string()],
        azp: None,
        exp: now + 600,
        iat: now,
        email: Some("user@example.com".to_string()),
        email_verified: Some(true),
        name: None,
        given_name: None,
        family_name: None,
        preferred_username: None,
        custom_claims: HashMap::new(),
    };

    let mut header = Header::new(Algorithm::HS256);
    header.kid = Some("test-kid".to_string());

    let token = jsonwebtoken::encode(&header, &claims, &EncodingKey::from_secret(b"secret")).unwrap();
    let validator = JwtValidator::new(
        "https://issuer.example.com".to_string(),
        "cf-client".to_string(),
    );
    let jwks: CoreJsonWebKeySet = serde_json::from_value(json!({"keys": []})).unwrap();

    let err = validator.validate_id_token(&token, &jwks).unwrap_err();
    assert!(err.to_string().contains("not allowed"));
}

#[tokio::test]
async fn security_agent_key_auth_path_accepts_valid_signature() {
    let mut rng = rand::rngs::OsRng;
    let signing_key = ed25519_dalek::SigningKey::generate(&mut rng);
    let verifying_key = signing_key.verifying_key();

    let hostname = "agent-host";
    let body = Bytes::from_static(b"agent heartbeat payload");
    let signature = signing_key.sign(&body);

    let mut headers = HeaderMap::new();
    headers.insert(HeaderName::from_static("x-key-id"), hostname.parse().unwrap());
    headers.insert(
        HeaderName::from_static("x-signature"),
        general_purpose::STANDARD
            .encode(signature.to_bytes())
            .parse()
            .unwrap(),
    );

    let lookup = MockLookup {
        system: Some(test_system(hostname, verifying_key)),
    };

    let result = authenticate_agent_request_with_lookup(&headers, body.clone(), &lookup).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap().key_id, hostname);
}

#[tokio::test]
async fn security_agent_key_auth_path_rejects_tampered_body() {
    let mut rng = rand::rngs::OsRng;
    let signing_key = ed25519_dalek::SigningKey::generate(&mut rng);
    let verifying_key = signing_key.verifying_key();

    let hostname = "agent-host";
    let original_body = b"original payload";
    let signature = signing_key.sign(original_body);

    let mut headers = HeaderMap::new();
    headers.insert(HeaderName::from_static("x-key-id"), hostname.parse().unwrap());
    headers.insert(
        HeaderName::from_static("x-signature"),
        general_purpose::STANDARD
            .encode(signature.to_bytes())
            .parse()
            .unwrap(),
    );

    let lookup = MockLookup {
        system: Some(test_system(hostname, verifying_key)),
    };

    let tampered_body = Bytes::from_static(b"tampered payload");
    let result = authenticate_agent_request_with_lookup(&headers, tampered_body, &lookup).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), StatusCode::UNAUTHORIZED);
}
