//! API handlers for deployment approval requests.

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
    authenticated_user_roles, has_admin_role, has_operator_or_admin_role,
    has_viewer_or_above_role,
};
use crate::queries::deployment_approval_requests;

/// GET /api/v1/deployment-approvals
#[derive(Debug, Deserialize)]
pub struct ListApprovalsQuery {
    pub status: Option<String>,
    pub system_id: Option<Uuid>,
    pub environment_id: Option<Uuid>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn list_approval_requests(
    State(state): State<CFState>,
    headers: HeaderMap,
    Query(query): Query<ListApprovalsQuery>,
) -> impl IntoResponse {
    let Some((_user_id, roles)) = authenticated_user_roles(&state.pool, &headers).await else {
        return forbidden();
    };
    if !has_viewer_or_above_role(&roles) {
        return forbidden();
    }

    let limit = query.limit.unwrap_or(50).min(200);
    let offset = query.offset.unwrap_or(0);

    match deployment_approval_requests::list_approval_requests(
        &state.pool,
        query.status.as_deref(),
        query.system_id,
        query.environment_id,
        limit,
        offset,
    )
    .await
    {
        Ok(requests) => (StatusCode::OK, Json(requests)).into_response(),
        Err(e) => {
            tracing::error!("Failed to list approval requests: {e:#}");
            internal_error()
        }
    }
}

/// GET /api/v1/deployment-approvals/:request_id
pub async fn get_approval_request(
    State(state): State<CFState>,
    headers: HeaderMap,
    Path(request_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&state.pool, &headers).await else {
        return forbidden();
    };
    if !has_viewer_or_above_role(&roles) {
        return forbidden();
    }

    let user_role = if has_admin_role(&roles) {
        "admin"
    } else if has_operator_or_admin_role(&roles) {
        "operator"
    } else {
        "viewer"
    };

    match deployment_approval_requests::get_approval_request_detail(
        &state.pool,
        request_id,
        user_id,
        user_role,
    )
    .await
    {
        Ok(Some(detail)) => (StatusCode::OK, Json(detail)).into_response(),
        Ok(None) => not_found("approval_request_not_found"),
        Err(e) => {
            tracing::error!("Failed to get approval request detail: {e:#}");
            internal_error()
        }
    }
}

/// POST /api/v1/deployment-approvals/:request_id/approve
#[derive(Debug, Deserialize)]
pub struct ApproveRequest {
    pub note: Option<String>,
}

pub async fn approve_request(
    State(state): State<CFState>,
    headers: HeaderMap,
    Path(request_id): Path<Uuid>,
    Json(payload): Json<ApproveRequest>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&state.pool, &headers).await else {
        return forbidden();
    };
    if !has_operator_or_admin_role(&roles) {
        return forbidden();
    }

    let actor_role = if has_admin_role(&roles) {
        "admin"
    } else {
        "operator"
    };

    let mut tx = match state.pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!("Failed to begin transaction: {e:#}");
            return internal_error();
        }
    };

    match deployment_approval_requests::record_approval_decision(
        &mut tx,
        request_id,
        user_id,
        "approve",
        payload.note.as_deref(),
        Some(actor_role),
    )
    .await
    {
        Ok((decision, updated_request)) => {
            if let Err(e) = tx.commit().await {
                tracing::error!("Failed to commit approval: {e:#}");
                return internal_error();
            }

            #[derive(Serialize)]
            struct ApproveResponse {
                decision_id: Uuid,
                request_status: String,
                current_approval_count: i64,
            }

            // Count approvals from updated request
            let count = deployment_approval_requests::list_decisions_for_request(
                &state.pool,
                request_id,
            )
            .await
            .map(|d| d.iter().filter(|d| d.decision == "approve").count() as i64)
            .unwrap_or(0);

            (
                StatusCode::OK,
                Json(ApproveResponse {
                    decision_id: decision.id,
                    request_status: updated_request.status,
                    current_approval_count: count,
                }),
            )
                .into_response()
        }
        Err(e) => {
            let msg = e.to_string();
            let (status, code) = match msg.as_str() {
                "approval_request_not_pending" => (StatusCode::CONFLICT, msg),
                "approval_request_expired" => (StatusCode::GONE, msg),
                "approval_role_required" => (StatusCode::FORBIDDEN, msg),
                "approval_requester_not_allowed" => (StatusCode::FORBIDDEN, msg),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error".to_string()),
            };
            (
                status,
                Json(ApiError {
                    error: code,
                    message: e.to_string(),
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

/// POST /api/v1/deployment-approvals/:request_id/reject
#[derive(Debug, Deserialize)]
pub struct RejectRequest {
    pub note: String,
}

pub async fn reject_request(
    State(state): State<CFState>,
    headers: HeaderMap,
    Path(request_id): Path<Uuid>,
    Json(payload): Json<RejectRequest>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&state.pool, &headers).await else {
        return forbidden();
    };
    if !has_operator_or_admin_role(&roles) {
        return forbidden();
    }

    if payload.note.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "validation_error".to_string(),
                message: "A rejection note is required".to_string(),
                details: None,
            }),
        )
            .into_response();
    }

    let actor_role = if has_admin_role(&roles) {
        "admin"
    } else {
        "operator"
    };

    let mut tx = match state.pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!("Failed to begin transaction: {e:#}");
            return internal_error();
        }
    };

    match deployment_approval_requests::record_approval_decision(
        &mut tx,
        request_id,
        user_id,
        "reject",
        Some(&payload.note),
        Some(actor_role),
    )
    .await
    {
        Ok((decision, updated_request)) => {
            if let Err(e) = tx.commit().await {
                tracing::error!("Failed to commit rejection: {e:#}");
                return internal_error();
            }

            #[derive(Serialize)]
            struct RejectResponse {
                decision_id: Uuid,
                request_status: String,
            }

            (
                StatusCode::OK,
                Json(RejectResponse {
                    decision_id: decision.id,
                    request_status: updated_request.status,
                }),
            )
                .into_response()
        }
        Err(e) => {
            let msg = e.to_string();
            let (status, code) = match msg.as_str() {
                "approval_request_not_pending" => (StatusCode::CONFLICT, msg),
                "approval_request_expired" => (StatusCode::GONE, msg),
                "approval_role_required" => (StatusCode::FORBIDDEN, msg),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error".to_string()),
            };
            (
                status,
                Json(ApiError {
                    error: code,
                    message: e.to_string(),
                    details: None,
                }),
            )
                .into_response()
        }
    }
}

/// POST /api/v1/deployment-approvals/:request_id/cancel
pub async fn cancel_request(
    State(state): State<CFState>,
    headers: HeaderMap,
    Path(request_id): Path<Uuid>,
) -> impl IntoResponse {
    let Some((user_id, roles)) = authenticated_user_roles(&state.pool, &headers).await else {
        return forbidden();
    };

    // Check the request exists and user can cancel
    let request = match deployment_approval_requests::get_approval_request(
        &state.pool,
        request_id,
    )
    .await
    {
        Ok(Some(r)) => r,
        Ok(None) => return not_found("approval_request_not_found"),
        Err(e) => {
            tracing::error!("Failed to get approval request: {e:#}");
            return internal_error();
        }
    };

    let is_requester = request.requested_by_user_id == Some(user_id);
    let is_admin = has_admin_role(&roles);

    if !is_requester && !is_admin {
        return forbidden();
    }

    let mut tx = match state.pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!("Failed to begin transaction: {e:#}");
            return internal_error();
        }
    };

    match deployment_approval_requests::cancel_request(&mut tx, request_id).await {
        Ok(()) => {
            if let Err(e) = tx.commit().await {
                tracing::error!("Failed to commit cancellation: {e:#}");
                return internal_error();
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            tracing::error!("Failed to cancel request: {e:#}");
            internal_error()
        }
    }
}

/// GET /api/v1/deployment-approvals/summary
pub async fn get_approval_summary(
    State(state): State<CFState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let Some((_user_id, roles)) = authenticated_user_roles(&state.pool, &headers).await else {
        return forbidden();
    };
    if !has_viewer_or_above_role(&roles) {
        return forbidden();
    }

    match deployment_approval_requests::get_approval_summary(&state.pool).await {
        Ok(summary) => (StatusCode::OK, Json(summary)).into_response(),
        Err(e) => {
            tracing::error!("Failed to get approval summary: {e:#}");
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
