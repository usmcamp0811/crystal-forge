// Deployment policy workflow API endpoints
// Handles approval submission and canary rollout status queries

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::handlers::agent_request::CFState;
use crate::handlers::api::rbac::{
    authenticated_user_roles, has_admin_role, has_operator_or_admin_role,
};
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
    State(state): State<CFState>,
    headers: HeaderMap,
    Path(commit_id): Path<String>,
    Json(request): Json<SubmitApprovalRequest>,
) -> Result<Json<SubmitApprovalResponse>, (StatusCode, String)> {
    let Some((user_id, roles)) = authenticated_user_roles(&state.pool, &headers).await else {
        return Err((StatusCode::UNAUTHORIZED, "Unauthorized".to_string()));
    };

    // Get policy config to validate role requirement
    let policy = crate::queries::deployment_policies::get_deployment_policy_by_id(
        &state.pool,
        &request.policy_id,
    )
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

    // Parse approval config to check required role
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

    // Verify user has required role for approval
    let has_required_role = match config.role.to_lowercase().as_str() {
        "admin" => has_admin_role(&roles),
        "operator" => has_operator_or_admin_role(&roles),
        "viewer" => true, // viewer or above is authenticated
        _ => {
            tracing::warn!(
                "Unknown role requirement in approval policy: {}",
                config.role
            );
            false
        }
    };

    if !has_required_role {
        return Err((
            StatusCode::FORBIDDEN,
            format!("Approval requires {} role", config.role),
        ));
    }

    let expires_after_hours = config.expires_after_hours;

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
    State(state): State<CFState>,
    headers: HeaderMap,
    Path((commit_id, policy_id)): Path<(String, Uuid)>,
) -> Result<Json<ApprovalStatusResponse>, (StatusCode, String)> {
    // Require authentication for status reads
    if authenticated_user_roles(&state.pool, &headers)
        .await
        .is_none()
    {
        return Err((StatusCode::UNAUTHORIZED, "Unauthorized".to_string()));
    }
    // Get policy config
    let policy =
        crate::queries::deployment_policies::get_deployment_policy_by_id(&state.pool, &policy_id)
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
    State(state): State<CFState>,
    headers: HeaderMap,
    Path((commit_id, policy_id)): Path<(String, Uuid)>,
) -> Result<Json<RolloutStatusResponse>, (StatusCode, String)> {
    // Require authentication for status reads
    if authenticated_user_roles(&state.pool, &headers)
        .await
        .is_none()
    {
        return Err((StatusCode::UNAUTHORIZED, "Unauthorized".to_string()));
    }
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
            phase_observation_end: state.phase_observation_end.map(|dt| dt.to_rfc3339()),
            halted_reason: state.halted_reason,
        })),
    }
}
