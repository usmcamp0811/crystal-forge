use crate::handlers::agent_request::{
    CFState, authenticate_agent_request, deserialize_agent_capabilities,
    deserialize_system_state_versioned,
};
use crate::models::agent_heartbeats::AgentHeartbeat;
use crate::models::cache_destination::CacheDestination;
use crate::queries::cache_destinations::eligible_cache_destinations_for_environment;
use crate::queries::systems::{
    BootIdChange, deactivate_duplicate_active_systems_by_public_key,
    get_agent_desired_target_by_hostname, get_system_heartbeat_interval_secs, update_boot_id_tx,
    update_restart_type_tx,
};
use crate::queries::{
    agent_heartbeat::insert_agent_heartbeat,
    system_events::{lock_observed_system_state_by_hostname_tx, record_report_events_tx},
    system_states::insert_system_state,
};
use axum::response::Response;
use axum::{
    body::Bytes,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use sqlx::PgPool;
use tracing::{debug, info, warn};
use uuid::Uuid;

// Re-export protocol types so agent binary imports remain backward-compatible:
//   use crystal_forge::handlers::agent::heartbeat::{LogResponse, RuntimeCacheConfig};
pub use cf_protocol::agent::{LogResponse, RuntimeCacheConfig};

fn destination_to_runtime_cache(
    destination: CacheDestination,
    confidential: bool,
) -> Option<RuntimeCacheConfig> {
    if !destination.enabled {
        return None;
    }
    let (cache_url, cache_public_keys, read_auth) = destination.read_config().ok()?;
    // SECURITY: Agent signatures authenticate requests, not confidentiality.
    // Never downgrade a private cache to a public read on insecure transport.
    if !confidential && !matches!(read_auth, cf_protocol::cache::CacheReadAuth::None) {
        return None;
    }
    Some(RuntimeCacheConfig {
        cache_type: destination.cache_type,
        cache_url,
        cache_public_key: destination.attic_public_key,
        attic_cache_name: destination.attic_cache_name,
        cache_public_keys,
        read_auth,
    })
}

async fn load_runtime_caches_for_agent(
    pool: &PgPool,
    environment_id: Option<uuid::Uuid>,
    confidential: bool,
    capabilities: cf_protocol::agent::AgentCapabilities,
) -> anyhow::Result<Vec<RuntimeCacheConfig>> {
    selected_runtime_cache(
        eligible_cache_destinations_for_environment(pool, environment_id).await?,
        confidential,
        capabilities,
    )
}

// INVARIANT: Select before checking capability or transport. Failure must not
// retarget a deployment to another eligible cache or local/static settings.
fn selected_runtime_cache(
    destinations: Vec<CacheDestination>,
    confidential: bool,
    capabilities: cf_protocol::agent::AgentCapabilities,
) -> anyhow::Result<Vec<RuntimeCacheConfig>> {
    let Some(selected) = destinations.into_iter().next() else {
        return Ok(Vec::new());
    };
    anyhow::ensure!(
        selected.cache_type != "Niks3" || capabilities.supports_niks3,
        "Agent does not support the selected cache type"
    );
    let runtime = destination_to_runtime_cache(selected, confidential)
        .ok_or_else(|| anyhow::anyhow!("Selected cache read settings cannot be delivered"))?;
    Ok(vec![runtime])
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
/// Handles `/agent/heartbeat` and delivers read-only cache settings.
/// Verifies the body signature using headers, parses the payload, and
/// stores system state info in the database.
/// Delivers only the canonical first selected cache. Niks3 requires capability
/// in the verified body. Private reads require a trusted HTTPS proxy. A selected
/// cache failure suppresses the target before claiming a pending deployment;
/// heartbeat ingestion still commits and the deployment remains retryable.
pub async fn log(
    State(state): State<CFState>,
    State(pool): State<PgPool>,
    peer: Option<ConnectInfo<std::net::SocketAddr>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Get verified agent request
    let agent_request = match authenticate_agent_request(&headers, body, &pool).await {
        Ok(req) => req,
        Err(status) => return status.into_response(),
    };
    let capabilities = match deserialize_agent_capabilities(&agent_request) {
        Ok(capabilities) => capabilities,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
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

    // CONCURRENCY: Snapshot publication, deployment creation, and agent state
    // ingestion acquire this lock before POA&M and system-row locks.
    if let Err(e) = crate::queries::evaluation_snapshots::lock_snapshot_writer_tx(&mut tx).await {
        debug!(
            "failed to lock snapshot state for {}: {e:?}",
            payload.hostname
        );
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    if let Err(e) = crate::services::composite_enforcement::lock_poam_derivations_for_store_path_tx(
        &mut tx,
        payload.store_path.as_deref(),
    )
    .await
    {
        debug!(
            "❌ failed to lock deployed derivation for {}: {e:?}",
            payload.hostname
        );
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let system_id = match crate::queries::system_events::find_system_id_by_hostname_tx(
        &mut tx,
        &payload.hostname,
    )
    .await
    {
        Ok(value) => value,
        Err(e) => {
            debug!("❌ failed to resolve system {}: {e:?}", payload.hostname);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    if let Some(system_id) = system_id {
        if let Err(e) = crate::services::composite_enforcement::lock_poam_findings_for_system_tx(
            &mut tx, system_id,
        )
        .await
        {
            debug!(
                "❌ failed to lock POA&M findings for {}: {e:?}",
                payload.hostname
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }
    let previous_observed =
        match lock_observed_system_state_by_hostname_tx(&mut tx, &payload.hostname).await {
            Ok(value) => value,
            Err(e) => {
                debug!(
                    "❌ failed to lock observed state for {}: {e:?}",
                    payload.hostname
                );
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

    let boot_id_change = if let Some(ref new_boot_id) = payload.boot_id {
        match update_boot_id_tx(&mut tx, &payload.hostname, new_boot_id).await {
            Ok(change) => Some(change),
            Err(e) => {
                debug!(
                    "❌ failed to update boot_id for {}: {e:?}",
                    payload.hostname
                );
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    } else {
        None // Older agent that does not send boot_id
    };

    // Persist restart classification inside the same transaction as the
    // heartbeat/state insert. See `classify_restart_type` for the full decision table.
    // A failure is logged and execution continues, but a SQL error here could leave
    // the transaction unusable and cause the later insert/commit to fail.
    let restart_type = classify_restart_type(boot_id_change, &payload.change_reason);
    if let Some(rtype) = restart_type {
        if let Err(e) = update_restart_type_tx(&mut tx, &payload.hostname, rtype).await {
            debug!(
                "⚠ failed to persist restart_type for {}: {e:?}",
                payload.hostname
            );
            // Non-fatal: continue; the heartbeat/state insert is more important.
        }
    }

    if let Err(e) = record_report_events_tx(
        &mut tx,
        previous_observed.as_ref(),
        &payload,
        boot_id_change,
        restart_type,
    )
    .await
    {
        debug!(
            "❌ failed to record system events for {}: {e:?}",
            payload.hostname
        );
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    // P1 (critical): System reboots MUST always insert a full system_states row,
    // even if from_system_state_if_heartbeat classified the payload as equivalent.
    // BootIdChange::Changed is the authoritative reboot signal; it overrides the
    // precomputed heartbeat_or_state decision to ensure we capture post-reboot state.
    let force_full_state_for_reboot = matches!(boot_id_change, Some(BootIdChange::Changed));

    if force_full_state_for_reboot {
        // Reboot detected: always write full system state regardless of equivalence.
        if let Err(e) = insert_system_state(
            &mut *tx,
            &payload,
            version_compatible,
            restart_type,
            Some("startup"),
        )
        .await
        {
            debug!("❌ failed to insert reboot system state: {e:?}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        if let Err(e) =
            crate::queries::evaluation_snapshots::retain_observed_generation_snapshot_tx(
                &mut tx,
                &payload.hostname,
                Some(agent_request.system.id),
                payload.generation,
                payload.store_path.as_deref(),
                payload.timestamp.unwrap_or_else(chrono::Utc::now),
            )
            .await
        {
            debug!("failed to retain reboot generation snapshot: {e:?}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    } else {
        // No reboot: use the precomputed heartbeat vs state decision.
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
                let (change_reason_override, event_restart_type) =
                    classify_non_reboot_state_change_reason(
                        &payload,
                        restart_type,
                        previous_observed.as_ref(),
                    );

                if let Err(e) = insert_system_state(
                    &mut *tx,
                    &payload,
                    version_compatible,
                    event_restart_type,
                    Some(change_reason_override),
                )
                .await
                {
                    debug!("❌ failed to insert system state: {e:?}");
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
                if let Err(e) =
                    crate::queries::evaluation_snapshots::retain_observed_generation_snapshot_tx(
                        &mut tx,
                        &payload.hostname,
                        Some(agent_request.system.id),
                        payload.generation,
                        payload.store_path.as_deref(),
                        payload.timestamp.unwrap_or_else(chrono::Utc::now),
                    )
                    .await
                {
                    debug!("failed to retain generation snapshot: {e:?}");
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            }
        }
    }

    if let Err(e) = tx.commit().await {
        debug!("❌ failed to commit heartbeat transaction: {e:?}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    // Routine heartbeats do not change Current evidence. A full state row
    // triggers best-effort membership repair only after its commit succeeds.
    if force_full_state_for_reboot || heartbeat_or_state.is_err() {
        crate::services::poam::schedule_scheduled_environment_cve_reconciliation_for_system(
            &pool,
            agent_request.system.id,
        );
    }

    // Reconcile system health attention after the heartbeat/state change.
    let _ = reconcile_system_health_attention(
        &pool,
        agent_request.system.id,
        &agent_request.system.hostname,
    )
    .await;

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

    // Log what was actually inserted (accounting for reboot override).
    if force_full_state_for_reboot {
        info!("📊 State change recorded for {} (reboot)", payload.hostname);
    } else {
        match heartbeat_or_state {
            Ok(_) => info!("💓 Heartbeat recorded for {}", payload.hostname),
            Err(_) => info!("📊 State change recorded for {}", payload.hostname),
        }
    }

    // Fetch desired target for this system. Manual systems only receive fresh,
    // explicit one-shot targets; stale manual targets are suppressed so agents
    // cannot revert hosts after an out-of-band/manual nixos-rebuild.
    let mut desired_target =
        match get_agent_desired_target_by_hostname(&pool, &agent_request.system.hostname).await {
            Ok(target) => target,
            Err(e) => {
                debug!("❌ Failed to fetch desired target: {e:?}");
                None // Continue with None if query fails
            }
        };

    let confidential = crate::handlers::api::builders::builder_https_verified_by_trusted_proxy(
        &state.server_config,
        &headers,
        peer.map(|peer| peer.0),
    );
    // SECURITY: Withhold the instruction before the one-shot deployment claim.
    // An empty response after a selected-cache error cannot authorize fallback.
    let runtime_caches = match load_runtime_caches_for_agent(
        &pool,
        agent_request.system.environment_id,
        confidential,
        capabilities,
    )
    .await
    {
        Ok(caches) => caches,
        Err(_) => {
            warn!("Selected agent cache unavailable; deployment delivery withheld");
            desired_target = None;
            Vec::new()
        }
    };

    if let Some(target) = desired_target.clone() {
        match crate::services::composite_enforcement::authorize_and_claim_desired_target(
            &pool,
            agent_request.system.id,
            &target,
        )
        .await
        {
            Ok(delivery) if delivery.target.is_some() => {
                desired_target = delivery.target;
            }
            Ok(delivery) => {
                warn!(
                    hostname = %agent_request.system.hostname,
                    target = %target,
                    outcome = ?delivery.authorization.outcome,
                    detail = %delivery.authorization.detail,
                    "Composite policy or desired-target guard blocked final deployment target delivery"
                );
                desired_target = None;
            }
            Err(error) => {
                warn!(
                    hostname = %agent_request.system.hostname,
                    target = %target,
                    "Composite final deployment authorization failed closed: {error:#}"
                );
                desired_target = None;
            }
        }
    }

    // Resolve per-system heartbeat interval, falling back to server-config default.
    let heartbeat_interval_secs = {
        let per_system = get_system_heartbeat_interval_secs(&pool, &agent_request.system.hostname)
            .await
            .unwrap_or_else(|e| {
                debug!(
                    "Failed to fetch heartbeat_interval_secs for {}: {e}",
                    agent_request.system.hostname
                );
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

fn classify_non_reboot_state_change_reason(
    payload: &crate::models::system_states::SystemState,
    restart_type: Option<&str>,
    previous: Option<&crate::queries::system_events::ObservedSystemState>,
) -> (&'static str, Option<&'static str>) {
    classify_non_reboot_state_change_reason_from_previous(
        payload.change_reason.as_str(),
        restart_type,
        payload.generation,
        payload.store_path.as_deref(),
        previous.map(|previous| (previous.generation, previous.store_path.as_deref())),
    )
}

fn classify_non_reboot_state_change_reason_from_previous(
    payload_change_reason: &str,
    restart_type: Option<&str>,
    payload_generation: Option<i32>,
    payload_store_path: Option<&str>,
    previous: Option<(Option<i32>, Option<&str>)>,
) -> (&'static str, Option<&'static str>) {
    let generation_or_store_changed = previous
        .map(|(previous_generation, previous_store_path)| {
            payload_generation != previous_generation || payload_store_path != previous_store_path
        })
        .unwrap_or(false);

    if generation_or_store_changed {
        // A changed generation/store path is the local rebuild/config delta.
        // It is not a restart classification event.
        return ("config_change", None);
    }

    if payload_change_reason == "startup" && restart_type == Some("agent_restart") {
        // Same generation/store path + unchanged boot_id means only the agent
        // service restarted, even if metadata such as agent build hash changed.
        return ("startup", Some("agent_restart"));
    }

    // Metadata-only full-state delta from heartbeat/startup. Do not invent a
    // local rebuild and do not carry restart_type on generic state changes.
    ("state_delta", None)
}

/// Pure function: given the boot_id comparison result and the change_reason string,
/// return the restart type to persist, or `None` if the classification should not be
/// updated (i.e. a routine periodic heartbeat with an unchanged boot_id).
///
/// Decision table:
///
/// | boot_id_change      | change_reason | result          |
/// |---------------------|---------------|-----------------|
/// | Changed             | any           | "system_reboot" |
/// | Unchanged           | "startup"     | "agent_restart" |
/// | Initialized         | "startup"     | "unknown"       |
/// | None (no boot_id)   | "startup"     | "unknown"       |
/// | Unchanged/Init/None | other         | None            |
///
/// `Changed` always classifies regardless of `change_reason` so that a reboot is
/// never lost when the startup heartbeat fails and a later periodic heartbeat is
/// the first to reach the server with the new `boot_id`.
fn classify_restart_type(
    boot_id_change: Option<BootIdChange>,
    change_reason: &str,
) -> Option<&'static str> {
    match boot_id_change {
        // A changed boot_id proves a host reboot on any heartbeat type.
        Some(BootIdChange::Changed) => Some("system_reboot"),
        // Same boot session, agent service restarted.
        Some(BootIdChange::Unchanged) if change_reason == "startup" => Some("agent_restart"),
        // No prior boot_id baseline; cannot distinguish reboot from restart.
        Some(BootIdChange::Initialized) if change_reason == "startup" => Some("unknown"),
        // Older agent without boot_id; overwrite any stale classification.
        None if change_reason == "startup" => Some("unknown"),
        // Periodic heartbeat with stable boot_id — keep the last classification.
        _ => None,
    }
}

/// Reconcile system health attention after a heartbeat or state change.
///
/// Queries the current health status from `view_system_list` and delegates
/// to the shared [`crate::tasks::attention_reconciliation::reconcile_system_attention`],
/// which also drives the periodic reconciliation sweep for systems that stop
/// heartbeating entirely (see that module for why a single request-triggered
/// hook cannot be the only producer for a continuously-recomputed health
/// status).
async fn reconcile_system_health_attention(pool: &PgPool, system_id: Uuid, hostname: &str) {
    let row: Option<(String, Option<Uuid>)> = sqlx::query_as(
        "SELECT vsl.health_status, s.environment_id \
         FROM view_system_list vsl \
         JOIN systems s ON s.id = vsl.id \
         WHERE vsl.id = $1",
    )
    .bind(system_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let Some((health, environment_id)) = row else {
        debug!(
            "Could not determine health status for system {hostname}; skipping attention reconciliation"
        );
        return;
    };

    crate::tasks::attention_reconciliation::reconcile_system_attention(
        pool,
        system_id,
        hostname,
        &health,
        environment_id,
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires verified isolated database and database creation privileges"]
    async fn niks3_signed_handler_preserves_pending_deployment_and_delivers_only_selected_reads(
        pool: PgPool,
    ) {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        use ed25519_dalek::Signer;
        let (key, verifying) = crate::test_utils::crypto::generate_keypair();
        let environment: Uuid = sqlx::query_scalar("INSERT INTO environments (name, description, is_active) VALUES ('signed-agent', 'test', TRUE) RETURNING id")
            .fetch_one(&pool).await.unwrap();
        let system_id: Uuid = sqlx::query_scalar("INSERT INTO systems (hostname, environment_id, public_key, derivation, deployment_policy, desired_target, desired_target_set_at) VALUES ('signed-host', $1, $2, '/nix/store/current', 'manual', '/nix/store/target', now()) RETURNING id")
            .bind(environment).bind(STANDARD.encode(verifying.to_bytes())).fetch_one(&pool).await.unwrap();
        let pending_id: Uuid = sqlx::query_scalar("INSERT INTO pending_system_deployments (system_id, target_store_path, source, request_action) VALUES ($1, '/nix/store/target', 'manual_rollback', 'rollback') RETURNING id")
            .bind(system_id).fetch_one(&pool).await.unwrap();
        let selected_id: i32 = sqlx::query_scalar("INSERT INTO cache_destinations (name, cache_type, enabled, push_to, niks3_server_url, niks3_write_auth_mode, niks3_public_keys, niks3_read_auth_mode, niks3_auth_token) VALUES ('z-assigned-niks3', 'Niks3', TRUE, 'https://selected.example', 'https://write.example', 'token', ARRAY['cache:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA='], 'none', 'write-token') RETURNING id")
            .fetch_one(&pool).await.unwrap();
        sqlx::query("INSERT INTO cache_destination_environments (cache_destination_id, environment_id) VALUES ($1, $2)")
            .bind(selected_id).bind(environment).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO cache_destinations (name, cache_type, enabled, push_to) VALUES ('a-global', 'Nix', TRUE, 'https://fallback.example')")
            .execute(&pool).await.unwrap();
        let mut config = crate::config::ServerConfig::default();
        config.trust_forwarded_builder_https = true;
        config.trusted_proxy_cidrs = vec!["127.0.0.1/32".into()];
        let state = CFState::new(
            pool.clone(),
            config,
            std::sync::Arc::new(crate::queue::QueueNotifier::new()),
            crate::server::jobs::BackgroundJobRegistry::new(),
        );
        let legacy_state = crate::test_utils::builders::SystemStateBuilder::new()
            .hostname("signed-host")
            .store_path("/nix/store/current")
            .build();
        let legacy = serde_json::to_value(legacy_state).unwrap();

        async fn send(
            state: &CFState,
            key: &ed25519_dalek::SigningKey,
            value: &serde_json::Value,
            confidential: bool,
        ) -> LogResponse {
            let body = serde_json::to_vec(value).unwrap();
            let mut headers = HeaderMap::new();
            headers.insert("x-key-id", "signed-host".parse().unwrap());
            headers.insert(
                "x-signature",
                STANDARD.encode(key.sign(&body).to_bytes()).parse().unwrap(),
            );
            // An attacker can supply headers even on an authenticated legacy body.
            headers.insert("x-agent-supports-niks3", "true".parse().unwrap());
            headers.insert(
                "x-agent-capabilities",
                "{\"supports_niks3\":true}".parse().unwrap(),
            );
            headers.insert("x-forwarded-proto", "https".parse().unwrap());
            let verified =
                authenticate_agent_request(&headers, Bytes::from(body.clone()), &state.pool)
                    .await
                    .unwrap();
            assert_eq!(
                deserialize_agent_capabilities(&verified)
                    .unwrap()
                    .supports_niks3,
                value["capabilities"]["supports_niks3"]
                    .as_bool()
                    .unwrap_or(false)
            );
            let (old_server_state, compatible) =
                deserialize_system_state_versioned(&verified).unwrap();
            assert!(compatible);
            assert_eq!(old_server_state.hostname, "signed-host");
            assert_eq!(
                old_server_state.store_path.as_deref(),
                Some("/nix/store/current")
            );
            let peer = confidential.then(|| ConnectInfo("127.0.0.1:12345".parse().unwrap()));
            let response = log(
                State(state.clone()),
                State(state.pool.clone()),
                peer,
                headers,
                Bytes::from(body),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            serde_json::from_slice(
                &axum::body::to_bytes(response.into_body(), 1024 * 1024)
                    .await
                    .unwrap(),
            )
            .unwrap()
        }
        for private in [false, true] {
            if private {
                sqlx::query("UPDATE cache_destinations SET niks3_read_auth_mode = 'mtls', niks3_read_client_cert = $2, niks3_read_client_key = 'read-secret' WHERE id = $1")
                    .bind(selected_id).bind(crate::security::cache_secrets::TEST_CERTIFICATE).execute(&pool).await.unwrap();
            }
            for (policy, confidential) in [
                ("manual", false),
                ("manual", true),
                ("pinned", false),
                ("pinned", true),
                ("auto_latest", false),
                ("auto_latest", true),
            ] {
                sqlx::query("UPDATE systems SET deployment_policy = $2 WHERE id = $1")
                    .bind(system_id)
                    .bind(policy)
                    .execute(&pool)
                    .await
                    .unwrap();
                let response = send(&state, &key, &legacy, confidential).await;
                assert!(response.desired_target.is_none());
                assert!(response.runtime_caches.is_empty());
                let pending: (String, Option<chrono::DateTime<chrono::Utc>>, Option<chrono::DateTime<chrono::Utc>>, Option<String>) = sqlx::query_as(
                    "SELECT status, completed_at, delivered_at, request_action FROM pending_system_deployments WHERE id = $1",
                )
                .bind(pending_id)
                .fetch_one(&pool)
                .await
                .unwrap();
                assert_eq!(
                    pending,
                    ("pending".into(), None, None, Some("rollback".into()))
                );
                assert_eq!(
                    get_agent_desired_target_by_hostname(&pool, "signed-host")
                        .await
                        .unwrap()
                        .as_deref(),
                    Some("/nix/store/target")
                );
            }
        }
        let ingested: i64 =
            sqlx::query_scalar("SELECT count(*) FROM system_states WHERE hostname = 'signed-host'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(ingested > 0);

        let mut current = legacy;
        current["capabilities"] = serde_json::json!({"supports_niks3": true});
        let response = send(&state, &key, &current, false).await;
        assert!(response.desired_target.is_none());
        assert!(response.runtime_caches.is_empty());
        // Keep positive read-delivery checks independent of composite target evidence.
        sqlx::query("UPDATE systems SET desired_target = NULL WHERE id = $1")
            .bind(system_id)
            .execute(&pool)
            .await
            .unwrap();
        let response = send(&state, &key, &current, true).await;
        assert_eq!(response.runtime_caches.len(), 1);
        assert_eq!(
            response.runtime_caches[0].cache_url,
            "https://selected.example"
        );
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("read-secret"));
        assert!(!json.contains("write-token"));
        assert!(!json.contains("fallback.example"));
        sqlx::query("UPDATE cache_destinations SET niks3_read_auth_mode = 'none', niks3_read_client_cert = NULL, niks3_read_client_key = NULL WHERE id = $1")
            .bind(selected_id).execute(&pool).await.unwrap();
        let response = send(&state, &key, &current, false).await;
        assert_eq!(response.runtime_caches.len(), 1);
        assert!(matches!(
            response.runtime_caches[0].read_auth,
            cf_protocol::cache::CacheReadAuth::None
        ));
        assert!(
            !serde_json::to_string(&response)
                .unwrap()
                .contains("write-token")
        );
        // An unreadable selected cache cannot expose the alphabetically earlier global.
        sqlx::query("UPDATE systems SET desired_target = '/nix/store/target', desired_target_set_at = now() WHERE id = $1")
            .bind(system_id).execute(&pool).await.unwrap();
        sqlx::query("UPDATE cache_destinations SET push_to = '' WHERE id = $1")
            .bind(selected_id)
            .execute(&pool)
            .await
            .unwrap();
        let response = send(&state, &key, &current, true).await;
        assert!(response.runtime_caches.is_empty());
        assert!(response.desired_target.is_none());
        let pending: String =
            sqlx::query_scalar("SELECT status FROM pending_system_deployments WHERE id = $1")
                .bind(pending_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(pending, "pending");
    }

    #[test]
    fn niks3_selected_cache_never_drops_private_or_unsupported_first_for_fallback() {
        let current = cf_protocol::agent::AgentCapabilities {
            supports_niks3: true,
        };
        let legacy = cf_protocol::agent::AgentCapabilities::default();
        let mut first = CacheDestination {
            enabled: true,
            cache_type: "Niks3".into(),
            push_to: Some("https://selected.example".into()),
            niks3_public_keys: vec!["cache:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into()],
            niks3_read_auth_mode: Some("none".into()),
            niks3_auth_token: Some("write-secret".into()),
            ..Default::default()
        };
        let fallback = CacheDestination {
            enabled: true,
            cache_type: "Nix".into(),
            push_to: Some("https://fallback.example".into()),
            ..Default::default()
        };
        for confidential in [false, true] {
            assert!(
                selected_runtime_cache(vec![first.clone(), fallback.clone()], confidential, legacy)
                    .is_err()
            );
            let caches = selected_runtime_cache(
                vec![first.clone(), fallback.clone()],
                confidential,
                current,
            )
            .unwrap();
            assert_eq!(caches.len(), 1);
            assert_eq!(caches[0].cache_url, "https://selected.example");
            assert!(
                !serde_json::to_string(&caches)
                    .unwrap()
                    .contains("write-secret")
            );
        }
        first.niks3_read_auth_mode = Some("mtls".into());
        first.niks3_read_client_cert =
            Some(crate::security::cache_secrets::TEST_CERTIFICATE.into());
        first.niks3_read_client_key = Some("read-secret".into());
        assert!(
            selected_runtime_cache(vec![first.clone(), fallback.clone()], false, current).is_err()
        );
        assert!(
            selected_runtime_cache(vec![first.clone(), fallback.clone()], true, legacy).is_err()
        );
        let caches =
            selected_runtime_cache(vec![first.clone(), fallback.clone()], true, current).unwrap();
        assert_eq!(caches.len(), 1);
        let json = serde_json::to_string(&caches).unwrap();
        assert!(json.contains("read-secret"));
        assert!(!json.contains("write-secret"));
        first.push_to = None;
        assert!(selected_runtime_cache(vec![first, fallback], true, current).is_err());
    }

    #[test]
    fn niks3_heartbeat_is_read_only_and_private_reads_fail_closed() {
        let mut destination = CacheDestination {
            enabled: true,
            cache_type: "Niks3".into(),
            push_to: Some("https://read.example".into()),
            niks3_public_keys: vec![
                "one:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into(),
                "two:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into(),
            ],
            niks3_read_auth_mode: Some("mtls".into()),
            niks3_read_client_cert: Some(crate::security::cache_secrets::TEST_CERTIFICATE.into()),
            niks3_read_client_key: Some("read-key".into()),
            niks3_auth_token: Some("write-token".into()),
            s3_secret_access_key: Some("aws-secret".into()),
            ..Default::default()
        };
        assert!(destination_to_runtime_cache(destination.clone(), false).is_none());
        let cache = destination_to_runtime_cache(destination.clone(), true).unwrap();
        assert_eq!(cache.cache_public_keys, destination.niks3_public_keys);
        let json = serde_json::to_string(&cache).unwrap();
        assert!(json.contains("read-key"));
        assert!(!json.contains("write-token"));
        assert!(!json.contains("aws-secret"));
        destination.niks3_read_auth_mode = Some("none".into());
        destination.niks3_read_client_cert = None;
        destination.niks3_read_client_key = None;
        assert!(destination_to_runtime_cache(destination.clone(), false).is_some());
        destination.enabled = false;
        assert!(destination_to_runtime_cache(destination.clone(), true).is_none());
        destination.enabled = true;
        destination.cache_type = "unknown".into();
        assert!(destination_to_runtime_cache(destination, true).is_none());
    }

    // ─── classify_restart_type tests ───────────────────────────────────────

    #[test]
    fn changed_boot_id_is_system_reboot_on_startup() {
        assert_eq!(
            classify_restart_type(Some(BootIdChange::Changed), "startup"),
            Some("system_reboot"),
        );
    }

    #[test]
    fn changed_boot_id_is_system_reboot_on_periodic_heartbeat() {
        // P1: a reboot must be captured even when the startup heartbeat failed
        // and the first successful request is a periodic "heartbeat".
        assert_eq!(
            classify_restart_type(Some(BootIdChange::Changed), "heartbeat"),
            Some("system_reboot"),
        );
    }

    #[test]
    fn unchanged_boot_id_on_startup_is_agent_restart() {
        assert_eq!(
            classify_restart_type(Some(BootIdChange::Unchanged), "startup"),
            Some("agent_restart"),
        );
    }

    #[test]
    fn unchanged_boot_id_on_heartbeat_returns_none() {
        // Periodic heartbeat must not overwrite the last startup classification.
        assert_eq!(
            classify_restart_type(Some(BootIdChange::Unchanged), "heartbeat"),
            None,
        );
    }

    #[test]
    fn initialized_boot_id_on_startup_is_unknown() {
        // P2: no baseline means we cannot prove it is an agent restart.
        assert_eq!(
            classify_restart_type(Some(BootIdChange::Initialized), "startup"),
            Some("unknown"),
        );
    }

    #[test]
    fn initialized_boot_id_on_heartbeat_returns_none() {
        assert_eq!(
            classify_restart_type(Some(BootIdChange::Initialized), "heartbeat"),
            None,
        );
    }

    #[test]
    fn no_boot_id_on_startup_is_unknown() {
        // P2: older agent without boot_id; overwrite stale classification.
        assert_eq!(classify_restart_type(None, "startup"), Some("unknown"));
    }

    #[test]
    fn no_boot_id_on_heartbeat_returns_none() {
        assert_eq!(classify_restart_type(None, "heartbeat"), None);
    }

    // ─── classify_non_reboot_state_change_reason tests ──────────────────────

    #[test]
    fn non_reboot_generation_change_is_config_change_without_restart_type() {
        assert_eq!(
            classify_non_reboot_state_change_reason_from_previous(
                "startup",
                Some("agent_restart"),
                Some(5056),
                Some("/nix/store/new-system"),
                Some((Some(5055), Some("/nix/store/old-system"))),
            ),
            ("config_change", None),
        );
    }

    #[test]
    fn unchanged_startup_preserves_agent_restart_classification() {
        assert_eq!(
            classify_non_reboot_state_change_reason_from_previous(
                "startup",
                Some("agent_restart"),
                Some(5056),
                Some("/nix/store/current-system"),
                Some((Some(5056), Some("/nix/store/current-system"))),
            ),
            ("startup", Some("agent_restart")),
        );
    }

    #[test]
    fn heartbeat_metadata_only_delta_is_state_delta_not_config_change() {
        assert_eq!(
            classify_non_reboot_state_change_reason_from_previous(
                "heartbeat",
                None,
                Some(5056),
                Some("/nix/store/current-system"),
                Some((Some(5056), Some("/nix/store/current-system"))),
            ),
            ("state_delta", None),
        );
    }

    #[test]
    fn missing_previous_state_does_not_invent_config_change() {
        assert_eq!(
            classify_non_reboot_state_change_reason_from_previous(
                "heartbeat",
                None,
                Some(5056),
                Some("/nix/store/current-system"),
                None,
            ),
            ("state_delta", None),
        );
    }

    // ─── duplicate-cleanup tests ────────────────────────────────────────────

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
