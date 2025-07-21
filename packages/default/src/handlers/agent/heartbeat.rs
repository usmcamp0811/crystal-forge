use crate::handlers::agent_request::{
    CFState, authenticate_agent_request, try_deserialize_system_state,
};
use crate::models::{
    agent_heartbeats::AgentHeartbeat,
    system_states::{SystemState, SystemStateV1},
};
use crate::queries::{agent_heartbeat::insert_agent_heartbeat, system_states::insert_system_state};
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
pub async fn log(
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

    match AgentHeartbeat::from_system_state_if_heartbeat(&payload, &pool).await {
        Ok(heartbeat) => {
            // This is a heartbeat - insert to heartbeats table
            if let Err(e) = insert_agent_heartbeat(&pool, &heartbeat).await {
                debug!("❌ failed to insert heartbeat: {e:?}");
                return StatusCode::INTERNAL_SERVER_ERROR;
            }
            info!("💓 Heartbeat recorded for {}", payload.hostname);
        }
        Err(_state_change_reason) => {
            info!("🔍 Heartbeat became state change: {}", _state_change_reason);
            // State changed - insert full state record
            if let Err(e) = insert_system_state(&pool, &payload, version_compatible).await {
                debug!("❌ failed to insert system state: {e:?}");
                return StatusCode::INTERNAL_SERVER_ERROR;
            }
            info!("📊 State change recorded for {}", payload.hostname);
        }
    }

    // Return different status codes based on compatibility
    if version_compatible {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED // 202 - accepted but agent should upgrade
    }
}
