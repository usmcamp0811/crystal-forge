// Deployment policy workflow API endpoints
// Handles approval submission and canary rollout status queries

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::server::AppState;
use crate::services::{
    approval_policy::{self, DeploymentContext},
    canary_rollout::{self, RolloutContext},
};

// =============================================================================
// Request/Response Types
// =============================================================================

#[derive(Debug, Deserialize)]
pub struct SubmitApprovalRequest {
    pub policy_id: Uuid,
    #[serde(default)]
    pub comment: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SubmitApprovalResponse {
    pub approval_id: Uuid,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ApprovalStatusResponse {
    pub policy_id: Uuid,
    pub approvals_received: usize,
    pub approvals_required: usize,
    pub deployment_allowed: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RolloutStatusResponse {
    pub rollout_id: Option<Uuid>,
    pub current_phase: Option<i32>,
    pub total_phases: Option<i32>,
    pub status: Option<String>,
    pub systems_in_current_phase: Vec<Uuid>,
    pub systems_completed: Vec<Uuid>,
    pub systems_failed: Vec<Uuid>,
    pub phase_observation_end: Option<String>,
    pub halted_reason: Option<String>,
}

// =============================================================================
// Approval Endpoints
// =============================================================================

/// Submit approval for a commit deployment
/// POST /api/v1/deployments/commit/:commit_id/approve
pub async fn submit_commit_approval(
    State(state): State<AppState>,
    Path(commit_id): Path<String>,
    user_id: Uuid, // TODO: Extract from session/auth
    Json(request): Json<SubmitApprovalRequest>,
) -> Result<Json<SubmitApprovalResponse>, (StatusCode, String)> {
    // Get policy config to determine expiration
    let policy = crate::queries::deployment_policies::get_deployment_policy_by_id(&state.pool, &request.policy_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch policy: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch policy".to_string(),
            )
        })?;

    let expires_after_hours = if let Some(policy_record) = policy {
        // Extract expires_after_hours from config if policy type is require_approvals
        if policy_record.policy_type == "require_approvals" {
            policy_record
                .config
                .get("expires_after_hours")
                .and_then(|v| v.as_u64())
                .map(|h| h as u32)
        } else {
            None
        }
    } else {
        return Err((StatusCode::NOT_FOUND, "Policy not found".to_string()));
    };

    let approval_id = approval_policy::submit_approval(
        &state.pool,
        DeploymentContext::Commit,
        &commit_id,
        request.policy_id,
        user_id,
        request.comment.clone(),
        expires_after_hours,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to submit approval: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to submit approval".to_string(),
        )
    })?;

    Ok(Json(SubmitApprovalResponse {
        approval_id,
        message: "Approval submitted successfully".to_string(),
    }))
}

/// Get approval status for a commit deployment
/// GET /api/v1/deployments/commit/:commit_id/approvals/:policy_id
pub async fn get_commit_approval_status(
    State(state): State<AppState>,
    Path((commit_id, policy_id)): Path<(String, Uuid)>,
) -> Result<Json<ApprovalStatusResponse>, (StatusCode, String)> {
    // Get policy config
    let policy = crate::queries::deployment_policies::get_deployment_policy_by_id(&state.pool, &policy_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch policy: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch policy".to_string(),
            )
        })?
        .ok_or((StatusCode::NOT_FOUND, "Policy not found".to_string()))?;

    if policy.policy_type != "require_approvals" {
        return Err((
            StatusCode::BAD_REQUEST,
            "Policy is not a require_approvals policy".to_string(),
        ));
    }

    // Parse config
    let config = serde_json::from_value::<crate::models::deployment_policies::ApprovalConfig>(
        policy.config.clone(),
    )
    .map_err(|e| {
        tracing::error!("Failed to parse approval config: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Invalid policy configuration".to_string(),
        )
    })?;

    // Check approvals
    let result = approval_policy::check_approvals(
        &state.pool,
        DeploymentContext::Commit,
        &commit_id,
        policy_id,
        &config,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to check approvals: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to check approvals".to_string(),
        )
    })?;

    Ok(Json(ApprovalStatusResponse {
        policy_id,
        approvals_received: result.approvals_received,
        approvals_required: result.approvals_required,
        deployment_allowed: result.deployment_allowed,
        reason: result.reason,
    }))
}

// =============================================================================
// Canary Rollout Endpoints
// =============================================================================

/// Get canary rollout status for a commit
/// GET /api/v1/deployments/commit/:commit_id/rollout/:policy_id
pub async fn get_commit_rollout_status(
    State(state): State<AppState>,
    Path((commit_id, policy_id)): Path<(String, Uuid)>,
) -> Result<Json<RolloutStatusResponse>, (StatusCode, String)> {
    let rollout_state = canary_rollout::get_rollout_state(
        &state.pool,
        RolloutContext::Commit,
        &commit_id,
        policy_id,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch rollout state: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to fetch rollout state".to_string(),
        )
    })?;

    match rollout_state {
        None => Ok(Json(RolloutStatusResponse {
            rollout_id: None,
            current_phase: None,
            total_phases: None,
            status: None,
            systems_in_current_phase: vec![],
            systems_completed: vec![],
            systems_failed: vec![],
            phase_observation_end: None,
            halted_reason: None,
        })),
        Some(state) => Ok(Json(RolloutStatusResponse {
            rollout_id: Some(state.id),
            current_phase: Some(state.current_phase),
            total_phases: Some(state.total_phases),
            status: Some(state.status.as_str().to_string()),
            systems_in_current_phase: state.systems_in_current_phase,
            systems_completed: state.systems_completed,
            systems_failed: state.systems_failed,
            phase_observation_end: state
                .phase_observation_end
                .map(|dt| dt.to_rfc3339()),
            halted_reason: state.halted_reason,
        })),
    }
}
