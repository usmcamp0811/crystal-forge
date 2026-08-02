//! Agent attestation submission handler.
//!
//! POST /api/v1/agent/running-state-attestations
//!
//! Uses agent authentication (Ed25519 key + signature), not account sessions.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::Utc;

use crate::api::models::ApiError;
use crate::handlers::agent_request::CFState;
use crate::queries::deployment_approval_requests;
use crate::queries::running_state_attestations;
use crate::queries::systems::get_by_hostname;
use crate::services::attestation_verification;
use crate::services::running_state_classification::{self, ClassificationInput};
use cf_protocol::attestation::{
    AttestationReceipt, SignedAttestationEnvelope, VerificationStatus,
};

/// Maximum request body size for attestation submissions (64 KiB).
const MAX_ATTESTATION_SIZE: usize = 65536;

/// Maximum allowed clock skew in seconds for attestation timestamps.
const MAX_CLOCK_SKEW_SECS: i64 = 300; // 5 minutes

pub async fn submit_attestation(
    State(state): State<CFState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Size limit
    if body.len() > MAX_ATTESTATION_SIZE {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(ApiError {
                error: "attestation_too_large".to_string(),
                message: format!("Attestation body exceeds {MAX_ATTESTATION_SIZE} bytes"),
                details: None,
            }),
        )
            .into_response();
    }

    // Parse the envelope
    let envelope: SignedAttestationEnvelope = match serde_json::from_slice(&body) {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiError {
                    error: "attestation_malformed".to_string(),
                    message: format!("Failed to parse attestation envelope: {e}"),
                    details: None,
                }),
            )
                .into_response();
        }
    };

    // Resolve the enrolled key by looking up the system by hostname
    let system = match get_by_hostname(&state.pool, &envelope.key_id).await {
        Ok(Some(system)) => system,
        Ok(None) => {
            return attestation_error(
                StatusCode::UNAUTHORIZED,
                "attestation_unknown_key",
                &format!("No enrolled system found for key ID '{}'", envelope.key_id),
            );
        }
        Err(e) => {
            tracing::error!("System lookup failed during attestation: {e:#}");
            return attestation_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "System lookup failed",
            );
        }
    };

    // Get the system's public key
    let public_key = system.public_key.verifying_key();

    // Verify the envelope
    let verification = attestation_verification::verify_envelope(
        &envelope,
        public_key,
        &system.hostname,
    );

    let verification_status = verification.status.to_string();
    let verification_reason = verification.reason.clone();

    // Begin transaction for insert + assessment
    let mut tx = match state.pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!("Failed to begin attestation transaction: {e:#}");
            return attestation_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Transaction failed",
            );
        }
    };

    // Check replay: attestation ID uniqueness
    if let Ok(true) = running_state_attestations::attestation_id_exists(
        &mut tx,
        envelope.payload.attestation_id,
    )
    .await
    {
        return attestation_error(
            StatusCode::CONFLICT,
            "attestation_replay",
            "Attestation ID already exists",
        );
    }

    // Check replay: monotonic counter
    if verification.status == VerificationStatus::Verified {
        if let Ok(Some(latest_counter)) = running_state_attestations::latest_counter_for_boot(
            &mut tx,
            &envelope.payload.agent_key_id,
            &envelope.payload.boot_id,
        )
        .await
        {
            if envelope.payload.monotonic_counter <= latest_counter {
                return attestation_error(
                    StatusCode::CONFLICT,
                    "attestation_counter_conflict",
                    &format!(
                        "Counter {} is not greater than latest accepted counter {}",
                        envelope.payload.monotonic_counter, latest_counter
                    ),
                );
            }
        }
    }

    // Check timestamp skew
    let now = Utc::now();
    let skew = (now - envelope.payload.observed_at).num_seconds().abs();
    if skew > MAX_CLOCK_SKEW_SECS {
        // Record but flag as stale_timestamp
        let stale_verification_status = VerificationStatus::StaleTimestamp.to_string();
        // Continue with insert but mark as stale
        tracing::warn!(
            "Attestation from {} has clock skew of {}s (max {}s)",
            envelope.key_id,
            skew,
            MAX_CLOCK_SKEW_SECS
        );
    }

    // Insert the attestation
    let row_id = match running_state_attestations::insert_attestation(
        &mut tx,
        envelope.payload.attestation_id,
        system.id,
        &envelope.payload.agent_key_id,
        envelope.payload.agent_session_id,
        envelope.payload.protocol_version,
        &envelope.payload.boot_id,
        envelope.payload.boot_timestamp,
        envelope.payload.observed_at,
        envelope.payload.monotonic_counter,
        &envelope.payload.current_system_store_path,
        envelope.payload.current_system_nar_hash.as_deref(),
        envelope.payload.system_profile_store_path.as_deref(),
        envelope.payload.booted_generation,
        envelope.payload.kernel_version.as_deref(),
        envelope.payload.nix_version.as_deref(),
        &envelope.payload.agent_version,
        envelope.payload.agent_build_hash.as_deref(),
        envelope.payload.deployment_authorization_id,
        envelope.payload.deployment_execution_id,
        envelope.payload.activation_source.as_deref(),
        &verification.canonical_bytes,
        &verification.payload_digest,
        &verification.signature_bytes,
        &verification_status,
        verification_reason.as_deref(),
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            // Check for unique constraint violations (replay)
            let err_str = e.to_string();
            if err_str.contains("rsa_attestation_id_unique")
                || err_str.contains("rsa_counter_unique")
            {
                return attestation_error(
                    StatusCode::CONFLICT,
                    "attestation_replay",
                    "Duplicate attestation ID or counter",
                );
            }
            tracing::error!("Failed to insert attestation: {e:#}");
            return attestation_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "Failed to store attestation",
            );
        }
    };

    // Calculate trust classification
    let has_valid_authorization = deployment_approval_requests::find_valid_authorization(
        &state.pool,
        system.id,
        &envelope.payload.current_system_store_path,
    )
    .await
    .unwrap_or(None)
    .is_some();

    // Check if artifact is known (exists in derivations or system store paths)
    // For now, simplified: if there's an authorization, it's known
    let artifact_is_known = has_valid_authorization; // TODO: check derivation store paths

    let classification_input = ClassificationInput {
        verification_status: verification.status,
        observed_store_path: envelope.payload.current_system_store_path.clone(),
        expected_store_path: system.desired_target.clone(),
        has_valid_authorization,
        artifact_is_known,
        was_previously_authorized: false, // TODO: check historical authorizations
        deployment_failed: false,         // TODO: check recent deployment status
        pending_reboot: false,            // TODO: check reboot status
        evidence_age_seconds: 0,          // freshly received
        freshness_threshold_seconds: crate::models::running_state_attestations::DEFAULT_EVIDENCE_FRESHNESS_SECS as i64,
    };

    let classification_result = running_state_classification::classify(&classification_input);
    let classification_str = classification_result.classification.to_string();

    // Insert assessment
    let _assessment_id = running_state_attestations::insert_assessment(
        &mut tx,
        envelope.payload.attestation_id,
        system.id,
        &classification_str,
        &classification_result.reason_code,
        None, // matched_authorization_id
        None, // matched_deployment_execution_id
        None, // matched_artifact_id
        1,    // classifier_version
    )
    .await;

    // Update system trust state
    let _ = running_state_attestations::upsert_system_trust_state(
        &mut tx,
        system.id,
        &classification_str,
        &classification_result.reason_code,
        Some(row_id),
        None, // latest_authorization_id
        Some(&envelope.payload.current_system_store_path),
        system.desired_target.as_deref(),
        Some(0), // fresh evidence
        None,    // investigation_id
    )
    .await;

    // Commit transaction
    if let Err(e) = tx.commit().await {
        tracing::error!("Failed to commit attestation: {e:#}");
        return attestation_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Transaction commit failed",
        );
    }

    // Return receipt
    let receipt = AttestationReceipt {
        id: row_id,
        attestation_id: envelope.payload.attestation_id,
        classification: classification_str,
        verification_status,
        reason: verification_reason,
        received_at: now,
    };

    (StatusCode::OK, Json(receipt)).into_response()
}

fn attestation_error(status: StatusCode, code: &str, message: &str) -> axum::response::Response {
    (
        status,
        Json(ApiError {
            error: code.to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
        .into_response()
}
