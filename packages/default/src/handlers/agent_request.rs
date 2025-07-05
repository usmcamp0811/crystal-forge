use crate::models::{system_states::SystemState, system_states::SystemStateV1, systems::System};
use crate::queries::systems::get_by_hostname;
use axum::extract::FromRef;
use axum::{http::HeaderMap, http::StatusCode};
use base64::engine::{Engine, general_purpose}; // Add Engine trait
use bytes::Bytes; // Add this import
use ed25519_dalek::Signature;
use sqlx::PgPool;

pub struct VerifiedAgentRequest {
    pub key_id: String,
    pub signature: Signature,
    pub system: System,
    pub body: Bytes,
}

/// Extract key ID, decode signature, and fetch the system entry.
/// Returns a VerifiedAgentRequest or an appropriate StatusCode error.
pub async fn authenticate_agent_request(
    headers: &HeaderMap,
    body: Bytes,
    pool: &PgPool,
) -> Result<VerifiedAgentRequest, StatusCode> {
    // Changed return type
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

    let system = get_by_hostname(pool, &key_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if system.verifying_key().verify(&body, &signature).is_err() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(VerifiedAgentRequest {
        key_id,
        signature,
        system,
        body,
    })
}

/// Shared server state containing authorized signing keys for current-system auth
#[derive(Clone)]
pub struct CFState {
    pool: PgPool,
}

impl CFState {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl FromRef<CFState> for PgPool {
    fn from_ref(state: &CFState) -> PgPool {
        state.pool.clone()
    }
}

pub fn try_deserialize_system_state(
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
