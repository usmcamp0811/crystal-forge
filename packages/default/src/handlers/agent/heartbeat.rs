use crate::handlers::agent_request::{
    CFState, authenticate_agent_request, deserialize_system_state_versioned,
};
use crate::models::agent_heartbeats::AgentHeartbeat;
use crate::models::cache_destination::CacheDestination;
use crate::queries::cache_destinations::{get_caches_for_environment, get_global_caches};
use crate::queries::systems::{
    BootIdChange, deactivate_duplicate_active_systems_by_public_key,
    get_agent_desired_target_by_hostname, get_system_heartbeat_interval_secs, update_boot_id_tx,
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
    /// Interval in seconds the agent should sleep between heartbeats.
    /// Absent when the server cannot determine the value; agent falls back to 600s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heartbeat_interval_secs: Option<u64>,
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

/// Best-effort result handler for duplicate-active-system cleanup.
///
/// Returns deactivated hostnames when successful, or empty vector on error.
fn handle_duplicate_active_system_cleanup_result(
    current_hostname: &str,
    result: anyhow::Result<Vec<String>>,
) -> Vec<String> {
    match result {
        Ok(deactivated) if !deactivated.is_empty() => {
            warn!(
                current_hostname = %current_hostname,
                duplicate_hostnames = ?deactivated,
                "Auto-deactivated duplicate active systems sharing agent public key"
            );
            deactivated
        }
        Ok(_) => Vec::new(),
        Err(e) => {
            // Non-fatal: do not reject heartbeat if de-duplication fails.
            warn!(
                current_hostname = %current_hostname,
                error = ?e,
                "Failed to auto-deactivate duplicate active systems; continuing heartbeat processing"
            );
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
    let _ = handle_duplicate_active_system_cleanup_result(
        &agent_request.system.hostname,
        deactivate_duplicate_active_systems_by_public_key(
            &pool,
            &agent_request.system.hostname,
            &public_key_base64,
        )
        .await,
    );

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

    // Classify heartbeat vs state change (read-only; safe outside the transaction).
    let heartbeat_or_state = AgentHeartbeat::from_system_state_if_heartbeat(&payload, &pool).await;

    // P2-6 (atomic): the boot_id update and the heartbeat/state insert share one
    // transaction. If the insert fails and we return an error, the boot_id write
    // rolls back too — so the agent's retry still observes the boot_id change and
    // the reboot event is not lost.
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            debug!("❌ failed to begin heartbeat transaction: {e:?}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let boot_id_change = if let Some(ref new_boot_id) = payload.boot_id {
        match update_boot_id_tx(&mut tx, &payload.hostname, new_boot_id).await {
            Ok(change) => Some(change),
            Err(e) => {
                debug!("❌ failed to update boot_id for {}: {e:?}", payload.hostname);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    } else {
        None // Older agent that does not send boot_id
    };

    match &heartbeat_or_state {
        Ok(heartbeat) => {
            // This is a heartbeat - insert to heartbeats table
            if let Err(e) = insert_agent_heartbeat(&mut *tx, heartbeat).await {
                debug!("❌ failed to insert heartbeat: {e:?}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
        Err(state_change_reason) => {
            info!("🔍 Heartbeat became state change: {}", state_change_reason);
            // State changed - insert full state record
            if let Err(e) = insert_system_state(&mut *tx, &payload, version_compatible).await {
                debug!("❌ failed to insert system state: {e:?}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }

    if let Err(e) = tx.commit().await {
        debug!("❌ failed to commit heartbeat transaction: {e:?}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    // Log only after the commit so we never report events that rolled back.
    match boot_id_change {
        Some(BootIdChange::Changed) => {
            info!(
                "🔄 System reboot detected for {} (boot_id changed)",
                payload.hostname
            );
        }
        Some(BootIdChange::Initialized) => {
            debug!(
                "boot_id initialized for {} (first heartbeat with boot_id; not a reboot)",
                payload.hostname
            );
        }
        Some(BootIdChange::Unchanged) | None => {}
    }
    match heartbeat_or_state {
        Ok(_) => info!("💓 Heartbeat recorded for {}", payload.hostname),
        Err(_) => info!("📊 State change recorded for {}", payload.hostname),
    }

    // Fetch desired target for this system. Manual systems only receive fresh,
    // explicit one-shot targets; stale manual targets are suppressed so agents
    // cannot revert hosts after an out-of-band/manual nixos-rebuild.
    let desired_target =
        match get_agent_desired_target_by_hostname(&pool, &agent_request.system.hostname).await {
            Ok(target) => target,
            Err(e) => {
                debug!("❌ Failed to fetch desired target: {e:?}");
                None // Continue with None if query fails
            }
        };

    let runtime_caches =
        load_runtime_caches_for_agent(&pool, agent_request.system.environment_id).await;

    // Resolve per-system heartbeat interval, falling back to server-config default.
    let heartbeat_interval_secs = {
        let per_system = get_system_heartbeat_interval_secs(&pool, &agent_request.system.hostname)
            .await
            .unwrap_or_else(|e| {
                debug!("Failed to fetch heartbeat_interval_secs for {}: {e}", agent_request.system.hostname);
                None
            });
        let interval = per_system
            .map(|v| v as u64)
            .unwrap_or(state.server_config.heartbeat_interval_secs);
        Some(interval)
    };

    let response = LogResponse {
        desired_target,
        runtime_caches,
        heartbeat_interval_secs,
    };

    // Return JSON response with appropriate status
    let status = if version_compatible {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED // 202 - accepted but agent should upgrade
    };

    (status, axum::Json(response)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn handle_duplicate_cleanup_returns_deactivated_hosts_on_success() {
        let host = "nix-builder";
        let deactivated =
            handle_duplicate_active_system_cleanup_result(host, Ok(vec!["base".to_string()]));

        assert_eq!(deactivated, vec!["base".to_string()]);
    }

    #[tokio::test]
    async fn handle_duplicate_cleanup_is_non_fatal_on_error() {
        let host = "nix-builder";
        let deactivated = handle_duplicate_active_system_cleanup_result(
            host,
            Err(anyhow::anyhow!("db unavailable")),
        );

        assert!(
            deactivated.is_empty(),
            "cleanup errors must be non-fatal and return empty deactivation set"
        );
    }
}
