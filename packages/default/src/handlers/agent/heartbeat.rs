use crate::handlers::agent_request::{
    CFState, authenticate_agent_request, deserialize_system_state_versioned,
};
use crate::models::agent_heartbeats::AgentHeartbeat;
use crate::models::cache_destination::CacheDestination;
use crate::queries::cache_destinations::{get_caches_for_environment, get_global_caches};
use crate::queries::systems::{
    deactivate_duplicate_active_systems_by_public_key, get_desired_target_by_hostname,
};
use crate::queries::{agent_heartbeat::insert_agent_heartbeat, system_states::insert_system_state};
use axum::response::Response;
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::Deserialize;
use serde::Serialize;
use sqlx::PgPool;
use tracing::{debug, info, warn};

#[derive(Serialize, Deserialize)]
pub struct RuntimeCacheConfig {
    pub cache_type: String,
    pub cache_url: String,
    pub cache_public_key: Option<String>,
    pub attic_cache_name: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct LogResponse {
    pub desired_target: Option<String>,
    #[serde(default)]
    pub runtime_caches: Vec<RuntimeCacheConfig>,
}

fn destination_to_runtime_cache(destination: CacheDestination) -> Option<RuntimeCacheConfig> {
    let cache_url = destination.push_to?;
    Some(RuntimeCacheConfig {
        cache_type: destination.cache_type,
        cache_url,
        cache_public_key: destination.attic_public_key,
        attic_cache_name: destination.attic_cache_name,
    })
}

async fn load_runtime_caches_for_agent(
    pool: &PgPool,
    environment_id: Option<uuid::Uuid>,
) -> Vec<RuntimeCacheConfig> {
    let destinations = match environment_id {
        Some(env_id) => get_caches_for_environment(pool, env_id).await,
        None => get_global_caches(pool).await,
    };

    match destinations {
        Ok(dests) => dests
            .into_iter()
            .filter_map(destination_to_runtime_cache)
            .collect(),
        Err(e) => {
            debug!("❌ Failed to load runtime cache config for agent: {e:?}");
            Vec::new()
        }
    }
}
/// Handles the `/current-system` POST route.
/// Verifies the body signature using headers, parses the payload, and
/// stores system state info in the database.
pub async fn log(
    State(state): State<CFState>,
    State(pool): State<PgPool>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Get verified agent request
    let agent_request = match authenticate_agent_request(&headers, body, &pool).await {
        Ok(req) => req,
        Err(status) => return status.into_response(),
    };

    // Hotfix: if the same public key appears on multiple active hostnames,
    // deactivate the stale duplicates and keep only the authenticated hostname active.
    // This prevents renamed/re-joined hosts from leaving old active rows that skew health.
    let public_key_base64 = agent_request.system.public_key.to_base64();
    match deactivate_duplicate_active_systems_by_public_key(
        &pool,
        &agent_request.system.hostname,
        &public_key_base64,
    )
    .await
    {
        Ok(deactivated) if !deactivated.is_empty() => {
            warn!(
                current_hostname = %agent_request.system.hostname,
                duplicate_hostnames = ?deactivated,
                "Auto-deactivated duplicate active systems sharing agent public key"
            );
        }
        Ok(_) => {}
        Err(e) => {
            // Non-fatal: do not reject heartbeat if de-duplication fails.
            warn!(
                current_hostname = %agent_request.system.hostname,
                error = ?e,
                "Failed to auto-deactivate duplicate active systems; continuing heartbeat processing"
            );
        }
    }

    // Try to deserialize with version detection
    let (payload, version_compatible) = match deserialize_system_state_versioned(&agent_request) {
        Ok((state, compatible)) => (state, compatible),
        Err(e) => {
            debug!("❌ All deserialization attempts failed: {e}");
            return StatusCode::BAD_REQUEST.into_response();
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
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            info!("💓 Heartbeat recorded for {}", payload.hostname);
        }
        Err(_state_change_reason) => {
            info!("🔍 Heartbeat became state change: {}", _state_change_reason);
            // State changed - insert full state record
            if let Err(e) = insert_system_state(&pool, &payload, version_compatible).await {
                debug!("❌ failed to insert system state: {e:?}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            info!("📊 State change recorded for {}", payload.hostname);
        }
    }

    // Fetch desired target for this system
    let desired_target =
        match get_desired_target_by_hostname(&pool, &agent_request.system.hostname).await {
            Ok(target) => target,
            Err(e) => {
                debug!("❌ Failed to fetch desired target: {e:?}");
                None // Continue with None if query fails
            }
        };

    let runtime_caches =
        load_runtime_caches_for_agent(&pool, agent_request.system.environment_id).await;
    let response = LogResponse {
        desired_target,
        runtime_caches,
    };

    // Return JSON response with appropriate status
    let status = if version_compatible {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED // 202 - accepted but agent should upgrade
    };

    (status, axum::Json(response)).into_response()
}
