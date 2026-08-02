//! API handlers for running-state attestations, trust state, and resolution actions.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::models::ApiError;
use crate::handlers::agent_request::CFState;
use crate::handlers::api::rbac::{
    authenticated_user_roles, has_admin_role,
    has_viewer_or_above_role,
};
use crate::queries::running_state_attestations;

/// GET /api/v1/systems/:system_id/running-state-trust
pub async fn get_system_trust_state(
    State(state): State<CFState>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&state.pool, &headers).await else {
        return forbidden();
    };
    if !has_viewer_or_above_role(&roles) {
        return forbidden();
    }

    let trust_state = match running_state_attestations::get_system_trust_state(
        &state.pool,
        system_id,
    )
    .await
    {
        Ok(state) => state,
        Err(e) => {
            tracing::error!("Failed to get system trust state: {e:#}");
            return internal_error();
        }
    };

    let latest_attestation = match running_state_attestations::get_latest_verified_attestation(
        &state.pool,
        system_id,
    )
    .await
    {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Failed to get latest attestation: {e:#}");
            None
        }
    };

    let investigation = match running_state_attestations::get_open_investigation(
        &state.pool,
        system_id,
    )
    .await
    {
        Ok(i) => i,
        Err(e) => {
            tracing::error!("Failed to get investigation: {e:#}");
            None
        }
    };

    // Determine allowed actions
    let mut allowed_actions = Vec::new();
    if let Some(ref ts) = trust_state {
        let classification = &ts.current_classification;
        let is_flagged = matches!(
            classification.as_str(),
            "unauthorized_artifact" | "unknown_artifact" | "agent_identity_invalid"
        );

        if is_flagged && has_admin_role(&roles) {
            if classification != "agent_identity_invalid" {
                allowed_actions.push("adopt".to_string());
            }
            allowed_actions.push("replace".to_string());
            allowed_actions.push("investigate".to_string());
        }

        if investigation.is_some() && has_admin_role(&roles) {
            allowed_actions.push("close_investigation".to_string());
        }
    }

    let classification_label = trust_state
        .as_ref()
        .and_then(|ts| {
            ts.current_classification
                .parse::<cf_protocol::attestation::TrustClassification>()
                .ok()
        })
        .map(|c| c.label().to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    let detail = crate::models::running_state_attestations::SystemTrustDetail {
        system_id,
        current_classification: trust_state
            .as_ref()
            .map(|s| s.current_classification.clone())
            .unwrap_or_else(|| "no_attestation".to_string()),
        reason_code: trust_state
            .as_ref()
            .map(|s| s.reason_code.clone())
            .unwrap_or_else(|| "no_data".to_string()),
        classification_label,
        latest_attestation,
        verification_status: trust_state
            .as_ref()
            .and_then(|_| None), // TODO: from latest attestation
        observed_store_path: trust_state.as_ref().and_then(|s| s.observed_store_path.clone()),
        expected_store_path: trust_state.as_ref().and_then(|s| s.expected_store_path.clone()),
        matched_authorization_id: trust_state.as_ref().and_then(|s| s.latest_authorization_id),
        evidence_age_seconds: trust_state.as_ref().and_then(|s| s.evidence_age_seconds),
        investigation,
        allowed_actions,
    };

    (StatusCode::OK, Json(detail)).into_response()
}

/// GET /api/v1/systems/:system_id/running-state-attestations
#[derive(Debug, Deserialize)]
pub struct ListAttestationsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn list_attestation_history(
    State(state): State<CFState>,
    headers: HeaderMap,
    Path(system_id): Path<Uuid>,
    Query(query): Query<ListAttestationsQuery>,
) -> impl IntoResponse {
    let Some((_user_id, roles)) = authenticated_user_roles(&state.pool, &headers).await else {
        return forbidden();
    };
    if !has_viewer_or_above_role(&roles) {
        return forbidden();
    }

    let limit = query.limit.unwrap_or(50).min(100);
    let offset = query.offset.unwrap_or(0);

    match running_state_attestations::list_attestation_history(
        &state.pool,
        system_id,
        limit,
        offset,
    )
    .await
    {
        Ok(attestations) => (StatusCode::OK, Json(attestations)).into_response(),
        Err(e) => {
            tracing::error!("Failed to list attestation history: {e:#}");
            internal_error()
        }
    }
}

/// POST /api/v1/running-state-attestations/:attestation_id/actions
#[derive(Debug, Deserialize)]
pub struct ResolutionActionRequest {
    pub action: String,
    pub note: String,
    pub owner_user_id: Option<Uuid>,
    pub resolution_reason: Option<String>,
}

pub async fn submit_resolution_action(
    State(state): State<CFState>,
    headers: HeaderMap,
    Path(attestation_row_id): Path<Uuid>,
    Json(payload): Json<ResolutionActionRequest>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&state.pool, &headers).await else {
        return forbidden();
    };
    if !has_admin_role(&roles) {
        return forbidden();
    }

    if payload.note.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "validation_error".to_string(),
                message: "An audit note is required".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    let valid_actions = ["adopt", "replace", "investigate", "close_investigation"];
    if !valid_actions.contains(&payload.action.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "validation_error".to_string(),
                message: format!(
                    "Invalid action '{}'; expected one of {:?}",
                    payload.action, valid_actions
                ),
                details: None,
            }),
        )
            .into_response();
    }

    let mut tx = match state.pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!("Failed to begin transaction: {e:#}");
            return internal_error();
        }
    };

    // Look up the attestation to get system_id
    let attestation_system_id = match sqlx::query_scalar::<_, Uuid>(
        "SELECT system_id FROM running_state_attestations WHERE id = $1",
    )
    .bind(attestation_row_id)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(Some(sid)) => sid,
        Ok(None) => return not_found("attestation_not_found"),
        Err(e) => {
            tracing::error!("Failed to look up attestation: {e:#}");
            return internal_error();
        }
    };

    // Handle specific actions
    let mut investigation_id = None;
    let mut created_authorization_id = None;

    match payload.action.as_str() {
        "investigate" => {
            match running_state_attestations::open_investigation(
                &mut tx,
                attestation_system_id,
                attestation_row_id,
                user_id,
                &payload.note,
                payload.owner_user_id,
            )
            .await
            {
                Ok(inv) => investigation_id = Some(inv.id),
                Err(e) => {
                    tracing::error!("Failed to open investigation: {e:#}");
                    return internal_error();
                }
            }
        }
        "close_investigation" => {
            let inv = match running_state_attestations::get_open_investigation(
                &state.pool,
                attestation_system_id,
            )
            .await
            {
                Ok(Some(inv)) => inv,
                Ok(None) => {
                    return (
                        StatusCode::CONFLICT,
                        Json(ApiError {
                            error: "no_open_investigation".to_string(),
                            message: "No open investigation exists for this system".to_string(),
                            details: None,
                        }),
                    )
                        .into_response();
                }
                Err(e) => {
                    tracing::error!("Failed to get investigation: {e:#}");
                    return internal_error();
                }
            };

            let reason = payload
                .resolution_reason
                .as_deref()
                .unwrap_or("resolved");

            if let Err(e) = running_state_attestations::close_investigation(
                &mut tx,
                inv.id,
                user_id,
                reason,
                &payload.note,
            )
            .await
            {
                tracing::error!("Failed to close investigation: {e:#}");
                return internal_error();
            }
            investigation_id = Some(inv.id);
        }
        "adopt" => {
            // Create a deployment authorization for the observed artifact
            let observed_path = match sqlx::query_scalar::<_, String>(
                "SELECT current_system_store_path FROM running_state_attestations WHERE id = $1",
            )
            .bind(attestation_row_id)
            .fetch_one(&mut *tx)
            .await
            {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("Failed to get attestation store path: {e:#}");
                    return internal_error();
                }
            };

            match crate::queries::deployment_approval_requests::create_bypass_authorization(
                &mut tx,
                attestation_system_id,
                &observed_path,
                None,
                None,
                None,
                Some(user_id),
                None,
            )
            .await
            {
                Ok(auth) => created_authorization_id = Some(auth.id),
                Err(e) => {
                    tracing::error!("Failed to create adoption authorization: {e:#}");
                    return internal_error();
                }
            }
        }
        "replace" => {
            // The replace action starts the normal deployment path.
            // For now, record the action. The actual deployment request
            // will be handled by the deployment policy manager on the next cycle.
        }
        _ => unreachable!("validated above"),
    }

    // Record the resolution action
    match running_state_attestations::insert_resolution_action(
        &mut tx,
        attestation_system_id,
        attestation_row_id,
        user_id,
        &payload.action,
        &payload.note,
        created_authorization_id,
        None, // created_deployment_request_id
        investigation_id,
    )
    .await
    {
        Ok(action) => {
            if let Err(e) = tx.commit().await {
                tracing::error!("Failed to commit resolution action: {e:#}");
                return internal_error();
            }
            (StatusCode::OK, Json(action)).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to record resolution action: {e:#}");
            internal_error()
        }
    }
}

/// GET /api/v1/running-state-attestations/summary
pub async fn get_trust_summary(
    State(state): State<CFState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let Some((_user_id, roles)) = authenticated_user_roles(&state.pool, &headers).await else {
        return forbidden();
    };
    if !has_viewer_or_above_role(&roles) {
        return forbidden();
    }

    match running_state_attestations::get_trust_summary(&state.pool).await {
        Ok(summary) => (StatusCode::OK, Json(summary)).into_response(),
        Err(e) => {
            tracing::error!("Failed to get trust summary: {e:#}");
            internal_error()
        }
    }
}

fn forbidden() -> axum::response::Response {
    (
        StatusCode::FORBIDDEN,
        Json(ApiError {
            error: "forbidden".to_string(),
            message: "Insufficient privileges".to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn not_found(code: &str) -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: code.to_string(),
            message: "Resource not found".to_string(),
            details: None,
        }),
    )
        .into_response()
}

fn internal_error() -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: "internal_error".to_string(),
            message: "An internal error occurred".to_string(),
            details: None,
        }),
    )
        .into_response()
}
