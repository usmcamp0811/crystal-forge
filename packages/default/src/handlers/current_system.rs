use crate::db::get_db_client;
use axum::extract::FromRef;

use crate::handlers::webhook::webhook_handler;
use crate::models::systems::SystemState;
use crate::queries::system_states::insert_system_state;
use anyhow::Result;
use base64::engine::Engine;
use base64::engine::general_purpose;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sqlx::PgPool;
use std::collections::HashMap;

use axum::{
    Json, Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
};
use serde::de;
use serde_json::Value;
use std::{future::Future, pin::Pin, sync::Arc};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

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

    // Deserialize JSON payload
    let payload: SystemState = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "❌ JSON deserialization failed: {e}\nBody:\n{}",
                String::from_utf8_lossy(&body)
            );
            return StatusCode::BAD_REQUEST;
        }
    };

    info!(
        "✅ accepted from {}: hostname={}, hash={}, context={}",
        key_id,
        payload.hostname,
        payload.system_derivation_id.as_deref().unwrap_or("unknown"),
        payload.context
    );

    // Insert system state into DB

    if let Err(e) = insert_system_state(&pool, &payload).await {
        eprintln!("❌ failed to insert into DB: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    StatusCode::OK
}
